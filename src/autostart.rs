//! Bật/tắt tự khởi động cùng Windows (HKCU\...\Run), không cần quyền admin.
use std::{os::windows::process::CommandExt, process::Command};

const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const NAME: &str = "KanaliRs";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn legacy_script() -> Option<std::path::PathBuf> {
    Some(
        std::path::PathBuf::from(std::env::var_os("APPDATA")?)
            .join(r"Microsoft\Windows\Start Menu\Programs\Startup\kanali-rs.vbs"),
    )
}

/// Older builds used a Startup-folder VBS containing an absolute build path.
/// Once that executable moves, Windows shows a Script Host error at every login.
pub fn cleanup_legacy() {
    if let Some(path) = legacy_script() {
        let _ = std::fs::remove_file(path);
    }
}

pub fn set(enable: bool) -> Result<(), String> {
    cleanup_legacy();
    let mut c = Command::new("reg");
    if enable {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let value = format!("\"{}\" --background", exe.display());
        c.args(["add", KEY, "/v", NAME, "/t", "REG_SZ", "/d", value.as_str(), "/f"]);
    } else {
        c.args(["delete", KEY, "/v", NAME, "/f"]);
    }
    let out = c.creation_flags(CREATE_NO_WINDOW).output().map_err(|e| e.to_string())?;
    if out.status.success() || !enable {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn is_enabled() -> bool {
    Command::new("reg")
        .args(["query", KEY, "/v", NAME])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
