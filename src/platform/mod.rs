//! Small OS helpers: open a directory in the file manager, respawn the app.

pub mod telemetry;

use std::path::Path;

pub fn open_dir(path: &Path) {
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "windows")]
    let cmd = "explorer";
    let _ = std::process::Command::new(cmd).arg(path).spawn();
}

/// Respawns the current executable and returns Ok if the child started.
/// ponytail: respawns WITHOUT CLI args on purpose — UI-driven restarts are interactive
/// and settings.toml already holds the new state; replaying args would override it.
pub fn respawn() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let cwd = std::env::current_dir()?;
    std::process::Command::new(exe).current_dir(cwd).spawn().map(|_| ())
}
