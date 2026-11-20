//! Giao diện cấu hình (egui). Chạy như một tiến trình riêng, đóng cửa sổ là giải phóng RAM.
use std::time::{Duration, Instant};

use eframe::egui::{self, vec2, Color32, Margin, RichText, Rounding, Stroke};
use kanali_rs::{
    autostart,
    config::{self, Config, State},
    update,
};

#[path="panorama.rs"]
mod panorama;

const BG: Color32 = Color32::from_rgb(29, 32, 37);
const CARD: Color32 = Color32::from_rgb(37, 40, 46);
const BORDER: Color32 = Color32::from_rgb(58, 60, 55);
const ACCENT: Color32 = Color32::from_rgb(225, 248, 72);
const DIM: Color32 = Color32::from_rgb(172, 176, 165);
const GOOD: Color32 = Color32::from_rgb(52, 211, 153);
const WARN: Color32 = Color32::from_rgb(251, 191, 36);
const BAD: Color32 = Color32::from_rgb(248, 113, 113);

pub fn run() {
    let Some(_lock) = config::acquire_lock("ui.lock", Duration::from_millis(100)) else {
        return; // giao diện đã mở rồi
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Kanali Lite")
            .with_inner_size([1150.0, 760.0])
            .with_min_inner_size([700.0, 420.0]),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "Kanali Lite",
        options,
        Box::new(|cc| {
            setup(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    );
}

fn setup(ctx: &egui::Context) {
    // Font Segoe UI có đủ dấu tiếng Việt; không có thì dùng font mặc định.
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert("segoe".to_owned(), egui::FontData::from_owned(bytes));
        if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            list.insert(0, "segoe".to_owned());
        }
        ctx.set_fonts(fonts);
    }

    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.extreme_bg_color = Color32::from_rgb(9, 11, 17);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.inactive.bg_fill = Color32::from_rgb(47, 49, 43);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(47, 49, 43);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(68, 72, 60);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.rounding = Rounding::same(8.0);
    }
    v.selection.bg_fill = ACCENT.gamma_multiply(0.30);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(14.0, 7.0);
    });
}

// ---------- thành phần nhỏ ----------

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .rounding(12.0)
        .inner_margin(Margin::same(14.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    ui.add_space(4.0);
}

fn title(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(3.0, 15.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 1.5, ACCENT);
        ui.label(RichText::new(text).strong().size(16.0).color(Color32::WHITE));
    });
    ui.add_space(2.0);
}

fn pill(ui: &mut egui::Ui, color: Color32, text: &str) {
    egui::Frame::none()
        .fill(color.gamma_multiply(0.15))
        .rounding(20.0)
        .inner_margin(Margin::symmetric(10.0, 5.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(9.0, 9.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.5, color);
                ui.label(RichText::new(text).color(color));
            });
        });
}

fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).strong().color(Color32::BLACK))
            .fill(ACCENT)
            .rounding(8.0),
    )
}

fn info_row(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).color(DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(if value.is_empty() { "—" } else { value });
        });
    });
}

fn file_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// "2026-11-20_00-38-41-483.mp4.h264_2240x1080" -> "2026-11-20  00:38:41"
fn label_of(file: &str) -> String {
    let base = file.split(".mp4").next().unwrap_or(file);
    if base.is_ascii() && base.len() >= 19 && base.as_bytes()[4] == b'-' && base.as_bytes()[10] == b'_' {
        format!("{}  {}", &base[..10], base[11..19].replace('-', ":"))
    } else {
        base.to_string()
    }
}

fn short(s: &str, max: usize) -> String {
    let t: String = s.chars().take(max).collect();
    if s.chars().count() > max { format!("{t}…") } else { t }
}

fn combo(ui: &mut egui::Ui, label: &str, cur: &mut String, files: &[String], extra: &str) -> bool {
    let mut changed = false;
    egui::ComboBox::from_label(label)
        .selected_text(label_of(cur))
        .width(240.0)
        .show_ui(ui, |ui| {
            let mut seen = std::collections::HashSet::new();
            for o in std::iter::once(extra).chain(files.iter().map(|p| file_of(p))) {
                if !seen.insert(o) { continue; }
                if ui.selectable_label(*cur == o, label_of(o)).clicked() {
                    *cur = o.to_owned();
                    changed = true;
                }
            }
        });
    changed
}

// ---------- ứng dụng ----------

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Page { Home, Panorama, Settings }

impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Home => "Trang chủ",
            Self::Panorama => "PANORAMA",
            Self::Settings => "Cài đặt",
        }
    }
}

struct Clip {
    dimensions:[usize;2],
    frames: Vec<u8>,
    texture: egui::TextureHandle,
    start: Instant,
    current: usize,
}

struct App {
    page: Page,
    display_tab: bool,
    media_job: Option<std::sync::mpsc::Receiver<Result<String,String>>>,
    media_note: String,
    clips: std::collections::HashMap<String,Option<Clip>>,
    carousel: String,
    draft: config::Layout,
    full: String,
    right_slot: bool,
    workshop_id: Option<u64>,
    gif_id:Option<String>,
    assets_ready: std::sync::mpsc::Receiver<()> ,
    cfg: Config,
    state: State,
    last_read: Instant,
    autostart: bool,
    notice: String,
}

impl App {
    fn new() -> Self {
        let cfg: Config = config::load_opt(&config::config_path()).unwrap_or_default();
        let state: State = config::load_opt(&config::state_path()).unwrap_or_default();
        let (tx, assets_ready)=std::sync::mpsc::channel();
        std::thread::spawn(move || { crate::presets::cache(); let _=tx.send(()); });
        Self { draft:cfg.layout.clone(), full:cfg.playing.clone(), carousel: cfg.playing.clone(), clips:Default::default(), right_slot:false, workshop_id:None, gif_id:None, assets_ready, media_job: None, media_note: String::new(), page: Page::Home, display_tab: false, cfg, state, last_read: Instant::now(), autostart: autostart::is_enabled(), notice: String::new() }
    }

    fn save(&self) {
        let _ = config::save(&config::config_path(), &self.cfg);
    }

    fn alive(&self) -> bool {
        config::now_ms().saturating_sub(self.state.updated_ms) < 8000
    }

    fn header(&self, ui: &mut egui::Ui) {
        let alive = self.alive();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(self.page.title()).size(26.0).strong().color(Color32::WHITE));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (color, text) = if !alive {
                    (BAD, "Dịch vụ nền đang tắt")
                } else if self.state.connected {
                    (GOOD, "Đã kết nối")
                } else {
                    (WARN, "Đang chờ màn hình")
                };
                pill(ui, color, text);
            });
        });
        ui.add_space(6.0);
    }

    fn status_card(&mut self, ui: &mut egui::Ui) {
        let alive = self.alive();
        if alive && self.state.connected && self.notice.is_empty() {
            return;
        }
        card(ui, |ui| {
            if !alive {
                ui.label(RichText::new("Dịch vụ nền chưa chạy nên màn hình sẽ không được giữ sáng.").color(BAD));
                if primary_button(ui, "Khởi động dịch vụ nền").clicked() {
                    crate::spawn_self("--background");
                }
            } else if !self.state.connected {
                ui.label(RichText::new(&self.state.message).color(WARN));
            }
            if !self.notice.is_empty() {
                ui.label(RichText::new(&self.notice).color(WARN));
            }
        });
    }

    fn pick_media(&mut self, poster_for: Option<String>) {
        let picker = rfd::FileDialog::new();
        let source = picker.add_filter("Video / GIF", &["mp4","mkv","mov","webm","avi","gif","m4v","h264"]).pick_file();
        let Some(source) = source else { return; };
        let (tx, rx) = std::sync::mpsc::channel();
        self.media_job = Some(rx);
        self.media_note = "Đang chuẩn bị media…".into();
        std::thread::spawn(move || {
            let result = match poster_for {
                Some(name) => crate::media::make_poster(&source, &name).map(|_| "Đã lưu ảnh xem trước".into()),
                None => crate::media::import(source, false),
            };
            let _ = tx.send(result);
        });
    }

    fn display_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            title(ui, "Hiển thị");
            let r = ui.add(
                egui::Slider::new(&mut self.cfg.level, 0..=100).text("Độ sáng (thử nghiệm)"),
            );
            if r.drag_stopped() || (r.changed() && !r.dragged()) {
                self.save();
            }

            ui.collapsing("Nâng cao: video khi bật nguồn và khi chờ", |ui| {
                let mut changed = false;
                changed |= combo(ui, "Khi bật nguồn", &mut self.cfg.poweron, &self.state.files, config::DEFAULT_POWERON);
                changed |= combo(ui, "Khi chờ", &mut self.cfg.standby, &self.state.files, config::DEFAULT_STANDBY);
                if changed {
                    self.save();
                }
            });
        });
    }

    fn device_card(&self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            title(ui, "Thiết bị");
            match &self.state.info {
                Some(i) => {
                    info_row(ui, "Model", &i.model);
                    info_row(ui, "Phiên bản firmware", &i.firmware);
                    info_row(ui, "Độ phân giải", &i.resolution);
                    info_row(ui, "Số serial", &i.serial);
                    info_row(ui, "Hệ điều hành", &format!("{} {}", i.os, i.kernel));
                }
                None => {
                    ui.label(RichText::new("Chưa có thông tin.").color(DIM));
                }
            }
        });
    }

    fn settings_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            title(ui, "Cài đặt");
            let mut auto = self.autostart;
            if ui.checkbox(&mut auto, "Tự khởi động cùng Windows").changed() {
                match autostart::set(auto) {
                    Ok(()) => {
                        self.autostart = auto;
                        self.cfg.autostart = auto;
                        self.notice.clear();
                        self.save();
                    }
                    Err(e) => self.notice = format!("Không đổi được tự khởi động: {e}"),
                }
            }
            if ui.checkbox(&mut self.cfg.check_updates, "Tự kiểm tra cập nhật").changed() {
                self.save();
            }
        });
    }

    fn update_card(&mut self, ui: &mut egui::Ui) {

        card(ui, |ui| {
            let up = &self.state.update;
            title(ui, "Cập nhật");
            ui.label(format!("Phiên bản hiện tại: v{}", env!("CARGO_PKG_VERSION")));
            if !up.status.is_empty() {
                ui.label(RichText::new(&up.status).color(DIM));
            }
            if up.busy {
                ui.add(egui::ProgressBar::new(up.progress).show_percentage());
            } else if up.available {
                ui.label(RichText::new(format!("Có bản mới: {}", up.latest)).color(GOOD).strong());
                if !up.notes.is_empty() {
                    ui.label(RichText::new(short(&up.notes, 300)).small().color(DIM));
                }
                if primary_button(ui, "Cập nhật ngay").clicked() {
                    config::send_command("update");
                }
            }
            if !up.busy && ui.button("Kiểm tra cập nhật").clicked() {
                config::send_command("check");
            }
            if update::DEFAULT_REPO.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new("Kho GitHub (tên-người-dùng/tên-repo):").small().color(DIM));
                let r = ui.text_edit_singleline(&mut self.cfg.repo);
                if r.lost_focus() {
                    self.save();
                }
            }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(job) = &self.media_job {
            match job.try_recv() {
                Ok(result) => {
                    self.media_note = result.unwrap_or_else(|e| format!("Lỗi: {e}"));
                    self.clips.clear();
                    self.media_job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.media_note = "Tác vụ media bị gián đoạn".into();
                    self.media_job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => (),
            }
        }
        if self.assets_ready.try_recv().is_ok() { self.clips.clear(); }
        if self.last_read.elapsed() >= Duration::from_millis(1000) {
            if let Some(s) = config::load_opt::<State>(&config::state_path()) {
                if s.update.restart && s.version == env!("CARGO_PKG_VERSION") && config::now_ms().saturating_sub(s.updated_ms) < 7000 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                self.state = s;
            }
            self.workshop_id = crate::workshop::selected();
            self.gif_id=config::load_opt(&config::data_dir().join("giphy-item.json"));
            self.last_read = Instant::now();
        }

        egui::SidePanel::left("navigation")
            .resizable(false)
            .exact_width(if ctx.screen_rect().width() >= 1050.0 { 202.0 } else { 138.0 })
            .frame(egui::Frame::none().fill(ACCENT).inner_margin(14.0))
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.label(RichText::new("KANALI").size(29.0).strong().color(BG));
                ui.label(RichText::new("LITE").strong().color(BG));
                ui.add_space(30.0);
                for page in [Page::Home, Page::Panorama, Page::Settings] {
                    let active = self.page == page;
                    let label = RichText::new(page.title()).strong()
                        .color(if active { ACCENT } else { BG });
                    if ui.add_sized([ui.available_width(), 36.0],
                        egui::Button::new(label).fill(if active { BG } else { ACCENT })
                            .stroke(Stroke::NONE).rounding(4.0)).clicked() {
                        self.page = page;
                    }
                }
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(Margin::same(16.0)))
            .show(ctx, |ui| {
                if self.page != Page::Panorama {self.header(ui);}
                egui::ScrollArea::vertical().id_salt(self.page).show(ui, |ui| {
                    self.status_card(ui);
                    match self.page {
                        Page::Home => {
                            card(ui, |ui| {
                                ui.label(RichText::new("TRYX").color(ACCENT).strong());
                                ui.label(RichText::new("PANORAMA").size(34.0).strong());
                                if self.alive() && self.state.connected {
                                    info_row(ui, "Video đã chọn", &label_of(&self.cfg.playing));
                                }
                                if primary_button(ui, "Điều khiển màn hình").clicked() {
                                    self.page = Page::Panorama;
                                }
                            });
                            self.device_card(ui);
                        }
                        Page::Panorama => {
                            self.panorama(ui);
                        }
                        Page::Settings => {
                            self.settings_card(ui);
                            self.update_card(ui);
                            ui.label(RichText::new("Đóng cửa sổ vẫn giữ màn hình hoạt động. Để thoát hẳn, dùng biểu tượng ở khay hệ thống.").small().color(DIM));
                        }
                    }
                });
            });

        ctx.request_repaint_after(Duration::from_millis(1000));
    }
}
