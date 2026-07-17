use std::path::Path;
use std::process::Command;

/// Sets `path` as the wallpaper via awww, updates the current-wallpaper
/// symlink, and sends a notification. Mirrors the old bg-apply script.
pub fn apply(path: &Path) -> Result<(), String> {
    let status = Command::new("awww")
        .arg("img")
        .arg(path)
        .args([
            "--transition-type",
            "grow",
            "--transition-pos",
            "center",
            "--transition-duration",
            "0.9",
            "--transition-fps",
            "120",
        ])
        .status()
        .map_err(|e| format!("failed to run awww: {e}"))?;
    if !status.success() {
        return Err(format!("awww exited with {status}"));
    }

    update_current_link(path)?;

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    notify(&format!("Changed to {name}"));
    Ok(())
}

fn update_current_link(target: &Path) -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let dir = Path::new(&home).join(".local/share/wallpaper");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    // Symlink into a temp name then rename, so the swap is atomic.
    let tmp = dir.join(".current.tmp");
    let _ = std::fs::remove_file(&tmp);
    std::os::unix::fs::symlink(target, &tmp)
        .map_err(|e| format!("cannot create symlink: {e}"))?;
    std::fs::rename(&tmp, dir.join("current"))
        .map_err(|e| format!("cannot update current link: {e}"))?;
    Ok(())
}

pub fn notify(msg: &str) {
    let _ = Command::new("notify-send").arg("Background").arg(msg).status();
}
