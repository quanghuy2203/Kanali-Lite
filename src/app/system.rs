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
            memory_used_gb: (mem.ullTotalPhys.saturating_sub(mem.ullAvailPhys)) as f32
                / 1_073_741_824.0,
            memory_total_gb: mem.ullTotalPhys as f32 / 1_073_741_824.0,
            memory_available_gb: mem.ullAvailPhys as f32 / 1_073_741_824.0,
            cpu_threads: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(0),
            uptime_seconds: unsafe { windows::Win32::System::SystemInformation::GetTickCount64() }
                / 1000,
            ..Default::default()
        };
        if let Some(nv) = &self.nvml {
            use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
            if let Ok(g) = nv.device_by_index(0) {
                out.gpu = g
                    .utilization_rates()
                    .map(|v| v.gpu.min(100) as u8)
                    .unwrap_or(0);
                out.gpu_temp = g.temperature(TemperatureSensor::Gpu).ok();
                out.gpu_power_w = g.power_usage().ok().map(|v| v as f32 / 1000.0);
                if let Ok(m) = g.memory_info() {
                    out.gpu_memory_used_gb = Some(m.used as f32 / 1_073_741_824.0);
                    out.gpu_memory_total_gb = Some(m.total as f32 / 1_073_741_824.0);
                }
                out.gpu_clock_mhz = g.clock_info(Clock::Graphics).ok();
                out.gpu_memory_clock_mhz = g.clock_info(Clock::Memory).ok();
                out.gpu_fan = g.fan_speed(0).ok();
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
    let watcher_was_running = service_running("LConnectServiceWatcher");
    let service_was_running = service_running("LConnectService");
    if watcher_was_running {
        sc("stop", "LConnectServiceWatcher")?;
    }
    if service_was_running {
        sc("stop", "LConnectService")?;
    }
    let _ = Command::new("taskkill.exe")
        .args(["/IM", "OpenRGB.exe", "/F"])
        .creation_flags(HIDDEN)
        .output();
    std::thread::sleep(std::time::Duration::from_millis(700));

    // Never issue a global OpenRGB command: L-Connect exposes fan controllers
    // too. Select only the Zotac controller, then immediately return fan control.
    let mut child = Command::new(OPENRGB)
        .args([
            "--noautoconnect",
            "--device",
            "ZOTAC",
            "--color",
            "000000",
            "--brightness",
            "0",
        ])
        .creation_flags(HIDDEN)
        .spawn()
        .map_err(|e| format!("Không mở được OpenRGB: {e}"))?;
    let mut gpu_ok = false;
    for _ in 0..40 {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            gpu_ok = status.success();
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let _ = child.kill();

    // Apply the DIMM command while L-Connect/OpenRGB are still stopped.  Both
    // backends touch SMBus; restarting L-Connect first can reclaim the bus and
    // make Lexar's Apply silently do nothing.
    let ram = lexar_ram_off();
    if service_was_running {
        let _ = sc("start", "LConnectService");
    }
    if watcher_was_running {
        let _ = sc("start", "LConnectServiceWatcher");
    }

    match (gpu_ok, ram) {
        (true, Ok(())) => Ok(
            "Đã tắt LED Zotac GPU và gửi lệnh tắt RAM Lexar; L-Connect đã được mở lại cho quạt"
                .into(),
        ),
        (true, Err(e)) => Err(format!("GPU đã tắt, RAM Lexar chưa tắt: {e}")),
        (false, Ok(())) => Err("RAM đã nhận lệnh, nhưng OpenRGB không tìm thấy Zotac GPU".into()),
        (false, Err(e)) => Err(format!("Không tắt được GPU hoặc RAM: {e}")),
    }
}

fn lexar_ram_off() -> Result<(), String> {
    let bridge = ensure_lexar_bridge()?;
    let config =
        std::path::PathBuf::from(std::env::var_os("APPDATA").ok_or("Không tìm thấy APPDATA")?)
            .join("Lexar RGB Sync")
            .join("config");
    let exe = bridge.join("Gaming.exe");

    for name in ["Gaming.exe", "hid.exe"] {
        let _ = Command::new("taskkill.exe")
            .args(["/IM", name, "/F"])
            .creation_flags(HIDDEN)
            .output();
    }
    // A first launch creates the vendor protocol configuration and discovers
    // the physical DIMMs.  The bridge lives under Kanali, not Program Files.
    if !config.is_file() {
        Command::new(&exe)
            .current_dir(&bridge)
            .creation_flags(HIDDEN)
            .spawn()
            .map_err(|e| format!("Không khởi tạo được bridge RAM: {e}"))?;
        for _ in 0..80 {
            if config.is_file() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    let mut text = std::fs::read_to_string(&config)
        .map_err(|e| format!("Bridge RAM chưa nhận được DIMM Lexar: {e}"))?;
    for (key, value) in [("led_sw", "1"), ("audio_sw", "0"), ("sync_sw", "0")] {
        let prefix = format!("{key}=");
        if let Some(line) = text.lines().find(|line| line.starts_with(&prefix)) {
            text = text.replacen(line, &format!("{prefix}{value}"), 1);
        }
    }
    let backup = config.with_extension("kanali-backup");
    if !backup.exists() {
        std::fs::copy(&config, &backup).map_err(|e| e.to_string())?;
    }
    std::fs::write(&config, text).map_err(|e| e.to_string())?;

    Command::new(&exe)
        .current_dir(&bridge)
        .creation_flags(HIDDEN)
        .spawn()
        .map_err(|e| e.to_string())?;

    // Lexar's own guide requires LED OFF to be committed with Apply.
    use windows::{
        core::HSTRING,
        Win32::{
            Foundation::{LPARAM, WPARAM},
            UI::WindowsAndMessaging::{
                FindWindowW, GetDlgItem, PostMessageW, SendMessageW, BM_CLICK, WM_CLOSE,
            },
        },
    };
    let title = HSTRING::from("Lexar RGB Sync_Config");
    let started = std::time::Instant::now();
    let window = loop {
        if let Ok(hwnd) = unsafe { FindWindowW(None, &title) } {
            break hwnd;
        }
        if started.elapsed() > std::time::Duration::from_secs(12) {
            return Err("Lexar RGB Sync không mở được cửa sổ điều khiển".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    };
    // Invoke the real Win32 controls by stable ID.  Unlike mouse injection,
    // this works while the bridge is hidden/backgrounded and is DPI-safe.
    let click = |id: i32| -> Result<(), String> {
        let button = unsafe { GetDlgItem(window, id) }
            .map_err(|_| format!("Bridge RAM thiếu nút điều khiển {id}"))?;
        unsafe { SendMessageW(button, BM_CLICK, WPARAM(0), LPARAM(0)) };
        Ok(())
    };
    click(18_310_624)?; // LED OFF
    std::thread::sleep(std::time::Duration::from_millis(350));
    click(18_309_376)?; // Apply
    std::thread::sleep(std::time::Duration::from_secs(2));
    let applied = std::fs::read_to_string(&config)
        .unwrap_or_default()
        .lines()
        .any(|l| l == "led_sw=0");
    let _ = unsafe { PostMessageW(window, WM_CLOSE, WPARAM(0), LPARAM(0)) };
    if applied {
        Ok(())
    } else {
        Err("Lexar RGB Sync không xác nhận trạng thái LED OFF".into())
    }
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Provision only Lexar's signed hardware bridge in Kanali's private data
/// directory.  This deliberately does not require the Lexar desktop app to be
/// installed or left running.  The original, user-downloaded package remains
/// the source of the proprietary files; Kanali does not redistribute them.
fn ensure_lexar_bridge() -> Result<std::path::PathBuf, String> {
    let local = std::path::PathBuf::from(
        std::env::var_os("LOCALAPPDATA").ok_or("Không tìm thấy LOCALAPPDATA")?,
    );
    let bridge = local.join("Kanali").join("vendor").join("lexar");
    if bridge.join("Gaming.exe").is_file() && bridge.join("hid.exe").is_file() {
        return Ok(bridge);
    }

    // Development/recovery extraction created while inspecting the official
    // package.  It is also useful after the user uninstalls the desktop app.
    let extracted = std::env::temp_dir()
        .join("kanali-lexar-payload")
        .join("app");
    if extracted.join("Gaming.exe").is_file() {
        copy_tree(&extracted, &bridge)?;
    } else {
        let home = std::path::PathBuf::from(
            std::env::var_os("USERPROFILE").ok_or("Không tìm thấy USERPROFILE")?,
        );
        let package = home
            .join("Downloads")
            .join("Lexar RGB Sync for Windows.zip");
        if !package.is_file() {
            return Err(
                "Không tìm thấy gói chính hãng 'Lexar RGB Sync for Windows.zip' trong Downloads"
                    .into(),
            );
        }
        let unpack = std::env::temp_dir().join("kanali-lexar-package");
        let _ = std::fs::remove_dir_all(&unpack);
        std::fs::create_dir_all(&unpack).map_err(|e| e.to_string())?;
        let status = Command::new("tar.exe")
            .args(["-xf"])
            .arg(&package)
            .arg("-C")
            .arg(&unpack)
            .creation_flags(HIDDEN)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Không giải nén được gói Lexar chính hãng".into());
        }
        let installer = unpack.join("Lexar RGB Sync for Windows.exe");
        let inno = Command::new("where.exe")
            .arg("innoextract.exe")
            .creation_flags(HIDDEN)
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.lines().next().map(std::path::PathBuf::from))
            .ok_or(
                "Thiếu bộ giải nén bridge; hãy giữ bản Kanali đã cài thành phần điều khiển RAM",
            )?;
        let payload = std::env::temp_dir().join("kanali-lexar-payload");
        let status = Command::new(inno)
            .args(["-e", "-d"])
            .arg(&payload)
            .arg(installer)
            .creation_flags(HIDDEN)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Không trích xuất được bridge Lexar chính hãng".into());
        }
        copy_tree(&payload.join("app"), &bridge)?;
    }

    let aac = bridge.join("Aac").join("AacSetupAIO.exe");
    let hal = std::path::Path::new(r"C:\Program Files\ENE\Aac_ENE_DRAM_RGB_AIO\x64\AacHal_x64.dll");
    if !hal.is_file() {
        let status = Command::new(&aac)
            .args(["/install", "/quiet", "/norestart"])
            .creation_flags(HIDDEN)
            .status()
            .map_err(|e| format!("Không cài được driver ENE: {e}"))?;
        if !status.success() || !hal.is_file() {
            return Err("Driver ENE không cài được; Kanali cần chạy bằng quyền quản trị".into());
        }
    }
    Ok(bridge)
}

pub fn release() -> Result<String, String> {
    sc("start", "LConnectService")?;
    sc("start", "LConnectServiceWatcher")?;
    Ok("Đã trả quyền điều khiển cho L-Connect".into())
}

#[cfg(test)]
mod rgb_tests {
    #[test]
    #[ignore = "changes the physical Lexar DIMM lighting"]
    fn lexar_bridge_turns_off_detected_dimms() {
        super::lexar_ram_off().expect("Lexar RAM bridge failed");
    }
}
