//! Tiến trình nền: icon khay hệ thống, giữ kết nối màn hình (heartbeat),
//! áp dụng cấu hình, kiểm tra và cài cập nhật.
use std::{
    fs::File,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use kanali_rs::{
    config::{self, Config, State},
    device, update,
};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIconBuilder, TrayIconEvent,
};

struct Shared {
    state: Mutex<State>,
    found: Mutex<Option<update::Found>>,
    refresh: AtomicBool,
    reapply: AtomicBool,
    updating: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        let state = State {
            version: env!("CARGO_PKG_VERSION").to_string(),
            ..State::default()
        };
        Self {
            state: Mutex::new(state),
            found: Mutex::new(None),
            refresh: AtomicBool::new(false),
            reapply: AtomicBool::new(false),
            updating: AtomicBool::new(false),
        }
    }

    /// Sửa trạng thái rồi ghi ra state.json cho giao diện đọc.
    fn with<F: FnOnce(&mut State)>(&self, f: F) {
        let mut state = self.state.lock().unwrap();
        f(&mut state);
        state.updated_ms = config::now_ms();
        let _ = config::save(&config::state_path(), &*state);
    }
}

fn load_cfg() -> Config {
    config::load_opt(&config::config_path()).unwrap_or_default()
}

fn effective_repo(cfg: &Config) -> String {
    if cfg.repo.trim().is_empty() {
        update::DEFAULT_REPO.to_string()
    } else {
        cfg.repo.trim().to_string()
    }
}

// ---------------- kết nối màn hình ----------------

fn worker(sh: Arc<Shared>) {
    let mut delay = 3u64;
    loop {
        let started = Instant::now();
        sh.with(|s| {
            s.connected = false;
            s.message = "Đang kết nối với màn hình...".into();
        });
        if let Err(e) = session(&sh) {
            config::log(&format!("phiên kết nối kết thúc: {e}"));
            sh.with(|s| {
                s.connected = false;
                s.message = e;
            });
        }
        // Lỗi lặp lại nhanh thì chờ lâu hơn: mỗi lần kết nối lại là một lần khởi tạo màn hình,
        // làm lặp liên tục có thể khiến tản đứng hình.
        let quick = started.elapsed() < Duration::from_secs(60);
        thread::sleep(Duration::from_secs(delay));
        delay = if quick { (delay * 2).min(60) } else { 3 };
    }
}

fn session(sh: &Arc<Shared>) -> Result<(), String> {
    let session_start = config::now_ms();
    let path = device::find_device()?;
    let mut f = device::open_device(&path)?;
    let replies = device::init_session(&mut f)?;
    let info = device::parse_info(&replies);
    let serial = info.serial.clone();
    let files = device::fetch_files(&mut f)?;

    let mut cfg = match config::load_opt::<Config>(&config::config_path()) {
        Some(c) => c,
        None => {
            let c = Config::default();
            let _ = config::save(&config::config_path(), &c);
            c
        }
    };
    let present = |name: &str| files.iter().any(|p| p.rsplit('/').next() == Some(name));
    if cfg.playing == config::DOWNLOAD_46
        && !present(&cfg.playing)
        && present(config::FACTORY_VIDEO)
    {
        cfg.playing = config::FACTORY_VIDEO.into();
        config::save(&config::config_path(), &cfg).map_err(|e| e.to_string())?;
    }
    let initial = device::apply_play(&mut f, &cfg);
    sh.with(|s| {
        s.connected = true;
        if initial.is_ok() { s.playback_started_ms = config::now_ms(); }
        s.message = initial.err().unwrap_or_else(|| "Đã kết nối".into());
        s.info = Some(info);
        s.files = files;
    });

    let request_path = config::data_dir().join("remove-request.json");
    let mut last_hb = Instant::now();
    let mut misses = 0u32;
    loop {
        thread::sleep(Duration::from_millis(400));
        if let Some(req) = config::load_opt::<config::Removal>(&request_path) {
            if std::fs::remove_file(&request_path).is_ok() {
                let result = if req.serial != serial
                    || req.requested_ms < session_start
                    || config::now_ms().saturating_sub(req.requested_ms) > 30_000
                {
                    Err("Yêu cầu xóa hết hạn sau khi đổi kết nối.".into())
                } else {
                    device::remove_media(
                        &mut f,
                        &req.path,
                        &config::load_opt::<Config>(&config::config_path())
                            .ok_or("Không đọc được cấu hình")?,
                    )
                };
                match result {
                    Ok(files) => {
                        let name = req.path.rsplit('/').next().unwrap_or("");
                        let _ = std::fs::remove_file(crate::media::poster(name));
                        let _ = std::fs::remove_file(
                            crate::media::poster(name).with_extension("numbered.rgb"),
                        );
                        sh.with(|s| {
                            s.files = files;
                            s.media_status = format!("Đã xóa {name}");
                        });
                    }
                    Err(e) => sh.with(|s| s.media_status = format!("Chưa xóa: {e}")),
                }
            }
        }
        if let Some(path) = crate::media::pending() {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let mut last = Instant::now();
            sh.with(|s| s.media_status = format!("Đang gửi {name}…"));
            let expected_size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
            let result = device::upload(&mut f, &path, &name, |done, total| {
                if last.elapsed() >= Duration::from_millis(500) {
                    sh.with(|s| {
                        s.media_status = format!("Đang gửi {name} · {}%", done * 100 / total)
                    });
                    last = Instant::now();
                }
            })
            .and_then(|_| device::verify_upload(&mut f, &name, expected_size))
            .and_then(|files| {
                sh.with(|s| {
                    s.files = files;
                    s.media_status = format!("Đã thêm {name}");
                });
                Ok(())
            });
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
            if let Err(e) = result {
                sh.with(|s| {
                    s.media_status =
                        format!("Không gửi được {name}: {e}. Có thể chọn lại tệp nguồn.")
                });
                return Err(e);
            }
        }
        let refresh = sh.refresh.swap(false, Ordering::Relaxed);
        if refresh {
            let files = device::fetch_files(&mut f)?;
            sh.with(|s| s.files = files);
        }
        if let Some(new) = config::load_opt::<Config>(&config::config_path()) {
            if sh.reapply.swap(false, Ordering::Relaxed)
                || new.playing != cfg.playing
                || new.poweron != cfg.poweron
                || new.standby != cfg.standby
                || new.level != cfg.level
                || new.layout != cfg.layout
            {
                match device::apply_play(&mut f, &new) {
                    Ok(()) => {
                        sh.with(|s| {
                            s.playback_started_ms = config::now_ms();
                            s.message = format!("PANORAMA đã nhận cấu hình phát {}", new.playing)
                        });
                    }
                    Err(e) => {
                        sh.with(|s| s.message = e);
                    }
                }
                // Cho màn hình thời gian nạp video mới trước nhịp heartbeat kế tiếp.
                last_hb = Instant::now();
            }
            cfg = new;
        }
        if last_hb.elapsed() >= Duration::from_secs(2) {
            // Một nhịp trả lời chậm (ví dụ lúc màn hình đang nạp video) không đáng để
            // ngắt kết nối và khởi tạo lại; chỉ ngắt sau 3 nhịp liên tiếp thất bại.
            match device::heartbeat(&mut f) {
                Ok(()) => misses = 0,
                Err(e) => {
                    misses += 1;
                    config::log(&format!("heartbeat lỗi ({misses}/3): {e}"));
                    if misses >= 3 {
                        return Err(e);
                    }
                    sh.with(|s| s.message = format!("Màn hình phản hồi chậm ({misses}/3): {e}"));
                }
            }
            last_hb = Instant::now();
        }
    }
}

// ---------------- lệnh từ giao diện + cập nhật ----------------

fn control(sh: Arc<Shared>) {
    let mut last_check: Option<Instant> = None;
    let mut last_pub = Instant::now();
    let mut last_lighting = Instant::now() - Duration::from_secs(30);
    let mut monitor = crate::system::Monitor::new();
    loop {
        // Lighting commands should feel immediate; the old 500 ms poll added
        // visible latency before either hardware bridge even started working.
        thread::sleep(Duration::from_millis(100));
        match config::take_command().as_deref() {
            Some("check") => {
                last_check = Some(Instant::now());
                let s = sh.clone();
                thread::spawn(move || do_check(&s));
            }
            Some("update") => {
                let s = sh.clone();
                thread::spawn(move || do_update(&s));
            }
            Some("apply") => sh.reapply.store(true, Ordering::Relaxed),
            Some("refresh") => sh.refresh.store(true, Ordering::Relaxed),
            Some("lighting-scan") => {
                monitor.refresh_lighting();
                let lighting = monitor.lighting.clone();
                sh.with(|s| s.lighting = lighting);
            }
            Some("lighting-takeover") => {
                let result = crate::system::take_over_and_off();
                monitor.refresh_lighting();
                monitor.lighting.message =
                    result.unwrap_or_else(|e| format!("Không thể giành quyền: {e}"));
                let lighting = monitor.lighting.clone();
                sh.with(|s| s.lighting = lighting);
            }
            Some("lighting-release") => {
                let result = crate::system::release();
                monitor.refresh_lighting();
                monitor.lighting.message =
                    result.unwrap_or_else(|e| format!("Không thể trả quyền: {e}"));
                let lighting = monitor.lighting.clone();
                sh.with(|s| s.lighting = lighting);
            }
            Some(action) if action.starts_with("lighting|") => {
                let result = crate::system::apply_lighting(action);
                monitor.refresh_lighting();
                monitor.lighting.message =
                    result.unwrap_or_else(|e| format!("Không áp dụng được hiệu ứng: {e}"));
                let lighting = monitor.lighting.clone();
                sh.with(|s| s.lighting = lighting);
            }
            _ => {}
        }
        let due = last_check.map_or(true, |t| t.elapsed() > Duration::from_secs(6 * 3600));
        if due && load_cfg().check_updates {
            last_check = Some(Instant::now());
            let s = sh.clone();
            thread::spawn(move || do_check(&s));
        }
        // Làm mới dấu thời gian để giao diện biết tiến trình nền còn sống.
        if last_pub.elapsed() >= Duration::from_secs(2) {
            let telemetry = monitor.sample();
            if last_lighting.elapsed() >= Duration::from_secs(10) {
                monitor.refresh_lighting();
                last_lighting = Instant::now();
            }
            let lighting = monitor.lighting.clone();
            sh.with(|s| {
                s.telemetry = telemetry;
                s.lighting = lighting;
            });
            last_pub = Instant::now();
        }
    }
}

fn do_check(sh: &Arc<Shared>) {
    let repo = effective_repo(&load_cfg());
    sh.with(|s| s.update.status = "Đang kiểm tra cập nhật...".into());
    match update::check(&repo, env!("CARGO_PKG_VERSION")) {
        Ok(Some(found)) => {
            let f = found.clone();
            *sh.found.lock().unwrap() = Some(found);
            sh.with(|s| {
                s.update.available = true;
                s.update.latest = f.latest;
                s.update.notes = f.notes;
                s.update.url = f.url;
                s.update.status = "Có bản mới".into();
            });
        }
        Ok(None) => {
            *sh.found.lock().unwrap() = None;
            sh.with(|s| {
                s.update.available = false;
                s.update.status = "Bạn đang dùng bản mới nhất".into();
            });
        }
        Err(e) => sh.with(|s| s.update.status = e),
    }
}

fn do_update(sh: &Arc<Shared>) {
    if sh.updating.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut found = sh.found.lock().unwrap().clone();
    if found.is_none() {
        do_check(sh);
        found = sh.found.lock().unwrap().clone();
    }
    let Some(found) = found else {
        sh.updating.store(false, Ordering::SeqCst);
        return;
    };
    sh.with(|s| {
        s.update.busy = true;
        s.update.progress = 0.0;
        s.update.status = "Đang tải bản cập nhật...".into();
    });
    let mut last = 0.0f32;
    let launch_path = std::env::current_exe();
    let res = update::download_and_replace(&found, &mut |p| {
        if p - last >= 0.01 || p >= 1.0 {
            last = p;
            sh.with(|s| s.update.progress = p);
        }
    });
    match res.and_then(|_| {
        let exe = launch_path.map_err(|e| e.to_string())?;
        launch_updated(&exe)
    }) {
        Ok(_) => {
            sh.with(|s| {
                s.update.restart = true;
                s.update.status = "Đang khởi động lại".into();
            });
            // Giao diện đọc state.json mỗi giây. Chờ nó nhả khóa trước khi bàn giao daemon.
            let _ = config::acquire_lock("ui.lock", Duration::from_secs(5));
            std::process::exit(0);
        }
        Err(e) => {
            sh.with(|s| {
                s.update.busy = false;
                s.update.status = format!("Cập nhật lỗi: {e}");
            });
            sh.updating.store(false, Ordering::SeqCst);
        }
    }
}

fn launch_updated(exe: &std::path::Path) -> Result<(), String> {
    use windows::{core::HSTRING, Win32::{Foundation::HWND, UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL}}};
    let verb=HSTRING::from("runas");
    let file=HSTRING::from(exe.as_os_str());
    let args=HSTRING::from("--after-update");
    let result=unsafe { ShellExecuteW(HWND(std::ptr::null_mut()),&verb,&file,&args,None,SW_SHOWNORMAL) };
    if result.0 as isize > 32 { Ok(()) } else { Err(format!("Không thể mở bản mới với quyền quản trị (mã {})",result.0 as isize)) }
}

// ---------------- khay hệ thống ----------------

fn make_icon(rgb: (u8, u8, u8)) -> Icon {
    let n = 32usize;
    let img = image::load_from_memory(include_bytes!("../../assets/kanali-logo.png"))
        .expect("logo")
        .resize_exact(n as u32, n as u32, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let mut px = img.into_raw();
    for y in 23..31 {
        for x in 23..31 {
            let dx = x as f32 - 26.5;
            let dy = y as f32 - 26.5;
            if dx * dx + dy * dy < 12.0 {
                let i = (y * n + x) * 4;
                px[i] = rgb.0;
                px[i + 1] = rgb.1;
                px[i + 2] = rgb.2;
                px[i + 3] = 255;
            }
        }
    }
    Icon::from_rgba(px, n as u32, n as u32).expect("tạo icon")
}

pub fn run(_lock: File) {
    let sh = Arc::new(Shared::new());
    {
        let s = sh.clone();
        thread::spawn(move || worker(s));
    }
    {
        let s = sh.clone();
        thread::spawn(move || control(s));
    }

    let event_loop = EventLoopBuilder::new().build();

    let menu = Menu::new();
    let open = MenuItem::new("Mở giao diện", true, None);
    let check = MenuItem::new("Kiểm tra cập nhật", true, None);
    let restore = MenuItem::new("Khôi phục màn hình (phát video gốc)", true, None);
    let quit = MenuItem::new("Thoát (màn hình sẽ tắt)", true, None);
    let _ = menu.append(&open);
    let _ = menu.append(&restore);
    let _ = menu.append(&check);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&quit);

    let icon_ok = make_icon((52, 211, 153));
    let icon_wait = make_icon((251, 191, 36));
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Kanali")
        .with_icon(icon_wait.clone())
        .build()
        .expect("tạo icon khay");

    let mut last_ok = false;
    let mut last_tip = String::new();

    event_loop.run(move |_event, _target, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(300));

        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == *open.id() {
                crate::spawn_self("--ui");
            } else if ev.id == *check.id() {
                config::send_command("check");
            } else if ev.id == *restore.id() {
                // Lối thoát khi một video làm tản đứng hình: về lại video gốc.
                let mut cfg = load_cfg();
                cfg.restore_screen();
                let _ = config::save(&config::config_path(), &cfg);
                config::send_command("apply");
            } else if ev.id == *quit.id() {
                *flow = ControlFlow::Exit;
            }
        }
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::DoubleClick { .. } = ev {
                crate::spawn_self("--ui");
            }
        }

        let (ok, msg, upd) = {
            let g = sh.state.lock().unwrap();
            (g.connected, g.message.clone(), g.update.available)
        };
        if ok != last_ok {
            last_ok = ok;
            let _ = tray.set_icon(Some(if ok {
                icon_ok.clone()
            } else {
                icon_wait.clone()
            }));
        }
        let tip = format!(
            "Kanali — {msg}{}",
            if upd { " (có bản cập nhật)" } else { "" }
        );
        if tip != last_tip {
            let _ = tray.set_tooltip(Some(tip.as_str()));
            last_tip = tip;
        }
    });
}
