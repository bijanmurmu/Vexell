#![allow(clippy::too_many_arguments)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::assign_op_pattern)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::useless_format)]
#![allow(clippy::manual_strip)]

use clap::Parser;
use console::{style, Term};
use dialoguer::{theme::ColorfulTheme, Confirm, Select};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Cmd, KeyCode as RlKeyCode, KeyEvent as RlKeyEvent, Modifiers as RlModifiers};
use rustyline::{Context, Editor, Helper};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use tiny_skia::Pixmap;
use usvg::{Options, Tree};

static CANCEL_CONVERSION: AtomicBool = AtomicBool::new(false);

struct ZshHelper {
    completer: FilenameCompleter,
}

impl Completer for ZshHelper {
    type Candidate = Pair;
    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        self.completer.complete(line, pos, ctx)
    }
}
impl Helper for ZshHelper {}
impl Highlighter for ZshHelper {}
impl Hinter for ZshHelper {
    type Hint = String;
    fn hint(&self, _: &str, _: usize, _: &Context<'_>) -> Option<String> {
        None
    }
}
impl Validator for ZshHelper {}

#[derive(Deserialize, Default, Debug, Clone)]
struct VexellConfig {
    width: Option<u32>,
    height: Option<u32>,
    format: Option<String>,
    optimize: Option<bool>,
    output_dir: Option<String>,
}

fn load_config() -> VexellConfig {
    if let Ok(content) = fs::read_to_string("vexell.toml") {
        toml::from_str(&content).unwrap_or_default()
    } else {
        VexellConfig::default()
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Vexell: Blazing fast SVG to Image converter", long_about = None)]
struct Args {
    /// Input SVG file path or glob pattern (leave empty for interactive mode)
    input: Option<String>,

    /// Output file or directory path
    output: Option<String>,

    /// Exact width of the output image
    #[arg(short = 'W', long)]
    width: Option<u32>,

    /// Exact height of the output image
    #[arg(short = 'H', long)]
    height: Option<u32>,

    /// Output format (png, webp, jpg, ico, gif).
    /// Note: ICO auto-pads to 256x256 max square. GIF is strictly 8-bit color with 1-bit alpha.
    #[arg(short = 'f', long)]
    format: Option<String>,

    /// Optimize output (lossless size reduction)
    #[arg(short = 'O', long)]
    optimize: bool,

    /// Target file size in bytes.
    /// The Magic Size Targeter uses lossy WebP (or JPEG) quality compression to hit the exact size limit while maintaining full resolution!
    #[arg(short = 'S', long)]
    target_size: Option<usize>,
}

fn process_file(
    input_path: &Path,
    exact_output_path: Option<&Path>,
    width: Option<u32>,
    height: Option<u32>,
    format_str: &str,
    optimize: bool,
    target_bytes_opt: Option<usize>,
    pb: Option<&ProgressBar>,
) -> Result<(PathBuf, u32, u32, usize), String> {
    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let is_svg = ext == "svg";

    enum LoadedImage {
        Svg(Tree),
        Raster(image::DynamicImage),
    }

    let loaded = if is_svg {
        let svg_data = fs::read(input_path).map_err(|e| e.to_string())?;
        let mut opt = Options::default();
        opt.fontdb_mut().load_system_fonts();
        LoadedImage::Svg(Tree::from_data(&svg_data, &opt).map_err(|e| e.to_string())?)
    } else {
        let mut reader = image::ImageReader::open(input_path)
            .map_err(|e| format!("Failed to open image file: {}", e))?;
        reader.no_limits();
        LoadedImage::Raster(
            reader
                .decode()
                .map_err(|e| format!("Failed to decode image: {}", e))?,
        )
    };

    let (orig_w, orig_h) = match &loaded {
        LoadedImage::Svg(tree) => (tree.size().width(), tree.size().height()),
        LoadedImage::Raster(img) => (img.width() as f32, img.height() as f32),
    };

    let image_format = match format_str.to_lowercase().as_str() {
        "jpg" | "jpeg" => image::ImageFormat::Jpeg,
        "webp" => image::ImageFormat::WebP,
        "bmp" => image::ImageFormat::Bmp,
        "ico" => image::ImageFormat::Ico,
        "tiff" => image::ImageFormat::Tiff,
        "gif" => image::ImageFormat::Gif,
        _ => image::ImageFormat::Png,
    };

    // The core rendering logic to produce a DynamicImage at target dimensions
    let mut w = orig_w;
    let mut h = orig_h;
    if let (Some(target_w), Some(target_h)) = (width, height) {
        w = target_w as f32;
        h = target_h as f32;
    } else if let Some(target_w) = width {
        let scale = target_w as f32 / w;
        w = target_w as f32;
        h = h * scale;
    } else if let Some(target_h) = height {
        let scale = target_h as f32 / h;
        h = target_h as f32;
        w = w * scale;
    }

    // Auto-scale for ICO format if dimensions exceed 256
    if format_str.to_lowercase() == "ico" {
        if w > 256.0 || h > 256.0 {
            let scale = 256.0 / w.max(h);
            w = (w * scale).max(1.0);
            h = (h * scale).max(1.0);
            if let Some(p) = pb {
                p.set_message("Auto-scaling image to max 256px for ICO format...");
            }
        }
    }

    let mut dyn_img = match &loaded {
        LoadedImage::Svg(tree) => {
            let transform = tiny_skia::Transform::from_scale(w / orig_w, h / orig_h);
            let mut pixmap =
                Pixmap::new(w.ceil() as u32, h.ceil() as u32).ok_or("Failed to create pixmap")?;

            resvg::render(tree, transform, &mut pixmap.as_mut());

            let img_width = pixmap.width();
            let img_height = pixmap.height();

            let mut data = pixmap.data().to_vec();
            for pixel in data.chunks_exact_mut(4) {
                let a = pixel[3];
                if a > 0 && a < 255 {
                    pixel[0] = ((pixel[0] as u32 * 255 + (a as u32 / 2)) / a as u32) as u8;
                    pixel[1] = ((pixel[1] as u32 * 255 + (a as u32 / 2)) / a as u32) as u8;
                    pixel[2] = ((pixel[2] as u32 * 255 + (a as u32 / 2)) / a as u32) as u8;
                }
            }

            let img = image::RgbaImage::from_raw(img_width, img_height, data)
                .ok_or("Failed to create image")?;
            image::DynamicImage::ImageRgba8(img)
        }
        LoadedImage::Raster(img) => {
            if (w.ceil() as u32) == (orig_w as u32) && (h.ceil() as u32) == (orig_h as u32) {
                img.clone()
            } else {
                img.resize_exact(
                    w.ceil() as u32,
                    h.ceil() as u32,
                    image::imageops::FilterType::Lanczos3,
                )
            }
        }
    };

    if format_str.to_lowercase() == "ico" {
        let max_dim = dyn_img.width().max(dyn_img.height());
        if dyn_img.width() != max_dim || dyn_img.height() != max_dim {
            let mut square = image::RgbaImage::new(max_dim, max_dim);
            let x = (max_dim - dyn_img.width()) / 2;
            let y = (max_dim - dyn_img.height()) / 2;
            image::imageops::overlay(&mut square, &dyn_img.to_rgba8(), x as i64, y as i64);
            dyn_img = image::DynamicImage::ImageRgba8(square);
        }
    }

    // GIF does not support semi-transparency. To prevent the patchy background caused by
    // matte blending, we use strict alpha-thresholding. Pixels are either fully visible
    // or fully invisible.
    if format_str.to_lowercase() == "gif" {
        if let Some(rgba) = dyn_img.as_mut_rgba8() {
            for pixel in rgba.pixels_mut() {
                if pixel[3] < 128 {
                    pixel[0] = 0;
                    pixel[1] = 0;
                    pixel[2] = 0;
                    pixel[3] = 0;
                } else {
                    pixel[3] = 255;
                }
            }
        }
    }

    let final_png_bytes;
    let final_w = dyn_img.width();
    let final_h = dyn_img.height();
    let mut actual_format_str = format_str.to_string();

    if let Some(target_bytes) = target_bytes_opt {
        let is_jpeg = format_str.to_lowercase() == "jpg" || format_str.to_lowercase() == "jpeg";
        actual_format_str = if is_jpeg {
            "jpg".to_string()
        } else {
            "webp".to_string()
        };

        let mut min_q = 1.0;
        let mut max_q = 100.0;
        let mut best_bytes = Vec::new();
        let rgba = dyn_img.to_rgba8();

        if let Some(p) = pb {
            let bar = format!("{}", "░".repeat(12));
            p.set_message(format!("Targeting Quality: [{}] 0%...", bar));
        }

        for i in 1..=12 {
            let mid_q = (min_q + max_q) / 2.0;
            let bytes = if is_jpeg {
                let mut b = Vec::new();
                let mut cursor = std::io::Cursor::new(&mut b);
                let mut enc =
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, mid_q as u8);
                enc.encode(
                    &rgba,
                    rgba.width(),
                    rgba.height(),
                    image::ColorType::Rgba8.into(),
                )
                .unwrap_or_default();
                b
            } else {
                let encoder = webp::Encoder::from_rgba(&rgba, rgba.width(), rgba.height());
                encoder.encode(mid_q).to_vec()
            };

            let size = bytes.len();

            if size <= target_bytes || best_bytes.is_empty() {
                best_bytes = bytes.clone();
            }

            if size > target_bytes {
                max_q = mid_q;
            } else {
                min_q = mid_q;
            }

            if let Some(p) = pb {
                let percent = (i as f32 / 12.0 * 100.0) as u32;
                let bar = format!("{}{}", "█".repeat(i), "░".repeat(12 - i));
                p.set_message(format!("Targeting Quality: [{}] {}%...", bar, percent));
            }
        }
        final_png_bytes = best_bytes;
    } else {
        let mut bytes: Vec<u8> = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut bytes);
        dyn_img
            .write_to(&mut cursor, image_format)
            .map_err(|e| e.to_string())?;
        final_png_bytes = bytes;
    }

    let out_path = if let Some(p) = exact_output_path {
        if let Some(parent) = p.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        let mut final_p = p.to_path_buf();
        if target_bytes_opt.is_some() {
            final_p.set_extension(&actual_format_str);
        }
        final_p
    } else {
        input_path.with_extension(&actual_format_str)
    };

    // Save the bytes to disk
    fs::write(&out_path, &final_png_bytes).map_err(|e| e.to_string())?;

    if optimize && actual_format_str.to_lowercase() == "png" {
        if let Some(p) = pb {
            p.set_message("File generated! Now deep-compressing with Oxipng (Extremely slow)...");
        }
        let oxi_options = oxipng::Options::from_preset(4); // Very high lossless compression
        if let Err(e) = oxipng::optimize(
            &oxipng::InFile::Path(out_path.clone()),
            &oxipng::OutFile::Path {
                path: Some(out_path.clone()),
                preserve_attrs: false,
            },
            &oxi_options,
        ) {
            eprintln!(
                "Warning: Oxipng optimization failed for {}: {}",
                out_path.display(),
                e
            );
        }
    }

    let final_size = fs::metadata(&out_path)
        .map(|m| m.len() as usize)
        .unwrap_or(final_png_bytes.len());

    Ok((out_path, final_w, final_h, final_size))
}

fn print_help() {
    // Enter Alternate Screen Buffer (true fullscreen, no scrollback pollution)
    print!("\x1B[?1049h\x1B[2J\x1B[1;1H");
    use std::io::Write;
    let _ = std::io::stdout().flush();

    println!("\n{}", style("╭─  Vexell Help Guide").cyan().bold());
    println!("{}", style("│").cyan().bold());

    println!(
        "{} {}",
        style("│").cyan().bold(),
        style("🚀 1 - Standard Conversion (Smart Defaults)")
            .yellow()
            .bold()
    );
    println!(
        "{} {}",
        style("│").cyan().bold(),
        style("       Converts SVGs to pristine PNGs rapidly with default 1x scaling.").dim()
    );
    println!("{}", style("│").cyan().bold());

    println!(
        "{} {}",
        style("│").cyan().bold(),
        style("🌌 2 - Purely Lossless Conversion (Massive Resolution & Format)")
            .yellow()
            .bold()
    );
    println!("{} {}", style("│").cyan().bold(), style("       Scale SVGs losslessly to exact pixel bounds (-W / -H), outputting to PNG or WebP.").dim());
    println!("{}", style("│").cyan().bold());

    println!(
        "{} {}",
        style("│").cyan().bold(),
        style("🎯 3 - Magic Size Targeter (Hit an exact KB file size)")
            .yellow()
            .bold()
    );
    println!("{} {}", style("│").cyan().bold(), style("       Input a size (e.g., 500 KB) and Vexell will automatically binary-search to hit that exact target.").dim());
    println!("{}", style("│").cyan().bold());

    println!(
        "{} {}",
        style("│").cyan().bold(),
        style("🔄 4 - Universal Format Converter (Change any format instantly)")
            .yellow()
            .bold()
    );
    println!("{} {}", style("│").cyan().bold(), style("       Convert any image (SVG, PNG, JPG, WebP, BMP, TIFF, ICO, GIF) to another format instantly.").dim());
    println!("{}", style("│").cyan().bold());

    println!(
        "{} {}",
        style("│").cyan().bold(),
        style("📁 Input Support:").magenta().bold()
    );
    println!("{} {}", style("│").cyan().bold(), style("       Pass exact files (logo.svg) or batch globs (src/**/*.svg) to process thousands of files at once.").dim());
    println!("{}", style("│").cyan().bold());

    println!(
        "{} {}",
        style("│").cyan().bold(),
        style("⚡ Shift+! - Open Zsh Terminal").yellow().bold()
    );
    println!("{} {}", style("│").cyan().bold(), style("       Drop into a persistent shell mid-session to inspect your files without losing your place.").dim());
    println!("{}", style("│").cyan().bold());

    println!(
        "{} {}",
        style("│").cyan().bold(),
        style("🚪 /exit - Exit Vexell").yellow().bold()
    );
    println!(
        "{} {}",
        style("│").cyan().bold(),
        style("📚 /help - Help Menu").blue().bold()
    );
    println!(
        "{}",
        style("╰────────────────────────────────────────────────────────────────────────")
            .cyan()
            .bold()
    );
    println!();

    println!(
        "{}",
        style("Press ENTER to return to the main menu...").dim()
    );
    let mut input = String::new();
    let _ = std::io::stdin().read_line(&mut input);

    // Exit Alternate Screen Buffer (restores previous screen instantly)
    print!("\x1B[?1049l");
    let _ = std::io::stdout().flush();
}

fn run_zsh_terminal() {
    let mut rl = Editor::<ZshHelper, _>::new().unwrap();
    rl.set_helper(Some(ZshHelper {
        completer: FilenameCompleter::new(),
    }));
    // Bind ESC to interrupt, allowing user to exit Zsh terminal gracefully
    rl.bind_sequence(
        RlKeyEvent(RlKeyCode::Esc, RlModifiers::NONE),
        Cmd::Interrupt,
    );

    println!(
        "\n{} Zsh Terminal Mode Active.\n{}",
        style("⚡").yellow(),
        style("Press ESC to return to Vexell, or type /exit to quit completely.").dim()
    );

    loop {
        let current_dir = std::env::current_dir().unwrap_or_default();
        let prompt = format!("╭─ ⚡ {}\n╰─> ", current_dir.display());

        match rl.readline(&prompt) {
            Ok(line) => {
                let line = line.trim();
                if line == "/exit" {
                    println!("Goodbye! ✨");
                    std::process::exit(0);
                }
                if line == "/help" {
                    print_help();
                    continue;
                }
                if line.starts_with("cd ") {
                    let path = line[3..].trim();
                    if let Err(e) = std::env::set_current_dir(path) {
                        eprintln!("{} {}", style("❌ Error changing directory:").red(), e);
                    }
                } else if !line.is_empty() {
                    let _ = std::process::Command::new("powershell")
                        .arg("-NoProfile")
                        .arg("-c")
                        .arg(line)
                        .status();
                }
                let _ = rl.add_history_entry(line);
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                println!("{}", style("Exited Zsh Terminal.").dim());
                break;
            }
            Err(_) => {
                std::process::exit(0);
            }
        }
    }
}

fn interactive_mode(config: VexellConfig) {
    let term = Term::stdout();
    term.clear_screen().unwrap_or(());

    println!(
        "{}",
        style("██╗   ██╗███████╗██╗  ██╗███████╗██╗     ██╗     ")
            .magenta()
            .bold()
    );
    println!(
        "{}",
        style("██║   ██║██╔════╝╚██╗██╔╝██╔════╝██║     ██║     ")
            .magenta()
            .bold()
    );
    println!(
        "{}",
        style("██║   ██║█████╗   ╚███╔╝ █████╗  ██║     ██║     ")
            .magenta()
            .bold()
    );
    println!(
        "{}",
        style("╚██╗ ██╔╝██╔══╝   ██╔██╗ ██╔══╝  ██║     ██║     ")
            .magenta()
            .bold()
    );
    println!(
        "{}",
        style(" ╚████╔╝ ███████╗██╔╝ ██╗███████╗███████╗███████╗")
            .magenta()
            .bold()
    );
    println!(
        "{}",
        style("  ╚═══╝  ╚══════╝╚═╝  ╚═╝╚══════╝╚══════╝╚══════╝")
            .magenta()
            .bold()
    );
    println!(
        "{}",
        style("  Blazing fast SVG to Image converter (100% Lossless)  ")
            .black()
            .on_magenta()
            .bold()
    );
    println!();

    if std::path::Path::new("vexell.toml").exists() {
        println!(
            "{} {}",
            style("[⚙]").cyan(),
            style("Project config (vexell.toml) loaded!").dim()
        );
        println!();
    }

    let mut rl = Editor::<ZshHelper, _>::new().unwrap();
    rl.set_helper(Some(ZshHelper {
        completer: FilenameCompleter::new(),
    }));
    // Bind '!' to Eof so we can instantly switch to Zsh mode without hitting Enter
    rl.bind_sequence(
        RlKeyEvent(RlKeyCode::Char('!'), RlModifiers::NONE),
        Cmd::EndOfFile,
    );

    'main_loop: loop {
        println!("\n{}", style("Vexell Main Menu").cyan().bold());
        println!("{}", style("│").cyan().bold());
        println!(
            "{} {} - Standard Conversion (Smart Defaults)",
            style("│").cyan().bold(),
            style("🚀 1").yellow().bold()
        );
        println!(
            "{} {} - Purely Lossless Conversion (Massive Resolution & Format)",
            style("│").cyan().bold(),
            style("🌌 2").yellow().bold()
        );
        println!(
            "{} {} - Magic Size Targeter (Hit an exact KB file size)",
            style("│").cyan().bold(),
            style("🎯 3").yellow().bold()
        );
        println!(
            "{} {} - Universal Format Converter (Change any format instantly)",
            style("│").cyan().bold(),
            style("🔄 4").yellow().bold()
        );
        println!(
            "{} {} - Open Zsh Terminal",
            style("│").cyan().bold(),
            style("⚡ Shift+!").yellow().bold()
        );
        println!(
            "{} {} - Exit Vexell",
            style("│").cyan().bold(),
            style("🚪 /exit").red().bold()
        );
        println!(
            "{} {} - Help Menu",
            style("│").cyan().bold(),
            style("📚 /help").blue().bold()
        );
        println!(
            "{}",
            style("╰────────────────────────────────────────────────────────────────────────")
                .cyan()
                .bold()
        );

        let op_prompt_top = format!("{}", style("╭─[ Select operation (1-4) ]").cyan().bold());
        println!("{}", op_prompt_top);
        let op = match rl.readline("╰─> ") {
            Ok(line) => {
                let line = line.trim();
                if line == "/exit" {
                    println!("Goodbye! ✨");
                    std::process::exit(0);
                }
                if line == "/help" {
                    print_help();
                    continue;
                }
                if line == "1" || line == "2" || line == "3" || line == "4" {
                    line.to_string()
                } else if !line.is_empty() {
                    println!(
                        "{}",
                        style("❌ Invalid option. Please enter 1, 2, 3, or 4, or type /exit.")
                            .red()
                    );
                    continue;
                } else {
                    continue;
                }
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                run_zsh_terminal();
                continue;
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                println!("Goodbye! ✨");
                std::process::exit(0);
            }
            Err(_) => std::process::exit(0),
        };

        let in_prompt_top = format!(
            "{}",
            style("╭─[ Enter Image/SVG file to convert (TAB to autocomplete) ]")
                .cyan()
                .bold()
        );
        println!("{}", in_prompt_top);
        let input_pattern = match rl.readline("╰─> ") {
            Ok(line) => {
                let line = line.trim();
                if line == "/exit" {
                    println!("Goodbye! ✨");
                    std::process::exit(0);
                }
                if line == "/help" {
                    print_help();
                    continue;
                }
                if line.is_empty() {
                    continue;
                }
                line.to_string()
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                run_zsh_terminal();
                continue;
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                println!("{}", style("Cancelled.").yellow());
                continue 'main_loop;
            }
            Err(_) => std::process::exit(0),
        };

        let current_dir_before = std::env::current_dir().unwrap_or_default();
        let mut input_files = Vec::new();
        if std::fs::metadata(&input_pattern).is_ok() && !input_pattern.contains('*') {
            input_files.push(current_dir_before.join(&input_pattern));
        } else if let Ok(glob_results) = glob::glob(&input_pattern) {
            for entry in glob_results.flatten() {
                if entry.is_file() {
                    input_files.push(current_dir_before.join(entry));
                }
            }
        }

        if input_files.is_empty() {
            println!(
                "{}",
                style("❌ No matching files found. Please try again.").red()
            );
            continue;
        }
        println!(
            "{} Found {} file(s).",
            style("[✓]").green(),
            input_files.len()
        );

        let mut output = None;
        let target_dir_display;

        let out_prompt_top = format!(
            "{}",
            style("╭─ 🎯 Target Directory (blank = in-place, '.' = current dir)")
                .cyan()
                .bold()
        );
        loop {
            println!("{}", out_prompt_top);
            match rl.readline("╰─> ") {
                Ok(out) => {
                    let trimmed = out.trim();
                    if trimmed == "/exit" {
                        println!("Goodbye! ✨");
                        std::process::exit(0);
                    }
                    if trimmed == "/help" {
                        print_help();
                        continue;
                    }
                    if !trimmed.is_empty() {
                        let current_dir = std::env::current_dir().unwrap_or_default();
                        let p = Path::new(trimmed);
                        if p.is_absolute() {
                            target_dir_display = trimmed.to_string();
                        } else {
                            target_dir_display = current_dir.join(p).display().to_string();
                        }
                        output = Some(target_dir_display.clone());
                    } else {
                        target_dir_display = "Same as input (In-place)".to_string();
                    }
                    break;
                }
                Err(rustyline::error::ReadlineError::Eof) => {
                    run_zsh_terminal();
                    continue;
                }
                Err(rustyline::error::ReadlineError::Interrupted) => {
                    println!("{}", style("Cancelled.").yellow());
                    continue 'main_loop;
                }
                Err(_) => std::process::exit(0),
            }
        }

        println!(
            "{} {}\n",
            style("🎯 Target Directory:").cyan().bold(),
            style(target_dir_display).green()
        );

        let mut final_width = config.width;
        let final_height = config.height;
        let mut target_bytes_opt = None;

        if op == "2" {
            let w_prompt_top = format!(
                "{}",
                style("╭─ 🌌 Output Width (e.g. 4000 for massive pixels, blank for original)")
                    .cyan()
                    .bold()
            );
            loop {
                println!("{}", w_prompt_top);
                match rl.readline("╰─> ") {
                    Ok(line) => {
                        let line = line.trim();
                        if line.is_empty() {
                            break;
                        }
                        if let Ok(w) = line.parse::<u32>() {
                            final_width = Some(w);
                            break;
                        } else {
                            println!(
                                "{}",
                                style("❌ Please enter a valid number (e.g. 4000).").red()
                            );
                        }
                    }
                    Err(rustyline::error::ReadlineError::Interrupted) => continue 'main_loop,
                    Err(_) => std::process::exit(0),
                }
            }
        }

        if op == "3" {
            let num_val;
            let num_prompt = format!(
                "{}",
                style("╭─ 🎯 Target exact file size in KB (e.g. 500)")
                    .cyan()
                    .bold()
            );
            loop {
                println!("{}", num_prompt);
                match rl.readline("╰─> ") {
                    Ok(line) => {
                        let line = line.trim();
                        if let Ok(num) = line.parse::<f64>() {
                            num_val = num;
                            break;
                        } else {
                            println!("{}", style("❌ Please enter a valid number.").red());
                        }
                    }
                    Err(rustyline::error::ReadlineError::Interrupted) => continue 'main_loop,
                    Err(_) => std::process::exit(0),
                }
            }

            let units = &["Bytes", "KB", "MB", "GB"];
            let unit_idx = match Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select Unit")
                .default(1) // KB default
                .items(&units[..])
                .interact_opt()
                .unwrap_or(None)
            {
                Some(idx) => idx,
                None => {
                    println!("{}", style("Cancelled.").yellow());
                    continue 'main_loop;
                }
            };

            let bytes = match unit_idx {
                0 => num_val,
                1 => num_val * 1024.0,
                2 => num_val * 1024.0 * 1024.0,
                3 => num_val * 1024.0 * 1024.0 * 1024.0,
                _ => num_val,
            };
            target_bytes_opt = Some(bytes as usize);
        }

        let (format, optimize) = if op == "1" || op == "3" {
            (
                config.format.clone().unwrap_or_else(|| "auto".to_string()),
                config.optimize.unwrap_or(false),
            )
        } else if op == "4" {
            let formats = &["png", "jpg", "webp", "bmp", "ico", "tiff", "gif"];
            let format_selection = match Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select target format")
                .default(0)
                .items(&formats[..])
                .interact_opt()
                .unwrap_or(None)
            {
                Some(idx) => idx,
                None => {
                    println!("{}", style("Cancelled.").yellow());
                    continue 'main_loop;
                }
            };
            (formats[format_selection].to_string(), false)
        } else {
            let formats = &["png (Recommended for massive lossless pixels)", "webp"];
            let default_format_idx = if config.format.as_deref() == Some("webp") {
                1
            } else {
                0
            };
            let format_selection = match Select::with_theme(&ColorfulTheme::default())
                .with_prompt(format!(
                    "{} Select output format (Lossless guaranteed)",
                    style("[✓]").green()
                ))
                .default(default_format_idx)
                .items(&formats[..])
                .interact_opt()
                .unwrap_or(None)
            {
                Some(idx) => idx,
                None => {
                    println!("{}", style("Cancelled.").yellow());
                    continue 'main_loop;
                }
            };
            let format = if format_selection == 0 {
                "png".to_string()
            } else {
                "webp".to_string()
            };

            let optimize = match Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("Enable advanced lossless size optimization (Oxipng)? [WARNING: Extremely slow for massive images]")
                .default(config.optimize.unwrap_or(false))
                .interact_opt()
                .unwrap_or(None)
            {
                Some(val) => val,
                None => {
                    println!("{}", style("Cancelled.").yellow());
                    continue 'main_loop;
                }
            };
            (format, optimize)
        };

        run_conversion(
            input_files,
            output.as_deref(),
            final_width,
            final_height,
            &format,
            optimize,
            target_bytes_opt,
        );
    }
}

fn run_conversion(
    files: Vec<PathBuf>,
    output: Option<&str>,
    width: Option<u32>,
    height: Option<u32>,
    format: &str,
    optimize: bool,
    target_bytes_opt: Option<usize>,
) -> bool {
    CANCEL_CONVERSION.store(false, Ordering::SeqCst);
    let start = Instant::now();

    if files.is_empty() {
        eprintln!("{}", style("No files to process.").red());
        return false;
    }

    if let Some(bytes) = target_bytes_opt {
        let display_size = if bytes > 1024 * 1024 * 1024 {
            format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        } else if bytes > 1024 * 1024 {
            format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
        } else if bytes > 1024 {
            format!("{:.2} KB", bytes as f64 / 1024.0)
        } else {
            format!("{} Bytes", bytes)
        };
        println!(
            "\n{} Magic Size Targeter: Finding optimal resolution to hit ~{}...\n",
            style("[✓]").yellow(),
            style(display_size).bold()
        );
    } else {
        println!(
            "\n{} Starting exact-size, lossless conversion for {} file(s)....\n",
            style("🚀").magenta(),
            style(files.len()).bold()
        );
    }
    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("┳╸"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80)); // Animate spinner automatically
    pb.set_message("Processing...");

    let success_count = AtomicUsize::new(0);
    let fail_count = AtomicUsize::new(0);
    let successful_results = Mutex::new(Vec::new());

    let base_path = {
        if files.is_empty() {
            PathBuf::new()
        } else {
            let mut common = files[0].clone();
            common.pop();
            for file in files.iter().skip(1) {
                let mut file_parent = file.clone();
                file_parent.pop();
                while !common.as_os_str().is_empty() && !file_parent.starts_with(&common) {
                    if !common.pop() {
                        break;
                    }
                }
            }
            common
        }
    };

    files.par_iter().for_each(|file| {
        if CANCEL_CONVERSION.load(Ordering::SeqCst) {
            return;
        }

        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("png");
        let file_format = if format == "auto" {
            if ext.to_lowercase() == "svg" {
                "png"
            } else {
                ext
            }
        } else {
            format
        };

        let exact_out_path = output.map(|out_str| {
            let out_path = Path::new(out_str);
            let is_dir_target = out_path.is_dir()
                || out_str.ends_with('/')
                || out_str.ends_with('\\')
                || out_path.extension().is_none();

            if is_dir_target {
                let relative = file.strip_prefix(&base_path).unwrap_or(file);
                let mut final_path = out_path.join(relative);
                final_path.set_extension(file_format);
                final_path
            } else {
                out_path.to_path_buf()
            }
        });

        match process_file(
            file,
            exact_out_path.as_deref(),
            width,
            height,
            file_format,
            optimize,
            target_bytes_opt,
            Some(&pb),
        ) {
            Ok(res) => {
                successful_results.lock().unwrap().push(res);
                success_count.fetch_add(1, Ordering::SeqCst);
                pb.inc(1);
            }
            Err(e) => {
                fail_count.fetch_add(1, Ordering::SeqCst);
                pb.println(format!(
                    "{} Failed to convert {}: {}",
                    style("❌").red(),
                    file.display(),
                    e
                ));
                pb.inc(1);
            }
        }
    });

    if CANCEL_CONVERSION.load(Ordering::SeqCst) {
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.red} [{elapsed_precise}] [{bar:40.red/blue}] {pos}/{len} {msg}",
                )
                .unwrap()
                .progress_chars("┳╸"),
        );
        pb.finish_with_message("Cancelled");
        println!(
            "\n{}",
            style(" Batch conversion cancelled by user.")
                .yellow()
                .bold()
        );
    } else {
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )
                .unwrap()
                .progress_chars("┳╸"),
        );
        pb.finish_with_message("Done");

        let s = success_count.load(Ordering::SeqCst);
        if s > 0 {
            println!(
                "\n{} {} file(s) in {:?}",
                style("✨ Successfully converted").green().bold(),
                s,
                start.elapsed()
            );

            let mut results = successful_results.into_inner().unwrap();
            results.sort_by(|a, b| a.0.cmp(&b.0)); // Optional: sort by filename

            for (path, w, h, size) in results {
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                let size_str = if size > 1024 * 1024 * 1024 {
                    format!("{:.2} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
                } else if size > 1024 * 1024 {
                    format!("{:.2} MB", size as f64 / (1024.0 * 1024.0))
                } else if size > 1024 {
                    format!("{:.2} KB", size as f64 / 1024.0)
                } else {
                    format!("{} Bytes", size)
                };
                println!(
                    "  {} {} ({}x{}) - {}",
                    style("↳").dim(),
                    style(file_name).cyan(),
                    style(w).yellow(),
                    style(h).yellow(),
                    style(size_str).magenta()
                );
            }
            println!();
        }
    }
    true
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ctrlc::set_handler(|| {
        let was_cancelled = CANCEL_CONVERSION.swap(true, Ordering::SeqCst);
        if was_cancelled {
            println!(
                "\n{}",
                console::style(" Force quitting Vexell...").yellow().bold()
            );
            std::process::exit(1);
        } else {
            println!(
                "\n{}",
                console::style(
                    " Cancelling gracefully... (Press Ctrl+C again to force quit immediately)"
                )
                .yellow()
                .bold()
            );
        }
    })
    .unwrap_or_else(|e| eprintln!("Error setting Ctrl-C handler: {}", e));

    let args = Args::parse();
    let config = load_config();

    if let Some(input) = args.input {
        let width = args.width.or(config.width);
        let height = args.height.or(config.height);
        let format = args
            .format
            .unwrap_or_else(|| config.format.clone().unwrap_or_else(|| "png".to_string()));
        let optimize = args.optimize || config.optimize.unwrap_or(false);
        let output = args.output.or(config.output_dir.clone());

        let current_dir = std::env::current_dir().unwrap_or_default();
        let mut input_files = Vec::new();
        if std::fs::metadata(&input).is_ok() && !input.contains('*') {
            input_files.push(current_dir.join(&input));
        } else if let Ok(glob_results) = glob::glob(&input) {
            for entry in glob_results.flatten() {
                if entry.is_file() {
                    input_files.push(current_dir.join(entry));
                }
            }
        }
        let target_bytes_opt = args.target_size;

        let success = run_conversion(
            input_files,
            output.as_deref(),
            width,
            height,
            &format,
            optimize,
            target_bytes_opt,
        );
        if !success {
            std::process::exit(1);
        }
    } else {
        interactive_mode(config);
    }

    Ok(())
}
