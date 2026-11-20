//! Tiến trình nền: icon khay hệ thống, giữ kết nối màn hình (heartbeat),
//! áp dụng cấu hình, kiểm tra và cài cập nhật.
use std::{
    fs::File,
    process::Command,
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
    write_lock: Mutex<()>,
    refresh: AtomicBool,
    updating: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        let state = State { version: env!("CARGO_PKG_VERSION").to_string(), ..State::default() };
        Self {
            state: Mutex::new(state),
            found: Mutex::new(None),
            write_lock: Mutex::new(()),
            refresh: AtomicBool::new(false),
            updating: AtomicBool::new(false),
        }
    }

    /// Sửa trạng thái rồi ghi ra state.json cho giao diện đọc.
    fn with<F: FnOnce(&mut State)>(&self, f: F) {
        let snapshot = {
            let mut g = self.state.lock().unwrap();
            f(&mut g);
            g.updated_ms = config::now_ms();
            g.clone()
        };
        let _w = self.write_lock.lock().unwrap();
        let _ = config::save(&config::state_path(), &snapshot);
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
    loop {
        sh.with(|s| {
            s.connected = false;
            s.message = "Đang kết nối với màn hình...".into();
        });
        if let Err(e) = session(&sh) {
            sh.with(|s| {
                s.connected = false;
                s.message = e;
            });
        }
        thread::sleep(Duration::from_secs(3));
    }
}

fn session(sh: &Arc<Shared>) -> Result<(), String> {
    let path = device::find_device()?;
    let mut f = device::open_device(&path)?;
    let replies = device::init_session(&mut f)?;
    let info = device::parse_info(&replies);
    let files = device::fetch_files(&mut f)?;

    let mut cfg = match config::load_opt::<Config>(&config::config_path()) {
        Some(c) => c,
        None => {
            let c = Config::default();
            let _ = config::save(&config::config_path(), &c);
            c
        }
    };
    device::apply_play(&mut f, &cfg)?;
    sh.with(|s| {
        s.connected = true;
        s.message = "Đã kết nối".into();
        s.info = Some(info);
        s.files = files;
    });

    let mut last_hb = Instant::now();
    loop {
        thread::sleep(Duration::from_millis(400));
        if sh.refresh.swap(false, Ordering::Relaxed) {
            let files = device::fetch_files(&mut f)?;
            sh.with(|s| s.files = files);
        }
        if let Some(new) = config::load_opt::<Config>(&config::config_path()) {
            if new.playing != cfg.playing
                || new.poweron != cfg.poweron
                || new.standby != cfg.standby
                || new.level != cfg.level
            {
                device::apply_play(&mut f, &new)?;
            }
            cfg = new;
        }
        if last_hb.elapsed() >= Duration::from_secs(2) {
            device::heartbeat(&mut f)?;
            last_hb = Instant::now();
        }
    }
}

// ---------------- lệnh từ giao diện + cập nhật ----------------

fn control(sh: Arc<Shared>) {
    let mut last_check: Option<Instant> = None;
    let mut last_pub = Instant::now();
    loop {
        thread::sleep(Duration::from_millis(500));
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
            Some("refresh") => sh.refresh.store(true, Ordering::Relaxed),
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
            sh.with(|_| {});
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
    let res = update::download_and_replace(&found, &mut |p| {
        if p - last >= 0.01 || p >= 1.0 {
            last = p;
            sh.with(|s| s.update.progress = p);
        }
    });
    match res {
        Ok(()) => {
            sh.with(|s| s.update.status = "Đã cập nhật, đang khởi động lại...".into());
            thread::sleep(Duration::from_millis(800));
            if let Ok(exe) = std::env::current_exe() {
                let _ = Command::new(exe).arg("--after-update").spawn();
            }
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

// ---------------- khay hệ thống ----------------

fn make_icon(rgb: (u8, u8, u8)) -> Icon {
    let n = 32usize;
    let mut px = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let d = (dx * dx + dy * dy).sqrt();
            if d < 5.0 {
                px.extend_from_slice(&[255, 255, 255, 255]);
            } else if d < 14.0 {
                px.extend_from_slice(&[rgb.0, rgb.1, rgb.2, 255]);
            } else if d < 15.0 {
                px.extend_from_slice(&[rgb.0, rgb.1, rgb.2, ((15.0 - d) * 255.0) as u8]);
            } else {
                px.extend_from_slice(&[0, 0, 0, 0]);
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
    let quit = MenuItem::new("Thoát (màn hình sẽ tắt)", true, None);
    let _ = menu.append(&open);
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
            let _ = tray.set_icon(Some(if ok { icon_ok.clone() } else { icon_wait.clone() }));
        }
        let tip = format!("Kanali — {msg}{}", if upd { " (có bản cập nhật)" } else { "" });
        if tip != last_tip {
            let _ = tray.set_tooltip(Some(tip.as_str()));
            last_tip = tip;
        }
    });
}
