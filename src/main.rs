use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
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
            min_sprite_size: 8,  // Reduced from 16 to catch smaller sprites
            max_sprite_size: 1024,  // Increased from 512 to handle larger sprites
            background_tolerance: 20,  // Increased from 10 for better background detection
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

    /// Process all image files in the Base, Ships, and Space directories
    fn process_directory(&self) -> Result<()> {
        let current_dir = std::env::current_dir()
            .context("Failed to get current directory")?;
        
        let folders_to_process = ["Base", "Ships", "Space"];
        let mut total_processed = 0;

        for folder_name in &folders_to_process {
            let folder_path = current_dir.join(folder_name);
            
            if !folder_path.exists() {
                println!("Folder '{}' not found, skipping...", folder_name);
                continue;
            }

            println!("\n=== Processing {} folder ===", folder_name);
            
            // Create output directory for this folder
            let output_path = current_dir.join(&self.config.output_dir).join(folder_name);
            fs::create_dir_all(&output_path)
                .context("Failed to create output directory")?;

            // Find all image files in this folder
            let image_files = self.find_image_files(&folder_path)?;
            
            if image_files.is_empty() {
                println!("No image files found in the {} directory.", folder_name);
                continue;
            }

            println!("Found {} image files to process in {}", image_files.len(), folder_name);

            for (index, image_path) in image_files.iter().enumerate() {
                println!("Processing {}/{}: {}", index + 1, image_files.len(), 
                        image_path.file_name().unwrap().to_string_lossy());
                
                match self.process_spritesheet(image_path, &output_path) {
                    Ok(frames_extracted) => {
                        if frames_extracted == 0 {
                            // If no frames were detected, copy the original image as a single sprite
                            self.copy_single_sprite(image_path, &output_path)?;
                            println!("  → Copied as single sprite");
                        } else {
                            println!("  → Extracted {} frames", frames_extracted);
                        }
                        total_processed += 1;
                    }
                    Err(e) => {
                        eprintln!("Error processing {}: {}", 
                                 image_path.file_name().unwrap().to_string_lossy(), e);
                    }
                }
            }
        }

        println!("\n=== Processing Complete! ===");
        println!("Successfully processed {} images across all folders.", total_processed);
        println!("Check the '{}' directory for results.", self.config.output_dir);
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
    fn process_spritesheet(&self, image_path: &Path, output_dir: &Path) -> Result<usize> {
        let img = image::open(image_path)
            .context("Failed to open image")?;

        let frames = self.detect_sprite_frames(&img)?;
        
        if frames.is_empty() {
            return Ok(0); // Return 0 frames detected
        }

        println!("  → Detected {} frames", frames.len());

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

        Ok(frames.len())
    }

    /// Copy a single sprite image to the output directory
    fn copy_single_sprite(&self, image_path: &Path, output_dir: &Path) -> Result<()> {
        let img = image::open(image_path)
            .context("Failed to open image")?;

        let processed = if self.config.remove_background {
            self.remove_background(&img)?
        } else {
            img
        };

        let filename = image_path.file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let output_path = output_dir.join(filename);
        
        processed.save(&output_path)
            .context("Failed to save single sprite")?;

        Ok(())
    }

    /// Detect sprite frames in the image using intelligent boundary detection
    fn detect_sprite_frames(&self, img: &DynamicImage) -> Result<Vec<SpriteFrame>> {
        let (_width, _height) = img.dimensions();
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

        // If no frames were detected, try fallback detection
        if frames.is_empty() {
            println!("  → No frames detected with main algorithm, trying fallback...");
            frames = self.fallback_detection(img)?;
            if !frames.is_empty() {
                println!("  → Fallback detection found {} frames", frames.len());
            }
        }

        Ok(frames)
    }

    /// Fallback detection method for spritesheets that the main algorithm misses
    fn fallback_detection(&self, img: &DynamicImage) -> Result<Vec<SpriteFrame>> {
        let (width, height) = img.dimensions();
        let mut frames = Vec::new();

        // Try to detect horizontal spritesheets by finding actual empty space boundaries
        let vertical_boundaries = self.find_empty_space_boundaries_horizontal(img)?;
        println!("    → Found {} vertical boundaries: {:?}", vertical_boundaries.len(), vertical_boundaries);
        
        if vertical_boundaries.len() > 1 {
            for i in 0..vertical_boundaries.len().saturating_sub(1) {
                let x = vertical_boundaries[i];
                let frame_width = vertical_boundaries[i + 1] - x;
                
                // Validate frame size
                if frame_width >= self.config.min_sprite_size 
                    && frame_width <= self.config.max_sprite_size {
                    
                    // Check if frame contains content
                    if self.frame_has_content(img, x, 0, frame_width, height) {
                        frames.push(SpriteFrame {
                            x,
                            y: 0,
                            width: frame_width,
                            height,
                        });
                    }
                }
            }
        }

        // If still no frames, try vertical spritesheets
        if frames.is_empty() {
            let horizontal_boundaries = self.find_empty_space_boundaries_vertical(img)?;
            println!("    → Found {} horizontal boundaries: {:?}", horizontal_boundaries.len(), horizontal_boundaries);
            
            if horizontal_boundaries.len() > 1 {
                for i in 0..horizontal_boundaries.len().saturating_sub(1) {
                    let y = horizontal_boundaries[i];
                    let frame_height = horizontal_boundaries[i + 1] - y;
                    
                    // Validate frame size
                    if frame_height >= self.config.min_sprite_size 
                        && frame_height <= self.config.max_sprite_size {
                        
                        // Check if frame contains content
                        if self.frame_has_content(img, 0, y, width, frame_height) {
                            frames.push(SpriteFrame {
                                x: 0,
                                y,
                                width,
                                height: frame_height,
                            });
                        }
                    }
                }
            }
        }

        Ok(frames)
    }

    /// Find vertical boundaries by detecting empty space columns
    fn find_empty_space_boundaries_horizontal(&self, img: &DynamicImage) -> Result<Vec<u32>> {
        let (width, height) = img.dimensions();
        let gray_img = img.to_luma8();
        let mut boundaries = vec![0]; // Start with left edge
        
        // Detect the most common background color
        let background_color = self.detect_most_common_color(&gray_img);
        
        for x in 1..width.saturating_sub(1) {
            let mut empty_pixels = 0;
            
            // Check if this column is mostly empty/background
            for y in 0..height {
                let pixel = gray_img.get_pixel(x, y);
                if (pixel[0] as i32 - background_color as i32).abs() <= 15 {
                    empty_pixels += 1;
                }
            }
            
            // If more than 85% of the column is background, it's a boundary
            if empty_pixels as f32 / height as f32 > 0.85 {
                boundaries.push(x);
            }
        }
        
        boundaries.push(width); // End with right edge
        boundaries.sort();
        boundaries.dedup();
        
        // Remove boundaries that are too close together (less than min_sprite_size)
        let mut filtered_boundaries = Vec::new();
        let mut last_boundary = 0;
        
        for &boundary in &boundaries {
            if boundary - last_boundary >= self.config.min_sprite_size || boundary == width {
                filtered_boundaries.push(boundary);
                last_boundary = boundary;
            }
        }
        
        Ok(filtered_boundaries)
    }

    /// Find horizontal boundaries by detecting empty space rows
    fn find_empty_space_boundaries_vertical(&self, img: &DynamicImage) -> Result<Vec<u32>> {
        let (width, height) = img.dimensions();
        let gray_img = img.to_luma8();
        let mut boundaries = vec![0]; // Start with top edge
        
        // Detect the most common background color
        let background_color = self.detect_most_common_color(&gray_img);
        
        for y in 1..height.saturating_sub(1) {
            let mut empty_pixels = 0;
            
            // Check if this row is mostly empty/background
            for x in 0..width {
                let pixel = gray_img.get_pixel(x, y);
                if (pixel[0] as i32 - background_color as i32).abs() <= 15 {
                    empty_pixels += 1;
                }
            }
            
            // If more than 85% of the row is background, it's a boundary
            if empty_pixels as f32 / width as f32 > 0.85 {
                boundaries.push(y);
            }
        }
        
        boundaries.push(height); // End with bottom edge
        boundaries.sort();
        boundaries.dedup();
        
        // Remove boundaries that are too close together (less than min_sprite_size)
        let mut filtered_boundaries = Vec::new();
        let mut last_boundary = 0;
        
        for &boundary in &boundaries {
            if boundary - last_boundary >= self.config.min_sprite_size || boundary == height {
                filtered_boundaries.push(boundary);
                last_boundary = boundary;
            }
        }
        
        Ok(filtered_boundaries)
    }

    /// Estimate sprite width by finding the first significant content region
    fn estimate_sprite_width(&self, img: &DynamicImage) -> Result<u32> {
        let (width, height) = img.dimensions();
        let gray_img = img.to_luma8();
        
        // Find the first column with significant content
        let mut first_content_x = None;
        for x in 0..width {
            let mut content_pixels = 0;
            for y in 0..height {
                let pixel = gray_img.get_pixel(x, y);
                if pixel[0] > 20 { // Not very dark/transparent
                    content_pixels += 1;
                }
            }
            if content_pixels as f32 / height as f32 > 0.1 { // More than 10% content
                first_content_x = Some(x);
                break;
            }
        }

        if let Some(start_x) = first_content_x {
            // Find the end of the first sprite
            for x in start_x + 1..width {
                let mut empty_pixels = 0;
                for y in 0..height {
                    let pixel = gray_img.get_pixel(x, y);
                    if pixel[0] <= 20 { // Very dark/transparent
                        empty_pixels += 1;
                    }
                }
                if empty_pixels as f32 / height as f32 > 0.8 { // More than 80% empty
                    return Ok(x - start_x);
                }
            }
        }

        // If the above method fails, try a different approach for spritesheets with uniform backgrounds
        // Look for the most common color (likely background) and find sprite boundaries
        let background_color = self.detect_most_common_color(&gray_img);
        println!("    → Most common color: {}", background_color);
        
        // Find first non-background column
        let mut first_sprite_x = None;
        for x in 0..width {
            let mut non_bg_pixels = 0;
            for y in 0..height {
                let pixel = gray_img.get_pixel(x, y);
                if (pixel[0] as i32 - background_color as i32).abs() > 10 {
                    non_bg_pixels += 1;
                }
            }
            if non_bg_pixels as f32 / height as f32 > 0.05 { // More than 5% non-background
                first_sprite_x = Some(x);
                break;
            }
        }

        if let Some(start_x) = first_sprite_x {
            // Find the end of the first sprite
            for x in start_x + 1..width {
                let mut bg_pixels = 0;
                for y in 0..height {
                    let pixel = gray_img.get_pixel(x, y);
                    if (pixel[0] as i32 - background_color as i32).abs() <= 10 {
                        bg_pixels += 1;
                    }
                }
                if bg_pixels as f32 / height as f32 > 0.7 { // More than 70% background
                    return Ok(x - start_x);
                }
            }
        }

        Ok(0)
    }

    /// Detect the most common color in the image (likely background)
    fn detect_most_common_color(&self, gray_img: &Image<image::Luma<u8>>) -> u8 {
        let (width, height) = gray_img.dimensions();
        let mut color_counts = std::collections::HashMap::new();
        
        // Sample every 4th pixel to speed up detection
        for y in (0..height).step_by(4) {
            for x in (0..width).step_by(4) {
                let pixel = gray_img.get_pixel(x, y);
                *color_counts.entry(pixel[0]).or_insert(0) += 1;
            }
        }
        
        color_counts.into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(color, _)| color)
            .unwrap_or(0)
    }

    /// Estimate sprite height by finding the first significant content region
    fn estimate_sprite_height(&self, img: &DynamicImage) -> Result<u32> {
        let (width, height) = img.dimensions();
        let gray_img = img.to_luma8();
        
        // Find the first row with significant content
        let mut first_content_y = None;
        for y in 0..height {
            let mut content_pixels = 0;
            for x in 0..width {
                let pixel = gray_img.get_pixel(x, y);
                if pixel[0] > 20 { // Not very dark/transparent
                    content_pixels += 1;
                }
            }
            if content_pixels as f32 / width as f32 > 0.1 { // More than 10% content
                first_content_y = Some(y);
                break;
            }
        }

        if let Some(start_y) = first_content_y {
            // Find the end of the first sprite
            for y in start_y + 1..height {
                let mut empty_pixels = 0;
                for x in 0..width {
                    let pixel = gray_img.get_pixel(x, y);
                    if pixel[0] <= 20 { // Very dark/transparent
                        empty_pixels += 1;
                    }
                }
                if empty_pixels as f32 / width as f32 > 0.8 { // More than 80% empty
                    return Ok(y - start_y);
                }
            }
        }

        Ok(0)
    }

    /// Find vertical boundaries (column separators)
    fn find_vertical_boundaries(&self, gray_img: &Image<image::Luma<u8>>) -> Vec<u32> {
        let (width, height) = gray_img.dimensions();
        let mut boundaries = vec![0]; // Start with left edge
        
        for x in 1..width.saturating_sub(1) {
            let _is_boundary = true;
            let mut transparent_count = 0;
            
            // Check if this column is mostly transparent or uniform
            for y in 0..height {
                let pixel = gray_img.get_pixel(x, y);
                if pixel[0] < 10 { // Very dark/transparent
                    transparent_count += 1;
                }
            }
            
            // If more than 60% of the column is transparent, it's likely a boundary (reduced from 80%)
            if transparent_count as f32 / height as f32 > 0.6 {
                boundaries.push(x);
            } else {
                // Check for sudden color changes (edge detection) - more sensitive
                let mut color_changes = 0;
                for y in 0..height.saturating_sub(1) {
                    let current = gray_img.get_pixel(x, y)[0] as i32;
                    let next = gray_img.get_pixel(x, y + 1)[0] as i32;
                    if (current - next).abs() > 30 { // Reduced threshold from 50 to 30
                        color_changes += 1;
                    }
                }
                
                if color_changes as f32 / height as f32 > 0.2 { // Reduced from 0.3 to 0.2
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
            
            // If more than 60% of the row is transparent, it's likely a boundary (reduced from 80%)
            if transparent_count as f32 / width as f32 > 0.6 {
                boundaries.push(y);
            } else {
                // Check for sudden color changes - more sensitive
                let mut color_changes = 0;
                for x in 0..width.saturating_sub(1) {
                    let current = gray_img.get_pixel(x, y)[0] as i32;
                    let next = gray_img.get_pixel(x + 1, y)[0] as i32;
                    if (current - next).abs() > 30 { // Reduced threshold from 50 to 30
                        color_changes += 1;
                    }
                }
                
                if color_changes as f32 / width as f32 > 0.2 { // Reduced from 0.3 to 0.2
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
                if px < img.width() && py < img.height() {
                    let pixel = img.get_pixel(px, py);
                    match pixel {
                        image::Rgba([_r, _g, _b, a]) => {
                            if a > 10 { // Not fully transparent
                                non_transparent_pixels += 1;
                            }
                        }
                    }
                }
            }
        }
        
        // Frame has content if more than 2% of pixels are non-transparent (reduced from 5%)
        non_transparent_pixels as f32 / total_pixels as f32 > 0.02
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
            .unwrap_or(&Rgba([255, 255, 255, 255]))
            .clone()
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
