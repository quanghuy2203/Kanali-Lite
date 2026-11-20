//! kanali-rs: công cụ dòng lệnh cho màn hình Tryx Panorama.
//!
//!   kanali-rs devices                          liệt kê thiết bị USB Printer Class
//!   kanali-rs probe                            thử mở thiết bị
//!   kanali-rs hello                            gửi "hello?" (mong nhận "Hey!")
//!   kanali-rs info                             in thông tin màn hình
//!   kanali-rs files                            liệt kê video trên màn hình
//!   kanali-rs play <khởi động> <chờ> <phát> [độ sáng=67]   (thêm --once để gửi rồi thoát)
//!   kanali-rs keepalive                        giữ kết nối, gửi heartbeat
//!   kanali-rs send <hex>                       ghi byte thô
//! Thêm --path <đường dẫn> để chỉ định thiết bị thủ công.

use std::{
    env,
    fs::File,
    io::{self, Write},
    process::ExitCode,
    thread,
    time::Duration,
};

use kanali_rs::{device, proto};

fn heartbeat_loop(f: &mut File) -> Result<(), String> {
    loop {
        thread::sleep(Duration::from_secs(2));
        device::heartbeat(f)?;
        print!(".");
        let _ = io::stdout().flush();
    }
}

fn play_args(args: &[String]) -> Result<(String, String, String, u64), String> {
    match (args.get(1), args.get(2), args.get(3)) {
        (Some(a), Some(b), Some(c)) => {
            let level = match args.get(4) {
                Some(v) => v.parse().map_err(|_| "Độ sáng phải là số".to_string())?,
                None => 67,
            };
            Ok((a.clone(), b.clone(), c.clone(), level))
        }
        _ => Err("Cú pháp: <lệnh> <khởi động> <chờ> <phát> [độ sáng]".into()),
    }
}

fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    let clean: String = s
        .replace("0x", "")
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != ':')
        .collect();
    if clean.len() % 2 != 0 {
        return Err("Chuỗi hex phải có số ký tự chẵn".into());
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

fn run() -> Result<(), String> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let mut manual = None;
    if let Some(i) = args.iter().position(|a| a == "--path") {
        if i + 1 >= args.len() {
            return Err("--path cần một giá trị".into());
        }
        manual = Some(args[i + 1].clone());
        args.drain(i..=i + 1);
    }
    let once = match args.iter().position(|a| a == "--once") {
        Some(i) => {
            args.remove(i);
            true
        }
        None => false,
    };
    let cmd = args.first().map(String::as_str).unwrap_or("devices");
    let connect = |m: &Option<String>| -> Result<File, String> {
        let path = match m {
            Some(p) => p.clone(),
            None => device::find_device()?,
        };
        device::open_device(&path)
    };

    match cmd {
        "devices" | "list" => {
            for p in device::list_interfaces()? {
                let mark = if p.to_lowercase().contains(device::VID_PID) { "  <== Tryx" } else { "" };
                println!("{p}{mark}");
            }
        }
        "probe" => {
            connect(&manual)?;
            println!("Mở thiết bị: OK");
        }
        "hello" => {
            let mut f = connect(&manual)?;
            device::send(&mut f, &proto::msg_hello())?;
            println!("{}", proto::pretty(&device::read_frame(&f)?));
        }
        "info" => {
            let mut f = connect(&manual)?;
            for r in device::init_session(&mut f)? {
                println!("{}---", proto::pretty(&r));
            }
        }
        "files" => {
            let mut f = connect(&manual)?;
            device::init_session(&mut f)?;
            for p in device::fetch_files(&mut f)? {
                println!("{p}");
            }
        }
        "play" => {
            let (po, sb, pl, level) = play_args(&args)?;
            let mut f = connect(&manual)?;
            device::init_session(&mut f)?;
            device::send(&mut f, &proto::msg_play(&po, &sb, &pl, level))?;
            device::send(&mut f, &proto::msg_commit())?;
            println!("Đã gửi lệnh phát: {pl} (độ sáng {level})");
            if !once {
                println!("Giữ kết nối, heartbeat mỗi 2 giây. Nhấn Ctrl+C để thoát.");
                heartbeat_loop(&mut f)?;
            }
        }
        "keepalive" => {
            let mut f = connect(&manual)?;
            device::init_session(&mut f)?;
            println!("Giữ kết nối, heartbeat mỗi 2 giây. Nhấn Ctrl+C để thoát.");
            heartbeat_loop(&mut f)?;
        }
        "send" => {
            let hex = args.get(1).ok_or("Thiếu chuỗi hex")?;
            let data = parse_hex(hex)?;
            let mut f = connect(&manual)?;
            f.write_all(&data).map_err(|e| format!("Ghi lỗi: {e}"))?;
            println!("Đã ghi {} byte", data.len());
        }
        other => return Err(format!("Lệnh không hợp lệ: {other}")),
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Lỗi: {e}");
            ExitCode::FAILURE
        }
    }
}
