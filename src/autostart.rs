//! Launch-at-login via an XDG autostart entry (`setOpenAtLogin` in the original).

use std::path::PathBuf;

fn entry_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("autostart").join("k86-studio.desktop"))
}

pub fn enabled() -> bool {
    entry_path().is_some_and(|p| p.exists())
}

pub fn set(on: bool) -> std::io::Result<()> {
    let Some(path) = entry_path() else {
        return Ok(());
    };
    if !on {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let exe = std::env::current_exe()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Started hidden, like the original's `openAsHidden`.
    std::fs::write(
        &path,
        format!(
            "[Desktop Entry]\nType=Application\nName=Attack Shark K86\nExec=\"{}\" --hidden\nIcon=k86-studio\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n",
            exe.display()
        ),
    )
}
