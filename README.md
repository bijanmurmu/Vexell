# Vexell

Blazing fast, **100% lossless** SVG to Image converter.

Vexell is designed for pixel-perfect graphics. Because SVGs are mathematically scalable vectors, Vexell converts them to perfectly crisp PNGs (and WebP) at massive resolutions without losing a single pixel of quality.

###  The Interactive Magic Menu

Run Vexell without any arguments to enter the **Magic Menu**.

```text
██╗   ██╗███████╗██╗  ██╗███████╗██╗     ██╗
██║   ██║██╔════╝╚██╗██╔╝██╔════╝██║     ██║
██║   ██║█████╗   ╚███╔╝ █████╗  ██║     ██║
╚██╗ ██╔╝██╔══╝   ██╔██╗ ██╔══╝  ██║     ██║
 ╚████╔╝ ███████╗██╔╝ ██╗███████╗███████╗███████╗
  ╚═══╝  ╚══════╝╚═╝  ╚═╝╚══════╝╚══════╝╚══════╝

  Blazing fast SVG to Image converter (100% Lossless)

Vexell Main Menu
│
│ 🚀 1 - Standard Conversion (Smart Defaults)
│ 🌌 2 - Purely Lossless Conversion (Massive Resolution & Format)
│ 🎯 3 - Magic Size Targeter (Hit an exact KB file size)
│ 🔄 4 - Universal Format Converter (Change any format instantly)
│ ⚡ Shift+! - Open Zsh Terminal
│ 🚪 /exit - Exit Vexell
│ 📚 /help - Help Menu
╰────────────────────────────────────────────────────────────────────────
╭─[ Select operation (1-4) ]
╰─> 
```

### 🔄 Universal Image Support

Vexell is no longer just for SVGs! You can now pass **ANY standard image** (`.png`, `.jpg`, `.webp`, `.bmp`, `.tiff`, `.ico`, `.gif`) into Vexell and use the menu options to dynamically resize them, hit exact file sizes, or just cleanly convert them to another format (Option 4).
*Note: ICO is strictly constrained to 256x256 and will automatically square-pad non-square inputs. GIF is mathematically limited to 8-bit color palettes.*

### 🎯 Option 3: The Magic Size Targeter
Have a strict `1.5 MB` upload limit? Need a logo that is exactly `500 KB`?
Select Option 3, type in `500 KB`, and Vexell will automatically binary-search quality thresholds to hit your target file size *using lossy compression (WebP/JPEG)* while perfectly maintaining 100% of your requested dimensions. No more muddy resolution scaling!

### ⚡ Built-in Terminal
Press `Shift+!` at any prompt to jump into Vexell's built-in Zsh terminal to quickly navigate your file system, copy files, and `cd` without ever leaving the app.

---

## Installation

### Via NPM (Node.js)
You can instantly run Vexell via `npx` without installing it:
```bash
npx vexell
```
Or install it globally:
```bash
npm install -g vexell
```

### Via Cargo (Rust)
If you prefer the ultra-fast Rust binary:
```bash
cargo install vexell
```

<details>
<summary><b>🛠️ Local Development & Contributing</b></summary>

### 1. NPM & Node Environment
If you are modifying the JavaScript wrapper or testing NPM deployment locally, the `node_modules/` folder is intentionally ignored in Git to prevent repository bloat. 
To recreate it and install all required JS dependencies, run:
```bash
npm install
```

### 2. Rust Core Engine Development
Vexell is primarily built in Rust. To work on the core engine locally, you will need to use Cargo.

**Run the engine in dev mode:**
```bash
cargo run
```
*(You can also pass arguments directly: `cargo run -- icon.svg output.png`)*

**Run the comprehensive E2E test suite:**
We have a robust 14-test E2E Python suite that verifies all edge cases, formats, and failure states.
```bash
python tests/test_vexell.py
```

**Build for Production (Release):**
When you are ready to compile the final, optimized binary:
```bash
cargo build --release
```
The blazing-fast compiled executable will be generated at `target/release/Vexell.exe`.
</details>

## Direct CLI Usage
You can bypass the interactive menu by passing arguments directly:
```bash
vexell <input.svg> <output.png> -W 4000 -O
```

* `-W, --width <pixels>`: Exact width of the output image
* `-H, --height <pixels>`: Exact height of the output image
* `-T, --target-size <bytes>`: Target exact output file size in bytes (Uses lossy WebP/JPG compression)
* `-f, --format <format>`: Output format (`png`, `webp`, `jpg`, `bmp`, `tiff`, `ico`, `gif`)
* `-O, --optimize`: Enable advanced lossless size optimization (Oxipng, extremely slow on massive images)
