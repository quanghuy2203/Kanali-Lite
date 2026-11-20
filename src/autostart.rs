//! Bật/tắt tự khởi động cùng Windows (HKCU\...\Run), không cần quyền admin.
use std::{os::windows::process::CommandExt, process::Command};

const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const NAME: &str = "KanaliRs";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn set(enable: bool) -> Result<(), String> {
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
