use eframe::egui;
use kanali_rs::config;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
struct Project {
    title: String,
    path: PathBuf,
    cover: Option<PathBuf>,
    kind: String,
}
pub fn libraries() -> Result<Vec<PathBuf>, String> {
    let steam = steamlocate::SteamDir::locate().map_err(|_| "Không tìm thấy Steam trên máy")?;
    Ok(steam
        .libraries()
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|lib| lib.path().join("steamapps"))
        .collect())
}

fn installed_projects() -> Result<Vec<Project>, String> {
    Ok(scan_projects(libraries()?))
}

fn scan_projects(libraries: Vec<PathBuf>) -> Vec<Project> {
    let mut projects = Vec::new();
    for lib in libraries {
        for root in [
            lib.join("workshop/content/431960"),
            lib.join("common/wallpaper_engine/projects/myprojects"),
            lib.join("common/wallpaper_engine/projects/defaultprojects"),
        ] {
            let Ok(dirs) = std::fs::read_dir(root) else {
                continue;
            };
            for dir in dirs.flatten() {
                let path = dir.path().join("project.json");
                if !path
                    .metadata()
                    .is_ok_and(|m| m.is_file() && m.len() < 4 * 1024 * 1024)
                {
                    continue;
                }
                let Some(meta) = config::load_opt::<serde_json::Value>(&path) else {
                    continue;
                };
                if !meta["type"].as_str().is_some_and(|t| {
                    matches!(t.to_ascii_lowercase().as_str(), "video" | "web" | "scene")
                }) {
                    continue;
                }
                let name = meta["title"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| dir.file_name().to_string_lossy().into_owned());
                let dir = dir.path();
                let cover = meta["preview"]
                    .as_str()
                    .map(|p| dir.join(p))
                    .into_iter()
                    .chain(["preview.jpg", "preview.png", "preview.gif"].map(|p| dir.join(p)))
                    .find_map(|p| {
                        p.canonicalize().ok().filter(|p| {
                            p.starts_with(dir.canonicalize().unwrap_or_else(|_| dir.clone()))
                                && p.is_file()
                        })
                    });
                projects.push(Project {
                    title: name,
                    path,
                    cover,
                    kind: meta["type"].as_str().unwrap_or("").into(),
                });
            }
        }
    }
    projects.sort_by_cached_key(|p| p.title.to_lowercase());
    projects
}

struct Library {
    projects: Vec<Project>,
    seen: Option<HashSet<PathBuf>>,
    scan: Option<Receiver<Result<Vec<Project>, String>>>,
    last_scan: Instant,
    search: String,
    auto: bool,
    queue: VecDeque<PathBuf>,
    job: Option<Receiver<Result<String, String>>>,
    note: String,
    scan_error: String,
    covers: HashMap<PathBuf, Option<egui::TextureHandle>>,
}
impl Library {
    fn new() -> Self {
        Self {
            projects: vec![],
            seen: None,
            scan: None,
            last_scan: Instant::now() - Duration::from_secs(10),
            search: String::new(),
            auto: false,
            queue: VecDeque::new(),
            job: None,
            note: String::new(),
            scan_error: String::new(),
            covers: HashMap::new(),
        }
    }
    fn poll(&mut self) {
        if let Some(rx) = &self.scan {
            if let Ok(result) = rx.try_recv() {
                self.scan = None;
                match result {
                    Ok(list) => {
                        let ids: HashSet<_> = list.iter().map(|p| p.path.clone()).collect();
                        if let Some(old) = &self.seen {
                            if self.auto {
                                self.queue.extend(ids.difference(old).cloned());
                            }
                        }
                        self.seen = Some(ids);
                        self.projects = list;
                        self.scan_error.clear();
                    }
                    Err(e) => self.scan_error = e,
                }
            }
        }
        if self.scan.is_none() && self.last_scan.elapsed() >= Duration::from_secs(6) {
            let (tx, rx) = mpsc::channel();
            self.scan = Some(rx);
            self.last_scan = Instant::now();
            std::thread::spawn(move || {
                let _ = tx.send(installed_projects());
            });
        }
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok(r) => {
                    self.note = r.unwrap_or_else(|e| format!("Lỗi: {e}"));
                    self.job = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.note = "Tác vụ nhập đã dừng".into();
                    self.job = None;
                }
                _ => (),
            }
        }
        if self.job.is_none() {
            if let Some(path) = self.queue.pop_front() {
                let (tx, rx) = mpsc::channel();
                self.job = Some(rx);
                let project = self.projects.iter().find(|p| p.path == path);
                let title = project.map(|p| p.title.as_str()).unwrap_or("wallpaper");
                self.note = if project.is_some_and(|p| !p.kind.eq_ignore_ascii_case("video")) {
                    format!("Đang nhập {title} · Ghi 8 giây từ Wallpaper Engine rồi chuyển video sang PANORAMA.")
                } else {
                    format!("Đang chuyển video {title} sang định dạng PANORAMA…")
                };
                std::thread::spawn(move || {
                    let _ = tx.send(super::media::import_wallpaper(path, true));
                });
            }
        }
    }
}
fn thumbnail(ctx: &egui::Context, path: &std::path::Path) -> Option<egui::TextureHandle> {
    if path.metadata().ok()?.len() > 16 * 1024 * 1024 {
        return None;
    }
    let mut reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(48 * 1024 * 1024);
    limits.max_image_width = Some(6000);
    limits.max_image_height = Some(6000);
    reader.limits(limits);
    let frame = reader.decode().ok()?.thumbnail(360, 200).to_rgba8();
    Some(ctx.load_texture(
        path.to_string_lossy(),
        egui::ColorImage::from_rgba_unmultiplied(
            [frame.width() as usize, frame.height() as usize],
            frame.as_raw(),
        ),
        Default::default(),
    ))
}

impl eframe::App for Library {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll();
        if self.job.is_some() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.note = "Đang hoàn tất nhập wallpaper…".into();
        }
        egui::TopBottomPanel::top("library-tools").show(ctx, |ui| {
            ui.heading("Thư viện Wallpaper Engine");
            ui.horizontal_wrapped(|ui| {
                ui.add_sized(
                    [230., 36.],
                    egui::TextEdit::singleline(&mut self.search).hint_text("Tìm tên wallpaper"),
                );
                if ui.button("Mở Wallpaper Engine để tải thêm").clicked() {
                    match super::media::open_engine_ui() {
                        Ok(()) => {
                            self.note =
                                "Đang mở Wallpaper Engine. Thư viện sẽ tự nhận mục tải xong.".into()
                        }
                        Err(e) => self.note = e,
                    }
                }
                if ui.add_enabled(self.job.is_none(), egui::Button::new("Cài công cụ ghi Scene")).on_hover_text("Tải FFmpeg cho Windows một lần từ GitHub").clicked() {
                    let (tx, rx) = mpsc::channel();
                    self.job = Some(rx);
                    self.note = "Đang tải và kiểm tra FFmpeg…".into();
                    std::thread::spawn(move || { let _ = tx.send(super::media::install_capture_tool()); });
                }
                ui.add_enabled(
                    self.seen.is_some(),
                    egui::Checkbox::new(&mut self.auto, "Tự nhập wallpaper mới tải"),
                )
                .on_hover_text("Chỉ nhập dự án mới xuất hiện trong lúc cửa sổ này mở.");
            });
            if !self.scan_error.is_empty() {
                ui.label(&self.scan_error);
            }
            if !self.note.is_empty() {
                ui.label(&self.note);
            }
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            let needle = self.search.trim().to_lowercase();
            let items: Vec<_> = self
                .projects
                .iter()
                .filter(|p| p.title.to_lowercase().contains(&needle))
                .collect();
            if items.is_empty() {
                ui.label(if self.seen.is_none() {
                    "Đang đọc thư viện…"
                } else {
                    "Không tìm thấy wallpaper phù hợp."
                });
            }
            let cols = ((ui.available_width() + 10.) / 235.).floor().max(1.) as usize;
            let width = (ui.available_width() - ui.spacing().item_spacing.x * (cols - 1) as f32)
                / cols as f32;
            let mut visible = HashSet::new();
            egui::ScrollArea::vertical().show_rows(
                ui,
                215.,
                items.len().div_ceil(cols),
                |ui, rows| {
                    for row in rows {
                        ui.horizontal(|ui| {
                            for item in items.iter().skip(row * cols).take(cols) {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(width, 215.),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::vec2(width, 130.),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(
                                            rect,
                                            6.,
                                            egui::Color32::from_gray(16),
                                        );
                                        let cover = item.cover.as_ref().and_then(|path| {
                                            visible.insert(path.clone());
                                            self.covers
                                                .entry(path.clone())
                                                .or_insert_with(|| thumbnail(ctx, path))
                                                .as_ref()
                                        });
                                        if let Some(tex) = cover {
                                            let size = tex.size_vec2();
                                            let scale =
                                                (rect.width() / size.x).min(rect.height() / size.y);
                                            ui.painter().image(
                                                tex.id(),
                                                egui::Rect::from_center_size(
                                                    rect.center(),
                                                    size * scale,
                                                ),
                                                egui::Rect::from_min_max(
                                                    egui::Pos2::ZERO,
                                                    egui::pos2(1., 1.),
                                                ),
                                                egui::Color32::WHITE,
                                            );
                                        } else {
                                            ui.painter().text(
                                                rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                "Chưa có ảnh bìa",
                                                egui::FontId::proportional(14.),
                                                egui::Color32::GRAY,
                                            );
                                        }
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&item.title).strong().size(16.),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(&item.title);
                                        ui.horizontal(|ui| {
                                            if ui
                                                .add_enabled(
                                                    self.job.is_none() && self.queue.is_empty(),
                                                    egui::Button::new("Nhập vào PANORAMA"),
                                                )
                                                .clicked()
                                            {
                                                self.queue.push_back(item.path.clone());
                                            }
                                            ui.label(&item.kind);
                                        });
                                    },
                                );
                            }
                        });
                    }
                },
            );
            self.covers.retain(|p, _| visible.contains(p));
        });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

pub fn run() {
    let Some(_lock) = config::acquire_lock("wallpaper-library.lock", Duration::ZERO) else {
        use windows::{
            core::w,
            Win32::UI::WindowsAndMessaging::{
                FindWindowW, SetForegroundWindow, ShowWindowAsync, SW_RESTORE,
            },
        };
        unsafe {
            if let Ok(win) = FindWindowW(None, w!("Thư viện Wallpaper Engine · Kanali Lite")) {
                let _ = ShowWindowAsync(win, SW_RESTORE);
                let _ = SetForegroundWindow(win);
            }
        }
        return;
    };
    let _ = eframe::run_native(
        "Thư viện Wallpaper Engine · Kanali Lite",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([980., 690.])
                .with_min_inner_size([530., 380.]),
            ..Default::default()
        },
        Box::new(|cc| {
            super::ui::setup(&cc.egui_ctx);
            Ok(Box::new(Library::new()))
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn downloaded_project_is_visible_with_its_cover() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().to_owned();
        assert!(scan_projects(vec![base.clone()]).is_empty());
        let project = base.join("workshop/content/431960/123");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("preview.jpg"), b"fixture").unwrap();
        std::fs::write(
            project.join("project.json"),
            r#"{"title":"Beach","type":"scene","preview":"preview.jpg"}"#,
        )
        .unwrap();
        let list = scan_projects(vec![base]);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Beach");
        assert!(list[0].cover.as_ref().unwrap().ends_with("preview.jpg"));
    }
}
