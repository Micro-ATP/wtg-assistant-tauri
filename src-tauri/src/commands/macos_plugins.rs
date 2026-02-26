use crate::{AppError, Result};
use crate::utils::macos_admin;
use lazy_static::lazy_static;
use serde::Serialize;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::Mutex;
use std::thread;
use tauri::Emitter;

const EVENT_MACOS_PLUGIN_INSTALL_LOG: &str = "macos-plugin-install-log";

#[derive(Debug, Serialize, Clone)]
pub struct MacosPluginItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub installed: bool,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct MacosPluginInstallStatus {
    pub running: bool,
    pub plugin_id: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct MacosPluginInstallEvent {
    phase: String,
    plugin_id: String,
    plugin_name: String,
    stream: String,
    line: String,
    exit_code: Option<i32>,
    success: Option<bool>,
}

#[derive(Debug, Clone)]
struct PluginSpec {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    install_cmd: &'static str,
}

#[derive(Debug, Default)]
struct InstallState {
    running: bool,
    plugin_id: Option<String>,
}

lazy_static! {
    static ref INSTALL_STATE: Mutex<InstallState> = Mutex::new(InstallState::default());
}

fn plugin_specs() -> Vec<PluginSpec> {
    vec![
        PluginSpec {
            id: "homebrew",
            name: "Homebrew",
            description: "Package manager required by all other macOS plugins.",
            install_cmd: r#"NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)""#,
        },
        PluginSpec {
            id: "macfuse",
            name: "macFUSE",
            description: "FUSE driver used by NTFS writable mounting.",
            install_cmd: "command -v brew >/dev/null 2>&1 || { echo 'Homebrew not found. Install Homebrew first.' >&2; exit 1; }; brew install --cask macfuse",
        },
        PluginSpec {
            id: "gromgit-homebrew-fuse",
            name: "gromgit/homebrew-fuse tap",
            description: "Homebrew tap that provides ntfs-3g-mac formula.",
            install_cmd: "command -v brew >/dev/null 2>&1 || { echo 'Homebrew not found. Install Homebrew first.' >&2; exit 1; }; brew tap gromgit/homebrew-fuse",
        },
        PluginSpec {
            id: "ntfs-3g-mac",
            name: "ntfs-3g-mac",
            description: "Writable NTFS support on macOS.",
            install_cmd: "command -v brew >/dev/null 2>&1 || { echo 'Homebrew not found. Install Homebrew first.' >&2; exit 1; }; brew tap gromgit/homebrew-fuse && brew install ntfs-3g-mac",
        },
        PluginSpec {
            id: "smartmontools",
            name: "smartmontools",
            description: "Provides smartctl for disk SMART diagnostics.",
            install_cmd: "command -v brew >/dev/null 2>&1 || { echo 'Homebrew not found. Install Homebrew first.' >&2; exit 1; }; brew install smartmontools",
        },
        PluginSpec {
            id: "wimlib",
            name: "wimlib",
            description: "Provides wimlib-imagex for reading WIM/ESD/ISO image indexes.",
            install_cmd: "command -v brew >/dev/null 2>&1 || { echo 'Homebrew not found. Install Homebrew first.' >&2; exit 1; }; brew install wimlib",
        },
    ]
}

fn build_shell_command(script: &str) -> Command {
    let mut cmd = Command::new("sh");
    let current_path = std::env::var("PATH").unwrap_or_default();
    let merged_path = format!(
        "/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/local/sbin:/usr/bin:/bin:/usr/sbin:/sbin:{}",
        current_path
    );
    cmd.args(["-lc", script]).env("PATH", merged_path);
    cmd
}

fn shell_escape_single_quotes(raw: &str) -> String {
    raw.replace('\'', "'\"'\"'")
}

fn running_as_root() -> bool {
    run_shell_capture("id -u")
        .map(|v| v.trim() == "0")
        .unwrap_or(false)
}

fn detect_console_user() -> Option<String> {
    if let Ok(sudo_user) = std::env::var("SUDO_USER") {
        let v = sudo_user.trim();
        if !v.is_empty() && v != "root" {
            return Some(v.to_string());
        }
    }

    let from_console = run_shell_capture("stat -f%Su /dev/console")
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    if from_console.is_empty() || from_console == "root" {
        return None;
    }
    Some(from_console)
}

fn build_user_shell_command(script: &str) -> Result<Command> {
    if !running_as_root() {
        return Ok(build_shell_command(script));
    }

    let user = detect_console_user().ok_or_else(|| {
        AppError::SystemError(
            "Installer is running as root, but no non-root console user was found.".to_string(),
        )
    })?;

    let path = "/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/local/sbin:/usr/bin:/bin:/usr/sbin:/sbin";
    let escaped_script = shell_escape_single_quotes(script);
    let payload = format!("export PATH='{}'; /bin/sh -lc '{}'", path, escaped_script);

    let mut cmd = Command::new("su");
    cmd.args(["-", &user, "-c", &payload]);
    Ok(cmd)
}

fn run_shell_success(script: &str) -> bool {
    build_shell_command(script)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_shell_capture(script: &str) -> Option<String> {
    let output = build_shell_command(script).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

fn command_exists(name: &str) -> bool {
    run_shell_success(&format!("command -v {} >/dev/null 2>&1", name))
}

fn brew_exists() -> bool {
    command_exists("brew")
}

fn brew_formula_installed(name: &str) -> bool {
    if !brew_exists() {
        return false;
    }
    run_shell_success(&format!("brew list --formula {} >/dev/null 2>&1", name))
}

fn brew_cask_installed(name: &str) -> bool {
    if !brew_exists() {
        return false;
    }
    run_shell_success(&format!("brew list --cask {} >/dev/null 2>&1", name))
}

fn brew_tap_present(name: &str) -> bool {
    if !brew_exists() {
        return false;
    }
    let Some(output) = run_shell_capture("brew tap") else {
        return false;
    };
    output.lines().any(|line| line.trim() == name)
}

fn macfuse_pkg_installed() -> bool {
    run_shell_success("pkgutil --pkg-info io.macfuse.installer.components.core >/dev/null 2>&1")
        || run_shell_success("pkgutil --pkgs | grep -Eiq 'io\\.macfuse|osxfuse'")
}

fn plugin_installed(spec: &PluginSpec) -> bool {
    match spec.id {
        "homebrew" => brew_exists(),
        "macfuse" => brew_cask_installed("macfuse") || macfuse_pkg_installed(),
        "gromgit-homebrew-fuse" => {
            brew_tap_present("gromgit/homebrew-fuse") || brew_tap_present("gromgit/fuse")
        }
        "ntfs-3g-mac" => {
            command_exists("ntfs-3g")
                || brew_formula_installed("ntfs-3g-mac")
                || brew_formula_installed("ntfs-3g")
        }
        "smartmontools" => command_exists("smartctl") || brew_formula_installed("smartmontools"),
        "wimlib" => command_exists("wimlib-imagex") || brew_formula_installed("wimlib"),
        _ => false,
    }
}

fn emit_install_event(
    app_handle: &tauri::AppHandle,
    plugin: &PluginSpec,
    phase: &str,
    stream: &str,
    line: String,
    exit_code: Option<i32>,
    success: Option<bool>,
) {
    let payload = MacosPluginInstallEvent {
        phase: phase.to_string(),
        plugin_id: plugin.id.to_string(),
        plugin_name: plugin.name.to_string(),
        stream: stream.to_string(),
        line,
        exit_code,
        success,
    };
    let _ = app_handle.emit(EVENT_MACOS_PLUGIN_INSTALL_LOG, payload);
}

fn spawn_stream_reader<R: Read + Send + 'static>(
    reader: R,
    stream: &'static str,
    sender: mpsc::Sender<(String, String)>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines() {
            match line {
                Ok(v) => {
                    if sender.send((stream.to_string(), v)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = sender.send((stream.to_string(), format!("read error: {}", e)));
                    break;
                }
            }
        }
    })
}

fn run_install_command_with_streaming(
    plugin: &PluginSpec,
    app_handle: &tauri::AppHandle,
) -> Result<i32> {
    let mut child = build_user_shell_command(plugin.install_cmd)?
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(AppError::io)?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::SystemError("Failed to capture install stdout".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::SystemError("Failed to capture install stderr".to_string()))?;

    let (sender, receiver) = mpsc::channel::<(String, String)>();
    let out_handle = spawn_stream_reader(stdout, "stdout", sender.clone());
    let err_handle = spawn_stream_reader(stderr, "stderr", sender.clone());
    drop(sender);

    let wait_handle = thread::spawn(move || child.wait());

    for (stream, line) in receiver {
        emit_install_event(app_handle, plugin, "line", &stream, line, None, None);
    }

    let _ = out_handle.join();
    let _ = err_handle.join();

    let status = wait_handle
        .join()
        .map_err(|_| AppError::SystemError("Install process thread panicked".to_string()))?
        .map_err(AppError::io)?;

    Ok(status.code().unwrap_or(-1))
}

fn find_ntfs_mount_script() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("useable_software/ntfs-mount.sh"));
        candidates.push(cwd.join("../useable_software/ntfs-mount.sh"));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../useable_software/ntfs-mount.sh"));
    candidates.into_iter().find(|p| p.exists())
}

fn set_install_running(plugin_id: Option<&str>, running: bool) -> Result<()> {
    let mut state = INSTALL_STATE
        .lock()
        .map_err(|_| AppError::SystemError("Installer state lock poisoned".to_string()))?;
    state.running = running;
    state.plugin_id = plugin_id.map(|v| v.to_string());
    Ok(())
}

#[tauri::command]
pub async fn list_macos_plugins() -> Result<Vec<MacosPluginItem>> {
    #[cfg(target_os = "macos")]
    {
        let mut result = Vec::new();
        for spec in plugin_specs() {
            result.push(MacosPluginItem {
                id: spec.id.to_string(),
                name: spec.name.to_string(),
                description: spec.description.to_string(),
                installed: plugin_installed(&spec),
            });
        }
        Ok(result)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(AppError::Unsupported(
            "macOS plugin installer is only available on macOS".to_string(),
        ))
    }
}

#[tauri::command]
pub async fn get_macos_plugin_install_status() -> Result<MacosPluginInstallStatus> {
    let state = INSTALL_STATE
        .lock()
        .map_err(|_| AppError::SystemError("Installer state lock poisoned".to_string()))?;
    Ok(MacosPluginInstallStatus {
        running: state.running,
        plugin_id: state.plugin_id.clone(),
    })
}

#[tauri::command]
pub async fn start_macos_plugin_install(
    plugin_id: String,
    app_handle: tauri::AppHandle,
) -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let spec = plugin_specs()
            .into_iter()
            .find(|p| p.id == plugin_id)
            .ok_or_else(|| {
                AppError::InvalidParameter(format!("Unknown plugin id: {}", plugin_id))
            })?;

        {
            let state = INSTALL_STATE
                .lock()
                .map_err(|_| AppError::SystemError("Installer state lock poisoned".to_string()))?;
            if state.running {
                return Err(AppError::SystemError(
                    "Another plugin install task is currently running".to_string(),
                ));
            }
        }

        set_install_running(Some(spec.id), true)?;

        let app = app_handle.clone();
        let started_name = spec.name.to_string();
        thread::spawn(move || {
            emit_install_event(
                &app,
                &spec,
                "started",
                "system",
                format!("Starting install: {}", spec.name),
                None,
                None,
            );

            let result = run_install_command_with_streaming(&spec, &app);
            let (success, code, summary) = match result {
                Ok(exit_code) => {
                    let ok = exit_code == 0;
                    let msg = if ok {
                        format!("Install completed: {}", spec.name)
                    } else {
                        format!("Install failed (exit code {}): {}", exit_code, spec.name)
                    };
                    (ok, Some(exit_code), msg)
                }
                Err(e) => (false, None, format!("Install failed: {}", e)),
            };

            let _ = set_install_running(None, false);
            emit_install_event(
                &app,
                &spec,
                "finished",
                "system",
                summary,
                code,
                Some(success),
            );
        });

        Ok(format!("Install task started: {}", started_name))
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (plugin_id, app_handle);
        Err(AppError::Unsupported(
            "macOS plugin installer is only available on macOS".to_string(),
        ))
    }
}

#[tauri::command]
pub async fn start_macos_ntfs_remount(app_handle: tauri::AppHandle) -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        let plugin = PluginSpec {
            id: "ntfs-remount",
            name: "NTFS Remount",
            description: "Re-mount NTFS volumes as writable via ntfs-mount.sh",
            install_cmd: "",
        };

        {
            let state = INSTALL_STATE
                .lock()
                .map_err(|_| AppError::SystemError("Installer state lock poisoned".to_string()))?;
            if state.running {
                return Err(AppError::SystemError(
                    "Another plugin install task is currently running".to_string(),
                ));
            }
        }

        set_install_running(Some(plugin.id), true)?;

        let app = app_handle.clone();
        thread::spawn(move || {
            emit_install_event(
                &app,
                &plugin,
                "started",
                "system",
                "Starting NTFS remount task".to_string(),
                None,
                None,
            );
            emit_install_event(
                &app,
                &plugin,
                "line",
                "stderr",
                "Warning: This operation unmounts and remounts all mounted NTFS volumes. Ongoing copy/read tasks may be interrupted."
                    .to_string(),
                None,
                None,
            );

            let result = (|| -> Result<()> {
                let script = find_ntfs_mount_script().ok_or_else(|| {
                    AppError::SystemError("Cannot find useable_software/ntfs-mount.sh".to_string())
                })?;
                let command = format!(
                    "export PATH='/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/local/sbin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH'; bash '{}'",
                    shell_escape_single_quotes(script.to_string_lossy().as_ref())
                );
                let output = macos_admin::run_privileged_macos(&command)?;
                if output.trim().is_empty() {
                    emit_install_event(
                        &app,
                        &plugin,
                        "line",
                        "system",
                        "NTFS remount command completed with no output".to_string(),
                        None,
                        None,
                    );
                } else {
                    for line in output.lines() {
                        emit_install_event(
                            &app,
                            &plugin,
                            "line",
                            "stdout",
                            line.to_string(),
                            None,
                            None,
                        );
                    }
                }
                Ok(())
            })();

            let _ = set_install_running(None, false);
            match result {
                Ok(()) => {
                    emit_install_event(
                        &app,
                        &plugin,
                        "finished",
                        "system",
                        "NTFS remount completed".to_string(),
                        Some(0),
                        Some(true),
                    );
                }
                Err(e) => {
                    emit_install_event(
                        &app,
                        &plugin,
                        "line",
                        "stderr",
                        e.to_string(),
                        None,
                        None,
                    );
                    emit_install_event(
                        &app,
                        &plugin,
                        "finished",
                        "system",
                        format!("NTFS remount failed: {}", e),
                        None,
                        Some(false),
                    );
                }
            }
        });

        Ok("NTFS remount task started".to_string())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app_handle;
        Err(AppError::Unsupported(
            "NTFS remount helper is only available on macOS".to_string(),
        ))
    }
}
