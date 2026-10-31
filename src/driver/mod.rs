use crate::error::PalpError;
use crate::probe::DeviceMetrics;

/// Universal interface for screen drivers across mobile devices and desktop host screens.
pub trait ScreenDriver {
    /// Human-readable identifier for this target
    fn name(&self) -> &str;

    /// Probes physical screen dimensions, DPI density, and system bar insets
    fn probe(&self) -> Result<DeviceMetrics, PalpError>;

    /// Captures a raw PNG screenshot directly into host computer memory
    fn screencap_png(&self) -> Result<Vec<u8>, PalpError>;

    /// Taps or clicks at physical screen coordinates (x, y)
    fn tap(&self, x: u32, y: u32) -> Result<(), PalpError>;

    /// Types text into the currently active input field
    fn input_text(&self, text: &str) -> Result<(), PalpError>;

    /// Sends a virtual or hardware key event
    fn keyevent(&self, key: &str) -> Result<(), PalpError>;

    /// Performs a swipe or drag gesture between two coordinates
    fn swipe(&self, x1: u32, y1: u32, x2: u32, y2: u32, duration_ms: u32) -> Result<(), PalpError>;
}
