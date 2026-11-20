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
    send(f, &proto::msg_list_files())?;
    let r = read_frame(f)?;
    let paths = proto::find_paths(&r);
    send(f, &proto::msg_commit())?;
    Ok(paths)
}

pub fn apply_play(f: &mut File, cfg: &Config) -> Result<(), String> {
    send(f, &proto::msg_play(&cfg.poweron, &cfg.standby, &cfg.playing, cfg.level))?;
    send(f, &proto::msg_commit())
}

pub fn heartbeat(f: &mut File) -> Result<(), String> {
    send(f, &proto::msg_hello())?;
    read_frame(f).map(|_| ())
}
