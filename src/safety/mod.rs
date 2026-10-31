use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use crate::error::PalpError;

/// Dangerous keys and shortcuts blocked by default to prevent OS lockups or data loss.
const BLOCKED_KEYS: &[&str] = &[
    "win+l",
    "ctrl+alt+del",
    "alt+f4",
    "win+x",
    "power",
    "sleep",
    "shutdown",
];

/// Validates whether a requested key or shortcut is safe to execute.
pub fn validate_key(key: &str) -> Result<(), PalpError> {
    let normalized = key.trim().to_lowercase().replace(' ', "");
    for blocked in BLOCKED_KEYS {
        if normalized == *blocked {
            return Err(PalpError::ActionBlocked(format!(
                "Key combination '{}' is blocked by safety guardrails to prevent system lockup or data loss.",
                key
            )));
        }
    }
    Ok(())
}

/// Validates whether target coordinates fall within the permitted application window.
pub fn validate_bounds(x: u32, y: u32, bounds: Option<(u32, u32, u32, u32)>) -> Result<(), PalpError> {
    if let Some((left, top, right, bottom)) = bounds {
        if x < left || x > right || y < top || y > bottom {
            return Err(PalpError::BoundsViolation {
                x,
                y,
                bounds: format!("left={}, top={}, right={}, bottom={}", left, top, right, bottom),
            });
        }
    }
    Ok(())
}

/// Checks if physical mouse position represents an intentional emergency stop (corner slam).
pub fn check_corner_fail_safe(x: i32, y: i32) -> Result<(), PalpError> {
    // Corner slam: top-left (0, 0)
    if x <= 1 && y <= 1 {
        return Err(PalpError::EmergencyStop);
    }
    Ok(())
}

/// Logs an action to the local audit trail (.palp/audit.log) for total transparency.
pub fn log_audit(action: &str, details: &str) {
    let log_dir = Path::new(".palp");
    if !log_dir.exists() {
        let _ = fs::create_dir_all(log_dir);
    }

    let log_file = log_dir.join("audit.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_file) {
        let timestamp = chrono_now_string();
        let _ = writeln!(file, "[{}] [{}] {}", timestamp, action, details);
    }
}

/// Simple ISO-like timestamp generator without heavy external dependencies.
fn chrono_now_string() -> String {
    // Falls back to system time representation
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => format!("T+{}s", d.as_secs()),
        Err(_) => "UNKNOWN_TIME".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocked_keys() {
        assert!(validate_key("alt+f4").is_err());
        assert!(validate_key("Win + L").is_err());
        assert!(validate_key("ctrl+alt+del").is_err());
        assert!(validate_key("power").is_err());
        assert!(validate_key("enter").is_ok());
        assert!(validate_key("back").is_ok());
        assert!(validate_key("tab").is_ok());
    }

    #[test]
    fn test_bounds_validation() {
        let bounds = Some((100, 100, 500, 800));
        assert!(validate_bounds(200, 400, bounds).is_ok());
        assert!(validate_bounds(50, 400, bounds).is_err());
        assert!(validate_bounds(200, 900, bounds).is_err());
        assert!(validate_bounds(200, 400, None).is_ok());
    }

    #[test]
    fn test_fail_safe_corner() {
        assert!(check_corner_fail_safe(0, 0).is_err());
        assert!(check_corner_fail_safe(1, 1).is_err());
        assert!(check_corner_fail_safe(500, 300).is_ok());
    }
}
