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

const CAPTURE_URL: &str = "https://github.com/GyanD/codexffmpeg/releases/download/8.1.1/ffmpeg-8.1.1-essentials_build.zip";
const CAPTURE_SHA256: &str = "6f58ce889f59c311410f7d2b18895b33c03456463486f3b1ebc93d97a0f54541";

fn capture_tool() -> PathBuf {
    config::data_dir().join("tools/ffmpeg-8.1.1.exe")
}

pub fn install_capture_tool() -> Result<String, String> {
    use std::io::{Read, Seek, Write};
    let target = capture_tool();
    let folder = dir("tools")?;
    if !target.exists() {
        let response = ureq::get(CAPTURE_URL).timeout(Duration::from_secs(180)).call().map_err(|e| e.to_string())?;
        let mut reader = response.into_reader().take(150_000_000);
        let mut archive = tempfile::tempfile_in(&folder).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut buf = [0; 65536];
        loop {
            let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 { break; }
            archive.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            hash.update(&buf[..n]);
        }
        if format!("{:x}", hash.finalize()) != CAPTURE_SHA256 { return Err("Tải FFmpeg không đầy đủ hoặc sai SHA-256".into()); }
        archive.rewind().map_err(|e| e.to_string())?;
        let mut zip = zip::ZipArchive::new(archive).map_err(|e| e.to_string())?;
        let name = zip.file_names().find(|n| n.ends_with("/bin/ffmpeg.exe")).ok_or("Thiếu ffmpeg.exe")?.to_owned();
        let mut entry = zip.by_name(&name).map_err(|e| e.to_string())?;
        let mut tmp = tempfile::NamedTempFile::new_in(folder).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut tmp).map_err(|e| e.to_string())?;
        if let Err(e) = tmp.persist_noclobber(&target) {
            if !target.is_file() { return Err(e.to_string()); }
        }
    }
    let out = ffmpeg().args(["-filters"]).output().map_err(|e| e.to_string())?;
    if !out.status.success() || !String::from_utf8_lossy(&out.stdout).contains("gfxcapture") { return Err("FFmpeg thiếu bộ lọc gfxcapture".into()); }
    Ok("Đã sẵn sàng ghi Scene. Chọn Nhập trên Scene cần dùng.".into())
}

fn ffmpeg() -> Command {
    let local = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("ffmpeg.exe")));
    let mut cmd = Command::new(
        Some(capture_tool()).filter(|p| p.is_file())
            .or_else(|| local.filter(|p| p.is_file()))
            .or_else(|| {
                std::env::split_paths(&std::env::var_os("PATH")?)
                    .map(|dir| dir.join("ffmpeg.exe"))
                    .find(|p| p.is_file())
            })
            .or_else(|| {
                ["ProgramFiles", "ProgramFiles(x86)"]
                    .into_iter()
                    .filter_map(std::env::var_os)
                    .map(|p| PathBuf::from(p).join("KANALI/resources/Connect/ffmpeg/ffmpeg.exe"))
                    .find(|p| p.is_file())
            })
            .unwrap_or_else(|| "ffmpeg.exe".into()),
    );
    use windows::Win32::System::Threading::{BELOW_NORMAL_PRIORITY_CLASS, CREATE_NO_WINDOW};
    cmd.creation_flags(CREATE_NO_WINDOW.0 | BELOW_NORMAL_PRIORITY_CLASS.0)
        .args(["-filter_threads", "2", "-filter_complex_threads", "2", "-nostdin", "-hide_banner", "-loglevel", "error", "-y"]);
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
    make_poster_at(source, name, 0.0)
}

fn make_poster_at(source: &Path, name: &str, start: f64) -> Result<(), String> {
    let dir = dir("previews")?;
    let tmp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| e.to_string())?
        .into_temp_path();
    run(ffmpeg().arg("-i").arg(source).args(["-ss", &start.to_string(), "-t","4","-frames:v","8","-vf","fps=2:round=up,scale=240:116:force_original_aspect_ratio=decrease,pad=240:116:(ow-iw)/2:(oh-ih)/2","-pix_fmt","rgb24","-f","rawvideo"]).arg(&*tmp))?;
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
    super::library::libraries()?
        .into_iter()
        .flat_map(|lib| {
            ["wallpaper64.exe", "wallpaper32.exe"]
                .map(|exe| lib.join("common/wallpaper_engine").join(exe))
        })
        .find(|p| p.is_file())
        .ok_or("Chưa tìm thấy Wallpaper Engine trong các thư viện Steam".into())
}

fn launch_engine() -> Result<(), String> {
    let exe = engine()?;
    let root = exe.parent().ok_or("Thiếu thư mục Wallpaper Engine")?;
    // Native Steam launcher opens the wallpaper chooser using the user's existing licence.
    let steam = steamlocate::SteamDir::locate().map_err(|e| e.to_string())?;
    Command::new(steam.path().join("steam.exe"))
        .current_dir(root)
        .args(["-applaunch", "431960"])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn show_engine_browser(exe: &Path) -> Result<(), String> {
    let parent = exe.parent().ok_or("Thiếu thư mục ứng dụng")?;
    if let Some(ui) = ["bin/wallpaperui.exe", "bin/ui32.exe", "wallpaperui.exe"]
        .iter()
        .map(|n| parent.join(n))
        .find(|p| p.is_file())
    {
        Command::new(ui)
            .current_dir(parent)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    } else {
        launch_engine()
    }
}

pub fn open_engine_ui() -> Result<(), String> {
    if let Some(exe) = running_engine() {
        show_engine_browser(&exe)
    } else {
        launch_engine()
    }
}

fn running_engine() -> Option<PathBuf> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Process {
        executable_path: Option<String>,
    }
    let com = wmi::COMLibrary::new().ok()?;
    let db = wmi::WMIConnection::new(com.into()).ok()?;
    let known = engine().ok()?;
    let parent = known.parent()?.canonicalize().ok()?;
    let rows:Vec<Process>=db.raw_query("SELECT ExecutablePath FROM Win32_Process WHERE Name='wallpaper32.exe' OR Name='wallpaper64.exe'").ok()?;
    rows.into_iter()
        .filter_map(|r| r.executable_path)
        .map(PathBuf::from)
        .find(|p| p.parent().and_then(|v| v.canonicalize().ok()).as_ref() == Some(&parent))
}

fn start_engine() -> Result<PathBuf, String> {
    if let Some(exe) = running_engine() {
        return Ok(exe);
    }
    launch_engine()?;
    for _ in 0..20 {
        thread::sleep(Duration::from_millis(750));
        if let Some(exe) = running_engine() {
            return Ok(exe);
        }
    }
    Err("Đã yêu cầu Steam mở Wallpaper Engine. Kiểm tra trạng thái khởi động trong Steam rồi kết nối lại.".into())
}

const ENCODE: &[&str] = &[
    "-an", "-pix_fmt", "yuv420p", "-c:v", "libx264", "-threads", "2", "-preset", "veryfast",
    // Match the PANORAMA stream produced by KANALI's MediaX H.264 converter.
    "-profile:v", "main", "-level:v", "4.1",
    "-x264-params", "repeat-headers=1:annexb=1:aud=1:b-pyramid=0:weightp=0:ref=3:range=pc:colorprim=bt709:transfer=bt709:colormatrix=bt709",
    "-color_range", "pc", "-crf", "23", "-maxrate", "12M", "-bufsize", "24M", "-g", "30", "-f", "h264",
];
const SCALE: &str="fps=60,scale=2240:1080:force_original_aspect_ratio=decrease:out_range=full:out_color_matrix=bt709,pad=2240:1080:(ow-iw)/2:(oh-ih)/2,setsar=1";

fn contains_picture(samples: &[u8]) -> bool {
    samples
        .chunks_exact(64 * 32)
        .any(|frame| frame.iter().filter(|&&v| v > 12).take(5).count() >= 5)
}

// Read one thumbnail at a time and stop decoding after the first visible sample.
fn first_picture(path: &Path) -> Result<Option<f64>, String> {
    use std::io::{ErrorKind, Read};
    use std::process::Stdio;
    let mut child = ffmpeg()
        .args(["-threads", "2", "-i"])
        .arg(path)
        .args([
            "-vf",
            "fps=2:round=up,scale=64:32,format=gray",
            "-vsync",
            "vfr",
            "-f",
            "rawvideo",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stream = child.stdout.take().unwrap();
    let mut frame = [0; 64 * 32];
    let mut seconds = 0.0;
    let result = loop {
        match stream.read_exact(&mut frame) {
            Ok(()) if contains_picture(&frame) => break Ok(Some(seconds)),
            Ok(()) => seconds += 0.5,
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => break Ok(None),
            Err(e) => break Err(e.to_string()),
        }
    };
    drop(stream);
    if !matches!(result, Ok(None)) {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() && matches!(result, Ok(None)) {
        return Err("Không giải mã được clip đã chuyển đổi".into());
    }
    result
}

fn capture_scene(w: &Wallpaper, target: &Path, intermediate: bool) -> Result<f64, String> {
    let filter = if intermediate { "fps=30,format=yuv420p" } else { SCALE };
    let capture_args: &[&str] = if intermediate {
        &["-an", "-c:v", "libx264", "-threads", "2", "-preset", "ultrafast", "-crf", "16", "-f", "mp4"]
    } else { ENCODE };
    let mut errors = Vec::new();
    for gpu in [true, false] {
        let mut cmd = ffmpeg();
        if gpu {
            // Match only our pop-out window, including when another app covers it.
            cmd.args(["-filter_complex",&format!("gfxcapture=window_title=^{}$:capture_cursor=0:max_framerate=30,hwdownload,format=bgra,{filter}[video]",w.window),"-map","[video]"]);
        } else {
            cmd.args([
                "-f",
                "gdigrab",
                "-framerate",
                "30",
                "-draw_mouse",
                "0",
                "-i",
                &format!("title={}", w.window),
                "-vf",
                filter,
            ]);
        }
        // Wall-clock limit also covers capture sources that stop delivering frames.
        let log = tempfile::tempfile().map_err(|e| e.to_string())?;
        let mut child = cmd
            .args(["-t", "15"])
            .args(capture_args)
            .arg(target)
            .stderr(log.try_clone().map_err(|e| e.to_string())?)
            .spawn()
            .map_err(|e| format!("Không mở được FFmpeg: {e}"))?;
        let start = std::time::Instant::now();
        let success = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status.success();
            }
            if start.elapsed() > Duration::from_secs(60) {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
            thread::sleep(Duration::from_millis(150));
        };
        if success {
            if let Some(start) = first_picture(target)? {
                return Ok(start);
            }
            errors.push("chỉ thu được khung đen".into());
        } else {
            use std::io::{Read, Seek};
            let mut log = log;
            let _ = log.rewind();
            let mut detail = String::new();
            let _ = log.take(450).read_to_string(&mut detail);
            errors.push(detail);
        }
    }
    Err(format!("Không thu được hình của Wallpaper Engine. Trong thư viện, bấm Cài công cụ ghi Scene rồi nhập lại và giữ cửa sổ wallpaper mở. Đã hủy nhập. {}",errors.join(" / ")))
}

pub fn import_wallpaper(source: PathBuf, project: bool) -> Result<String, String> {
    import_media(source, project, true)
}

pub fn import(source: PathBuf, project: bool) -> Result<String, String> {
    import_media(source, project, project)
}

fn import_media(source: PathBuf, project: bool, wallpaper: bool) -> Result<String, String> {
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
        let kind = json["type"].as_str().unwrap_or("").to_ascii_lowercase();
        // A Scene's logical scene.json may only exist inside scene.pkg.
        // Wallpaper Engine resolves those internal resources from project.json itself.
        if kind == "video" {
            let entry = root.join(json["file"].as_str().ok_or("Video thiếu trường file")?);
            input = entry.canonicalize().map_err(|e| format!("Thiếu video nguồn: {e}"))?;
            if !input.starts_with(&root) { return Err("Tệp nằm ngoài thư mục dự án".into()); }
        }
        if let Some(title) = json["title"].as_str() {
            label = title.into();
        }
        match kind.as_str() {
            "video" => (),
            "scene" | "web" => {
                let engine = start_engine()?;
                let window = format!("Kanali-Panorama-{}", config::now_ms());
                let result = Command::new(&engine)
                    .args(["-control", "openWallpaper", "-file"])
                    .arg(&source)
                    .args(["-playInWindow", &window, "-width", "896", "-height", "432", "-borderless"])
                    .creation_flags(0x08000000)
                    .status()
                    .map_err(|e| e.to_string())?;
                if !result.success() {
                    return Err("Wallpaper Engine không mở được dự án".into());
                }
                capture = Some(Wallpaper { engine, window });
                use windows::{core::HSTRING, Win32::UI::WindowsAndMessaging::{FindWindowW, SetWindowPos, HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE}};
                let title = HSTRING::from(capture.as_ref().unwrap().window.as_str());
                let start = std::time::Instant::now();
                loop {
                    if let Ok(window) = unsafe { FindWindowW(None, &title) } {
                        // Leave the render surface alive for window capture without taking focus.
                        unsafe { SetWindowPos(window, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE) }.map_err(|e| e.to_string())?;
                        break;
                    }
                    if start.elapsed() > Duration::from_secs(20) {
                        return Err("Wallpaper Engine chưa mở cửa sổ Scene sau 20 giây".into());
                    }
                    thread::sleep(Duration::from_millis(200));
                }
                thread::sleep(Duration::from_secs(1));
            }
            _ => return Err("Chỉ hỗ trợ dự án Video, Web và Scene".into()),
        }
    }
    let queue = dir("upload")?;
    let temp = tempfile::NamedTempFile::new_in(&queue)
        .map_err(|e| e.to_string())?
        .into_temp_path();
    let native = super::mediax::installed().is_some();
    let preview_start;
    let mut encoder = String::from("FFmpeg");
    if native {
        let recording = if let Some(w) = &capture {
            let clip = tempfile::Builder::new().suffix(".mp4").tempfile_in(&queue)
                .map_err(|e| e.to_string())?.into_temp_path();
            capture_scene(w, &clip, true)?;
            Some(clip)
        } else { None };
        drop(capture);
        let source: &Path = recording.as_deref().unwrap_or(&input);
        match super::mediax::encode(source, &temp) {
            Ok(()) => encoder = "KANALI MediaX".into(),
            Err(e) => {
                run(ffmpeg().arg("-i").arg(source).args(["-vf", SCALE]).args(ENCODE).arg(&*temp))?;
                encoder = format!("FFmpeg, {e}");
            }
        }
        preview_start = first_picture(&temp)?.ok_or("Video đã mã hóa chỉ có khung đen")?;
    } else if let Some(w) = &capture {
        preview_start = capture_scene(w, &temp, false)?;
    } else {
        run(ffmpeg().arg("-i").arg(&input).args(["-vf", SCALE]).args(ENCODE).arg(&*temp))?;
        preview_start = if wallpaper {
            first_picture(&temp)?.ok_or("Wallpaper chỉ có khung đen. Đã hủy nhập.")?
        } else { 0.0 };
    }
    if !(1..=500_000_000).contains(&fs::metadata(&temp).map_err(|e| e.to_string())?.len()) {
        return Err("Video sau chuyển đổi phải dưới 500 MB".into());
    }
    let slug: String = label
        .chars()
        .take(40)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let name = format!("{slug}-{}.mp4.h264_2240x1080", config::now_ms());
    make_poster_at(&temp, &name, preview_start)?;
    temp.persist_noclobber(queue.join(&name))
        .map_err(|e| e.to_string())?;
    Ok(format!("Đã chuẩn bị {name} ({encoder}). Đang chờ truyền qua USB."))
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
    let exe = start_engine()?;
    show_engine_browser(&exe)?;
    let mut child = Command::new(exe)
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

#[cfg(test)]
mod capture_tests {
    use super::*;
    #[test]
    #[ignore = "requires an installed original KANALI encoder"]
    fn native_mediax_converts_video() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("APPDATA", tmp.path());
        install_capture_tool().unwrap();
        let source = tmp.path().join("source.mp4");
        let output = tmp.path().join("device.h264");
        run(ffmpeg().args(["-f", "lavfi", "-i", "testsrc2=size=480x240:rate=30:duration=2",
                           "-c:v", "libx264"]).arg(&source)).unwrap();
        let status = Command::new("target/release/kanali.exe").arg("--encode-mediax")
            .arg(&source).arg(&output).status().unwrap();
        assert!(status.success(), "MediaX returned {status}");
        assert_eq!(first_picture(&output).unwrap(), Some(0.0));
        let frames = ffmpeg().arg("-i").arg(&output)
            .args(["-vf", "fps=2,scale=64:32,format=gray","-f","rawvideo","-"])
            .output().unwrap();
        assert!(frames.status.success());
        let frames: Vec<_> = frames.stdout.chunks_exact(2048).collect();
        assert!(frames.len() >= 3 && frames.windows(2).any(|p| p[0] != p[1]));
    }
    #[test]
    #[ignore = "downloads the pinned Windows FFmpeg distribution"]
    fn windows_capture_tool_encodes_visible_device_clip() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("APPDATA", tmp.path());
        install_capture_tool().unwrap();
        let clip = tmp.path().join("device.h264");
        run(ffmpeg().args(["-f","lavfi","-i","testsrc2=size=320x180:rate=30:duration=2","-vf", SCALE])
            .args(ENCODE).arg(&clip)).unwrap();
        let bytes = fs::read(&clip).unwrap();
        let units: Vec<_> = bytes.windows(3).enumerate()
            .filter(|(_, prefix)| *prefix == [0, 0, 1])
            .map(|(i, _)| &bytes[i + 3..]).collect();
        let sps: Vec<_> = units.iter().filter(|u| u[0] & 31 == 7).collect();
        assert!(sps.len() >= 2 && sps.iter().all(|u| u[1] == 77 && u[3] == 41));
        assert_eq!(units.iter().filter(|u| u[0] & 31 == 9).count(), 120);
        // Decode the entire stream, checking that the animation advances.
        let decoded = ffmpeg().arg("-i").arg(&clip)
            .args(["-vf", "fps=2,scale=64:32,format=gray", "-f", "rawvideo", "-"])
            .output().unwrap();
        assert!(decoded.status.success());
        let frames: Vec<_> = decoded.stdout.chunks_exact(64 * 32).collect();
        assert_eq!(frames.len(), 4);
        assert!(frames.windows(2).all(|pair| pair[0] != pair[1]));
        assert_eq!(first_picture(&clip).unwrap(), Some(0.0));
        make_poster(&clip, "test.mp4").unwrap();
    }
    #[test]
    fn rejects_black_but_accepts_dark_scene_after_fade() {
        let mut samples=vec![0;64*32*3];
        assert!(!contains_picture(&samples));
        samples[4100..4120].fill(24);
        assert!(contains_picture(&samples));
    }
}
