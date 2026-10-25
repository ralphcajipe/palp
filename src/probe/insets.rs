use serde::{Deserialize, Serialize};

/// Display specifications and inset measurements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceMetrics {
    pub width: u32,
    pub height: u32,
    pub density_dpi: u32,
    pub scale_factor: f32,
    pub status_bar_height: u32,
    pub navigation_bar_height: u32,
}

impl DeviceMetrics {
    /// Safe viewport width (same as display width).
    pub fn content_width(&self) -> u32 {
        self.width
    }

    /// Safe viewport height (excluding top status bar and bottom gesture bar).
    pub fn content_height(&self) -> u32 {
        self.height.saturating_sub(self.status_bar_height + self.navigation_bar_height)
    }

    /// Top coordinate where app content begins.
    pub fn content_top(&self) -> u32 {
        self.status_bar_height
    }

    /// Bottom coordinate where app content ends.
    pub fn content_bottom(&self) -> u32 {
        self.height.saturating_sub(self.navigation_bar_height)
    }
}
