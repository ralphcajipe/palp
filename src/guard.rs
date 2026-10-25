use crate::adb::Device;

/// Ian Sommerville Dependability Guard (Fail-Safe Restoration).
///
/// Many modern Android apps (React Native, Flutter, Jetpack Compose) have
/// endless looping animations or spring physics. When animations run continuously,
/// standard `uiautomator dump` hangs waiting for the UI to become "idle"
/// and eventually crashes with: "ERROR: could not get idle state".
///
/// `AnimationGuard` temporarily pauses animations (scale = 0), performs the
/// inspection or action, and automatically restores the user's original animation
/// settings when dropped—even if a panic or error occurs.
pub struct AnimationGuard<'a> {
    device: &'a Device,
    original_animator: Option<String>,
    original_window: Option<String>,
    original_transition: Option<String>,
}

impl<'a> AnimationGuard<'a> {
    /// Pauses phone animations and records original values for safe restoration.
    pub fn acquire(device: &'a Device) -> Self {
        let original_animator = device.get_setting("global", "animator_duration_scale").ok();
        let original_window = device.get_setting("global", "window_animation_scale").ok();
        let original_transition = device.get_setting("global", "transition_animation_scale").ok();

        // Temporarily set scales to 0 for instant uiautomator capture
        let _ = device.set_setting("global", "animator_duration_scale", "0");
        let _ = device.set_setting("global", "window_animation_scale", "0");
        let _ = device.set_setting("global", "transition_animation_scale", "0");

        Self {
            device,
            original_animator,
            original_window,
            original_transition,
        }
    }
}

impl<'a> Drop for AnimationGuard<'a> {
    fn drop(&mut self) {
        // Unconditionally restore original animation settings
        if let Some(ref val) = self.original_animator {
            let _ = self.device.set_setting("global", "animator_duration_scale", val);
        }
        if let Some(ref val) = self.original_window {
            let _ = self.device.set_setting("global", "window_animation_scale", val);
        }
        if let Some(ref val) = self.original_transition {
            let _ = self.device.set_setting("global", "transition_animation_scale", val);
        }
    }
}
