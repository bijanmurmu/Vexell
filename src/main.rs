use clap::Parser;
use std::fs;
use usvg::{Options, Tree};
use tiny_skia::Pixmap;

#[derive(Parser, Debug)]
#[command(author, version, about = "Vexell: Blazing fast SVG to PNG converter", long_about = None)]
struct Args {
    /// Input SVG file path
    input: String,

    /// Output PNG file path
    output: String,

    /// Width of the output image (preserves aspect ratio)
    #[arg(short = 'W', long)]
    width: Option<u32>,

    /// Height of the output image (preserves aspect ratio)
    #[arg(short = 'H', long)]
    height: Option<u32>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let svg_data = fs::read(&args.input)?;
    let mut opt = Options::default();
    opt.fontdb_mut().load_system_fonts();
    
    let tree = Tree::from_data(&svg_data, &opt)?;
    
    let svg_size = tree.size();
    let mut w = svg_size.width();
    let mut h = svg_size.height();
    
    if let Some(target_w) = args.width {
        let scale = target_w as f32 / w;
        w = target_w as f32;
        h = h * scale;
    } else if let Some(target_h) = args.height {
        let scale = target_h as f32 / h;
        h = target_h as f32;
        w = w * scale;
    }

    let transform = tiny_skia::Transform::from_scale(w / svg_size.width(), h / svg_size.height());
    
    let mut pixmap = Pixmap::new(w.ceil() as u32, h.ceil() as u32).ok_or("Failed to create pixmap")?;
    
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    
    pixmap.save_png(&args.output)?;
    
    println!("Successfully rendered {} to {} ({}x{})", args.input, args.output, w.ceil(), h.ceil());
    
    Ok(())
}
