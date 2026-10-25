use thiserror::Error;

#[derive(Error, Debug)]
pub enum PalpError {
    #[error("ADB tool was not found.\nPlease make sure Android SDK platform-tools is installed or set ANDROID_HOME in your environment.")]
    AdbNotFound,

    #[error("No Android phone or emulator was found.\nPlease connect your device via USB or start an emulator, and enable USB Debugging.")]
    NoDeviceConnected,

    #[error("Found multiple connected devices ({count}). Please specify one with --device <ID>.\nAvailable devices:\n{devices}")]
    MultipleDevices { count: usize, devices: String },

    #[error("Device '{0}' is unauthorized. Please unlock your phone and tap 'Allow USB Debugging'.")]
    DeviceUnauthorized(String),

    #[error("Device screen appears to be turned off or locked. Please unlock the screen to continue.")]
    ScreenLocked,

    #[error("Could not find any visible text or button matching '{0}' on the current screen.")]
    TextNotFound(String),

    #[error("Unknown target '{target}'. Available targets in .palprc.json:\n{available}")]
    UnknownTarget { target: String, available: String },

    #[error("ADB command failed: {0}")]
    AdbFailed(String),

    #[error("Operation timed out after {0} seconds. The phone or ADB may be busy or disconnected.")]
    Timeout(u64),

    #[error("Failed to read or write image: {0}")]
    Image(#[from] image::ImageError),

    #[error("File or system error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration format error: {0}")]
    Json(#[from] serde_json::Error),
}
