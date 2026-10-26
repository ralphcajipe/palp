use crate::error::PalpError;
use crate::probe::DeviceMetrics;
use image::{imageops, GenericImageView, ImageFormat};
use std::path::Path;

/// Crops phone status bar and bottom navigation bar in host memory using SIMD.
/// Zero disk writes to the phone or intermediate uncropped temp files.
pub fn crop_and_save(
    png_bytes: &[u8],
    metrics: &DeviceMetrics,
    output_path: &Path,
) -> Result<(u32, u32), PalpError> {
    let img = image::load_from_memory_with_format(png_bytes, ImageFormat::Png)?;
    let (img_w, img_h) = img.dimensions();

    // Verify dimensions match probed metrics
    let status_bar = if img_h == metrics.height {
        metrics.status_bar_height
    } else {
        // Scaled capture fallback
        let ratio = img_h as f32 / metrics.height as f32;
        (metrics.status_bar_height as f32 * ratio) as u32
    };

    let nav_bar = if img_h == metrics.height {
        metrics.navigation_bar_height
    } else {
        let ratio = img_h as f32 / metrics.height as f32;
        (metrics.navigation_bar_height as f32 * ratio) as u32
    };

    let content_h = img_h.saturating_sub(status_bar + nav_bar);

    let cropped = imageops::crop_imm(&img, 0, status_bar, img_w, content_h).to_image();
    cropped.save(output_path)?;

    Ok((img_w, content_h))
}
