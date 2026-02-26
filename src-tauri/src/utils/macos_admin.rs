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

fn with_macos_privileged_env(script: &str) -> String {
    format!(
        "export HOME=/var/root; export TMPDIR=/tmp; export PATH='/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/local/sbin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH'; cd /tmp || true; {}",
        script
    )
}

fn sanitize_admin_detail(detail: &str) -> String {
    let lines: Vec<&str> = detail
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.contains("shell-init: error retrieving current directory"))
        .filter(|line| !line.contains("chdir: error retrieving current directory"))
        .collect();
    if lines.is_empty() {
        detail.trim().to_string()
    } else {
        lines.join("\n")
    }
}

fn clip_detail(detail: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in detail.chars() {
        if out.chars().count() >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

fn map_privileged_error(detail: &str) -> AppError {
    let clean = sanitize_admin_detail(detail);
    let lower = clean.to_ascii_lowercase();

    if lower.contains("unsafe state")
        || lower.contains("fast restarting")
        || lower.contains("hibernation")
    {
        return AppError::DiskError(format!(
            "Target NTFS volume is still in an unsafe state after force remount attempts. Fully shut down Windows (disable Fast Startup/hibernation), reconnect the disk, and retry. If needed, run ntfsfix on the target partition from terminal. Detail: {}",
            clip_detail(&clean, 2000)
        ));
    }

    if (lower.contains("error opening '/dev/") || lower.contains("could not open /dev/"))
        && lower.contains("operation not permitted")
    {
        return AppError::DiskError(
            "Cannot access raw disk device from this app session. Please grant Full Disk Access to this app in macOS Privacy & Security, then retry."
                .to_string(),
        );
    }

    AppError::SystemError(format!("Administrator command failed: {}", clean))
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
    let prepared = with_macos_privileged_env(script);
    let escaped = escape_applescript(&prepared);
    let apple_script = format!(
        "do shell script \"{}\" with administrator privileges",
        escaped
    );
    let output = Command::new("osascript")
        .args(["-e", &apple_script])
        .current_dir("/tmp")
        .output()
        .map_err(AppError::io)?;

    if output.status.success() {
        set_authorized()?;
        return Ok(to_text(&output.stdout));
    }

    let err = to_text(&output.stderr);
    let out = to_text(&output.stdout);
    let detail = if err.is_empty() { out } else { err };
    let clean_detail = sanitize_admin_detail(&detail);
    let _ = set_last_error(clean_detail.clone());

    if is_user_cancelled_error(&clean_detail) {
        return Err(AppError::PermissionDenied);
    }

    Err(map_privileged_error(&clean_detail))
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
    let prepared = with_macos_privileged_env(script);
    let output = Command::new("sh")
        .args(["-lc", &prepared])
        .current_dir("/tmp")
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
