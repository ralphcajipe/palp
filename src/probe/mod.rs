pub mod insets;

pub use insets::DeviceMetrics;
use crate::adb::Device;
use crate::error::PalpError;

/// Probes a connected device for physical screen dimensions and system bar insets.
pub fn probe_device(device: &Device) -> Result<DeviceMetrics, PalpError> {
    // 1. Screen size
    let wm_size_out = device.shell("wm size")?;
    let (width, height) = parse_wm_size(&wm_size_out)?;

    // 2. Screen density
    let wm_density_out = device.shell("wm density")?;
    let density_dpi = parse_wm_density(&wm_density_out).unwrap_or(420);
    let scale_factor = density_dpi as f32 / 160.0;

    // 3. System insets (status bar & navigation bar)
    let dumpsys_out = device.shell("dumpsys window displays").unwrap_or_default();
    let (status_bar_height, navigation_bar_height) = parse_insets(&dumpsys_out, height, scale_factor);

    Ok(DeviceMetrics {
        width,
        height,
        density_dpi,
        scale_factor,
        status_bar_height,
        navigation_bar_height,
    })
}

fn parse_wm_size(output: &str) -> Result<(u32, u32), PalpError> {
    for line in output.lines() {
        let line = line.trim();
        // Looking for "Physical size: 1080x2392" or "Override size: ..."
        if let Some(idx) = line.find("size:") {
            let part = line[idx + 5..].trim();
            let mut dims = part.split('x');
            if let (Some(w_str), Some(h_str)) = (dims.next(), dims.next()) {
                if let (Ok(w), Ok(h)) = (w_str.trim().parse::<u32>(), h_str.trim().parse::<u32>()) {
                    return Ok((w, h));
                }
            }
        }
    }
    // Default fallback to 1080x2400
    Ok((1080, 2400))
}

fn parse_wm_density(output: &str) -> Option<u32> {
    for line in output.lines() {
        let line = line.trim();
        if let Some(idx) = line.find("density:") {
            let part = line[idx + 8..].trim();
            if let Ok(dpi) = part.parse::<u32>() {
                return Some(dpi);
            }
        }
    }
    None
}

fn parse_insets(dumpsys: &str, screen_height: u32, scale: f32) -> (u32, u32) {
    let mut status_bar = None;
    let mut nav_bar = None;

    for line in dumpsys.lines() {
        let line = line.trim();
        // Look for statusBars frame=[0,0][1080,126]
        if line.contains("type=statusBars") && line.contains("frame=[0,0][") {
            if let Some(start) = line.find("frame=[0,0][") {
                let rest = &line[start + 12..];
                if let Some(end) = rest.find(']') {
                    let rect_str = &rest[..end]; // e.g. "1080,126"
                    let mut coords = rect_str.split(',');
                    let _w = coords.next();
                    if let Some(h_str) = coords.next() {
                        if let Ok(h) = h_str.trim().parse::<u32>() {
                            status_bar = Some(h);
                        }
                    }
                }
            }
        }

        // Look for navigationBars frame=[0,2329][1080,2392]
        if line.contains("type=navigationBars") && line.contains("frame=[0,") {
            if let Some(start) = line.find("frame=[0,") {
                let rest = &line[start + 9..];
                if let Some(end) = rest.find(']') {
                    let top_y_str = &rest[..end];
                    if let Ok(top_y) = top_y_str.trim().parse::<u32>() {
                        if top_y < screen_height {
                            nav_bar = Some(screen_height - top_y);
                        }
                    }
                }
            }
        }
    }

    let final_status = status_bar.unwrap_or_else(|| (48.0 * scale).round() as u32);
    let final_nav = nav_bar.unwrap_or_else(|| (24.0 * scale).round() as u32);

    (final_status, final_nav)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_wm_size() {
        let raw = "Physical size: 1080x2392\n";
        assert_eq!(parse_wm_size(raw).unwrap(), (1080, 2392));
    }

    #[test]
    fn test_parse_wm_density() {
        let raw = "Physical density: 420\n";
        assert_eq!(parse_wm_density(raw), Some(420));
    }

    #[test]
    fn test_parse_insets_dumpsys() {
        let sample = "InsetsSource id=92400000 type=statusBars frame=[0,0][1080,126] visible=true flags= sideHint=TOP\n\
                      InsetsSource id=b0860001 type=navigationBars frame=[0,2329][1080,2392] visible=true flags= sideHint=BOTTOM";
        let (sb, nb) = parse_insets(sample, 2392, 2.625);
        assert_eq!(sb, 126);
        assert_eq!(nb, 63);
    }
}
