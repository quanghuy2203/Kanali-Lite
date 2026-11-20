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
    thread::spawn(move || {
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
        Err(_) => Err("Hết thời gian chờ phản hồi từ màn hình".into()),
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

pub fn fetch_files(f: &mut File) -> Result<Vec<String>, String> {
    let catalog = tracked_reply(f, 103, &proto::f_str(1, "N/A"), 503)?;
    Ok(proto::find_paths(&catalog))
}

pub fn apply_play(f: &mut File, cfg: &Config) -> Result<(), String> {
    let request = proto::configured_play(&cfg.poweron, &cfg.standby, &cfg.playing, cfg.level, &cfg.layout);
    let fields = proto::parse(&request).ok_or("Cấu hình phát không hợp lệ")?;
    tracked_reply(f, 200, proto::find_bytes(&fields, 200).ok_or("Thiếu cấu hình phát")?, 600)?;
    send(f, &proto::overlay(&cfg.layout))?;
    let readback = tracked_reply(f, 104, &[], 504)?;
    if !matches_playback(&readback, cfg) {
        return Err("PANORAMA chưa xác nhận video đã chọn. Hãy dùng Khôi phục màn hình.".into());
    }
    Ok(())
}

fn matches_playback(body: &[u8], cfg: &Config) -> bool {
    let Some(work) = proto::parse(body).and_then(|fields| proto::find_bytes(&fields, 3))
        .and_then(proto::parse) else { return false; };
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
    read_frame(f).map(|_| ())
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

fn tracked_reply(f:&mut File, field:u32, body:&[u8], expected:u32)->Result<Vec<u8>,String>{
    use std::sync::atomic::{AtomicU64,Ordering};
    static NEXT:AtomicU64=AtomicU64::new(1);
    let id=NEXT.fetch_add(1,Ordering::Relaxed);
    let header = [proto::f_varint(1, 1), proto::f_varint(2, id)].concat();
    send(f, &[proto::f_bytes(1, &header), proto::f_bytes(field, body)].concat())?;
    for _ in 0..16 {
        let raw=read_frame(f)?;
        let fields=proto::parse(&raw).ok_or("Phản hồi USB không hợp lệ")?;
        if proto::find_bytes(&fields,1).and_then(proto::parse).and_then(|h|proto::find_int(&h,2))!=Some(id){continue;}
        if let Some(err)=proto::find_bytes(&fields,2).and_then(proto::parse){
            if proto::find_int(&err,1).unwrap_or(0)!=0{return Err(format!("Thiết bị từ chối: {}",proto::find_str(&err,2)));}
        }
        if let Some(body)=proto::find_bytes(&fields,expected){return Ok(body.to_vec());}
    }
    Err("Thiếu xác nhận từ PANORAMA".into())
}

fn removal_type(catalog:&[u8],path:&str)->Option<&'static str>{
    let (dir,name)=path.rsplit_once('/')?;
    if name.is_empty(){return None;}
    let fields=proto::parse(catalog)?;
    let mut allowed=None;
    let mut count=0;
    for (kind,val) in fields{
        if let proto::Val::Bytes(bytes)=val{
            if let Some(entry)=proto::parse(bytes){
                let file=proto::find_str(&entry,1);
                if file.rsplit('/').next()==Some(name){
                    count+=1;
                    let user_media = kind == 1 && dir.ends_with("/media");
                    let downloaded = kind == 2 && name == crate::config::DOWNLOAD_46;
                    if file == path && proto::find_int(&entry, 4).unwrap_or(0) == 0 {
                        if user_media { allowed = Some("media"); }
                        if downloaded { allowed = Some("preset"); }
                    }
                }
            }
        }
    }
    allowed.filter(|_| count == 1)
}

pub fn remove_media(f:&mut File,path:&str,cfg:&Config)->Result<Vec<String>,String>{
    let catalog=tracked_reply(f,103,&proto::f_str(1,"N/A"),503)?;
    let file_type = removal_type(&catalog,path).ok_or("Tệp chỉ đọc, trùng tên hoặc không phải media có thể xóa.")?;
    let name=path.rsplit('/').next().unwrap_or("");
    let mut next = cfg.clone();
    if next.playing == name || (next.layout.split && [&next.layout.left, &next.layout.right].iter().any(|s| s.as_str() == name)) {
        next.playing = crate::config::FACTORY_VIDEO.into();
        next.layout = Default::default();
        next.level = next.level.clamp(67, 100);
    }
    if next.poweron == name { next.poweron = crate::config::DEFAULT_POWERON.into(); }
    if next.standby == name { next.standby = crate::config::DEFAULT_STANDBY.into(); }
    if next.layout.left == name { next.layout.left.clear(); }
    if next.layout.right == name { next.layout.right.clear(); }
    if next != *cfg && !proto::find_paths(&catalog).iter().any(|p| p.rsplit('/').next() == Some(next.playing.as_str())) {
        return Err("Thiếu video thay thế trong thư viện PANORAMA.".into());
    }
    // The draft on disk may differ from what the device is actually playing.
    apply_play(f, &next)?;
    if next != *cfg {
        crate::config::save(&crate::config::config_path(), &next).map_err(|e| e.to_string())?;
    }
    tracked_reply(f,403,&[proto::f_str(1,name),proto::f_str(2,file_type)].concat(),600)?;
    let catalog=tracked_reply(f,103,&proto::f_str(1,"N/A"),503)?;
    if proto::parse(&catalog).is_none(){return Err("Chưa xác minh được thư viện sau khi xóa".into());}
    let files=proto::find_paths(&catalog);
    if files.iter().any(|p|p==path){return Err("Thiết bị vẫn còn tệp sau khi xóa".into());}
    Ok(files)
}

#[cfg(test)]
mod removal_tests{
    use super::*;
    #[test]
    fn only_unique_writable_user_media_can_be_removed(){
        let path="/mnt/data/media/clip.mp4";
        let entry=|kind,readonly|proto::f_bytes(kind,&[proto::f_str(1,path),proto::f_varint(4,readonly)].concat());
        assert_eq!(removal_type(&entry(1,0),path), Some("media"));
        assert_eq!(removal_type(&entry(1,1),path), None);
        assert_eq!(removal_type(&entry(2,0),path), None);
        assert_eq!(removal_type(&[entry(1,0),entry(2,0)].concat(),path), None);
    }
    #[test]
    fn downloaded_preset_is_removable_only_if_writable() {
        for (name, readonly, expected) in [
            (crate::config::DOWNLOAD_46, 0, Some("preset")),
            (crate::config::DOWNLOAD_46, 1, None),
            (crate::config::FACTORY_VIDEO, 0, None),
        ] {
            let path = format!("/mnt/data/preset/{name}");
            let entry = proto::f_bytes(2, &[proto::f_str(1, &path), proto::f_varint(4, readonly)].concat());
            assert_eq!(removal_type(&entry, &path), expected);
        }
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
