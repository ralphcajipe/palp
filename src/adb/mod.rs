pub mod device;

use crate::error::PalpError;
pub use device::Device;
use std::env;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Long enough for a slow `uiautomator dump` or a first `adb devices` that starts the
/// adb server; short enough that an unplugged phone fails instead of hanging an agent.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// The per-call limit: `PALP_TIMEOUT` in whole seconds, or the default.
pub fn parse_timeout(value: Option<String>) -> Duration {
    value
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_TIMEOUT)
}

pub fn adb_timeout() -> Duration {
    parse_timeout(env::var("PALP_TIMEOUT").ok())
}

/// Runs a command and kills it if it outlives `timeout`.
///
/// Output is read on two threads while waiting: a 2 MB screencap fills a pipe long before
/// the process exits, and waiting first would deadlock. The readers are not joined after a
/// timeout, since a process the command started could still hold the pipe open.
pub fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Result<Output, PalpError> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let stdout = read_in_background(child.stdout.take());
    let stderr = read_in_background(child.stderr.take());

    let deadline = Instant::now() + timeout;
    let timed_out = || PalpError::Timeout(timeout.as_secs());
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(timed_out());
        }
        thread::sleep(Duration::from_millis(20));
    };

    let remaining = || deadline.saturating_duration_since(Instant::now());
    let stdout = stdout.recv_timeout(remaining()).map_err(|_| timed_out())?;
    let stderr = stderr.recv_timeout(remaining()).map_err(|_| timed_out())?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn read_in_background<R: Read + Send + 'static>(pipe: Option<R>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        let _ = tx.send(buf);
    });
    rx
}

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
                let mut version = Command::new(exe);
                version.arg("version");
                if let Ok(output) = run_with_timeout(version, Duration::from_secs(10)) {
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
    let mut devices_cmd = Command::new(adb_path);
    devices_cmd.arg("devices").arg("-l");
    let output = run_with_timeout(devices_cmd, adb_timeout())?;

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
    fn palp_timeout_overrides_the_default_only_with_a_positive_number() {
        assert_eq!(parse_timeout(None), DEFAULT_TIMEOUT);
        assert_eq!(parse_timeout(Some("5".into())), Duration::from_secs(5));
        assert_eq!(parse_timeout(Some(" 90 ".into())), Duration::from_secs(90));
        assert_eq!(parse_timeout(Some("0".into())), DEFAULT_TIMEOUT);
        assert_eq!(parse_timeout(Some("soon".into())), DEFAULT_TIMEOUT);
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_command_is_stopped_at_the_timeout() {
        let mut sleep = Command::new("sleep");
        sleep.arg("5");
        let started = std::time::Instant::now();
        let result = run_with_timeout(sleep, Duration::from_secs(1));
        assert!(matches!(result, Err(PalpError::Timeout(1))));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[test]
    fn a_finished_command_returns_its_output() {
        let mut echo = Command::new("echo");
        echo.arg("hi");
        let out = run_with_timeout(echo, Duration::from_secs(5)).unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, b"hi\n");
    }

    #[cfg(unix)]
    #[test]
    fn output_bigger_than_a_pipe_buffer_does_not_stall() {
        let mut big = Command::new("/bin/sh");
        big.arg("-c")
            .arg("head -c 3000000 /dev/zero; echo done >&2");
        let out = run_with_timeout(big, Duration::from_secs(10)).unwrap();
        assert_eq!(out.stdout.len(), 3_000_000);
        assert_eq!(out.stderr, b"done\n");
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
