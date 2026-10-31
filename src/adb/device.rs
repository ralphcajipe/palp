use crate::error::PalpError;
use std::path::PathBuf;
use std::process::Command;

use super::{adb_timeout, run_with_timeout};

/// Represents a connected Android device or emulator.
#[derive(Debug, Clone)]
pub struct Device {
    pub serial: String,
    pub model: String,
    pub state: String,
    pub is_emulator: bool,
    pub adb_path: PathBuf,
}

impl Device {
    /// Helper to execute an ADB command targeted at this specific device.
    pub fn exec_adb(&self, args: &[&str]) -> Result<Vec<u8>, PalpError> {
        let mut cmd = Command::new(&self.adb_path);
        cmd.arg("-s").arg(&self.serial);
        for arg in args {
            cmd.arg(arg);
        }
        let output = run_with_timeout(cmd, adb_timeout())?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PalpError::AdbFailed(stderr.trim().to_string()));
        }

        Ok(output.stdout)
    }

    /// Helper to execute an ADB shell command and return trimmed UTF-8 output.
    pub fn shell(&self, command: &str) -> Result<String, PalpError> {
        let stdout = self.exec_adb(&["shell", command])?;
        Ok(String::from_utf8_lossy(&stdout).trim().to_string())
    }

    /// Read a device setting (system, secure, or global).
    pub fn get_setting(&self, namespace: &str, key: &str) -> Result<String, PalpError> {
        // Raw on purpose: "null" (never set) must be told apart from a value to restore it.
        self.shell(&format!("settings get {} {}", namespace, key))
    }

    /// Write a device setting (system, secure, or global).
    pub fn set_setting(&self, namespace: &str, key: &str, value: &str) -> Result<(), PalpError> {
        self.shell(&format!("settings put {} {} {}", namespace, key, value))?;
        Ok(())
    }

    /// Stream screen capture directly into host computer memory (no SD card write).
    pub fn screencap_png(&self) -> Result<Vec<u8>, PalpError> {
        self.exec_adb(&["exec-out", "screencap", "-p"])
    }

    /// One UI XML hierarchy dump.
    ///
    /// Callers hold an `AnimationGuard` (see `poll_screen` in main) so that a wait loop pauses
    /// animations once for the whole wait instead of on every read.
    pub fn dump_ui_xml(&self) -> Result<String, PalpError> {
        // Single atomic command: dump to tmp file, print to stdout, and clean up tmp file.
        let output = self.shell("uiautomator dump /data/local/tmp/uidump.xml >/dev/null 2>&1 && cat /data/local/tmp/uidump.xml && rm -f /data/local/tmp/uidump.xml")?;

        if output.contains("<?xml") {
            // Find the start of the XML declaration in case of header noise
            if let Some(idx) = output.find("<?xml") {
                Ok(output[idx..].to_string())
            } else {
                Ok(output)
            }
        } else {
            Err(PalpError::AdbFailed(format!(
                "Failed to retrieve UI hierarchy:\n{}",
                output
            )))
        }
    }

    /// Tap the screen at exact physical pixel coordinates.
    pub fn tap(&self, x: u32, y: u32) -> Result<(), PalpError> {
        self.shell(&format!("input tap {} {}", x, y))?;
        Ok(())
    }

    /// Swipe across the screen from (x1, y1) to (x2, y2) over duration_ms milliseconds.
    pub fn swipe(
        &self,
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
        duration_ms: u32,
    ) -> Result<(), PalpError> {
        self.shell(&format!(
            "input swipe {} {} {} {} {}",
            x1, y1, x2, y2, duration_ms
        ))?;
        Ok(())
    }

    /// Type text into the active input field with shell escaping.
    pub fn input_text(&self, text: &str) -> Result<(), PalpError> {
        self.shell(&format!("input text {}", quote_for_input_text(text)))?;
        Ok(())
    }

    /// Opens an app's launcher screen, as tapping its icon would.
    pub fn launch(&self, package: &str) -> Result<(), PalpError> {
        if !is_package_name(package) {
            return Err(PalpError::InvalidInput(format!(
                "'{package}' is not an Android package name, such as com.example.app."
            )));
        }
        // monkey exits non-zero with its reason on stdout; `; true` keeps that reason readable.
        let output = self.shell(&format!(
            "monkey -p {package} -c android.intent.category.LAUNCHER 1 2>&1; true"
        ))?;
        match launch_failure(&output) {
            Some(reason) => Err(PalpError::AdbFailed(reason)),
            None => Ok(()),
        }
    }

    /// Send hardware or navigation key event (e.g. BACK, HOME, ENTER).
    pub fn keyevent(&self, key: &str) -> Result<(), PalpError> {
        self.shell(&format!("input keyevent {}", key_code(key)?))?;
        Ok(())
    }
}

impl crate::driver::ScreenDriver for Device {
    fn name(&self) -> &str {
        &self.model
    }

    fn probe(&self) -> Result<crate::probe::DeviceMetrics, PalpError> {
        crate::probe::probe_device(self)
    }

    fn screencap_png(&self) -> Result<Vec<u8>, PalpError> {
        self.screencap_png()
    }

    fn tap(&self, x: u32, y: u32) -> Result<(), PalpError> {
        self.tap(x, y)
    }

    fn input_text(&self, text: &str) -> Result<(), PalpError> {
        self.input_text(text)
    }

    fn keyevent(&self, key: &str) -> Result<(), PalpError> {
        self.keyevent(key)
    }

    fn swipe(
        &self,
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
        duration_ms: u32,
    ) -> Result<(), PalpError> {
        self.swipe(x1, y1, x2, y2, duration_ms)
    }
}

/// Quotes text for `input text` so the phone's shell hands it over exactly as typed.
///
/// Escaping inside double quotes let backticks run commands and typed a stray backslash
/// before `& ( ) < > '`. Inside single quotes nothing is special except the quote itself.
/// Spaces become `%s` because `input text` turns `%s` back into a space, so a literal
/// `%s` cannot be typed.
pub fn quote_for_input_text(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''").replace(' ', "%s"))
}

/// The keycode `input keyevent` should receive for a key name, alias or number.
///
/// The result reaches the phone's shell unquoted, so anything other than letters, digits and
/// `_` is refused rather than passed through.
pub fn key_code(key: &str) -> Result<String, PalpError> {
    let upper = key.trim().to_uppercase();
    let code = match upper.as_str() {
        "BACK" => "KEYCODE_BACK",
        "HOME" => "KEYCODE_HOME",
        "ENTER" => "KEYCODE_ENTER",
        "TAB" => "KEYCODE_TAB",
        "DEL" | "BACKSPACE" => "KEYCODE_DEL",
        "POWER" => "KEYCODE_POWER",
        other => other,
    };
    if is_plain_word(code, &['_']) {
        Ok(code.to_string())
    } else {
        Err(PalpError::InvalidInput(format!(
            "'{key}' is not a key name. Use back, home, enter, tab, del, power, a KEYCODE_ name or a number."
        )))
    }
}

/// Dot-separated parts that each start with a letter, as Android requires. Checked before the
/// name reaches the phone's shell.
pub fn is_package_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|part| {
            part.starts_with(|c: char| c.is_ascii_alphabetic()) && is_plain_word(part, &['_'])
        })
}

/// Why `monkey` could not open the app, when its output says it could not.
pub fn launch_failure(monkey_output: &str) -> Option<String> {
    (monkey_output.contains("No activities found") || monkey_output.contains("monkey aborted"))
        .then(|| {
            "No app with that package name is installed, or it has no launcher screen.".to_string()
        })
}

/// True when `s` is non-empty ASCII letters and digits plus the `extra` characters, so it is
/// one harmless word for the phone's shell.
pub(crate) fn is_plain_word(s: &str, extra: &[char]) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || extra.contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AWKWARD: &str = r#"Tom & Jerry (x) it's `id` $(y) \ ; "q" <b>"#;

    #[test]
    fn typed_text_is_one_single_quoted_word() {
        assert_eq!(
            quote_for_input_text(AWKWARD),
            r#"'Tom%s&%sJerry%s(x)%sit'\''s%s`id`%s$(y)%s\%s;%s"q"%s<b>'"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_posix_shell_hands_typed_text_over_untouched() {
        let quoted = quote_for_input_text(AWKWARD);
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("printf %s {quoted}"))
            .output()
            .unwrap();
        assert!(out.status.success());
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            AWKWARD.replace(' ', "%s")
        );
    }

    #[test]
    fn only_real_looking_package_names_reach_the_shell() {
        for good in ["com.vision_drafter.app", "com.example.app", "org.x1.y_2"] {
            assert!(is_package_name(good), "{good}");
        }
        for bad in [
            "app",
            ".com.x",
            "com..x",
            "com.x.",
            "com.x; reboot",
            "com.x $(id)",
            "1com.x",
            "",
        ] {
            assert!(!is_package_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_launch_that_found_no_app_is_an_error() {
        assert!(
            launch_failure("Events injected: 1\n## Network stats: elapsed time=12ms").is_none()
        );
        let none = "  bash arg: -p\n** No activities found to run, monkey aborted.";
        assert!(launch_failure(none).is_some());
    }

    #[test]
    fn key_names_map_to_keycodes_and_nothing_else_reaches_the_shell() {
        assert_eq!(key_code("back").unwrap(), "KEYCODE_BACK");
        assert_eq!(key_code(" Enter ").unwrap(), "KEYCODE_ENTER");
        assert_eq!(key_code("keycode_volume_up").unwrap(), "KEYCODE_VOLUME_UP");
        assert_eq!(key_code("66").unwrap(), "66");
        for bad in ["home; reboot", "$(id)", "", "a b", "`id`"] {
            assert!(key_code(bad).is_err(), "{bad:?} should be refused");
        }
    }
}
