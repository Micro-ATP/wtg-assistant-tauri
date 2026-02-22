#![allow(dead_code)]

use std::process::Command;
use crate::AppError;
use tracing::{info, warn, error};

/// Decode command output bytes to String.
/// On Chinese/Japanese/Korean Windows, system commands (DISM, diskpart, etc.)
/// output text in the system's OEM code page (e.g. GBK for Chinese).
/// We try UTF-8 first; if that fails, fall back to GBK decoding.
fn decode_output(bytes: &[u8]) -> String {
    // Try UTF-8 first (works for English output and already-UTF-8 systems)
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }

    // Fall back to GBK (Windows code page 936, covers Simplified/Traditional Chinese)
    #[cfg(target_os = "windows")]
    {
        let (decoded, _, _had_errors) = encoding_rs::GBK.decode(bytes);
        return decoded.into_owned();
    }

    #[cfg(not(target_os = "windows"))]
    {
        String::from_utf8_lossy(bytes).to_string()
    }
}

/// Configure a Command to hide the console window on Windows
#[cfg(target_os = "windows")]
fn hide_console(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(target_os = "windows"))]
fn hide_console(cmd: &mut Command) -> &mut Command {
    cmd
}

pub struct CommandExecutor;

impl CommandExecutor {
    /// Execute a command and return stdout
    pub fn execute(cmd: &str, args: &[&str]) -> crate::Result<String> {
        info!("Executing: {} {}", cmd, args.join(" "));

        let mut command = Command::new(cmd);
        command.args(args);
        hide_console(&mut command);

        let output = command.output().map_err(AppError::io)?;

        let stdout = decode_output(&output.stdout);
        let stderr = decode_output(&output.stderr);

        if !output.status.success() {
            warn!("Command failed: {} - stderr: {}", cmd, stderr);
            return Err(AppError::CommandFailed(
                format!("{}: {}", cmd, if stderr.trim().is_empty() { &stdout } else { &stderr })
            ));
        }

        info!("Command succeeded: {}", cmd);
        Ok(stdout)
    }

    /// Execute a command via cmd.exe /c (Windows)
    #[cfg(target_os = "windows")]
    pub fn run_cmd(args: &str) -> crate::Result<String> {
        info!("Running cmd: {}", args);

        let mut command = Command::new("cmd.exe");
        command.args(&["/c", args]);
        hide_console(&mut command);

        let output = command.output().map_err(AppError::io)?;

        let stdout = decode_output(&output.stdout);
        let stderr = decode_output(&output.stderr);

        if !output.status.success() {
            warn!("CMD failed: {} - stderr: {}", args, stderr);
        }

        info!("CMD output: {}", stdout.trim());
        Ok(stdout)
    }

    /// Execute a command and get exit code
    #[cfg(target_os = "windows")]
    pub fn run_cmd_with_exit_code(args: &str) -> crate::Result<(String, i32)> {
        info!("Running cmd (with exit code): {}", args);

        let mut command = Command::new("cmd.exe");
        command.args(&["/c", args]);
        hide_console(&mut command);

        let output = command.output().map_err(AppError::io)?;

        let stdout = decode_output(&output.stdout);
        let exit_code = output.status.code().unwrap_or(-1);

        Ok((stdout, exit_code))
    }

    /// Execute a command allowing failure (returns output regardless)
    pub fn execute_allow_fail(cmd: &str, args: &[&str]) -> crate::Result<String> {
        info!("Executing (allow fail): {} {}", cmd, args.join(" "));

        let mut command = Command::new(cmd);
        command.args(args);
        hide_console(&mut command);

        let output = command.output().map_err(AppError::io)?;

        let stdout = decode_output(&output.stdout);
        let stderr = decode_output(&output.stderr);

        if !output.status.success() {
            warn!("Command returned non-zero (allowed): {} - {}", cmd, stderr);
        }

        Ok(format!("{}{}", stdout, stderr))
    }

    /// Kill a process by name (Windows)
    #[cfg(target_os = "windows")]
    pub fn kill_process(name: &str) -> crate::Result<()> {
        info!("Killing process: {}", name);
        let mut cmd = Command::new("taskkill.exe");
        cmd.args(&["/f", "/IM", name]);
        hide_console(&mut cmd);
        let _ = cmd.output();
        Ok(())
    }

    /// Kill a process by name (Unix)
    #[cfg(not(target_os = "windows"))]
    pub fn kill_process(name: &str) -> crate::Result<()> {
        info!("Killing process: {}", name);
        let _ = Command::new("pkill")
            .args(&["-f", name])
            .output();
        Ok(())
    }
}

/// Run a diskpart script on Windows
#[cfg(target_os = "windows")]
pub fn run_diskpart_script(script: &str) -> crate::Result<String> {
    use std::io::Write;

    info!("Running diskpart script:\n{}", script);

    // Write script to temp file
    let temp_dir = std::env::temp_dir();
    let script_path = temp_dir.join(format!("wtga_dp_{}.txt", uuid::Uuid::new_v4()));

    let mut file = std::fs::File::create(&script_path).map_err(AppError::io)?;
    file.write_all(script.as_bytes()).map_err(AppError::io)?;
    drop(file);

    // Execute diskpart with the script
    let mut cmd = Command::new("diskpart.exe");
    cmd.args(&["/s", &script_path.to_string_lossy()]);
    hide_console(&mut cmd);

    let output = cmd.output().map_err(AppError::io)?;

    let stdout = decode_output(&output.stdout);
    let stderr = decode_output(&output.stderr);

    info!("Diskpart output: {}", stdout);

    // Clean up temp file
    let _ = std::fs::remove_file(&script_path);

    if !output.status.success() {
        error!("Diskpart failed: {}", stderr);
        return Err(AppError::DiskError(format!("Diskpart failed: {}", stderr)));
    }

    Ok(stdout)
}

/// Run a diskpart script and capture output to a file (Windows)
#[cfg(target_os = "windows")]
pub fn run_diskpart_script_with_output(script: &str) -> crate::Result<(String, String)> {
    use std::io::Write;

    let temp_dir = std::env::temp_dir();
    let script_path = temp_dir.join(format!("wtga_dp_{}.txt", uuid::Uuid::new_v4()));
    let output_path = temp_dir.join(format!("wtga_dp_out_{}.txt", uuid::Uuid::new_v4()));

    let mut file = std::fs::File::create(&script_path).map_err(AppError::io)?;
    file.write_all(script.as_bytes()).map_err(AppError::io)?;
    drop(file);

    let mut cmd = Command::new("diskpart.exe");
    cmd.args(&["/s", &script_path.to_string_lossy()]);
    hide_console(&mut cmd);

    let output = cmd.output().map_err(AppError::io)?;

    let stdout = decode_output(&output.stdout);

    // Write output to file for parsing
    if let Ok(mut out_file) = std::fs::File::create(&output_path) {
        let _ = out_file.write_all(stdout.as_bytes());
    }

    let _ = std::fs::remove_file(&script_path);
    let output_file_path = output_path.to_string_lossy().to_string();
    let _ = std::fs::remove_file(&output_path);

    Ok((stdout, output_file_path))
}

/// Placeholder for non-Windows platforms
#[cfg(not(target_os = "windows"))]
pub fn run_diskpart_script(_script: &str) -> crate::Result<String> {
    Err(AppError::SystemError("Diskpart is only available on Windows".to_string()))
}

#[cfg(not(target_os = "windows"))]
pub fn run_diskpart_script_with_output(_script: &str) -> crate::Result<(String, String)> {
    Err(AppError::SystemError("Diskpart is only available on Windows".to_string()))
}

/// Check if a path exists, with retries
pub fn wait_for_path(path: &str, max_retries: u32, delay_ms: u64) -> bool {
    for i in 0..max_retries {
        if std::path::Path::new(path).exists() {
            info!("Path {} found after {} checks", path, i);
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    }
    warn!("Path {} not found after {} retries", path, max_retries);
    false
}

/// Prevent system sleep (Windows)
#[cfg(target_os = "windows")]
pub fn prevent_sleep() {
    use std::os::raw::c_uint;
    // ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_AWAYMODE_REQUIRED
    const ES_CONTINUOUS: c_uint = 0x80000000;
    const ES_SYSTEM_REQUIRED: c_uint = 0x00000001;

    extern "system" {
        fn SetThreadExecutionState(esFlags: c_uint) -> c_uint;
    }

    unsafe {
        SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
    }
    info!("System sleep prevented");
}

/// Restore system sleep (Windows)
#[cfg(target_os = "windows")]
pub fn restore_sleep() {
    use std::os::raw::c_uint;
    const ES_CONTINUOUS: c_uint = 0x80000000;

    extern "system" {
        fn SetThreadExecutionState(esFlags: c_uint) -> c_uint;
    }

    unsafe {
        SetThreadExecutionState(ES_CONTINUOUS);
    }
    info!("System sleep restored");
}

#[cfg(not(target_os = "windows"))]
pub fn prevent_sleep() {
    info!("System sleep prevention not implemented on this platform");
}

#[cfg(not(target_os = "windows"))]
pub fn restore_sleep() {
    info!("System sleep restore not implemented on this platform");
}
