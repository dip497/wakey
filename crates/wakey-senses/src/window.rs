//! Window sensor — captures active window app name and title.
//!
//! On Linux, uses xdotool. Simple and reliable.
//! Can be optimized later with x11rb if needed.

use std::process::Command;
use tracing::{debug, warn};

/// Get the active window's app name and title.
///
/// Returns `(app_name, window_title)` or `None` if detection fails.
///
/// Linux implementation uses xdotool:
/// - `xdotool getactivewindow getwindowname` for title
/// - `xdotool getactivewindow` + `xprop WM_CLASS` for app name
pub fn get_active_window() -> Option<(String, String)> {
    get_active_window_linux()
}

fn get_active_window_linux() -> Option<(String, String)> {
    // Get window ID
    let window_id = run_command("xdotool", &["getactivewindow"])?;

    // Get window title
    let title = run_command("xdotool", &["getwindowname", &window_id])?;

    // Get window class (app name)
    let class_output = run_command("xprop", &["-id", &window_id, "WM_CLASS"])?;
    let app_name = parse_wm_class(&class_output);

    if app_name.is_empty() || title.is_empty() {
        debug!(app = %app_name, title = %title, "Window info incomplete");
        return None;
    }

    debug!(app = %app_name, title = %title, "Active window detected");
    Some((app_name, title))
}

/// Run a command and capture its stdout.
fn run_command(cmd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| {
            warn!(cmd = %cmd, error = %e, "Command failed to execute");
            e
        })
        .ok()?;

    if !output.status.success() {
        warn!(
            cmd = %cmd,
            code = output.status.code(),
            stderr = String::from_utf8_lossy(&output.stderr).trim(),
            "Command returned non-zero status"
        );
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Some(stdout)
}

/// Parse WM_CLASS output from xprop.
///
/// Output format: `WM_CLASS(STRING) = "app_name", "AppName"`
/// We take the first value (the instance name, lowercase).
fn parse_wm_class(output: &str) -> String {
    // Example: WM_CLASS(STRING) = "firefox", "Firefox"
    let parts: Vec<&str> = output.split('=').collect();
    if parts.len() < 2 {
        return String::new();
    }

    let classes = parts[1].trim();
    // Extract the first quoted string
    let start = classes.find('"');
    let end = classes.find(',').or_else(|| classes.rfind('"'));

    match (start, end) {
        (Some(s), Some(e)) if e > s => {
            let first_class = &classes[s + 1..e];
            first_class.trim_matches('"').trim().to_string()
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_wm_class() {
        let output = "WM_CLASS(STRING) = \"firefox\", \"Firefox\"";
        assert_eq!(parse_wm_class(output), "firefox");

        let output = "WM_CLASS(STRING) = \"code\", \"Code\"";
        assert_eq!(parse_wm_class(output), "code");

        let output = "WM_CLASS = \"terminal\"";
        assert_eq!(parse_wm_class(output), "terminal");
    }

    #[test]
    fn test_parse_wm_class_malformed() {
        let output = "WM_CLASS(STRING)";
        assert_eq!(parse_wm_class(output), "");

        let output = "";
        assert_eq!(parse_wm_class(output), "");
    }
}
