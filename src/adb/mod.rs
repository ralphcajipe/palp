pub mod device;

use crate::error::PalpError;
pub use device::Device;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Finds the ADB executable on the host system.
pub fn find_adb() -> Result<PathBuf, PalpError> {
    // 1. Check PALP_ADB override
    if let Ok(custom_path) = env::var("PALP_ADB") {
        let p = PathBuf::from(custom_path);
        if p.exists() {
            return Ok(p);
        }
    }

    // 2. Check well-known D:\Dev\ToolData path (standard on this dev setup)
    let well_known = if cfg!(windows) {
        PathBuf::from(r"D:\Dev\ToolData\Android-Sdk\platform-tools\adb.exe")
    } else {
        PathBuf::from("/opt/android-sdk/platform-tools/adb")
    };
    if well_known.exists() {
        return Ok(well_known);
    }

    // 3. Check ANDROID_HOME or ANDROID_SDK_ROOT
    for var in &["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(sdk_root) = env::var(var) {
            let exe_name = if cfg!(windows) { "adb.exe" } else { "adb" };
            let adb_path = Path::new(&sdk_root).join("platform-tools").join(exe_name);
            if adb_path.exists() {
                return Ok(adb_path);
            }
        }
    }

    // 4. Check system PATH
    let exe = if cfg!(windows) { "adb.exe" } else { "adb" };
    if let Ok(output) = Command::new(exe).arg("version").output() {
        if output.status.success() {
            return Ok(PathBuf::from(exe));
        }
    }

    Err(PalpError::AdbNotFound)
}

/// Enumerates connected devices via `adb devices -l`.
pub fn list_devices(adb_path: &Path) -> Result<Vec<Device>, PalpError> {
    let output = Command::new(adb_path)
        .arg("devices")
        .arg("-l")
        .output()
        .map_err(PalpError::Io)?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(PalpError::AdbFailed(err.trim().to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("List of devices") || line.starts_with('*') {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let serial = parts[0].to_string();
        let state = parts[1].to_string();

        let mut model = "Unknown".to_string();
        for part in &parts[2..] {
            if let Some(m) = part.strip_prefix("model:") {
                model = m.to_string();
            }
        }

        let is_emulator = serial.starts_with("emulator-")
            || model.to_lowercase().contains("emulator")
            || model.to_lowercase().contains("gphone");

        devices.push(Device {
            serial,
            model,
            state,
            is_emulator,
            adb_path: adb_path.to_path_buf(),
        });
    }

    Ok(devices)
}

/// Selects the best device automatically or based on a user-provided target string.
pub fn select_device(target: Option<&str>) -> Result<Device, PalpError> {
    let adb = find_adb()?;
    let devices = list_devices(&adb)?;

    if devices.is_empty() {
        return Err(PalpError::NoDeviceConnected);
    }

    // Filter to authorized devices
    let authorized: Vec<&Device> = devices.iter().filter(|d| d.state == "device").collect();
    if authorized.is_empty() {
        // Return unauthorized error for first device
        return Err(PalpError::DeviceUnauthorized(devices[0].serial.clone()));
    }

    // If target is explicitly specified
    if let Some(query) = target {
        if let Some(d) = authorized
            .iter()
            .find(|d| d.serial == query || d.model.eq_ignore_ascii_case(query))
        {
            return Ok((*d).clone());
        }
        let list_str = authorized
            .iter()
            .map(|d| format!(" - {} ({})", d.serial, d.model))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(PalpError::UnknownTarget {
            target: query.to_string(),
            available: list_str,
        });
    }

    // If only one device is connected, use it
    if authorized.len() == 1 {
        return Ok(authorized[0].clone());
    }

    // If multiple devices, prioritize a physical device if there is exactly 1 physical and others are emulators
    let physical: Vec<&Device> = authorized
        .iter()
        .filter(|d| !d.is_emulator)
        .copied()
        .collect();
    if physical.len() == 1 {
        return Ok(physical[0].clone());
    }

    // Otherwise, request user specification
    let list_str = authorized
        .iter()
        .map(|d| format!(" - {} ({})", d.serial, d.model))
        .collect::<Vec<_>>()
        .join("\n");
    Err(PalpError::MultipleDevices {
        count: authorized.len(),
        devices: list_str,
    })
}
