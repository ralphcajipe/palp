pub mod device;

use crate::error::PalpError;
pub use device::Device;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The host operating system, which decides where Android Studio puts the SDK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    MacOs,
    Linux,
    Windows,
}

impl HostOs {
    pub fn current() -> Self {
        if cfg!(windows) {
            HostOs::Windows
        } else if cfg!(target_os = "macos") {
            HostOs::MacOs
        } else {
            HostOs::Linux
        }
    }

    fn adb_exe(self) -> &'static str {
        match self {
            HostOs::Windows => "adb.exe",
            _ => "adb",
        }
    }
}

/// One place adb might be, in the order `find_adb` tries them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdbCandidate {
    File { source: &'static str, path: PathBuf },
    OnPath,
}

impl AdbCandidate {
    pub fn source(&self) -> &'static str {
        match self {
            AdbCandidate::File { source, .. } => source,
            AdbCandidate::OnPath => "PATH",
        }
    }
}

/// Where adb was found, and which setting led there (shown by `palp doctor`).
#[derive(Debug, Clone)]
pub struct AdbLocation {
    pub path: PathBuf,
    pub source: &'static str,
}

/// Every place to look for adb, most explicit first.
///
/// What the user set (`PALP_ADB`, `ANDROID_HOME`, `PATH`) beats any folder palp guesses, so
/// Android Studio's default folder comes after `PATH`. The last entry is the original
/// hard-coded location, kept so the setup it was written for keeps working.
pub fn adb_candidates(env_get: impl Fn(&str) -> Option<String>, os: HostOs) -> Vec<AdbCandidate> {
    let get = |key: &str| env_get(key).filter(|v| !v.trim().is_empty());
    let exe = os.adb_exe();
    let in_sdk = |sdk: PathBuf| sdk.join("platform-tools").join(exe);

    let mut candidates = Vec::new();
    if let Some(path) = get("PALP_ADB") {
        candidates.push(AdbCandidate::File {
            source: "PALP_ADB",
            path: PathBuf::from(path),
        });
    }
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(sdk) = get(var) {
            candidates.push(AdbCandidate::File {
                source: var,
                path: in_sdk(PathBuf::from(sdk)),
            });
        }
    }
    candidates.push(AdbCandidate::OnPath);

    let default_sdk = match os {
        HostOs::MacOs => get("HOME").map(|h| Path::new(&h).join("Library/Android/sdk")),
        HostOs::Linux => get("HOME").map(|h| Path::new(&h).join("Android/Sdk")),
        HostOs::Windows => get("LOCALAPPDATA").map(|l| Path::new(&l).join("Android").join("Sdk")),
    };
    if let Some(sdk) = default_sdk {
        candidates.push(AdbCandidate::File {
            source: "Android Studio's default SDK folder",
            path: in_sdk(sdk),
        });
    }

    let legacy = match os {
        HostOs::Windows => PathBuf::from(r"D:\Dev\ToolData\Android-Sdk\platform-tools\adb.exe"),
        _ => PathBuf::from("/opt/android-sdk/platform-tools/adb"),
    };
    candidates.push(AdbCandidate::File {
        source: "legacy fallback",
        path: legacy,
    });
    candidates
}

/// Finds the ADB executable on the host system.
pub fn find_adb() -> Result<AdbLocation, PalpError> {
    let os = HostOs::current();
    for candidate in adb_candidates(|key| env::var(key).ok(), os) {
        match candidate {
            AdbCandidate::File { source, path } => {
                if path.is_file() {
                    return Ok(AdbLocation { path, source });
                }
            }
            AdbCandidate::OnPath => {
                let exe = os.adb_exe();
                if let Ok(output) = Command::new(exe).arg("version").output() {
                    if output.status.success() {
                        return Ok(AdbLocation {
                            path: PathBuf::from(exe),
                            source: "PATH",
                        });
                    }
                }
            }
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
    let devices = list_devices(&adb.path)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    fn files(candidates: &[AdbCandidate]) -> Vec<PathBuf> {
        candidates
            .iter()
            .filter_map(|c| match c {
                AdbCandidate::File { path, .. } => Some(path.clone()),
                AdbCandidate::OnPath => None,
            })
            .collect()
    }

    #[test]
    fn explicit_settings_come_first_then_path_then_standard_folders() {
        let env = env_of(&[
            ("PALP_ADB", "/custom/adb"),
            ("ANDROID_HOME", "/sdk-home"),
            ("ANDROID_SDK_ROOT", "/sdk-root"),
            ("HOME", "/Users/dev"),
        ]);
        let candidates = adb_candidates(env, HostOs::MacOs);
        let sources: Vec<&str> = candidates.iter().map(AdbCandidate::source).collect();
        assert_eq!(
            sources,
            [
                "PALP_ADB",
                "ANDROID_HOME",
                "ANDROID_SDK_ROOT",
                "PATH",
                "Android Studio's default SDK folder",
                "legacy fallback",
            ]
        );
        assert_eq!(
            files(&candidates)[..3],
            [
                PathBuf::from("/custom/adb"),
                Path::new("/sdk-home").join("platform-tools").join("adb"),
                Path::new("/sdk-root").join("platform-tools").join("adb"),
            ]
        );
    }

    #[test]
    fn mac_finds_the_android_studio_sdk_without_android_home() {
        let candidates = adb_candidates(env_of(&[("HOME", "/Users/dev")]), HostOs::MacOs);
        assert!(files(&candidates).contains(
            &Path::new("/Users/dev")
                .join("Library/Android/sdk")
                .join("platform-tools")
                .join("adb")
        ));
    }

    #[test]
    fn linux_and_windows_use_their_own_default_sdk_folders() {
        let linux = adb_candidates(env_of(&[("HOME", "/home/dev")]), HostOs::Linux);
        assert!(files(&linux).contains(
            &Path::new("/home/dev")
                .join("Android/Sdk")
                .join("platform-tools")
                .join("adb")
        ));

        let local = r"C:\Users\dev\AppData\Local";
        let windows = adb_candidates(env_of(&[("LOCALAPPDATA", local)]), HostOs::Windows);
        assert!(files(&windows).contains(
            &Path::new(local)
                .join("Android")
                .join("Sdk")
                .join("platform-tools")
                .join("adb.exe")
        ));
    }

    #[test]
    fn empty_variables_are_ignored() {
        let candidates = adb_candidates(
            env_of(&[("PALP_ADB", ""), ("ANDROID_HOME", "  ")]),
            HostOs::Linux,
        );
        let sources: Vec<&str> = candidates.iter().map(AdbCandidate::source).collect();
        assert_eq!(sources, ["PATH", "legacy fallback"]);
    }
}
