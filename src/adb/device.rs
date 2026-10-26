use crate::error::PalpError;
use crate::guard::AnimationGuard;
use std::path::PathBuf;
use std::process::{Command, Stdio};

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
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().map_err(PalpError::Io)?;
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
        let res = self.shell(&format!("settings get {} {}", namespace, key))?;
        if res == "null" || res.is_empty() {
            Ok("1.0".to_string())
        } else {
            Ok(res)
        }
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

    /// Fast, animation-safe UI XML hierarchy dump.
    /// Temporarily pauses animations via RAII guard to prevent idle state timeouts.
    pub fn dump_ui_xml(&self) -> Result<String, PalpError> {
        let _guard = AnimationGuard::acquire(self);

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
        // Android adb input text requires replacing space with %s
        let sanitized = text
            .replace(' ', "%s")
            .replace('&', "\\&")
            .replace('<', "\\<")
            .replace('>', "\\>")
            .replace('(', "\\(")
            .replace(')', "\\)")
            .replace('$', "\\$")
            .replace('"', "\\\"")
            .replace('\'', "\\'");

        self.shell(&format!("input text \"{}\"", sanitized))?;
        Ok(())
    }

    /// Send hardware or navigation key event (e.g. BACK, HOME, ENTER).
    pub fn keyevent(&self, key: &str) -> Result<(), PalpError> {
        let binding = key.to_uppercase();
        let key_code = match binding.as_str() {
            "BACK" => "KEYCODE_BACK",
            "HOME" => "KEYCODE_HOME",
            "ENTER" => "KEYCODE_ENTER",
            "TAB" => "KEYCODE_TAB",
            "DEL" | "BACKSPACE" => "KEYCODE_DEL",
            "POWER" => "KEYCODE_POWER",
            other => other,
        };
        self.shell(&format!("input keyevent {}", key_code))?;
        Ok(())
    }
}
