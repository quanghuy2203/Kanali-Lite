//! Giao diện cấu hình (egui). Chạy như một tiến trình riêng, đóng cửa sổ là giải phóng RAM.
use std::time::{Duration, Instant};

use eframe::egui::{self, vec2, Color32, Margin, RichText, Rounding, Stroke};
use kanali_rs::{
    autostart,
    config::{self, Config, State},
    update,
};

const BG: Color32 = Color32::from_rgb(12, 15, 22);
const CARD: Color32 = Color32::from_rgb(21, 26, 37);
const BORDER: Color32 = Color32::from_rgb(38, 46, 64);
const ACCENT: Color32 = Color32::from_rgb(34, 211, 238);
const DIM: Color32 = Color32::from_rgb(138, 148, 170);
const GOOD: Color32 = Color32::from_rgb(52, 211, 153);
const WARN: Color32 = Color32::from_rgb(251, 191, 36);
const BAD: Color32 = Color32::from_rgb(248, 113, 113);

pub fn run() {
    let Some(_lock) = config::acquire_lock("ui.lock", Duration::from_millis(100)) else {
        return; // giao diện đã mở rồi
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Kanali")
            .with_inner_size([500.0, 740.0])
            .with_min_inner_size([440.0, 520.0]),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "Kanali",
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
    v.widgets.inactive.bg_fill = Color32::from_rgb(30, 37, 52);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(30, 37, 52);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(40, 49, 68);
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

fn group_of(path: &str) -> &'static str {
    if path.contains("/media/") {
        "Video của bạn"
    } else if path.contains("/preset/") {
        "Mẫu có sẵn"
    } else {
        "Mặc định"
    }
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

struct App {
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
        Self { cfg, state, last_read: Instant::now(), autostart: autostart::is_enabled(), notice: String::new() }
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
                ui.label(RichText::new("Kanali").size(26.0).strong().color(Color32::WHITE));
                ui.label(RichText::new("Điều khiển màn hình Tryx Panorama").color(DIM));
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

    fn video_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            title(ui, "Video đang phát");
            if self.state.files.is_empty() {
                ui.label(RichText::new("Chưa có danh sách video (chưa kết nối màn hình).").color(DIM));
                return;
            }
            let mut chosen: Option<String> = None;
            egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
                for group in ["Video của bạn", "Mẫu có sẵn", "Mặc định"] {
                    let mut items =
                        self.state.files.iter().filter(|p| group_of(p) == group).peekable();
                    if items.peek().is_none() {
                        continue;
                    }
                    ui.label(RichText::new(group).small().color(DIM));
                    for p in items {
                        let file = file_of(p);
                        let selected = self.cfg.playing == file;
                        let resp = ui.add_sized(
                            [ui.available_width(), 28.0],
                            egui::SelectableLabel::new(selected, label_of(file)),
                        );
                        if resp.clicked() {
                            chosen = Some(file.to_owned());
                        }
                    }
                    ui.add_space(6.0);
                }
            });
            if let Some(f) = chosen {
                self.cfg.playing = f;
                self.save();
            }
            ui.horizontal(|ui| {
                if ui.button("Làm mới danh sách").clicked() {
                    config::send_command("refresh");
                }
            });
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
        if self.last_read.elapsed() >= Duration::from_millis(1000) {
            if let Some(s) = config::load_opt::<State>(&config::state_path()) {
                self.state = s;
            }
            self.last_read = Instant::now();
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(Margin::same(16.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.header(ui);
                    self.status_card(ui);
                    self.video_card(ui);
                    self.display_card(ui);
                    self.device_card(ui);
                    self.settings_card(ui);
                    self.update_card(ui);
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Đóng cửa sổ này không tắt màn hình. Dùng icon khay hệ thống để thoát hẳn.")
                            .small()
                            .color(DIM),
                    );
                });
            });

        ctx.request_repaint_after(Duration::from_millis(1000));
    }
}
