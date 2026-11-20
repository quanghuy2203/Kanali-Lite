//! Kanali: ứng dụng chính (khay hệ thống + giao diện), không hiện cửa sổ console.
//!
//!   kanali.exe                chạy nền (nếu chưa chạy) và mở giao diện
//!   kanali.exe --background   chỉ chạy nền (dùng khi tự khởi động cùng Windows)
//!   kanali.exe --ui           chỉ mở giao diện
#![windows_subsystem = "windows"]

mod daemon;
mod media;
mod ui;

use std::{env, process::Command, time::Duration};

use kanali_rs::config;

pub(crate) fn spawn_self(arg: &str) {
    if let Ok(exe) = env::current_exe() {
        let _ = Command::new(exe).arg(arg).spawn();
    }
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let _ = std::fs::write(config::data_dir().join("crash.log"), format!("{info}"));
    }));

    let args: Vec<String> = env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);

    // Dọn file chương trình cũ còn sót sau lần cập nhật trước.
    if let Ok(exe) = env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("exe.old"));
    }

    if has("--ui") {
        ui::run();
        return;
    }

    let quiet = has("--background");
    // Sau khi cập nhật, tiến trình cũ cần vài giây để nhả khóa.
    let wait = if quiet || has("--after-update") { Duration::from_secs(8) } else { Duration::from_millis(300) };

    match config::acquire_lock("daemon.lock", wait) {
        Some(lock) => {
            if !quiet {
                spawn_self("--ui");
            }
            daemon::run(lock);
        }
        None => {
            // Dịch vụ nền đã chạy: chỉ mở giao diện.
            if !quiet {
                spawn_self("--ui");
            }
        }
    }
}
