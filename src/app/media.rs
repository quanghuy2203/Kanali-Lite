//! Convert locally, then atomically queue the encoded stream for the USB worker.
use kanali_rs::config;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

fn dir(name: &str) -> Result<PathBuf, String> {
    let path = config::data_dir().join(name);
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn poster(name: &str) -> PathBuf {
    config::data_dir()
        .join("previews")
        .join(format!("{:x}.rgb", Sha256::digest(name.as_bytes())))
}

fn ffmpeg() -> Command {
    let local = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("ffmpeg.exe")));
    let mut cmd = Command::new(
        local
            .filter(|p| p.is_file())
            .or_else(|| {
                ["ProgramFiles", "ProgramFiles(x86)"]
                    .into_iter()
                    .filter_map(std::env::var_os)
                    .map(|p| PathBuf::from(p).join("KANALI/resources/Connect/ffmpeg/ffmpeg.exe"))
                    .find(|p| p.is_file())
            })
            .unwrap_or_else(|| "ffmpeg.exe".into()),
    );
    cmd.creation_flags(0x08000000)
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"]);
    cmd
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let output = cmd.output().map_err(|e| {
        format!("Không chạy được FFmpeg. Đặt ffmpeg.exe cạnh Kanali hoặc trong PATH. {e}")
    })?;
    if !output.status.success() {
        return Err(format!(
            "FFmpeg: {}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(450)
                .collect::<String>()
        ));
    }
    Ok(())
}

pub fn make_poster(source: &Path, name: &str) -> Result<(), String> {
    let dir = dir("previews")?;
    let tmp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| e.to_string())?
        .into_temp_path();
    run(ffmpeg().arg("-i").arg(source).args(["-t","4","-frames:v","8","-vf","fps=2:round=up,scale=240:116:force_original_aspect_ratio=decrease,pad=240:116:(ow-iw)/2:(oh-ih)/2","-pix_fmt","rgb24","-f","rawvideo"]).arg(&*tmp))?;
    let len = fs::metadata(&tmp).map_err(|e| e.to_string())?.len();
    if len == 0 || len > 8 * 240 * 116 * 3 || len % (240 * 116 * 3) != 0 {
        return Err("Không đọc được hình xem trước".into());
    }
    tmp.persist(poster(name)).map_err(|e| e.to_string())?;
    Ok(())
}

struct Wallpaper {
    engine: PathBuf,
    window: String,
}
impl Drop for Wallpaper {
    fn drop(&mut self) {
        let _ = Command::new(&self.engine)
            .args(["-control", "closeWallpaper", "-location", &self.window])
            .creation_flags(0x08000000)
            .status();
    }
}

fn engine() -> Result<PathBuf, String> {
    super::workshop::libraries()?
        .into_iter()
        .flat_map(|lib| {
            ["wallpaper64.exe", "wallpaper32.exe"]
                .map(|exe| lib.join("common/wallpaper_engine").join(exe))
        })
        .find(|p| p.is_file())
        .ok_or("Chưa tìm thấy Wallpaper Engine trong các thư viện Steam".into())
}

pub fn import(source: PathBuf, project: bool) -> Result<String, String> {
    let mut input = source.clone();
    let mut capture = None;
    let mut label = source
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    if project {
        let json: serde_json::Value =
            serde_json::from_reader(fs::File::open(&source).map_err(|e| e.to_string())?)
                .map_err(|e| format!("project.json: {e}"))?;
        let root = source
            .parent()
            .ok_or("Thiếu thư mục dự án")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let relative = json["file"].as_str().ok_or("Dự án thiếu trường file")?;
        let mut entry = root.join(relative);
        if !entry.exists()
            && json["type"]
                .as_str()
                .is_some_and(|t| t.eq_ignore_ascii_case("web") || t.eq_ignore_ascii_case("scene"))
        {
            entry = root.join("scene.pkg");
        }
        input = entry
            .canonicalize()
            .map_err(|e| format!("Thiếu tệp dự án: {e}"))?;
        if !input.starts_with(&root) {
            return Err("Tệp nằm ngoài thư mục dự án".into());
        }
        if let Some(title) = json["title"].as_str() {
            label = title.into();
        }
        match json["type"]
            .as_str()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "video" => (),
            "scene" | "web" => {
                let engine = engine()?;
                let window = format!("Kanali-Panorama-{}", config::now_ms());
                let result = Command::new(&engine)
                    .args(["-control", "openWallpaper", "-file"])
                    .arg(&input)
                    .args(["-playInWindow", &window, "-width", "1120", "-height", "540"])
                    .creation_flags(0x08000000)
                    .status()
                    .map_err(|e| e.to_string())?;
                if !result.success() {
                    return Err("Wallpaper Engine không mở được dự án".into());
                }
                capture = Some(Wallpaper { engine, window });
                thread::sleep(Duration::from_secs(3));
            }
            _ => return Err("Chỉ hỗ trợ dự án Video, Web và Scene".into()),
        }
    }
    let queue = dir("upload")?;
    let temp = tempfile::NamedTempFile::new_in(&queue)
        .map_err(|e| e.to_string())?
        .into_temp_path();
    let mut cmd = ffmpeg();
    if let Some(w) = &capture {
        cmd.args([
            "-f",
            "gdigrab",
            "-framerate",
            "24",
            "-draw_mouse",
            "0",
            "-i",
        ])
        .arg(format!("title={}", w.window))
        .args(["-t", "15"]);
    } else {
        cmd.arg("-i").arg(&input);
    }
    run(cmd.args(["-an","-vf","fps=24,scale=2240:1080:force_original_aspect_ratio=decrease,pad=2240:1080:(ow-iw)/2:(oh-ih)/2,setsar=1","-pix_fmt","yuv420p","-c:v","libx264","-threads","2","-preset","veryfast","-crf","23","-f","h264"]).arg(&*temp))?;
    drop(capture);
    if !(1..=500_000_000).contains(&fs::metadata(&temp).map_err(|e| e.to_string())?.len()) {
        return Err("Video sau chuyển đổi phải dưới 500 MB".into());
    }
    let slug: String = label
        .chars()
        .take(40)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let name = format!("{slug}-{}.mp4.h264_2240x1080", config::now_ms());
    make_poster(&temp, &name)?;
    temp.persist_noclobber(queue.join(&name))
        .map_err(|e| e.to_string())?;
    Ok(format!("Đã chuẩn bị {name}. Đang chờ truyền qua USB."))
}

pub fn pending() -> Option<PathBuf> {
    fs::read_dir(config::data_dir().join("upload"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension() == Some(OsStr::new("h264_2240x1080")))
}

/// Query the running Wallpaper Engine instance through its documented control interface.
pub struct ActiveWallpaper {
    pub source: PathBuf,
    pub project: bool,
    pub title: String,
}

pub fn active_wallpaper(monitor: u32) -> Result<ActiveWallpaper, String> {
    use std::process::Stdio;
    let mut child = Command::new(engine()?)
        .args(["-control", "getWallpaper", "-monitor", &monitor.to_string()])
        .creation_flags(0x08000000)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = std::time::Instant::now();
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if start.elapsed() >= Duration::from_secs(8) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Wallpaper Engine chưa phản hồi. Hãy mở ứng dụng rồi kết nối lại.".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Wallpaper Engine chưa trả về wallpaper đang chạy.".into());
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    let path = raw
        .lines()
        .rev()
        .map(|s| PathBuf::from(s.trim().trim_matches('"')))
        .find(|p| p.exists() && p.is_absolute())
        .ok_or("Không tìm thấy wallpaper đang chạy. Kiểm tra Wallpaper Engine và số màn hình.")?;
    let dir = if path.is_dir() {
        path.as_path()
    } else {
        path.parent().ok_or("Thiếu thư mục wallpaper")?
    };
    if let Some((project, metadata)) = dir.ancestors().take(5).find_map(|p| {
        let project = p.join("project.json");
        let metadata: serde_json::Value =
            serde_json::from_reader(fs::File::open(&project).ok()?).ok()?;
        Some((project, metadata))
    }) {
        let title = metadata["title"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| {
                dir.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        return Ok(ActiveWallpaper {
            source: project,
            project: true,
            title,
        });
    }
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if !matches!(ext.as_str(), "mp4" | "webm" | "mkv" | "mov" | "gif" | "avi") {
        return Err("Dự án Web/Scene đang chạy thiếu project.json.".into());
    }
    Ok(ActiveWallpaper {
        title: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        source: path,
        project: false,
    })
}
