use crate::{AppError, Result};
use lazy_static::lazy_static;
use serde::Serialize;
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Clone)]
pub struct MacosAdminSessionStatus {
    pub supported: bool,
    pub authorized: bool,
    pub authorized_at_unix: Option<u64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Default)]
struct MacosAdminSessionState {
    authorized_at: Option<SystemTime>,
    last_error: Option<String>,
}

lazy_static! {
    static ref MACOS_ADMIN_STATE: Mutex<MacosAdminSessionState> =
        Mutex::new(MacosAdminSessionState::default());
}

fn to_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

fn to_unix_secs(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

#[cfg(target_os = "macos")]
fn state_snapshot(state: &MacosAdminSessionState) -> MacosAdminSessionStatus {
    let authorized = state.authorized_at.is_some();
    let authorized_at_unix = state.authorized_at.and_then(to_unix_secs);

    MacosAdminSessionStatus {
        supported: true,
        authorized,
        authorized_at_unix,
        last_error: state.last_error.clone(),
    }
}

#[cfg(not(target_os = "macos"))]
fn state_snapshot(state: &MacosAdminSessionState) -> MacosAdminSessionStatus {
    MacosAdminSessionStatus {
        supported: false,
        authorized: false,
        authorized_at_unix: None,
        last_error: state.last_error.clone(),
    }
}

fn escape_applescript(raw: &str) -> String {
    raw.replace('\n', "; ")
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn set_authorized() -> Result<()> {
    let mut state = MACOS_ADMIN_STATE
        .lock()
        .map_err(|_| AppError::SystemError("macOS admin state lock poisoned".to_string()))?;
    state.authorized_at = Some(SystemTime::now());
    state.last_error = None;
    Ok(())
}

fn set_last_error(message: String) -> Result<()> {
    let mut state = MACOS_ADMIN_STATE
        .lock()
        .map_err(|_| AppError::SystemError("macOS admin state lock poisoned".to_string()))?;
    state.last_error = Some(message);
    Ok(())
}

fn is_user_cancelled_error(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("user canceled")
        || lower.contains("user cancelled")
        || lower.contains("not authorized")
}

#[cfg(target_os = "macos")]
pub fn run_privileged_macos(script: &str) -> Result<String> {
    let escaped = escape_applescript(script);
    let apple_script = format!(
        "do shell script \"{}\" with administrator privileges",
        escaped
    );
    let output = Command::new("osascript")
        .args(["-e", &apple_script])
        .output()
        .map_err(AppError::io)?;

    if output.status.success() {
        set_authorized()?;
        return Ok(to_text(&output.stdout));
    }

    let err = to_text(&output.stderr);
    let out = to_text(&output.stdout);
    let detail = if err.is_empty() { out } else { err };
    let _ = set_last_error(detail.clone());

    if is_user_cancelled_error(&detail) {
        return Err(AppError::PermissionDenied);
    }

    Err(AppError::SystemError(format!(
        "Administrator command failed: {}",
        detail
    )))
}

#[cfg(not(target_os = "macos"))]
pub fn run_privileged_macos(script: &str) -> Result<String> {
    let _ = script;
    Err(AppError::Unsupported(
        "Administrator command is only available on macOS".to_string(),
    ))
}

#[cfg(target_os = "macos")]
pub fn run_shell_with_auto_privilege(script: &str) -> Result<()> {
    let output = Command::new("sh")
        .args(["-lc", script])
        .output()
        .map_err(AppError::io)?;

    if output.status.success() {
        return Ok(());
    }

    let _ = run_privileged_macos(script)?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn run_shell_with_auto_privilege(script: &str) -> Result<()> {
    let _ = script;
    Err(AppError::Unsupported(
        "Administrator command is only available on macOS".to_string(),
    ))
}

pub fn get_macos_admin_session_status() -> Result<MacosAdminSessionStatus> {
    let state = MACOS_ADMIN_STATE
        .lock()
        .map_err(|_| AppError::SystemError("macOS admin state lock poisoned".to_string()))?;
    Ok(state_snapshot(&state))
}

#[cfg(target_os = "macos")]
pub fn authorize_macos_admin_session() -> Result<MacosAdminSessionStatus> {
    let _ = run_privileged_macos("/usr/bin/true")?;
    get_macos_admin_session_status()
}

#[cfg(not(target_os = "macos"))]
pub fn authorize_macos_admin_session() -> Result<MacosAdminSessionStatus> {
    Err(AppError::Unsupported(
        "Administrator authorization is only available on macOS".to_string(),
    ))
}
