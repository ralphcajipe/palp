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
    max_width: Option<u32>,
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
    let (out_w, out_h) = scaled_size((img_w, content_h), max_width);
    if (out_w, out_h) == (img_w, content_h) {
        cropped.save(output_path)?;
    } else {
        imageops::resize(&cropped, out_w, out_h, imageops::FilterType::Triangle)
            .save(output_path)?;
    }

    Ok((out_w, out_h))
}

/// The output size for `snap --width`: the height follows the width, and a screenshot is
/// never enlarged. A 540 px image costs a vision model about a quarter of a full-width one.
pub fn scaled_size((width, height): (u32, u32), max_width: Option<u32>) -> (u32, u32) {
    match max_width {
        Some(target) if target < width => {
            let scaled =
                (u64::from(height) * u64::from(target) + u64::from(width) / 2) / u64::from(width);
            (target, (scaled as u32).max(1))
        }
        _ => (width, height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_width_scales_the_height_with_it_and_never_enlarges() {
        assert_eq!(scaled_size((1080, 2203), Some(540)), (540, 1102));
        assert_eq!(scaled_size((1080, 2203), Some(1080)), (1080, 2203));
        assert_eq!(scaled_size((1080, 2203), Some(2000)), (1080, 2203));
        assert_eq!(scaled_size((1080, 2203), None), (1080, 2203));
        assert_eq!(
            scaled_size((1080, 1), Some(10)),
            (10, 1),
            "never below one pixel"
        );
    }
}
