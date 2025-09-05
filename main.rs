use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba, RgbaImage};
use imageproc::definitions::Image;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Configuration for the spritesheet cutter
#[derive(Debug, Clone)]
struct CutterConfig {
    /// Minimum width/height for a sprite frame
    min_sprite_size: u32,
    /// Maximum width/height for a sprite frame
    max_sprite_size: u32,
    /// Tolerance for background color detection
    background_tolerance: u8,
    /// Whether to remove backgrounds
    remove_background: bool,
    /// Output directory name
    output_dir: String,
}

impl Default for CutterConfig {
    fn default() -> Self {
        Self {
            min_sprite_size: 16,
            max_sprite_size: 512,
            background_tolerance: 10,
            remove_background: true,
            output_dir: "assets2".to_string(),
        }
    }
}

/// Represents a detected sprite frame
#[derive(Debug, Clone)]
struct SpriteFrame {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

/// Main spritesheet cutter structure
struct SpritesheetCutter {
    config: CutterConfig,
}

impl SpritesheetCutter {
    fn new(config: CutterConfig) -> Self {
        Self { config }
    }

    /// Process all image files in the current directory
    fn process_directory(&self) -> Result<()> {
        let current_dir = std::env::current_dir()
            .context("Failed to get current directory")?;
        
        // Create output directory
        let output_path = current_dir.join(&self.config.output_dir);
        fs::create_dir_all(&output_path)
            .context("Failed to create output directory")?;

        // Find all image files
        let image_files = self.find_image_files(&current_dir)?;
        
        if image_files.is_empty() {
            println!("No image files found in the current directory.");
            return Ok(());
        }

        println!("Found {} image files to process", image_files.len());

        for (index, image_path) in image_files.iter().enumerate() {
            println!("Processing {}/{}: {}", index + 1, image_files.len(), 
                    image_path.file_name().unwrap().to_string_lossy());
            
            if let Err(e) = self.process_spritesheet(image_path, &output_path) {
                eprintln!("Error processing {}: {}", 
                         image_path.file_name().unwrap().to_string_lossy(), e);
            }
        }

        println!("Processing complete! Check the '{}' directory for results.", 
                self.config.output_dir);
        Ok(())
    }

    /// Find all image files in the directory
    fn find_image_files(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        let mut image_files = Vec::new();
        let supported_extensions: HashSet<&str> = 
            ["png", "jpg", "jpeg", "bmp", "gif", "tiff", "webp"].iter().cloned().collect();

        for entry in WalkDir::new(dir)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                if let Some(extension) = entry.path().extension() {
                    if let Some(ext_str) = extension.to_str() {
                        if supported_extensions.contains(ext_str.to_lowercase().as_str()) {
                            image_files.push(entry.path().to_path_buf());
                        }
                    }
                }
            }
        }

        Ok(image_files)
    }

    /// Process a single spritesheet
    fn process_spritesheet(&self, image_path: &Path, output_dir: &Path) -> Result<()> {
        let img = image::open(image_path)
            .context("Failed to open image")?;

        let frames = self.detect_sprite_frames(&img)?;
        
        if frames.is_empty() {
            println!("No sprite frames detected in {}", 
                    image_path.file_name().unwrap().to_string_lossy());
            return Ok(());
        }

        println!("Detected {} frames", frames.len());

        // Extract and save each frame
        let base_name = image_path.file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();

        for (frame_index, frame) in frames.iter().enumerate() {
            let cropped = self.extract_frame(&img, frame)?;
            let processed = if self.config.remove_background {
                self.remove_background(&cropped)?
            } else {
                cropped
            };

            let filename = format!("{}_frame_{:03}.png", base_name, frame_index + 1);
            let output_path = output_dir.join(filename);
            
            processed.save(&output_path)
                .context("Failed to save frame")?;
        }

        Ok(())
    }

    /// Detect sprite frames in the image using intelligent boundary detection
    fn detect_sprite_frames(&self, img: &DynamicImage) -> Result<Vec<SpriteFrame>> {
        let (width, height) = img.dimensions();
        let mut frames = Vec::new();

        // Convert to grayscale for analysis
        let gray_img = img.to_luma8();
        
        // Find vertical and horizontal boundaries
        let vertical_boundaries = self.find_vertical_boundaries(&gray_img);
        let horizontal_boundaries = self.find_horizontal_boundaries(&gray_img);

        // Generate frames from boundaries
        for i in 0..vertical_boundaries.len().saturating_sub(1) {
            for j in 0..horizontal_boundaries.len().saturating_sub(1) {
                let x = vertical_boundaries[i];
                let y = horizontal_boundaries[j];
                let frame_width = vertical_boundaries[i + 1] - x;
                let frame_height = horizontal_boundaries[j + 1] - y;

                // Validate frame size
                if frame_width >= self.config.min_sprite_size 
                    && frame_height >= self.config.min_sprite_size
                    && frame_width <= self.config.max_sprite_size 
                    && frame_height <= self.config.max_sprite_size {
                    
                    // Check if frame contains non-transparent content
                    if self.frame_has_content(img, x, y, frame_width, frame_height) {
                        frames.push(SpriteFrame {
                            x,
                            y,
                            width: frame_width,
                            height: frame_height,
                        });
                    }
                }
            }
        }

        Ok(frames)
    }

    /// Find vertical boundaries (column separators)
    fn find_vertical_boundaries(&self, gray_img: &Image<image::Luma<u8>>) -> Vec<u32> {
        let (width, height) = gray_img.dimensions();
        let mut boundaries = vec![0]; // Start with left edge
        
        for x in 1..width.saturating_sub(1) {
            let mut is_boundary = true;
            let mut transparent_count = 0;
            
            // Check if this column is mostly transparent or uniform
            for y in 0..height {
                let pixel = gray_img.get_pixel(x, y);
                if pixel[0] < 10 { // Very dark/transparent
                    transparent_count += 1;
                }
            }
            
            // If more than 80% of the column is transparent, it's likely a boundary
            if transparent_count as f32 / height as f32 > 0.8 {
                boundaries.push(x);
            } else {
                // Check for sudden color changes (edge detection)
                let mut color_changes = 0;
                for y in 0..height.saturating_sub(1) {
                    let current = gray_img.get_pixel(x, y)[0] as i32;
                    let next = gray_img.get_pixel(x, y + 1)[0] as i32;
                    if (current - next).abs() > 50 {
                        color_changes += 1;
                    }
                }
                
                if color_changes as f32 / height as f32 > 0.3 {
                    boundaries.push(x);
                }
            }
        }
        
        boundaries.push(width); // End with right edge
        boundaries.sort();
        boundaries.dedup();
        boundaries
    }

    /// Find horizontal boundaries (row separators)
    fn find_horizontal_boundaries(&self, gray_img: &Image<image::Luma<u8>>) -> Vec<u32> {
        let (width, height) = gray_img.dimensions();
        let mut boundaries = vec![0]; // Start with top edge
        
        for y in 1..height.saturating_sub(1) {
            let mut transparent_count = 0;
            
            // Check if this row is mostly transparent
            for x in 0..width {
                let pixel = gray_img.get_pixel(x, y);
                if pixel[0] < 10 { // Very dark/transparent
                    transparent_count += 1;
                }
            }
            
            // If more than 80% of the row is transparent, it's likely a boundary
            if transparent_count as f32 / width as f32 > 0.8 {
                boundaries.push(y);
            } else {
                // Check for sudden color changes
                let mut color_changes = 0;
                for x in 0..width.saturating_sub(1) {
                    let current = gray_img.get_pixel(x, y)[0] as i32;
                    let next = gray_img.get_pixel(x + 1, y)[0] as i32;
                    if (current - next).abs() > 50 {
                        color_changes += 1;
                    }
                }
                
                if color_changes as f32 / width as f32 > 0.3 {
                    boundaries.push(y);
                }
            }
        }
        
        boundaries.push(height); // End with bottom edge
        boundaries.sort();
        boundaries.dedup();
        boundaries
    }

    /// Check if a frame contains meaningful content
    fn frame_has_content(&self, img: &DynamicImage, x: u32, y: u32, width: u32, height: u32) -> bool {
        let mut non_transparent_pixels = 0;
        let total_pixels = width * height;
        
        for py in y..y + height {
            for px in x..x + width {
                if let Some(pixel) = img.get_pixel_checked(px, py) {
                    match pixel {
                        image::Rgba([r, g, b, a]) => {
                            if a > 10 { // Not fully transparent
                                non_transparent_pixels += 1;
                            }
                        }
                        _ => {
                            non_transparent_pixels += 1;
                        }
                    }
                }
            }
        }
        
        // Frame has content if more than 5% of pixels are non-transparent
        non_transparent_pixels as f32 / total_pixels as f32 > 0.05
    }

    /// Extract a frame from the image
    fn extract_frame(&self, img: &DynamicImage, frame: &SpriteFrame) -> Result<DynamicImage> {
        let cropped = img.crop_imm(frame.x, frame.y, frame.width, frame.height);
        Ok(cropped)
    }

    /// Remove background from the image
    fn remove_background(&self, img: &DynamicImage) -> Result<DynamicImage> {
        let mut rgba_img = img.to_rgba8();
        let (width, height) = rgba_img.dimensions();
        
        // Detect background color (most common color in corners)
        let background_color = self.detect_background_color(&rgba_img);
        
        // Make background transparent
        for y in 0..height {
            for x in 0..width {
                let pixel = rgba_img.get_pixel(x, y);
                if self.is_background_pixel(pixel, &background_color) {
                    rgba_img.put_pixel(x, y, Rgba([0, 0, 0, 0])); // Transparent
                }
            }
        }
        
        Ok(DynamicImage::ImageRgba8(rgba_img))
    }

    /// Detect the background color by analyzing corner pixels
    fn detect_background_color(&self, img: &RgbaImage) -> Rgba<u8> {
        let (width, height) = img.dimensions();
        let mut color_counts = std::collections::HashMap::new();
        
        // Sample corner regions
        let sample_size = 10;
        for y in 0..sample_size.min(height) {
            for x in 0..sample_size.min(width) {
                let pixel = img.get_pixel(x, y);
                *color_counts.entry(pixel).or_insert(0) += 1;
            }
        }
        
        // Find most common color
        color_counts.into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(color, _)| color)
            .unwrap_or(Rgba([255, 255, 255, 255]))
    }

    /// Check if a pixel matches the background color
    fn is_background_pixel(&self, pixel: &Rgba<u8>, background: &Rgba<u8>) -> bool {
        let tolerance = self.config.background_tolerance as i32;
        
        (pixel[0] as i32 - background[0] as i32).abs() <= tolerance &&
        (pixel[1] as i32 - background[1] as i32).abs() <= tolerance &&
        (pixel[2] as i32 - background[2] as i32).abs() <= tolerance
    }
}

fn main() -> Result<()> {
    env_logger::init();
    
    println!("Spritesheet Cutter - Automatic Sprite Frame Extraction");
    println!("=====================================================");
    
    let config = CutterConfig::default();
    let cutter = SpritesheetCutter::new(config);
    
    cutter.process_directory()?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = CutterConfig::default();
        assert_eq!(config.min_sprite_size, 16);
        assert_eq!(config.max_sprite_size, 512);
        assert_eq!(config.background_tolerance, 10);
        assert!(config.remove_background);
        assert_eq!(config.output_dir, "assets2");
    }

    #[test]
    fn test_background_pixel_detection() {
        let config = CutterConfig::default();
        let cutter = SpritesheetCutter::new(config);
        
        let background = Rgba([255, 255, 255, 255]);
        let similar_pixel = Rgba([250, 250, 250, 255]);
        let different_pixel = Rgba([100, 100, 100, 255]);
        
        assert!(cutter.is_background_pixel(&similar_pixel, &background));
        assert!(!cutter.is_background_pixel(&different_pixel, &background));
    }
}
