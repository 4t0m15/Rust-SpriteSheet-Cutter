# Spritesheet Cutter

A robust Rust program that automatically processes spritesheet images and extracts individual sprite frames with intelligent background removal.

## Features

- **Automatic Frame Detection**: Intelligently detects sprite boundaries using edge detection and transparency analysis
- **Background Removal**: Automatically removes background colors using corner sampling and color tolerance
- **Multiple Format Support**: Handles PNG, JPG, JPEG, BMP, GIF, TIFF, and WebP files
- **Smart Naming**: Outputs files with systematic naming (e.g., `spritesheet1_frame_001.png`)
- **Memory Efficient**: Processes large spritesheets without excessive memory usage
- **Error Handling**: Comprehensive error handling for corrupted images and file system issues

## Usage

1. **Setup**: Make sure you have Rust installed on your system
2. **Place Images**: Put your spritesheet images in the same directory as the program
3. **Run**: Execute the program with `cargo run`
4. **Results**: Check the `assets2` directory for extracted frames

## How It Works

### Frame Detection Algorithm
1. Converts images to grayscale for analysis
2. Scans for vertical and horizontal boundaries using:
   - Transparency detection (80%+ transparent columns/rows)
   - Edge detection (sudden color changes)
3. Validates detected frames for minimum content (5% non-transparent pixels)
4. Filters frames by size constraints (16-512 pixels by default)

### Background Removal
1. Samples corner regions to detect background color
2. Uses configurable color tolerance for background matching
3. Makes background pixels transparent
4. Preserves sprite content with high accuracy

### Output Organization
- Creates `assets2` directory automatically
- Names files as `{original_name}_frame_{number}.png`
- Saves all frames as PNG with transparency support

## Configuration

The program uses sensible defaults but can be customized by modifying the `CutterConfig` struct:

```rust
CutterConfig {
    min_sprite_size: 16,        // Minimum frame size
    max_sprite_size: 512,       // Maximum frame size  
    background_tolerance: 10,   // Color matching tolerance
    remove_background: true,    // Enable background removal
    output_dir: "assets2",      // Output directory name
}
```

## Dependencies

- `image`: Core image processing
- `imageproc`: Advanced image operations
- `walkdir`: Directory traversal
- `anyhow`: Error handling
- `log` & `env_logger`: Logging

## Example Output

For a spritesheet named `character.png` with 8 frames:
```
assets2/
├── character_frame_001.png
├── character_frame_002.png
├── character_frame_003.png
├── character_frame_004.png
├── character_frame_005.png
├── character_frame_006.png
├── character_frame_007.png
└── character_frame_008.png
```

## Error Handling

The program handles various error conditions gracefully:
- Corrupted or unsupported image files
- File system permission issues
- Memory allocation problems
- Invalid image dimensions

Failed images are logged but don't stop the entire process.

## Performance

- Optimized for large spritesheets
- Memory-efficient processing
- Parallel-ready architecture
- Fast boundary detection algorithms

## Testing

Run tests with: `cargo test`

The test suite includes:
- Configuration validation
- Background detection accuracy
- Edge case handling
