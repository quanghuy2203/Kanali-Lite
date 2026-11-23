//! Giao tiếp USB với màn hình Tryx (USB Printer Class, VID 391A / PID 1061).
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    sync::mpsc,
    thread,
    time::Duration,
};

use windows::{
    core::{GUID, PCWSTR},
    Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_Device_Interface_ListW, CM_Get_Device_Interface_List_SizeW,
        CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CR_SUCCESS,
    },
};

use crate::{
    config::{now_ms, Config, DeviceInfo},
    proto,
};

/// GUID_DEVINTERFACE_USBPRINT
const GUID_USBPRINT: GUID = GUID::from_u128(0x28d78fad_5a12_11d1_ae5b_0000f803a8c2);
pub const VID_PID: &str = "vid_391a&pid_1061";
const READ_TIMEOUT: Duration = Duration::from_secs(4);

pub fn list_interfaces() -> Result<Vec<String>, String> {
    // SAFETY: các lời gọi CM_* chỉ ghi vào `len` và `buf` do ta sở hữu,
    // `buf` có đúng kích thước mà hàm Size trả về.
    unsafe {
        let mut len = 0u32;
        let r = CM_Get_Device_Interface_List_SizeW(
            &mut len,
            &GUID_USBPRINT,
            PCWSTR::null(),
            CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
        );
        if r != CR_SUCCESS {
            return Err(format!("CM_Get_Device_Interface_List_SizeW lỗi: {}", r.0));
        }
        let mut buf = vec![0u16; len as usize];
        let r = CM_Get_Device_Interface_ListW(
            &GUID_USBPRINT,
            PCWSTR::null(),
            &mut buf,
            CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
        );
        if r != CR_SUCCESS {
            return Err(format!("CM_Get_Device_Interface_ListW lỗi: {}", r.0));
        }
        Ok(buf
            .split(|&c| c == 0)
            .filter(|s| !s.is_empty())
            .map(String::from_utf16_lossy)
            .collect())
    }
}

pub fn find_device() -> Result<String, String> {
    list_interfaces()?
        .into_iter()
        .find(|p| p.to_lowercase().contains(VID_PID))
        .ok_or_else(|| "Không thấy màn hình. Hãy kiểm tra cáp USB và nguồn.".to_string())
}

pub fn open_device(path: &str) -> Result<File, String> {
    OpenOptions::new().read(true).write(true).open(path).map_err(|e| {
        format!(
            "Không mở được thiết bị: {e} (mã {:?}). Hãy tắt Kanali gốc rồi thử lại.",
            e.raw_os_error()
        )
    })
}

/// Đọc một khung TRYX từ bulk IN, có timeout (đọc ở luồng riêng để không treo).
pub fn read_frame(f: &File) -> Result<Vec<u8>, String> {
    let mut f = f.try_clone().map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::channel::<io::Result<Vec<u8>>>();
    let reader = thread::spawn(move || {
        let r = (|| {
            let mut h = [0u8; 8];
            f.read_exact(&mut h)?;
            if &h[..4] != b"TRYX" {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "thiếu tiêu đề TRYX"));
            }
            let n = u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as usize;
            if n > 4 * 1024 * 1024 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "khung quá lớn"));
            }
            let mut body = vec![0u8; n];
            f.read_exact(&mut body)?;
            Ok(body)
        })();
        let _ = tx.send(r);
    });
    match rx.recv_timeout(READ_TIMEOUT) {
        Ok(Ok(b)) => Ok(b),
        Ok(Err(e)) => Err(format!("Đọc lỗi: {e}")),
        Err(_) => {
            use std::os::windows::io::AsRawHandle;
            use windows::Win32::{Foundation::HANDLE, System::IO::CancelSynchronousIo};
            // A detached read could consume the acknowledgement for the next command.
            let start = std::time::Instant::now();
            while !reader.is_finished() && start.elapsed() < Duration::from_millis(500) {
                // SAFETY: the JoinHandle owns the thread handle until this loop ends.
                let _ = unsafe { CancelSynchronousIo(HANDLE(reader.as_raw_handle())) };
                thread::sleep(Duration::from_millis(10));
            }
            Err("Hết thời gian chờ phản hồi từ màn hình".into())
        }
    }
}

pub fn send(f: &mut File, payload: &[u8]) -> Result<(), String> {
    f.write_all(&proto::frame(payload)).map_err(|e| format!("Ghi lỗi: {e}"))?;
    f.flush().map_err(|e| format!("Flush lỗi: {e}"))
}

/// Chuỗi khởi tạo giống Kanali gốc. Trả về các phản hồi theo thứ tự đến.
pub fn init_session(f: &mut File) -> Result<Vec<Vec<u8>>, String> {
    let mut replies = Vec::new();
    for m in [proto::msg_handshake(), proto::msg_hwconfig(), proto::msg_102()] {
        send(f, &m)?;
    }
    for _ in 0..3 {
        replies.push(read_frame(f)?);
    }
    send(f, &proto::msg_101())?;
    replies.push(read_frame(f)?);
    send(f, &proto::msg_time(now_ms()))?;
    replies.push(read_frame(f)?);
    Ok(replies)
}

pub fn parse_info(replies: &[Vec<u8>]) -> DeviceInfo {
    let mut info = DeviceInfo::default();
    for r in replies {
        let Some(top) = proto::parse(r) else { continue };
        if let Some(m) = proto::find_bytes(&top, 500).and_then(proto::parse) {
            info.os = proto::find_str(&m, 1);
            info.kernel = proto::find_str(&m, 3);
            info.model = proto::find_str(&m, 4);
            info.firmware = proto::find_str(&m, 5);
            info.serial = proto::find_str(&m, 8);
        }
        if let Some(m) = proto::find_bytes(&top, 502).and_then(proto::parse) {
            if let Some(res) = proto::find_bytes(&m, 10).and_then(proto::parse) {
                if let (Some(w), Some(h)) = (proto::find_int(&res, 1), proto::find_int(&res, 2)) {
                    info.resolution = format!("{w} × {h}");
                }
            }
        }
    }
    info
}

struct CatalogEntry {
    path: String,
    kind: u32,
    writable: bool,
    size: Option<u64>,
}

fn catalog_entries(body: &[u8]) -> Result<Vec<CatalogEntry>, String> {
    let mut entries = Vec::new();
    for (kind, value) in proto::parse(body).ok_or("Thư viện USB không hợp lệ")? {
        if !matches!(kind, 1 | 2) { continue; }
        let proto::Val::Bytes(bytes) = value else { return Err("Sai kiểu mục thư viện".into()); };
        let entry = proto::parse(bytes).ok_or("Không đọc được mục thư viện")?;
        let mut path = proto::find_str(&entry, 1);
        let ext = proto::find_str(&entry, 2);
        if path.is_empty() || path.ends_with('/') || ext.contains(['/', '\\']) {
            return Err("Tên media trong thư viện không hợp lệ".into());
        }
        if !ext.is_empty() {
            let suffix = format!(".{}", ext.trim_start_matches('.'));
            if !path.to_ascii_lowercase().ends_with(&suffix.to_ascii_lowercase()) { path.push_str(&suffix); }
        }
        if !path.contains('/') {
            let folder = if kind == 1 { "media" } else { "preset" };
            path = format!("/mnt/data/{folder}/{path}");
        }
        entries.push(CatalogEntry { path, kind, size: proto::find_int(&entry, 3), writable: proto::find_int(&entry,4).unwrap_or(0) == 0 });
    }
    Ok(entries)
}

pub fn fetch_files(f: &mut File) -> Result<Vec<String>, String> {
    let catalog = tracked_reply(f, 103, &proto::f_str(1, "N/A"), 503)?;
    match catalog_entries(&catalog) {
        Ok(entries) => Ok(entries.into_iter().map(|e| e.path).collect()),
        // Một mục lạ không được làm hỏng cả phiên: mỗi lần kết nối lại là một lần
        // khởi tạo màn hình. Dùng cách đọc lỏng để vẫn lấy được danh sách.
        Err(e) => {
            crate::config::log(&format!("danh mục USB lạ, dùng cách đọc lỏng: {e}"));
            Ok(proto::find_paths(&catalog))
        }
    }
}

pub fn verify_upload(f: &mut File, name: &str, expected_size: u64) -> Result<Vec<String>, String> {
    let mut detail = String::new();
    for delay in [100, 300, 1000] {
        thread::sleep(Duration::from_millis(delay));
        let catalog = tracked_reply(f, 103, &proto::f_str(1, "N/A"), 503)?;
        let entries = catalog_entries(&catalog)?;
        if let Some(entry) = entries.iter().find(|e| e.kind == 1 && e.path.rsplit('/').next() == Some(name)) {
            if entry.size.is_none() || entry.size == Some(expected_size) {
                return Ok(entries.into_iter().map(|e| e.path).collect());
            }
            detail = format!(" ({} / {expected_size} byte)", entry.size.unwrap_or(0));
        }
    }
    Err(format!("PANORAMA chưa nhận đủ tệp {name}{detail}"))
}

pub fn apply_play(f: &mut File, cfg: &Config) -> Result<(), String> {
    // File đánh dấu %APPDATA%\kanali-rs\simple-play: chỉ gửi chuỗi lệnh phát tối giản (200 + 201).
    if crate::config::data_dir().join("simple-play").exists() {
        let result = legacy_play(f, cfg);
        crate::config::log(&format!("phát (tối giản) {}: {}", cfg.playing, match &result { Ok(()) => "ok".to_string(), Err(e) => e.clone() }));
        return result;
    }
    let mut trace = format!("Kanali Lite {} · {} ms\nVideo: {} · split={}\n",
        env!("CARGO_PKG_VERSION"), now_ms(), cfg.playing, cfg.layout.split);
    let mut pending = None;
    let result = switch_video(cfg, &mut trace, |field, body| {
        let id = send_tracked(f, field, body)?;
        if field == 104 {
            receive_reply(f, id, 504, pending)
        } else {
            pending = Some(id);
            Ok(Vec::new())
        }
    });
    // Firmware không trả lời lệnh đọc cấu hình (104): dùng cách phát tối giản
    // (chuỗi 200 + 201 đã chạy được trên PANORAMA thật) thay vì bỏ màn hình giữa chừng.
    let result = match result {
        Err(e) if e.starts_with("Đọc cấu hình trước khi lưu") => {
            trace.push_str(&format!("{e}\nDùng phương án phát tối giản.\n"));
            legacy_play(f, cfg)
        }
        other => other,
    };
    if let Err(e) = &result { trace.push_str(e); }
    crate::config::log(&format!("phát {}: {}", cfg.playing, match &result { Ok(()) => "ok".to_string(), Err(e) => e.clone() }));
    let _ = crate::config::save(&crate::config::data_dir().join("last-playback.json"), &trace);
    result
}

fn legacy_play(f: &mut File, cfg: &Config) -> Result<(), String> {
    send(f, &proto::configured_play(&cfg.poweron, &cfg.standby, &cfg.playing, cfg.level, &cfg.layout))?;
    send(f, &proto::msg_commit())
}

fn switch_video(cfg: &Config, trace: &mut String,
    mut command: impl FnMut(u32, &[u8]) -> Result<Vec<u8>, String>) -> Result<(), String> {
    let current = command(104, &[]).map_err(|e| format!("Đọc cấu hình trước khi lưu: {e}"))?;
    trace.push_str(&format!("Trước khi lưu: {}\n", playback_summary(&current)));
    let body = proto::update_playback_config(&current, cfg).ok_or("Cấu hình thiết bị không hợp lệ")?;
    command(200, &body).map_err(|e| format!("Ghi cấu hình: {e}"))?;
    // A query is a barrier even on firmware that omits the write acknowledgement.
    let stored = command(104, &[]).map_err(|e| format!("Xác nhận cấu hình đã ghi: {e}"))?;
    trace.push_str(&format!("Sau lệnh 200: {}\n", playback_summary(&stored)));
    // Firmware may expose the new media only after it processes the run configuration.
    command(201, &[])
        .map_err(|e| format!("Kích hoạt màn hình: {e}"))?;
    for wait in [0, 150, 500] {
        if wait != 0 { thread::sleep(Duration::from_millis(wait)); }
        let after = command(104, &[]).map_err(|e| format!("Kiểm tra sau khi kích hoạt: {e}"))?;
        trace.push_str(&format!("Sau lệnh 201: {}\n", playback_summary(&after)));
        if matches_playback(&after, cfg) { return Ok(()); }
    }
    Err("Cấu hình tản trả về khác cấu hình đã chọn. Xem chẩn đoán phát.".into())
}

fn playback_summary(body: &[u8]) -> String {
    let fields = proto::parse(body).unwrap_or_default();
    let work = proto::find_bytes(&fields, 3).and_then(proto::parse).unwrap_or_default();
    let display = proto::find_bytes(&fields, 5).and_then(proto::parse).unwrap_or_default();
    format!("mode={} loop={} video={} left={} right={} backlight={} brightness={}",
        proto::find_int(&work,1).unwrap_or(0),proto::find_int(&work,2).unwrap_or(0),
        proto::find_str(&work,3),proto::find_str(&work,4),proto::find_str(&work,5),
        proto::find_int(&display,1).unwrap_or(0),proto::find_int(&display,2).unwrap_or(0))
}

fn matches_playback(body: &[u8], cfg: &Config) -> bool {
    let Some(fields) = proto::parse(body) else { return false; };
    if let Some(display) = proto::find_bytes(&fields, 5).and_then(proto::parse) {
        if (proto::find_int(&display, 1).unwrap_or(0) != 0) != cfg.layout.enabled
            || proto::find_int(&display, 2).unwrap_or(0) != cfg.level { return false; }
    }
    let Some(work) = proto::find_bytes(&fields, 3).and_then(proto::parse) else { return false; };
    let expected_loop = if cfg.layout.split { 0 } else { cfg.layout.loop_mode.min(2) };
    if proto::find_int(&work, 2).unwrap_or(0) != expected_loop { return false; }
    let matches = |field, name: &str| proto::find_str(&work, field).rsplit('/').next() == Some(name);
    let split = proto::find_int(&work, 1).unwrap_or(0);
    if cfg.layout.split {
        split == 1 && matches(4, &cfg.layout.left) && matches(5, &cfg.layout.right)
    } else {
        split == 0 && matches(3, &cfg.playing)
    }
}

pub fn heartbeat(f: &mut File) -> Result<(), String> {
    send(f, &proto::msg_hello())?;
    for _ in 0..16 {
        let frame = read_frame(f)?;
        if proto::parse(&frame).is_some_and(|fields| proto::find_bytes(&fields, 10).is_some()) {
            return Ok(());
        }
    }
    Err("Thiếu phản hồi heartbeat từ PANORAMA".into())
}

/// The Panorama printer-class protocol acknowledges each part of one tracked transfer.
pub fn upload(f: &mut File, path: &std::path::Path, name: &str, mut progress: impl FnMut(u64, u64)) -> Result<(), String> {
    let mut input = File::open(path).map_err(|e| e.to_string())?;
    let size = input.metadata().map_err(|e| e.to_string())?.len();
    if !(1..=500_000_000).contains(&size) { return Err("Video phải nhỏ hơn 500 MB".into()); }
    let track = now_ms();
    let mut exchange = |field, body: Vec<u8>| -> Result<(), String> {
        let header = proto::f_bytes(1, &proto::f_varint(2, track));
        send(f, &[header, proto::f_bytes(field, &body)].concat())?;
        for _ in 0..16 {
            let reply = read_frame(f)?;
            let fields = proto::parse(&reply).ok_or("Phản hồi USB không hợp lệ")?;
            let id = proto::find_bytes(&fields, 1).and_then(proto::parse).and_then(|h| proto::find_int(&h, 2));
            if id != Some(track) { continue; }
            if let Some(err) = proto::find_bytes(&fields, 2).and_then(proto::parse) {
                if proto::find_int(&err, 1).unwrap_or(0) != 0 { return Err(proto::find_str(&err, 2)); }
            }
            if let Some(status) = proto::find_bytes(&fields, field + 400).and_then(proto::parse) {
                let code = proto::find_int(&status, 1).unwrap_or(0);
                return if code == 0 { Ok(()) } else { Err(format!("Thiết bị từ chối truyền tệp (mã {code})")) };
            }
        }
        Err("Thiếu xác nhận truyền tệp từ màn hình".into())
    };
    exchange(400, [proto::f_str(1, name), proto::f_varint(2, size)].concat())?;
    let mut buf = vec![0; 256 * 1024];
    let mut done = 0;
    while done < size {
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 { return Err("Tệp nguồn bị rút ngắn".into()); }
        exchange(401, proto::f_bytes(1, &buf[..n]))?;
        done += n as u64;
        progress(done, size);
    }
    exchange(402, proto::f_str(1, "media"))
}

fn tracked_reply(f: &mut File, field: u32, body: &[u8], expected: u32) -> Result<Vec<u8>, String> {
    let id = send_tracked(f, field, body)?;
    receive_reply(f, id, expected, None)
}

fn send_tracked(f: &mut File, field: u32, body: &[u8]) -> Result<u64, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let header = [proto::f_varint(1, 1), proto::f_varint(2, id)].concat();
    send(
        f,
        &[proto::f_bytes(1, &header), proto::f_bytes(field, body)].concat(),
    )?;
    Ok(id)
}

fn receive_reply(f: &mut File, id: u64, expected: u32, pending: Option<u64>) -> Result<Vec<u8>, String> {
    for _ in 0..16 {
        let raw = read_frame(f)?;
        let fields = proto::parse(&raw).ok_or("Phản hồi USB không hợp lệ")?;
        let reply_id = proto::find_bytes(&fields, 1)
            .and_then(proto::parse)
            .and_then(|h| proto::find_int(&h, 2))
            .unwrap_or(0);
        if reply_id != 0 && reply_id != id && Some(reply_id) != pending {
            continue;
        }
        if let Some(err) = proto::find_bytes(&fields, 2).and_then(proto::parse) {
            if proto::find_int(&err, 1).unwrap_or(0) != 0 {
                return Err(format!("Thiết bị từ chối: {}", proto::find_str(&err, 2)));
            }
        }
        if reply_id != 0 && reply_id != id { continue; }
        if let Some(body) = proto::find_bytes(&fields, expected) {
            return Ok(body.to_vec());
        }
    }
    Err("Thiếu xác nhận từ PANORAMA".into())
}

fn removal_type(catalog: &[u8], path: &str) -> Option<&'static str> {
    let (dir, name) = path.rsplit_once('/')?;
    if name.is_empty() {
        return None;
    }
    let entries = catalog_entries(catalog).ok()?;
    let mut matches = entries.iter().filter(|e| e.path.rsplit('/').next() == Some(name));
    let entry = matches.next()?;
    if matches.next().is_some() || entry.path != path || !entry.writable { return None; }
    match entry.kind {
        1 if dir.ends_with("/media") || dir.ends_with("/user") => Some("media"),
        2 if name == crate::config::DOWNLOAD_46 => Some("preset"),
        _ => None,
    }
}

pub fn remove_media(f: &mut File, path: &str, cfg: &Config) -> Result<Vec<String>, String> {
    let catalog = tracked_reply(f, 103, &proto::f_str(1, "N/A"), 503)?;
    let file_type = removal_type(&catalog, path)
        .ok_or("Tệp chỉ đọc, trùng tên hoặc không phải media có thể xóa.")?;
    let name = path.rsplit('/').next().unwrap_or("");
    let mut next = cfg.clone();
    if next.playing == name
        || (next.layout.split
            && [&next.layout.left, &next.layout.right]
                .iter()
                .any(|s| s.as_str() == name))
    {
        next.playing = crate::config::FACTORY_VIDEO.into();
        next.layout = Default::default();
        next.level = next.level.clamp(67, 100);
    }
    if next.poweron == name {
        next.poweron = crate::config::DEFAULT_POWERON.into();
    }
    if next.standby == name {
        next.standby = crate::config::DEFAULT_STANDBY.into();
    }
    if next.layout.left == name {
        next.layout.left.clear();
    }
    if next.layout.right == name {
        next.layout.right.clear();
    }
    if next != *cfg
        && !catalog_entries(&catalog)?
            .iter()
            .any(|e| e.path.rsplit('/').next() == Some(next.playing.as_str()))
    {
        return Err("Thiếu video thay thế trong thư viện PANORAMA.".into());
    }
    let actual = tracked_reply(f, 104, &[], 504)?;
    let actual = proto::parse(&actual).ok_or("Không đọc được cấu hình PANORAMA")?;
    let in_use = [(1,1), (2,2), (3,3), (3,4), (3,5), (3,6)].iter().any(|&(group, field)| {
        proto::find_bytes(&actual, group).and_then(proto::parse).is_some_and(|m| {
            proto::find_str(&m, field).rsplit('/').next() == Some(name)
        })
    });
    if in_use || next != *cfg {
        apply_play(f, &next)?;
        crate::config::save(&crate::config::config_path(), &next).map_err(|e| e.to_string())?;
    }
    let removal = [proto::f_str(1, name), proto::f_str(2, file_type)].concat();
    send(f, &[proto::f_bytes(1, &[]), proto::f_bytes(403, &removal)].concat())?;
    // Some firmware sends no ACK for FileRemove. Reconcile without reissuing it.
    for delay in [300, 600, 1500] {
        thread::sleep(Duration::from_millis(delay));
        let files = fetch_files(f)?;
        if !files.iter().any(|p| p == path) { return Ok(files); }
    }
    Err(format!("PANORAMA vẫn báo còn {name} sau lệnh xóa"))
}

#[cfg(test)]
mod removal_tests {
    use super::*;
    #[test]
    fn only_unique_writable_user_media_can_be_removed() {
        let path = "/mnt/data/media/clip.mp4";
        let entry = |kind, readonly| {
            proto::f_bytes(
                kind,
                &[proto::f_str(1, path), proto::f_varint(4, readonly)].concat(),
            )
        };
        assert_eq!(removal_type(&entry(1, 0), path), Some("media"));
        assert_eq!(removal_type(&entry(1, 1), path), None);
        assert_eq!(removal_type(&entry(2, 0), path), None);
        assert_eq!(
            removal_type(&[entry(1, 0), entry(2, 0)].concat(), path),
            None
        );
    }
    #[test]
    fn downloaded_preset_is_removable_only_if_writable() {
        for (name, readonly, expected) in [
            (crate::config::DOWNLOAD_46, 0, Some("preset")),
            (crate::config::DOWNLOAD_46, 1, None),
            (crate::config::FACTORY_VIDEO, 0, None),
        ] {
            let path = format!("/mnt/data/preset/{name}");
            let entry = proto::f_bytes(
                2,
                &[proto::f_str(1, &path), proto::f_varint(4, readonly)].concat(),
            );
            assert_eq!(removal_type(&entry, &path), expected);
        }
    }

    #[test]
    fn catalog_joins_separate_extension_once_for_play_and_delete() {
        for base in ["/userdata/user/test.mp4", "/userdata/user/test.mp4.h264_2240x1080"] {
            let full = "/userdata/user/test.mp4.h264_2240x1080";
            let body = proto::f_bytes(1, &[proto::f_str(1, base), proto::f_str(2, ".h264_2240x1080")].concat());
            assert_eq!(catalog_entries(&body).unwrap()[0].path, full);
            assert_eq!(removal_type(&body, full), Some("media"));
        }
    }

    #[test]
    fn switch_waits_for_device_and_preserves_firmware_fields() {
        let cfg = Config::default();
        let old_display = [proto::f_varint(4, 90), proto::f_varint(18, 12)].concat();
        let old = [proto::f_bytes(3, &proto::f_str(3, "old.mp4")),
            proto::f_bytes(5, &old_display), proto::f_bytes(62, b"vendor")].concat();
        let mut stages = Vec::new();
        let mut stored = Vec::new();
        switch_video(&cfg, &mut String::new(), |field, body| {
            stages.push(field);
            match field {
                200 => {
                    stored = body.to_vec();
                    let top = proto::parse(body).unwrap();
                    assert_eq!(proto::find_bytes(&top,62),Some(&b"vendor"[..]));
                    let display = proto::parse(proto::find_bytes(&top,5).unwrap()).unwrap();
                    assert_eq!(proto::find_int(&display,4),Some(90));
                    assert_eq!(proto::find_int(&display,18),Some(12));
                }
                104 if stages.contains(&201) => return Ok(stored.clone()),
                104 => return Ok(old.clone()),
                _ => (),
            }
            Ok(Vec::new())
        }).unwrap();
        assert_eq!(stages, [104,200,104,201,104]);
    }
    #[test]
    fn rejected_write_never_activates() {
        let mut stages = Vec::new();
        let result = switch_video(&Config::default(), &mut String::new(), |stage, _| {
            stages.push(stage);
            if stages.len() == 3 { return Err("device rejected".into()); }
            Ok(Vec::new())
        });
        assert!(result.unwrap_err().contains("device rejected"));
        assert_eq!(stages,[104,200,104]);
    }

    #[test]
    fn recovery_requires_readback_of_the_factory_video() {
        let mut cfg = Config::default();
        cfg.playing = "black.mp4".into();
        cfg.layout.split = true;
        cfg.layout.enabled = false;
        cfg.level = 0;
        cfg.restore_screen();
        assert!(cfg.layout.enabled && cfg.level > 0);
        let body = |name| proto::f_bytes(3, &proto::f_str(3, name));
        assert!(matches_playback(&body(crate::config::FACTORY_VIDEO), &cfg));
        assert!(!matches_playback(&body("black.mp4"), &cfg));
        assert!(!matches_playback(&[], &cfg));
    }
}
