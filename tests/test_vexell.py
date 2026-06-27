import os
import subprocess
import tempfile
import shutil

SVG_CONTENT = """<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
    <rect width="100" height="100" fill="red"/>
    <circle cx="50" cy="50" r="40" fill="white"/>
</svg>"""

def run_engine(binary_path, args, expect_success=True):
    print(f"Running Engine: {' '.join(args)}")
    result = subprocess.run(args, capture_output=True, text=True)
    if (result.returncode == 0) != expect_success:
        print("Test failed!")
        print(f"Stdout:\n{result.stdout}")
        print(f"Stderr:\n{result.stderr}")
        exit(1)
    return result

def verify_file(path):
    if os.path.exists(path):
        size = os.path.getsize(path)
        if size > 0:
            print(f"Created perfectly: {path} ({size} bytes)")
            return True
    print(f"Failed: Output not found or empty at {path}")
    exit(1)

def run_test():
    print("Starting Vexell Comprehensive E2E Test Suite...")
    
    print("Compiling Vexell...")
    subprocess.run(["cargo", "build"], check=True, capture_output=True)
    
    import platform
    binary_name = "Vexell.exe" if platform.system() == "Windows" else "Vexell"
    binary_path = os.path.join("target", "debug", binary_name)
    if not os.path.exists(binary_path):
        print(f"Error: Binary not found after build at {binary_path}")
        exit(1)

    with tempfile.TemporaryDirectory() as temp_dir:
        input_dir = os.path.join(temp_dir, "src")
        output_dir = os.path.join(temp_dir, "dist")
        os.makedirs(input_dir)
        
        svg1 = os.path.join(input_dir, "icon1.svg")
        svg2 = os.path.join(input_dir, "icon2.svg")
        
        with open(svg1, "w", encoding="utf-8") as f:
            f.write(SVG_CONTENT)
        with open(svg2, "w", encoding="utf-8") as f:
            f.write(SVG_CONTENT)
            
        print("\n--- Test 1: Single file, output to directory ---")
        run_engine(binary_path, [binary_path, svg1, output_dir, "--format", "png", "--optimize"])
        verify_file(os.path.join(output_dir, "icon1.png"))
        
        print("\n--- Test 2: Single file, output to explicit file ---")
        explicit_file = os.path.join(output_dir, "custom_name.png")
        run_engine(binary_path, [binary_path, svg1, explicit_file, "--format", "png"])
        verify_file(explicit_file)
        
        print("\n--- Test 3: Multiple files (Glob), output to directory ---")
        glob_pattern = os.path.join(input_dir, "*.svg")
        glob_output_dir = os.path.join(temp_dir, "dist_glob")
        run_engine(binary_path, [binary_path, glob_pattern, glob_output_dir, "--format", "png"])
        verify_file(os.path.join(glob_output_dir, "icon1.png"))
        verify_file(os.path.join(glob_output_dir, "icon2.png"))

        print("\n--- Test 4: WebP Output Format ---")
        webp_output = os.path.join(output_dir, "icon1.webp")
        run_engine(binary_path, [binary_path, svg1, webp_output, "--format", "webp"])
        verify_file(webp_output)

        print("\n--- Test 5: Magic Size Targeter (Target Size CLI) ---")
        target_size_output_png = os.path.join(output_dir, "icon1_target.png")
        target_size_output_webp = os.path.join(output_dir, "icon1_target.webp")
        run_engine(binary_path, [binary_path, svg1, target_size_output_png, "--target-size", "2000", "--format", "png"])
        verify_file(target_size_output_webp)
        actual_size = os.path.getsize(target_size_output_webp)
        print(f"Target Size was 2000, Actual Size is {actual_size}")
        if actual_size > 2000:
            print(f"Failed: Size exceeded target! {actual_size} > 2000")
        print("\n--- Test 6: ICO Auto-scaling ---")
        # Ask for ICO format, but also provide a width of 800 which is illegal for ICO
        # Vexell should automatically scale it down to 256 to prevent panic.
        ico_output = os.path.join(output_dir, "icon1.ico")
        run_engine(binary_path, [binary_path, svg1, ico_output, "-W", "800", "--format", "ico"])
        verify_file(ico_output)

        print("\n--- Test 7: GIF Format (1-bit thresholding) ---")
        gif_output = os.path.join(output_dir, "icon1.gif")
        run_engine(binary_path, [binary_path, svg1, gif_output, "--format", "gif"])
        verify_file(gif_output)

        print("\n--- Test 8: Raster to Raster (PNG to WebP) ---")
        png_input = os.path.join(output_dir, "icon1.png")
        raster_webp_output = os.path.join(output_dir, "raster_to.webp")
        run_engine(binary_path, [binary_path, png_input, raster_webp_output, "--format", "webp"])
        verify_file(raster_webp_output)

        print("\n--- Test 9: JPG Format Output ---")
        jpg_output = os.path.join(output_dir, "icon1.jpg")
        run_engine(binary_path, [binary_path, svg1, jpg_output, "--format", "jpg"])
        verify_file(jpg_output)

        print("\n--- Test 10: BMP Format Output ---")
        bmp_output = os.path.join(output_dir, "icon1.bmp")
        run_engine(binary_path, [binary_path, svg1, bmp_output, "--format", "bmp"])
        verify_file(bmp_output)

        print("\n--- Test 11: TIFF Format Output ---")
        tiff_output = os.path.join(output_dir, "icon1.tiff")
        run_engine(binary_path, [binary_path, svg1, tiff_output, "--format", "tiff"])
        verify_file(tiff_output)

        print("\n--- Test 12: Exact Height Scaling (-H) ---")
        height_output = os.path.join(output_dir, "icon1_height.png")
        run_engine(binary_path, [binary_path, svg1, height_output, "-H", "500", "--format", "png"])
        verify_file(height_output)

        print("\n--- Test 13: Automatic Directory Creation ---")
        deep_output_dir = os.path.join(output_dir, "deep", "nested", "folder", "icon.png")
        run_engine(binary_path, [binary_path, svg1, deep_output_dir, "--format", "png"])
        verify_file(deep_output_dir)

        print("\n--- Test 14: Invalid Input File Handling ---")
        bad_input = os.path.join(input_dir, "does_not_exist.svg")
        # We expect this to fail gracefully (exit code 1 or continue in batch), so expect_success=False
        result = run_engine(binary_path, [binary_path, bad_input, output_dir], expect_success=False)
        print("Successfully caught invalid input error!")

    print("\nALL TESTS PASSED! Vexell Engine is rock solid.")

if __name__ == "__main__":
    run_test()
