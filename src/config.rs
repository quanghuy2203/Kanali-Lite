//! Cấu hình, trạng thái và lệnh trao đổi giữa tiến trình nền và giao diện (qua file JSON).
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const FACTORY_VIDEO: &str = "default_01.mp4.h264_2240x1080";
pub const DOWNLOAD_46: &str = "download_46.mp4.h264_2240x1080";
pub const DEFAULT_POWERON: &str = "default_poweron.mp4.h264_2240x1080";
pub const DEFAULT_STANDBY: &str = "default_standby.mp4.h264_2240x1080";

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let d = base.join("kanali-rs");
    let _ = fs::create_dir_all(&d);
    d
}
pub fn config_path() -> PathBuf {
    data_dir().join("config.json")
}
pub fn state_path() -> PathBuf {
    data_dir().join("state.json")
}
fn cmd_path() -> PathBuf {
    data_dir().join("cmd.json")
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Layout {
    pub split: bool,
    pub left: String,
    pub right: String,
    pub loop_mode: u64,
    pub enabled: bool,
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            split: false,
            left: String::new(),
            right: String::new(),
            loop_mode: 0,
            enabled: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Config {
    pub layout: Layout,
    pub playing: String,
    pub poweron: String,
    pub standby: String,
    pub level: u64,
    pub autostart: bool,
    pub check_updates: bool,
    /// Kho GitHub dạng "owner/repo". Để trống thì dùng giá trị gắn lúc build.
    pub repo: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            layout: Layout::default(),
            playing: FACTORY_VIDEO.into(),
            poweron: DEFAULT_POWERON.into(),
            standby: DEFAULT_STANDBY.into(),
            level: 67,
            autostart: false,
            check_updates: true,
            repo: String::new(),
        }
    }
}

impl Config {
    pub fn restore_screen(&mut self) {
        self.playing = FACTORY_VIDEO.into();
        self.layout = Layout::default();
        self.level = self.level.clamp(67, 100);
        self.poweron = DEFAULT_POWERON.into();
        self.standby = DEFAULT_STANDBY.into();
    }
}

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(default)]
pub struct DeviceInfo {
    pub os: String,
    pub kernel: String,
    pub model: String,
    pub firmware: String,
    pub serial: String,
    pub resolution: String,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(default)]
pub struct UpdateInfo {
    pub available: bool,
    pub latest: String,
    pub notes: String,
    pub url: String,
    pub status: String,
    pub progress: f32,
    pub busy: bool,
    pub restart: bool,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(default)]
pub struct State {
    pub version: String,
    pub connected: bool,
    pub message: String,
    pub info: Option<DeviceInfo>,
    pub files: Vec<String>,
    pub update: UpdateInfo,
    pub updated_ms: u64,
    pub media_status: String,
    pub telemetry: Telemetry,
    pub lighting: LightingInfo,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(default)]
pub struct Telemetry {
    pub cpu: u8,
    pub memory: u8,
    pub gpu: u8,
    pub gpu_temp: Option<u32>,
    pub gpu_power_w: Option<f32>,
    pub memory_used_gb: f32,
    pub memory_total_gb: f32,
    pub gpu_memory_used_gb: Option<f32>,
    pub gpu_memory_total_gb: Option<f32>,
    pub gpu_clock_mhz: Option<u32>,
    pub gpu_memory_clock_mhz: Option<u32>,
    pub gpu_fan: Option<u32>,
    pub memory_available_gb: f32,
    pub cpu_threads: usize,
    pub uptime_seconds: u64,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(default)]
pub struct LightingInfo {
    pub ram: String,
    pub gpu: String,
    pub lconnect_running: bool,
    pub openrgb_available: bool,
    pub exclusive: bool,
    pub message: String,
}

pub fn load_opt<T: DeserializeOwned>(path: &Path) -> Option<T> {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}

pub fn save<T: Serialize>(path: &Path, v: &T) -> io::Result<()> {
    let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(Path::new(".")))?;
    serde_json::to_writer_pretty(tmp.as_file_mut(), v).map_err(io::Error::other)?;
    tmp.persist(path).map(|_| ()).map_err(|e| e.error)
}

#[derive(Serialize, Deserialize)]
struct Cmd {
    action: String,
}

/// Giao diện gửi lệnh ("check", "update", "refresh") cho tiến trình nền.
pub fn send_command(action: &str) {
    let _ = save(
        &cmd_path(),
        &Cmd {
            action: action.to_string(),
        },
    );
}

pub fn take_command() -> Option<String> {
    let p = cmd_path();
    let c: Cmd = load_opt(&p)?;
    let _ = fs::remove_file(&p);
    Some(c.action)
}

/// Khóa độc quyền theo tên: giữ File này sống thì tiến trình khác không lấy được.
pub fn acquire_lock(name: &str, wait: Duration) -> Option<File> {
    let p = data_dir().join(name);
    let start = Instant::now();
    loop {
        match OpenOptions::new()
            .write(true)
            .create(true)
            .share_mode(0)
            .open(&p)
        {
            Ok(f) => return Some(f),
            Err(_) if start.elapsed() < wait => thread::sleep(Duration::from_millis(200)),
            Err(_) => return None,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Removal {
    pub path: String,
    pub requested_ms: u64,
    pub serial: String,
}

pub fn request_removal(path: String, serial: String) -> Result<(), String> {
    let dir = data_dir();
    let tmp = tempfile::NamedTempFile::new_in(&dir).map_err(|e| e.to_string())?;
    serde_json::to_writer(
        tmp.as_file(),
        &Removal {
            path,
            serial,
            requested_ms: now_ms(),
        },
    )
    .map_err(|e| e.to_string())?;
    tmp.persist_noclobber(dir.join("remove-request.json"))
        .map_err(|_| "Đang có một yêu cầu xóa chờ xử lý")?;
    Ok(())
}

/// Ghi nhật ký vào %APPDATA%\kanali-rs\kanali.log (tự xoay vòng khi quá 256 KB).
pub fn log(msg: &str) {
    use std::io::Write;
    let p = data_dir().join("kanali.log");
    if fs::metadata(&p)
        .map(|m| m.len() > 256 * 1024)
        .unwrap_or(false)
    {
        let _ = fs::rename(&p, data_dir().join("kanali.log.1"));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{} {}", now_ms(), msg);
    }
}
