//! Giao diện cấu hình (egui). Chạy như một tiến trình riêng, đóng cửa sổ là giải phóng RAM.
use std::time::{Duration, Instant};

use eframe::egui::{self, vec2, Color32, Margin, RichText, Rounding, Stroke};
use kanali_rs::{
    autostart,
    config::{self, Config, State},
    update,
};

#[path = "panorama.rs"]
mod panorama;

const BG: Color32 = Color32::from_rgb(14, 17, 23);
const SIDEBAR: Color32 = Color32::from_rgb(19, 23, 30);
const CARD: Color32 = Color32::from_rgb(24, 29, 37);
const CARD_HOVER: Color32 = Color32::from_rgb(30, 36, 45);
const BORDER: Color32 = Color32::from_rgb(46, 54, 65);
const ACCENT: Color32 = Color32::from_rgb(221, 245, 76);
const DIM: Color32 = Color32::from_rgb(154, 164, 178);
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
            .with_icon(app_icon())
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

fn app_icon() -> std::sync::Arc<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../../assets/kanali-logo.png"))
        .expect("logo")
        .into_rgba8();
    std::sync::Arc::new(egui::IconData {
        width: img.width(),
        height: img.height(),
        rgba: img.into_raw(),
    })
}

pub(super) fn setup(ctx: &egui::Context) {
    // Segoe UI keeps native Windows rendering and full Vietnamese coverage.
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("segoe".to_owned(), egui::FontData::from_owned(bytes));
        if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            list.insert(0, "segoe".to_owned());
        }
        if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\seguisb.ttf") {
            fonts.font_data.insert(
                "segoe-semibold".to_owned(),
                egui::FontData::from_owned(bytes),
            );
            fonts.families.insert(
                egui::FontFamily::Name("segoe-semibold".into()),
                vec!["segoe-semibold".to_owned(), "segoe".to_owned()],
            );
        }
        ctx.set_fonts(fonts);
    }

    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.extreme_bg_color = Color32::from_rgb(10, 12, 17);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.inactive.bg_fill = Color32::from_rgb(32, 38, 47);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(32, 38, 47);
    v.widgets.hovered.bg_fill = CARD_HOVER;
    v.widgets.hovered.weak_bg_fill = CARD_HOVER;
    v.widgets.active.bg_fill = Color32::from_rgb(39, 46, 56);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
    ] {
        w.rounding = Rounding::same(10.0);
    }
    v.selection.bg_fill = ACCENT.gamma_multiply(0.30);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        let semibold = egui::FontFamily::Name("segoe-semibold".into());
        s.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(14.5, semibold.clone()),
        );
        s.text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::new(24.0, semibold));
        s.spacing.interact_size.y = 36.0;
        s.spacing.item_spacing = vec2(10.0, 10.0);
        s.spacing.button_padding = vec2(15.0, 8.0);
        s.animation_time = 0.16;
    });
}

// ---------- thành phần nhỏ ----------

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .rounding(14.0)
        .inner_margin(Margin::same(16.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    ui.add_space(6.0);
}

fn title(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(3.0, 15.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 1.5, ACCENT);
        ui.label(
            RichText::new(text)
                .strong()
                .size(16.0)
                .color(Color32::WHITE),
        );
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

const RGB_PRESETS: [(&str, Color32); 5] = [
    ("FF2D2D", Color32::from_rgb(255, 45, 45)),
    ("F5F54C", Color32::from_rgb(245, 245, 76)),
    ("34D399", Color32::from_rgb(52, 211, 153)),
    ("22D3EE", Color32::from_rgb(34, 211, 238)),
    ("3B82F6", Color32::from_rgb(59, 130, 246)),
];

fn effect_code(label: &str) -> &'static str {
    match label {
        "Tắt" => "off",
        "Nhịp thở" => "breathing",
        "Cầu vồng" => "rainbow",
        _ => "static",
    }
}

fn lighting_controls(ui: &mut egui::Ui, effect: &mut String, color: &mut usize) {
    egui::ComboBox::from_id_salt(ui.next_auto_id())
        .selected_text(effect.as_str())
        .width(150.0)
        .show_ui(ui, |ui| {
            for name in ["Tắt", "Tĩnh", "Nhịp thở", "Cầu vồng"] {
                ui.selectable_value(effect, name.into(), name);
            }
        });
    ui.horizontal(|ui| {
        for (i, (_, shade)) in RGB_PRESETS.iter().enumerate() {
            let button = egui::Button::new("")
                .fill(*shade)
                .stroke(Stroke::new(if *color == i { 3.0_f32 } else { 1.0_f32 }, if *color == i { Color32::WHITE } else { BORDER }))
                .min_size(vec2(30.0, 28.0));
            if ui.add(button).clicked() { *color = i; }
        }
    });
}

fn lighting_command(target: &str, effect: &str, color: usize) {
    let hex = RGB_PRESETS[color.min(RGB_PRESETS.len() - 1)].0;
    config::send_command(&format!("lighting|{target}|{}|{hex}", effect_code(effect)));
}

fn info_row(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).color(DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(if value.is_empty() { "—" } else { value });
        });
    });
}

fn metric_card(ui: &mut egui::Ui, label: &str, value: &str, detail: &str, color: Color32) {
    egui::Frame::none()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .rounding(13.0)
        .inner_margin(Margin::same(14.0))
        .show(ui, |ui| {
            ui.set_min_height(76.0);
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.0, color);
                ui.label(RichText::new(label).small().color(DIM));
            });
            ui.label(
                RichText::new(value)
                    .size(22.0)
                    .strong()
                    .color(Color32::WHITE),
            );
            ui.label(RichText::new(detail).small().color(DIM));
        });
}

fn ring_gauge(ui: &mut egui::Ui, label: &str, value: f32, color: Color32, detail: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 132.0), egui::Sense::hover());
    let center = egui::pos2(rect.center().x, rect.top() + 58.0);
    let radius = 39.0;
    let start = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;
    let arc = |fraction: f32| -> Vec<egui::Pos2> {
        let steps = (42.0 * fraction.max(0.05)).ceil() as usize;
        (0..=steps).map(|i| {
            let angle = start + sweep * fraction * i as f32 / steps as f32;
            center + vec2(angle.cos(), angle.sin()) * radius
        }).collect()
    };
    ui.painter().add(egui::Shape::line(arc(1.0), Stroke::new(7.0_f32, Color32::from_rgb(39, 47, 59))));
    ui.painter().add(egui::Shape::line(arc(value.clamp(0.0, 1.0)), Stroke::new(7.0_f32, color)));
    ui.painter().text(center, egui::Align2::CENTER_CENTER, format!("{:.0}%", value * 100.0), egui::FontId::proportional(22.0), Color32::WHITE);
    ui.painter().text(egui::pos2(center.x, rect.top() + 8.0), egui::Align2::CENTER_TOP, label, egui::FontId::proportional(12.0), color);
    ui.painter().text(egui::pos2(center.x, rect.bottom() - 5.0), egui::Align2::CENTER_BOTTOM, detail, egui::FontId::proportional(11.0), DIM);
}

fn file_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// "2026-11-20_00-38-41-483.mp4.h264_2240x1080" -> "2026-11-20  00:38:41"
fn label_of(file: &str) -> String {
    let base = file.split(".mp4").next().unwrap_or(file);
    if base.is_ascii()
        && base.len() >= 19
        && base.as_bytes()[4] == b'-'
        && base.as_bytes()[10] == b'_'
    {
        format!("{}  {}", &base[..10], base[11..19].replace('-', ":"))
    } else {
        base.to_string()
    }
}

fn short(s: &str, max: usize) -> String {
    let t: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        format!("{t}…")
    } else {
        t
    }
}

fn uptime_text(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    if days > 0 { format!("{days} ngày {hours} giờ") } else { format!("{hours} giờ {} phút", (seconds % 3_600) / 60) }
}

fn combo(ui: &mut egui::Ui, label: &str, cur: &mut String, files: &[String], extra: &str) -> bool {
    let mut changed = false;
    egui::ComboBox::from_label(label)
        .selected_text(label_of(cur))
        .width(240.0)
        .show_ui(ui, |ui| {
            let mut seen = std::collections::HashSet::new();
            for o in std::iter::once(extra).chain(files.iter().map(|p| file_of(p))) {
                if !seen.insert(o) {
                    continue;
                }
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
enum Page {
    Lighting,
    Home,
    Panorama,
    Settings,
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Lighting => "Đèn RGB",
            Self::Home => "Trang chủ",
            Self::Panorama => "PANORAMA",
            Self::Settings => "Cài đặt",
        }
    }
}

struct Clip {
    dimensions: [usize; 2],
    frames: Vec<u8>,
    texture: egui::TextureHandle,
    start: Instant,
    current: usize,
    fps: f32,
    stream: Option<std::sync::mpsc::Receiver<Vec<u8>>>,
    stream_stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

impl Drop for Clip {
    fn drop(&mut self) {
        if let Some(stop) = &self.stream_stop {
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

struct App {
    page: Page,
    display_tab: bool,
    media_job: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    media_note: String,
    clips: std::collections::HashMap<String, Option<Clip>>,
    carousel: String,
    draft: config::Layout,
    full: String,
    right_slot: bool,
    wallpaper: Option<crate::media::ActiveWallpaper>,
    wallpaper_query:
        Option<std::sync::mpsc::Receiver<Result<crate::media::ActiveWallpaper, String>>>,
    wallpaper_monitor: u32,
    wallpaper_note: String,
    gif_id: Option<String>,
    assets_ready: std::sync::mpsc::Receiver<()>,
    cfg: Config,
    state: State,
    last_read: Instant,
    autostart: bool,
    notice: String,
    ram_effect: String,
    ram_color: usize,
    gpu_effect: String,
    gpu_color: usize,
    sync_effect: String,
    sync_color: usize,
    logo: Option<egui::TextureHandle>,
    scroll_top: bool,
}

impl App {
    fn new() -> Self {
        let cfg: Config = config::load_opt(&config::config_path()).unwrap_or_default();
        let state: State = config::load_opt(&config::state_path()).unwrap_or_default();
        let (tx, assets_ready) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            crate::presets::cache();
            let _ = tx.send(());
        });
        Self {
            draft: cfg.layout.clone(),
            full: cfg.playing.clone(),
            carousel: cfg.playing.clone(),
            clips: Default::default(),
            right_slot: false,
            wallpaper: None,
            wallpaper_query: None,
            wallpaper_monitor: 0,
            wallpaper_note: String::new(),
            gif_id: None,
            assets_ready,
            media_job: None,
            media_note: String::new(),
            page: Page::Lighting,
            display_tab: false,
            cfg,
            state,
            last_read: Instant::now(),
            autostart: autostart::is_enabled(),
            notice: String::new(),
            ram_effect: "Tĩnh".into(),
            ram_color: 0,
            gpu_effect: "Tĩnh".into(),
            gpu_color: 0,
            sync_effect: "Tĩnh".into(),
            sync_color: 0,
            logo: None,
            scroll_top: true,
        }
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
                ui.label(
                    RichText::new(self.page.title())
                        .size(26.0)
                        .strong()
                        .color(Color32::WHITE),
                );
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
                if self.media_job.is_some() || self.wallpaper_query.is_some() {
                    ui.spinner();
                    ui.label(RichText::new("Đang xử lý").small().color(DIM));
                }
            });
        });
        ui.add_space(6.0);
    }

    fn home_hero(&mut self, ui: &mut egui::Ui) {
        let playing = self.cfg.playing.clone();
        let hero = egui::Frame::none()
            .fill(CARD)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .rounding(16.0)
            .inner_margin(Margin::same(22.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("TRYX PANORAMA")
                                .color(ACCENT)
                                .strong()
                                .size(13.0),
                        );
                        ui.label(
                            RichText::new("Không gian hiển thị của bạn")
                                .size(30.0)
                                .strong(),
                        );
                        ui.label(
                            RichText::new(
                                "Nội dung đang chạy trên PANORAMA, cập nhật mượt mà ngay tại đây.",
                            )
                            .color(DIM),
                        );
                        ui.add_space(10.0);
                        if primary_button(ui, "Mở điều khiển  →").clicked() {
                            self.page = Page::Panorama;
                            self.scroll_top = true;
                        }
                    });
                });
                ui.add_space(16.0);
                let width = ui.available_width();
                let height = (width * 1080.0 / 2240.0).clamp(180.0, 330.0);
                let (rect, _) = ui.allocate_exact_size(vec2(width, height), egui::Sense::hover());
                self.stage(ui, rect, &playing, true);
            });
        let phase = ((ui.ctx().input(|i| i.time) * 0.16) % 1.0) as f32;
        let time = ui.ctx().input(|i| i.time) as f32;
        let bounds = hero.response.rect;
        let painter = ui.painter().with_clip_rect(bounds);
        // Lightweight ambient telemetry field: deterministic particles and a
        // slow scan line, drawn only while Home is visible.
        for (i, color) in [Color32::from_rgb(221, 245, 76), Color32::from_rgb(56, 189, 248), Color32::from_rgb(167, 139, 250)].into_iter().enumerate() {
            let angle = time * (0.11 + i as f32 * 0.025) + i as f32 * 2.1;
            let center = bounds.center() + vec2(angle.cos() * bounds.width() * 0.34, angle.sin() * bounds.height() * 0.28);
            painter.circle_filled(center, 75.0 + i as f32 * 24.0, color.gamma_multiply(0.018));
        }
        for i in 0..14 {
            let seed = i as f32 * 0.618_034;
            let x = bounds.left() + ((seed + time * (0.008 + i as f32 * 0.0004)).fract()) * bounds.width();
            let y = bounds.top() + ((seed * 1.73 + time * 0.012).fract()) * bounds.height();
            painter.circle_filled(egui::pos2(x, y), 1.3 + (i % 3) as f32 * 0.5, ACCENT.gamma_multiply(0.08));
        }
        let scan_y = bounds.top() + ((time * 0.055).fract()) * bounds.height();
        painter.line_segment([egui::pos2(bounds.left(), scan_y), egui::pos2(bounds.right(), scan_y)], Stroke::new(1.0_f32, ACCENT.gamma_multiply(0.055)));
        let x = egui::lerp(hero.response.rect.left()..=hero.response.rect.right(), phase);
        ui.painter().circle_filled(
            egui::pos2(x, hero.response.rect.top() + 1.0),
            24.0,
            ACCENT.gamma_multiply(0.07),
        );
        ui.painter().line_segment(
            [
                egui::pos2(hero.response.rect.left() + 16.0, hero.response.rect.top()),
                egui::pos2(x, hero.response.rect.top()),
            ],
            Stroke::new(1.5_f32, ACCENT.gamma_multiply(0.65)),
        );
        ui.add_space(6.0);
    }

    fn home_dashboard(&self, ui: &mut egui::Ui) {
        let info = self.state.info.as_ref();
        let status = if !self.alive() {
            ("Offline", "Dịch vụ nền chưa chạy", BAD)
        } else if self.state.connected {
            ("Online", "USB đang phản hồi", GOOD)
        } else {
            ("Đang chờ", "Kiểm tra cáp USB", WARN)
        };
        let resolution = info
            .map(|i| i.resolution.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("—");
        let personal = self.state.files.iter().filter(|p| p.contains("/media/") || p.contains("/user/")).count();
        let metrics = [
            (
                "PANORAMA",
                status.0.to_owned(),
                status.1.to_owned(),
                status.2,
            ),
            (
                "ĐỘ PHÂN GIẢI",
                resolution.to_owned(),
                "PANORAMA canvas".to_owned(),
                ACCENT,
            ),
            (
                "THƯ VIỆN",
                personal.to_string(),
                "Media cá nhân".to_owned(),
                Color32::from_rgb(96, 165, 250),
            ),
            (
                "ĐỘ SÁNG",
                format!("{}%", self.cfg.level),
                "Mức trên màn hình".to_owned(),
                WARN,
            ),
        ];
        let columns = if ui.available_width() >= 760.0 { 4 } else { 2 };
        for row in metrics.chunks(columns) {
            ui.columns(columns, |cols| {
                for (col, (label, value, detail, color)) in cols.iter_mut().zip(row.iter()) {
                    metric_card(col, label, value, detail, *color);
                }
            });
            ui.add_space(4.0);
        }
    }

    fn telemetry_detail(&self, ui: &mut egui::Ui) {
        let t = &self.state.telemetry;
        card(ui, |ui| {
            title(ui, "Hiệu năng thời gian thực");
            let vram_ratio = match t.gpu_memory_used_gb.zip(t.gpu_memory_total_gb) { Some((u, n)) if n > 0.0 => u / n, _ => 0.0 };
            ui.columns(4, |cols| {
                ring_gauge(&mut cols[0], "CPU", t.cpu as f32 / 100.0, Color32::from_rgb(56, 189, 248), "Tải bộ xử lý");
                ring_gauge(&mut cols[1], "GPU", t.gpu as f32 / 100.0, Color32::from_rgb(167, 139, 250), &t.gpu_temp.map(|v| format!("{v}°C")).unwrap_or_else(|| "Đang đọc cảm biến".into()));
                ring_gauge(&mut cols[2], "RAM", t.memory as f32 / 100.0, GOOD, &format!("{:.1} / {:.1} GB", t.memory_used_gb, t.memory_total_gb));
                ring_gauge(&mut cols[3], "VRAM", vram_ratio, Color32::from_rgb(244, 114, 182), &match (t.gpu_memory_used_gb, t.gpu_memory_total_gb) { (Some(u), Some(n)) => format!("{u:.1} / {n:.1} GB"), _ => "Chưa có dữ liệu".into() });
            });
            ui.add_space(8.0);
            ui.separator();
            ui.columns(4, |c| {
                let values = [
                    ("NHIỆT ĐỘ GPU", t.gpu_temp.map(|v| format!("{v}°C")).unwrap_or_else(|| "—".into())),
                    ("XUNG GPU", t.gpu_clock_mhz.map(|v| format!("{v} MHz")).unwrap_or_else(|| "—".into())),
                    ("CÔNG SUẤT", t.gpu_power_w.map(|v| format!("{v:.0} W")).unwrap_or_else(|| "—".into())),
                    ("QUẠT GPU", t.gpu_fan.map(|v| format!("{v}%")).unwrap_or_else(|| "0% / passive".into())),
                ];
                for (col, (label, value)) in c.iter_mut().zip(values) { col.label(RichText::new(label).small().color(DIM)); col.label(RichText::new(value).size(18.0).strong()); }
            });
            ui.add_space(8.0);
            ui.columns(4, |c| {
                let values = [
                    ("LUỒNG CPU", format!("{} threads", t.cpu_threads)),
                    ("RAM KHẢ DỤNG", format!("{:.1} GB", t.memory_available_gb)),
                    ("XUNG VRAM", t.gpu_memory_clock_mhz.map(|v| format!("{v} MHz")).unwrap_or_else(|| "—".into())),
                    ("THỜI GIAN HOẠT ĐỘNG", uptime_text(t.uptime_seconds)),
                ];
                for (col, (label, value)) in c.iter_mut().zip(values) { col.label(RichText::new(label).small().color(DIM)); col.label(RichText::new(value).size(16.0).strong()); }
            });
            ui.label(RichText::new("Cảm biến lấy mẫu mỗi 2 giây; hiệu ứng chỉ chạy khi mở Trang chủ.").small().color(DIM));
        });
    }

    fn lighting_page(&mut self, ui: &mut egui::Ui) {
        let l = self.state.lighting.clone();
        egui::Frame::none().fill(Color32::from_rgb(20,27,35)).stroke(Stroke::new(1.0_f32,ACCENT.gamma_multiply(0.45))).rounding(18.0).inner_margin(Margin::same(22.0)).show(ui,|ui|{
            ui.label(RichText::new("KANALI LIGHTING").small().strong().color(ACCENT));
            ui.label(RichText::new("Ánh sáng theo cách của bạn").size(28.0).strong());
            ui.label(RichText::new("Điều khiển riêng RAM Lexar và Zotac GPU, hoặc đồng bộ cả hai từ một bảng màu.").color(DIM));
            ui.add_space(12.0);
            pill(ui,if !l.lconnect_running{GOOD}else{WARN},if !l.lconnect_running{"Kanali đang giữ quyền độc lập"}else{"Cần tắt L-Connect để tránh xung đột"});
        });
        ui.add_space(8.0);
        card(ui, |ui| {
            title(ui, "RAM Lexar RGB");
            ui.label(RichText::new(if l.ram.is_empty(){"Chưa nhận diện"}else{&l.ram}).size(17.0).strong());
            ui.label(RichText::new("Hai DIMM · ENE bridge chính hãng").small().color(DIM));
            lighting_controls(ui, &mut self.ram_effect, &mut self.ram_color);
            if primary_button(ui, "Áp dụng cho RAM").clicked() {
                lighting_command("ram", &self.ram_effect, self.ram_color);
            }
        });
        card(ui, |ui| {
            title(ui, "Zotac GPU RGB");
            ui.label(RichText::new(if l.gpu.is_empty(){"Chưa nhận diện"}else{&l.gpu}).size(17.0).strong());
            ui.label(RichText::new("Chỉ nhắm controller ZOTAC · không chạm quạt").small().color(DIM));
            lighting_controls(ui, &mut self.gpu_effect, &mut self.gpu_color);
            if primary_button(ui, "Áp dụng cho GPU").clicked() {
                lighting_command("gpu", &self.gpu_effect, self.gpu_color);
            }
        });
        card(ui, |ui| {
            title(ui, "Đồng bộ chung");
            ui.label(RichText::new("Một hiệu ứng, một màu cho RAM và GPU").color(DIM));
            lighting_controls(ui, &mut self.sync_effect, &mut self.sync_color);
            if primary_button(ui, "Đồng bộ RAM + GPU").clicked() {
                lighting_command("all", &self.sync_effect, self.sync_color);
            }
        });
        card(ui, |ui| {
            title(ui, "Trạng thái điều khiển");
            info_row(ui,"L-Connect 3",if l.lconnect_running{"Đang chạy · Kanali sẽ dừng khi áp dụng"}else{"Đã tắt · không xung đột"});
            info_row(ui,"OpenRGB Zotac",if l.openrgb_available{"Sẵn sàng"}else{"Không tìm thấy"});
            ui.label(RichText::new(&l.message).color(if !l.lconnect_running { GOOD } else { WARN }));
            ui.add_space(8.0);
            if ui.button("Quét lại thiết bị").clicked() { config::send_command("lighting-scan"); }
            ui.label(RichText::new("Kanali không tự khởi động lại L-Connect. Mỗi thiết bị được điều khiển bằng backend riêng.").small().color(DIM));
        });
    }

    fn status_card(&mut self, ui: &mut egui::Ui) {
        let alive = self.alive();
        if alive && self.state.connected && self.notice.is_empty() {
            return;
        }
        card(ui, |ui| {
            if !alive {
                ui.label(
                    RichText::new("Dịch vụ nền chưa chạy nên màn hình sẽ không được giữ sáng.")
                        .color(BAD),
                );
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
        let source = picker
            .add_filter(
                "Video / GIF",
                &["mp4", "mkv", "mov", "webm", "avi", "gif", "m4v", "h264"],
            )
            .pick_file();
        let Some(source) = source else {
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.media_job = Some(rx);
        self.media_note = "Đang chuẩn bị media…".into();
        std::thread::spawn(move || {
            let result = match poster_for {
                Some(name) => {
                    crate::media::make_poster(&source, &name).map(|_| "Đã lưu ảnh xem trước".into())
                }
                None => crate::media::import(source, false),
            };
            let _ = tx.send(result);
        });
    }

    fn display_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            title(ui, "Hiển thị");
            let r = ui
                .add(egui::Slider::new(&mut self.cfg.level, 0..=100).text("Độ sáng (thử nghiệm)"));
            if r.drag_stopped() || (r.changed() && !r.dragged()) {
                self.save();
            }

            ui.collapsing("Nâng cao: video khi bật nguồn và khi chờ", |ui| {
                let mut changed = false;
                changed |= combo(
                    ui,
                    "Khi bật nguồn",
                    &mut self.cfg.poweron,
                    &self.state.files,
                    config::DEFAULT_POWERON,
                );
                changed |= combo(
                    ui,
                    "Khi chờ",
                    &mut self.cfg.standby,
                    &self.state.files,
                    config::DEFAULT_STANDBY,
                );
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
            if ui
                .checkbox(&mut auto, "Tự khởi động cùng Windows")
                .changed()
            {
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
            if ui
                .checkbox(&mut self.cfg.check_updates, "Tự kiểm tra cập nhật")
                .changed()
            {
                self.save();
            }
        });
    }

    fn update_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            let up = &self.state.update;
            title(ui, "Cập nhật");
            ui.label(format!(
                "Phiên bản hiện tại: v{}",
                env!("CARGO_PKG_VERSION")
            ));
            if !up.status.is_empty() {
                ui.label(RichText::new(&up.status).color(DIM));
            }
            if up.busy {
                ui.add(egui::ProgressBar::new(up.progress).show_percentage());
            } else if up.available {
                ui.label(
                    RichText::new(format!("Có bản mới: {}", up.latest))
                        .color(GOOD)
                        .strong(),
                );
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
                ui.label(
                    RichText::new("Kho GitHub (tên-người-dùng/tên-repo):")
                        .small()
                        .color(DIM),
                );
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
        if let Some(rx) = &self.wallpaper_query {
            match rx.try_recv() {
                Ok(Ok(active)) => {
                    self.wallpaper = Some(active);
                    self.wallpaper_note.clear();
                    self.wallpaper_query = None;
                }
                Ok(Err(e)) => {
                    self.wallpaper = None;
                    self.wallpaper_note = e;
                    self.wallpaper_query = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.wallpaper_note = "Kết nối bị gián đoạn".into();
                    self.wallpaper_query = None;
                }
                _ => (),
            }
        }
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
        if self.assets_ready.try_recv().is_ok() {
            self.clips.clear();
        }
        if self.last_read.elapsed() >= Duration::from_millis(1000) {
            if let Some(s) = config::load_opt::<State>(&config::state_path()) {
                if s.update.restart
                    && s.version == env!("CARGO_PKG_VERSION")
                    && config::now_ms().saturating_sub(s.updated_ms) < 7000
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if s.files != self.state.files && !s.files.is_empty() {
                    if let Some(cfg) = config::load_opt::<Config>(&config::config_path()) {
                        let missing = |name: &str| {
                            !s.files.iter().any(|p| p.rsplit('/').next() == Some(name))
                        };
                        if missing(&self.full) {
                            self.full = cfg.playing.clone();
                        }
                        if missing(&self.carousel) {
                            self.carousel = cfg.playing.clone();
                        }
                        if self.draft.split
                            && (missing(&self.draft.left) || missing(&self.draft.right))
                        {
                            self.draft = cfg.layout.clone();
                        }
                        self.cfg = cfg;
                    }
                }
                self.state = s;
            }
            if let Some(id) = config::load_opt::<String>(&config::data_dir().join("giphy-item.json")) {
                let _ = std::fs::remove_file(config::data_dir().join("giphy-item.json"));
                self.gif_id = Some(id.clone());
                if self.media_job.is_none() {
                    let (tx, rx) = std::sync::mpsc::channel();
                    self.media_job = Some(rx);
                    self.media_note = "Đang tải, chuyển đổi và gửi GIF tới PANORAMA…".into();
                    std::thread::spawn(move || { let _ = tx.send(crate::giphy::import_gif(&id)); });
                }
            }
            self.last_read = Instant::now();
        }

        egui::SidePanel::left("navigation")
            .resizable(false)
            .exact_width(if ctx.screen_rect().width() >= 1050.0 {
                202.0
            } else {
                138.0
            })
            .frame(egui::Frame::none().fill(SIDEBAR).inner_margin(14.0))
            .show(ctx, |ui| {
                ui.add_space(10.0);
                let tex = self
                    .logo
                    .get_or_insert_with(|| {
                        let logo =
                            image::load_from_memory(include_bytes!("../../assets/kanali-logo.png"))
                                .expect("logo")
                                .into_rgba8();
                        ui.ctx().load_texture(
                            "kanali-logo",
                            egui::ColorImage::from_rgba_unmultiplied(
                                [logo.width() as usize, logo.height() as usize],
                                logo.as_raw(),
                            ),
                            egui::TextureOptions::LINEAR,
                        )
                    })
                    .clone();
                ui.horizontal(|ui| {
                    ui.image((tex.id(), vec2(38., 38.)));
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("KANALI")
                                .size(22.0)
                                .strong()
                                .color(Color32::WHITE),
                        );
                        ui.label(
                            RichText::new("CONTROL STUDIO")
                                .small()
                                .strong()
                                .color(ACCENT),
                        );
                    });
                });
                ui.add_space(28.0);
                for page in [Page::Lighting, Page::Home, Page::Panorama, Page::Settings] {
                    let active = self.page == page;
                    let label = RichText::new(page.title()).strong().color(if active {
                        Color32::BLACK
                    } else {
                        DIM
                    });
                    if ui
                        .add_sized(
                            [ui.available_width(), 42.0],
                            egui::Button::new(label)
                                .fill(if active { ACCENT } else { Color32::TRANSPARENT })
                                .stroke(if active {
                                    Stroke::NONE
                                } else {
                                    Stroke::new(1.0_f32, Color32::TRANSPARENT)
                                })
                                .rounding(10.0),
                        )
                        .clicked()
                    {
                        self.page = page;
                        self.scroll_top = true;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(
                        RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                            .small()
                            .color(DIM),
                    );
                    let online = self.alive() && self.state.connected;
                    ui.label(
                        RichText::new(if online {
                            "PANORAMA online"
                        } else {
                            "PANORAMA offline"
                        })
                        .small()
                        .color(if online { GOOD } else { DIM }),
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(Margin::same(16.0)))
            .show(ctx, |ui| {
                if self.page == Page::Panorama {self.panorama_header(ui);} else {self.header(ui); }
                let mut scroll=egui::ScrollArea::vertical().id_salt((self.page,self.display_tab));
                if self.scroll_top { scroll=scroll.vertical_scroll_offset(0.0); self.scroll_top=false; }
                scroll.show(ui, |ui| {
                    if matches!(self.page, Page::Home | Page::Panorama) {
                        self.status_card(ui);
                    }
                    match self.page {
                        Page::Lighting => self.lighting_page(ui),
                        Page::Home => {
                            self.home_hero(ui);
                            self.home_dashboard(ui);
                            self.telemetry_detail(ui);
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

        ctx.request_repaint_after(if self.page==Page::Home {Duration::from_millis(100)} else {Duration::from_millis(1000)});
    }
}
