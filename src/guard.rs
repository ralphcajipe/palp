use crate::adb::{Device, HostOs};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Ian Sommerville Dependability Guard (Fail-Safe Restoration).
///
/// Many modern Android apps (React Native, Flutter, Jetpack Compose) have
/// endless looping animations or spring physics. When animations run continuously,
/// standard `uiautomator dump` hangs waiting for the UI to become "idle"
/// and eventually crashes with: "ERROR: could not get idle state".
///
/// `AnimationGuard` temporarily pauses animations (scale = 0) and puts the user's own
/// values back afterwards: when dropped, on Ctrl+C or termination (the handler in `main`),
/// and after a hard kill, from a state file that the next palp run adopts.
pub struct AnimationGuard {
    active: bool,
}

const KEYS: [&str; 3] = [
    "animator_duration_scale",
    "window_animation_scale",
    "transition_animation_scale",
];

/// The phone's three animation scales. `None` means never set, which Android treats as 1
/// and which is restored by deleting the setting rather than writing a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimationScales {
    pub animator_duration_scale: Option<String>,
    pub window_animation_scale: Option<String>,
    pub transition_animation_scale: Option<String>,
}

impl AnimationScales {
    /// Parses `settings get` output for the three keys in `KEYS` order; `None` when any
    /// reading is not `null` or a number, such as an error from an unplugged phone.
    pub fn from_raw(raw: [&str; 3]) -> Option<Self> {
        let parse = |r: &str| match r.trim() {
            "null" => Some(None),
            v if is_scale_value(v) => Some(Some(v.to_string())),
            _ => None,
        };
        Some(Self {
            animator_duration_scale: parse(raw[0])?,
            window_animation_scale: parse(raw[1])?,
            transition_animation_scale: parse(raw[2])?,
        })
    }

    fn entries(&self) -> [(&'static str, Option<&str>); 3] {
        [
            (KEYS[0], self.animator_duration_scale.as_deref()),
            (KEYS[1], self.window_animation_scale.as_deref()),
            (KEYS[2], self.transition_animation_scale.as_deref()),
        ]
    }

    /// Every value is `null` or a plain number, so it is safe to hand to the phone's shell.
    pub fn is_valid(&self) -> bool {
        self.entries()
            .iter()
            .all(|(_, v)| v.is_none_or(is_scale_value))
    }

    /// All three at 0: the state palp leaves the phone in while it reads the screen.
    pub fn is_all_zero(&self) -> bool {
        self.entries().iter().all(|(_, v)| {
            v.and_then(|v| v.parse::<f64>().ok())
                .is_some_and(|n| n == 0.0)
        })
    }
}

fn is_scale_value(s: &str) -> bool {
    let mut parts = s.split('.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next();
    parts.next().is_none()
        && !whole.is_empty()
        && whole.chars().all(|c| c.is_ascii_digit())
        && fraction.is_none_or(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()))
}

/// The values to restore later.
///
/// A leftover file is only trusted while the phone still shows all zeros, which means a palp
/// run was killed (or is still running) before putting things back. Anything else means the
/// scales were restored since, so the file is stale and the current values are the user's.
pub fn choose_originals(
    leftover: Option<AnimationScales>,
    current: AnimationScales,
) -> AnimationScales {
    match leftover {
        Some(saved) if saved.is_valid() && current.is_all_zero() => saved,
        _ => current,
    }
}

pub fn restore_command(key: &str, value: Option<&str>) -> String {
    match value {
        Some(v) => format!("settings put global {key} {v}"),
        None => format!("settings delete global {key}"),
    }
}

/// A per-user folder for the state file. Shared `/tmp` is avoided, where another account
/// could plant the file or a symlink in its place.
pub fn state_dir(env_get: impl Fn(&str) -> Option<String>, os: HostOs) -> Option<PathBuf> {
    let get = |key: &str| env_get(key).filter(|v| !v.trim().is_empty());
    match os {
        HostOs::MacOs => get("HOME").map(|h| Path::new(&h).join("Library/Caches/palp")),
        HostOs::Linux => get("XDG_CACHE_HOME")
            .map(|x| Path::new(&x).join("palp"))
            .or_else(|| get("HOME").map(|h| Path::new(&h).join(".cache/palp"))),
        HostOs::Windows => get("LOCALAPPDATA").map(|l| Path::new(&l).join("palp")),
    }
}

/// A wireless serial such as `192.168.1.5:5555` is not a valid filename everywhere.
pub fn state_file_name(serial: &str) -> String {
    let safe: String = serial
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("palp-animations-{safe}.json")
}

fn state_file(device: &Device) -> Option<PathBuf> {
    state_dir(|key| std::env::var(key).ok(), HostOs::current())
        .map(|dir| dir.join(state_file_name(&device.serial)))
}

pub fn save_state(path: &Path, scales: &AnimationScales) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, serde_json::to_string(scales)?)
}

/// The saved originals, or `None` when there is no file or it holds anything but numbers.
pub fn load_leftover(path: &Path) -> Option<AnimationScales> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str::<AnimationScales>(&text)
        .ok()
        .filter(AnimationScales::is_valid)
}

fn read_scales(device: &Device) -> Option<AnimationScales> {
    let mut raw = Vec::with_capacity(KEYS.len());
    for key in KEYS {
        raw.push(device.get_setting("global", key).ok()?);
    }
    AnimationScales::from_raw([&raw[0], &raw[1], &raw[2]])
}

struct PendingRestore {
    device: Device,
    originals: AnimationScales,
    state_file: Option<PathBuf>,
}

impl PendingRestore {
    fn restore(&self) -> bool {
        // Collected first so one failed key does not stop the others being put back.
        let results: Vec<bool> = self
            .originals
            .entries()
            .iter()
            .map(|(key, value)| self.device.shell(&restore_command(key, *value)).is_ok())
            .collect();
        let all_restored = results.iter().all(|ok| *ok);
        // Kept when a restore failed (an unplugged phone), so the next run can finish the job.
        if all_restored {
            if let Some(file) = &self.state_file {
                let _ = fs::remove_file(file);
            }
        }
        all_restored
    }
}

/// Set while animations are paused; whoever takes it (the guard's `Drop` or the signal
/// handler) does the one restore.
static PENDING: Mutex<Option<PendingRestore>> = Mutex::new(None);

fn take_pending() -> Option<PendingRestore> {
    PENDING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
}

/// Restores the paused animations, if any. Called from `Drop` and from the Ctrl+C handler.
pub fn restore_pending() {
    if let Some(pending) = take_pending() {
        pending.restore();
    }
}

impl AnimationGuard {
    /// Pauses phone animations after recording the originals in memory and on disk.
    ///
    /// Leaves the phone alone when the current values cannot be read: zeroing without a way
    /// back would be worse than a slow screen dump.
    pub fn acquire(device: &Device) -> Self {
        let Some(current) = read_scales(device) else {
            return Self { active: false };
        };
        let state_file = state_file(device);
        let leftover = state_file.as_deref().and_then(load_leftover);
        let originals = choose_originals(leftover, current);
        if let Some(file) = &state_file {
            let _ = save_state(file, &originals);
        }

        *PENDING
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(PendingRestore {
            device: device.clone(),
            originals,
            state_file,
        });
        for key in KEYS {
            let _ = device.set_setting("global", key, "0");
        }
        Self { active: true }
    }
}

impl Drop for AnimationGuard {
    fn drop(&mut self) {
        if self.active {
            restore_pending();
        }
    }
}

/// What `palp doctor` found in a leftover state file.
pub enum Leftover {
    None,
    Restored,
    RestoreFailed,
    Stale,
}

/// Puts back animation scales a killed palp run left at 0, or deletes a stale state file.
pub fn heal_leftover(device: &Device) -> Leftover {
    let Some(file) = state_file(device) else {
        return Leftover::None;
    };
    if !file.exists() {
        return Leftover::None;
    }
    let Some(current) = read_scales(device) else {
        return Leftover::RestoreFailed;
    };
    match load_leftover(&file) {
        Some(saved) if current.is_all_zero() => {
            let pending = PendingRestore {
                device: device.clone(),
                originals: saved,
                state_file: Some(file),
            };
            if pending.restore() {
                Leftover::Restored
            } else {
                Leftover::RestoreFailed
            }
        }
        _ => {
            let _ = fs::remove_file(&file);
            Leftover::Stale
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adb::HostOs;
    use std::collections::HashMap;

    fn scales(a: Option<&str>, w: Option<&str>, t: Option<&str>) -> AnimationScales {
        AnimationScales {
            animator_duration_scale: a.map(String::from),
            window_animation_scale: w.map(String::from),
            transition_animation_scale: t.map(String::from),
        }
    }

    const ZEROS: [&str; 3] = ["0", "0.0", "0"];

    #[test]
    fn raw_readings_keep_never_set_apart_from_a_value() {
        assert_eq!(
            AnimationScales::from_raw(["null", "1", "0.5"]),
            Some(scales(None, Some("1"), Some("0.5")))
        );
        assert_eq!(
            AnimationScales::from_raw(["Error: device offline", "1", "1"]),
            None
        );
        assert_eq!(AnimationScales::from_raw(["", "1", "1"]), None);
    }

    #[test]
    fn only_three_zeros_look_like_palp_left_them() {
        assert!(AnimationScales::from_raw(ZEROS).unwrap().is_all_zero());
        assert!(!AnimationScales::from_raw(["0", "null", "0"])
            .unwrap()
            .is_all_zero());
        assert!(!AnimationScales::from_raw(["0", "0.5", "0"])
            .unwrap()
            .is_all_zero());
    }

    #[test]
    fn a_leftover_is_trusted_only_while_the_phone_is_still_zeroed() {
        let leftover = scales(Some("1"), Some("1.0"), None);
        let zeroed = AnimationScales::from_raw(ZEROS).unwrap();
        let user_changed = scales(Some("0.5"), Some("0.5"), Some("0.5"));

        assert_eq!(
            choose_originals(Some(leftover.clone()), zeroed.clone()),
            leftover
        );
        assert_eq!(
            choose_originals(Some(leftover), user_changed.clone()),
            user_changed
        );
        assert_eq!(
            choose_originals(Some(scales(Some("1; reboot"), None, None)), zeroed.clone()),
            zeroed
        );
        assert_eq!(choose_originals(None, zeroed.clone()), zeroed);
    }

    #[test]
    fn restoring_puts_a_value_back_or_deletes_a_never_set_one() {
        assert_eq!(
            restore_command("window_animation_scale", Some("1.0")),
            "settings put global window_animation_scale 1.0"
        );
        assert_eq!(
            restore_command("window_animation_scale", None),
            "settings delete global window_animation_scale"
        );
    }

    #[test]
    fn state_files_are_named_safely_per_device() {
        assert_eq!(
            state_file_name("192.168.1.5:5555"),
            "palp-animations-192.168.1.5_5555.json"
        );
        assert_eq!(
            state_file_name("emulator-5554"),
            "palp-animations-emulator-5554.json"
        );
        assert_eq!(state_file_name("a/b\\c"), "palp-animations-a_b_c.json");
    }

    #[test]
    fn state_lives_in_the_users_own_cache_folder() {
        let env = |pairs: &[(&str, &str)]| {
            let map: HashMap<String, String> = pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            move |key: &str| map.get(key).cloned()
        };
        assert_eq!(
            state_dir(env(&[("HOME", "/Users/dev")]), HostOs::MacOs),
            Some(Path::new("/Users/dev").join("Library/Caches/palp"))
        );
        assert_eq!(
            state_dir(
                env(&[("HOME", "/home/dev"), ("XDG_CACHE_HOME", "/xdg")]),
                HostOs::Linux
            ),
            Some(Path::new("/xdg").join("palp"))
        );
        assert_eq!(
            state_dir(env(&[("HOME", "/home/dev")]), HostOs::Linux),
            Some(Path::new("/home/dev").join(".cache/palp"))
        );
        assert_eq!(
            state_dir(env(&[("LOCALAPPDATA", r"C:\Local")]), HostOs::Windows),
            Some(Path::new(r"C:\Local").join("palp"))
        );
        assert_eq!(state_dir(env(&[]), HostOs::Linux), None);
    }

    #[test]
    fn a_tampered_state_file_is_ignored() {
        let dir = std::env::temp_dir().join(format!("palp-guard-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let good = dir.join("good.json");
        let saved = scales(Some("1"), None, Some("0.5"));
        save_state(&good, &saved).unwrap();
        assert_eq!(load_leftover(&good), Some(saved));

        let bad = dir.join("bad.json");
        std::fs::write(
            &bad,
            r#"{"animator_duration_scale":"1; reboot","window_animation_scale":"1","transition_animation_scale":"1"}"#,
        )
        .unwrap();
        assert_eq!(load_leftover(&bad), None);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
