//! Startup through Task Scheduler so an elevated app starts after logon without another UAC prompt.
use std::{os::windows::process::CommandExt, process::Command};

const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const OLD_NAME: &str = "KanaliRs";
const TASK: &str = "Kanali Lite";
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
    let _ = Command::new("reg.exe").args(["delete", KEY, "/v", OLD_NAME, "/f"])
        .creation_flags(CREATE_NO_WINDOW).output();
    if enable {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let value = format!("\"{}\" --background", exe.display());
        let out=Command::new("schtasks.exe").args(["/Create","/TN",TASK,"/SC","ONLOGON","/RL","HIGHEST","/DELAY","0000:10","/TR",&value,"/F"])
            .creation_flags(CREATE_NO_WINDOW).output().map_err(|e|e.to_string())?;
        if out.status.success(){return Ok(())}
        return Err(format!("Không tạo được tác vụ khởi động quyền cao nhất: {}",String::from_utf8_lossy(&out.stderr).trim()));
    } else {
        let out=Command::new("schtasks.exe").args(["/Delete","/TN",TASK,"/F"])
            .creation_flags(CREATE_NO_WINDOW).output().map_err(|e|e.to_string())?;
        if out.status.success() || !is_enabled(){return Ok(())}
        return Err(String::from_utf8_lossy(&out.stderr).trim().into());
    }
}

pub fn is_enabled() -> bool {
    Command::new("schtasks.exe")
        .args(["/Query", "/TN", TASK])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
