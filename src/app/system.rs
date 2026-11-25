//! Lightweight host telemetry and opt-in RGB ownership helpers.
use kanali_rs::config::{LightingInfo, Telemetry};
use serde::Deserialize;
use std::{os::windows::process::CommandExt, process::Command};
use windows::Win32::{
    Foundation::FILETIME,
    System::{
        SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
        Threading::GetSystemTimes,
    },
};
use wmi::{COMLibrary, WMIConnection};

const HIDDEN: u32 = 0x0800_0000;
const OPENRGB: &str = r"C:\Program Files\Lian-Li\L-Connect 3\OpenRGB\OpenRGB.exe";

pub struct Monitor {
    ticks: Option<(u64, u64)>,
    nvml: Option<nvml_wrapper::Nvml>,
    pub lighting: LightingInfo,
}

impl Monitor {
    pub fn new() -> Self {
        let nvml = nvml_wrapper::Nvml::init().ok();
        Self {
            ticks: None,
            nvml,
            lighting: scan_lighting(),
        }
    }

    pub fn sample(&mut self) -> Telemetry {
        let (mut idle, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let tick = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
        let now = if unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }
            .is_ok()
        {
            Some((tick(idle), tick(kernel).saturating_add(tick(user))))
        } else {
            None
        };
        let cpu = now
            .and_then(|n| {
                self.ticks.replace(n).map(|p| {
                    let total = n.1.saturating_sub(p.1);
                    let busy = total.saturating_sub(n.0.saturating_sub(p.0));
                    if total == 0 {
                        0
                    } else {
                        (busy.saturating_mul(100) / total).min(100) as u8
                    }
                })
            })
            .unwrap_or(0);
        let mut mem = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        let memory = if unsafe { GlobalMemoryStatusEx(&mut mem) }.is_ok() {
            mem.dwMemoryLoad as u8
        } else {
            0
        };
        let mut out = Telemetry {
            cpu,
            memory,
            memory_used_gb:(mem.ullTotalPhys.saturating_sub(mem.ullAvailPhys)) as f32/1_073_741_824.0,
            memory_total_gb:mem.ullTotalPhys as f32/1_073_741_824.0,
            ..Default::default()
        };
        if let Some(nv) = &self.nvml {
            use nvml_wrapper::enum_wrappers::device::{Clock,TemperatureSensor};
            if let Ok(g) = nv.device_by_index(0) {
                out.gpu = g
                    .utilization_rates()
                    .map(|v| v.gpu.min(100) as u8)
                    .unwrap_or(0);
                out.gpu_temp = g.temperature(TemperatureSensor::Gpu).ok();
                out.gpu_power_w = g.power_usage().ok().map(|v| v as f32 / 1000.0);
                if let Ok(m)=g.memory_info(){out.gpu_memory_used_gb=Some(m.used as f32/1_073_741_824.0);out.gpu_memory_total_gb=Some(m.total as f32/1_073_741_824.0);}
                out.gpu_clock_mhz=g.clock_info(Clock::Graphics).ok();
                out.gpu_fan=g.fan_speed(0).ok();
            }
        }
        out
    }

    pub fn refresh_lighting(&mut self) {
        self.lighting = scan_lighting();
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Memory {
    manufacturer: Option<String>,
    part_number: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Video {
    name: Option<String>,
}

fn service_running(name: &str) -> bool {
    Command::new("sc.exe")
        .args(["query", name])
        .creation_flags(HIDDEN)
        .output()
        .ok()
        .is_some_and(|o| String::from_utf8_lossy(&o.stdout).contains("RUNNING"))
}

pub fn scan_lighting() -> LightingInfo {
    let mut info = LightingInfo {
        openrgb_available: std::path::Path::new(OPENRGB).is_file(),
        ..Default::default()
    };
    info.lconnect_running =
        service_running("LConnectService") || service_running("LConnectServiceWatcher");
    info.exclusive = !info.lconnect_running;
    if let Ok(com) = COMLibrary::new() {
        if let Ok(db) = WMIConnection::new(com.into()) {
            if let Ok(rows) =
                db.raw_query::<Memory>("SELECT Manufacturer,PartNumber FROM Win32_PhysicalMemory")
            {
                let mut names: Vec<String> = rows
                    .into_iter()
                    .filter_map(|r| {
                        let m = r.manufacturer.unwrap_or_default().trim().to_string();
                        let p = r.part_number.unwrap_or_default().trim().to_string();
                        (!m.is_empty() || !p.is_empty())
                            .then(|| format!("{m} {p}").trim().to_string())
                    })
                    .collect();
                names.sort();
                names.dedup();
                info.ram = names.join(" · ");
            }
            if let Ok(rows) = db.raw_query::<Video>("SELECT Name FROM Win32_VideoController") {
                info.gpu = rows
                    .into_iter()
                    .filter_map(|r| r.name)
                    .find(|n| n.to_ascii_lowercase().contains("nvidia"))
                    .unwrap_or_default();
            }
        }
    }
    info.message = if info.lconnect_running {
        "L-Connect đang giữ quyền điều khiển RGB"
    } else if info.openrgb_available {
        "Sẵn sàng điều khiển độc quyền"
    } else {
        "Không tìm thấy bộ điều khiển OpenRGB"
    }
    .into();
    info
}

fn sc(action: &str, name: &str) -> Result<(), String> {
    let o = Command::new("sc.exe")
        .args([action, name])
        .creation_flags(HIDDEN)
        .output()
        .map_err(|e| e.to_string())?;
    if o.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr)
            .trim()
            .to_string()
            .if_empty("Cần chạy Kanali bằng quyền quản trị"))
    }
}

trait Empty {
    fn if_empty(self, fallback: &str) -> String;
}
impl Empty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.into()
        } else {
            self
        }
    }
}

pub fn take_over_and_off() -> Result<String, String> {
    if service_running("LConnectServiceWatcher") { sc("stop", "LConnectServiceWatcher")?; }
    if service_running("LConnectService") { sc("stop", "LConnectService")?; }
    // L-Connect leaves its bundled OpenRGB worker alive after its service stops.
    let _=Command::new("taskkill.exe").args(["/IM","OpenRGB.exe","/F"]).creation_flags(HIDDEN).output();
    std::thread::sleep(std::time::Duration::from_millis(700));
    // "off" is not a portable OpenRGB mode. Black + zero brightness works for
    // both Direct and Static controllers and applies to every detected device.
    let mut child = Command::new(OPENRGB)
        .args(["--noautoconnect", "--color", "000000", "--brightness", "0"])
        .creation_flags(HIDDEN)
        .spawn()
        .map_err(|e| format!("Không mở được OpenRGB: {e}"))?;
    for _ in 0..40 {
        if let Some(status)=child.try_wait().map_err(|e|e.to_string())? {
            return if status.success(){Ok("Đã giành quyền và đặt toàn bộ LED về 0% / màu đen".into())}else{Err("OpenRGB không tìm thấy bộ điều khiển tương thích".into())};
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let _=child.kill();
    Ok("Đã gửi lệnh tắt LED; OpenRGB được đóng sau khi áp dụng".into())
}

pub fn release() -> Result<String, String> {
    sc("start", "LConnectService")?;
    sc("start", "LConnectServiceWatcher")?;
    Ok("Đã trả quyền điều khiển cho L-Connect".into())
}
