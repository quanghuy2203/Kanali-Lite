use super::*;
use egui::{Align2, FontId, Rect, Sense};

fn segment(ui: &mut egui::Ui, selected: bool, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).strong().color(if selected { Color32::BLACK } else { DIM }))
            .fill(if selected { ACCENT } else { Color32::from_rgb(30, 36, 45) })
            .stroke(Stroke::new(1.0_f32, if selected { ACCENT } else { BORDER }))
            .rounding(9.0),
    )
}

fn title_for(name: &str) -> String {
    crate::presets::title(name)
        .map(str::to_owned)
        .unwrap_or_else(|| label_of(name))
}

impl App {
    fn artwork(&mut self, ui: &egui::Ui, name: &str, rect: Rect, animate: bool) -> bool {
        if name.is_empty() {
            return false;
        }
        let clip = self.clips.entry(name.into()).or_insert_with(|| {
            let path = crate::media::poster(name);
            let (frames, dimensions) =
                std::fs::read(&path)
                    .ok()
                    .map(|f| (f, [240, 116]))
                    .or_else(|| {
                        std::fs::read(path.with_extension("numbered.rgb"))
                            .ok()
                            .map(|f| {
                                (
                                    f,
                                    if crate::presets::title(name).is_some() {
                                        [456, 320]
                                    } else {
                                        [640, 310]
                                    },
                                )
                            })
                    })?;
            let size = dimensions[0] * dimensions[1] * 3;
            // Bound decoded preview memory to 12 small RGB frames (~1 MiB/file).
            if frames.is_empty() || frames.len() > size * 12 || frames.len() % size != 0 {
                return None;
            }
            let texture = ui.ctx().load_texture(
                name,
                egui::ColorImage::from_rgb(dimensions, &frames[..size]),
                Default::default(),
            );
            Some(Clip {
                dimensions,
                frames,
                texture,
                start: Instant::now(),
                current: 0,
            })
        });
        let Some(clip) = clip else { return false };
        let size = clip.dimensions[0] * clip.dimensions[1] * 3;
        let count = clip.frames.len() / size;
        if animate && count > 1 {
            // Cross-fade the compact source frames at display refresh speed. This
            // keeps old 6 FPS posters visually fluid without decoding video in UI.
            let elapsed = clip.start.elapsed().as_secs_f32();
            let position = (elapsed * 6.0) % count as f32;
            let i = position.floor() as usize;
            let next = (i + 1) % count;
            let mix = position.fract();
            if i != clip.current || mix > 0.02 {
                let a = &clip.frames[i * size..(i + 1) * size];
                let b = &clip.frames[next * size..(next + 1) * size];
                let blended: Vec<u8> = a.iter().zip(b).map(|(&x, &y)| {
                    (x as f32 + (y as f32 - x as f32) * mix) as u8
                }).collect();
                clip.texture.set(
                    egui::ColorImage::from_rgb(clip.dimensions, &blended),
                    Default::default(),
                );
                clip.current = i;
            }
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
        let aspect = clip.dimensions[0] as f32 / clip.dimensions[1] as f32;
        let width = rect.width().min(rect.height() * aspect);
        let fit = Rect::from_center_size(rect.center(), vec2(width, width / aspect));
        ui.painter().image(
            clip.texture.id(),
            fit,
            Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
            Color32::WHITE,
        );
        true
    }

    pub(super) fn stage(&mut self, ui: &mut egui::Ui, rect: Rect, name: &str, focus: bool) {
        let response = ui.interact(rect, ui.id().with(("preview-stage", name)), Sense::hover());
        let glow = ui.ctx().animate_bool(response.id, focus || response.hovered());
        let p = ui.painter();
        p.rect_filled(rect, 13., Color32::from_rgb(8, 11, 15));
        if !self.artwork(ui, name, rect.shrink(5.), focus) {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                if name.is_empty() {
                    "Chọn video"
                } else {
                    "Chưa có ảnh nguồn"
                },
                FontId::proportional(14.),
                DIM,
            );
        }
        if !name.is_empty() {
            let band = Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 34.), rect.max);
            ui.painter()
                .rect_filled(band, 10., Color32::from_black_alpha(205));
            ui.painter().text(
                band.left_center() + vec2(12., 0.),
                Align2::LEFT_CENTER,
                short(&title_for(name), (band.width() / 8.) as usize),
                FontId::proportional(13.5),
                Color32::WHITE,
            );
            ui.painter().text(
                band.right_center() - vec2(12., 0.),
                Align2::RIGHT_CENTER,
                if focus { "ĐANG PHÁT" } else { "2240 × 1080" },
                FontId::proportional(11.0),
                if focus { ACCENT } else { DIM },
            );
        }
        ui.painter().rect_stroke(
            rect,
            13.,
            Stroke::new(1.0_f32 + glow, if focus { ACCENT } else { BORDER }),
        );
    }

    fn carousel(&mut self, ui: &mut egui::Ui, files: &[String]) {
        if files.is_empty() {
            ui.label("Kết nối màn hình để đọc danh sách preset.");
            return;
        }
        let mut i = files.iter().position(|f| f == &self.carousel).unwrap_or(0);
        let w = ui.available_width();
        let h = (w * 0.32).clamp(155., 300.);
        let (area, _) = ui.allocate_exact_size(vec2(w, h + 20.), Sense::hover());
        let mut shown = Vec::new();
        for offset in [-2isize, 2, -1, 1, 0] {
            if offset != 0 && files.len() <= offset.unsigned_abs() {
                continue;
            }
            let j = (i as isize + offset).rem_euclid(files.len() as isize) as usize;
            let depth = offset.unsigned_abs() as f32;
            let size = vec2(
                (w * 0.66).min(h * 1.6) - depth * w * 0.055,
                h * (1. - 0.12 * depth),
            );
            let rect =
                Rect::from_center_size(area.center() + vec2(offset as f32 * w * 0.103, 0.), size);
            self.stage(ui, rect, &files[j], offset == 0);
            shown.push(files[j].clone());
        }
        self.clips.retain(|k, _| shown.contains(k));
        for (x, label, step) in [
            (area.left() + 18., "‹", -1isize),
            (area.right() - 18., "›", 1),
        ] {
            let rect = Rect::from_center_size(egui::pos2(x, area.center().y), vec2(34., 34.));
            let resp = ui.interact(rect, ui.id().with(("carousel-arrow", step)), Sense::click());
            ui.painter().circle_filled(
                rect.center(),
                17.,
                if resp.hovered() {
                    ACCENT
                } else {
                    Color32::WHITE
                },
            );
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(24.),
                BG,
            );
            if resp.clicked() {
                i = (i as isize + step).rem_euclid(files.len() as isize) as usize;
            }
        }
        self.carousel = files[i].clone();
        ui.horizontal(|ui| {
            ui.label(format!(
                "{} / {}   {}",
                i + 1,
                files.len(),
                title_for(&self.carousel)
            ));
            if self.clips.get(&self.carousel).is_some_and(|x| x.is_none())
                && ui
                    .add_enabled(
                        self.media_job.is_none(),
                        egui::Button::new("Ghép ảnh từ video"),
                    )
                    .clicked()
            {
                self.pick_media(Some(self.carousel.clone()));
            }
        });
    }

    pub(super) fn panorama_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("PANORAMA");
            if segment(ui, !self.display_tab, "Preset").clicked() { self.display_tab = false; }
            if segment(ui, self.display_tab, "Tùy chỉnh").clicked() { self.display_tab = true; }
            ui.checkbox(&mut self.draft.enabled, "Screen");
            let available = |name: &str| self.state.files.iter().any(|p| file_of(p) == name);
            let valid = if !self.display_tab {
                available(&self.carousel)
            } else if self.draft.split {
                available(&self.draft.left) && available(&self.draft.right)
            } else {
                available(&self.full)
            };
            ui.add_enabled_ui(valid, |ui| {
                if primary_button(ui, "Lưu vào màn hình").clicked() {
                    self.cfg.layout = self.draft.clone();
                    if self.display_tab {
                        self.cfg.playing = self.full.clone();
                    } else {
                        self.cfg.layout.split = false;
                        self.cfg.layout.loop_mode = 0;
                        self.cfg.playing = self.carousel.clone();
                    }
                    let repeat = config::load_opt::<config::Config>(&config::config_path()).as_ref() == Some(&self.cfg);
                    match config::save(&config::config_path(), &self.cfg) {
                        Ok(()) => { if repeat { config::send_command("apply"); } self.media_note.clear(); },
                        Err(e) => self.media_note = e.to_string(),
                    }
                }
            });
            if ui.button("Khôi phục màn hình").on_hover_text("Phát Cooling delivery toàn màn hình và bật đèn nền").clicked() {
                let mut recovered = self.cfg.clone();
                recovered.restore_screen();
                match config::save(&config::config_path(), &recovered) {
                    Ok(()) => {
                        self.full = recovered.playing.clone();
                        self.carousel = recovered.playing.clone();
                        self.draft = recovered.layout.clone();
                        self.cfg = recovered;
                        config::send_command("apply");
                        self.media_note.clear();
                    }
                    Err(e) => self.media_note = e.to_string(),
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(&self.state.message);
            if ui.small_button("Chép chẩn đoán phát").clicked() {
                if let Some(trace) = config::load_opt::<String>(&config::data_dir().join("last-playback.json")) {
                    ui.ctx().copy_text(trace);
                    self.media_note = "Đã chép chẩn đoán phát.".into();
                } else { self.media_note = "Chưa có kết quả lệnh phát từ tản.".into(); }
            }
        });
        if !self.media_note.is_empty() {
            ui.label(&self.media_note);
        }
        if !self.state.media_status.is_empty() {
            ui.label(&self.state.media_status);
        }
        ui.separator();
    }

    pub(super) fn panorama(&mut self, ui: &mut egui::Ui) {
        let files: Vec<String> = self
            .state
            .files
            .iter()
            .map(|p| file_of(p).to_owned())
            .collect();
        if self.display_tab {
            egui::Frame::none().fill(CARD).stroke(Stroke::new(1.0_f32, BORDER))
                .rounding(12.0).inner_margin(Margin::symmetric(14.0, 10.0)).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("BỐ CỤC").small().strong().color(DIM));
                    if segment(ui, !self.draft.split, "Toàn màn hình").clicked() { self.draft.split = false; }
                    if segment(ui, self.draft.split, "Chia đôi").clicked() { self.draft.split = true; }
                    if self.draft.split {
                        ui.separator();
                        if segment(ui, !self.right_slot, "Ô trái").clicked() { self.right_slot = false; }
                        if segment(ui, self.right_slot, "Ô phải").clicked() { self.right_slot = true; }
                    }
                    ui.separator();
                    ui.label(RichText::new("CHẾ ĐỘ PHÁT").small().strong().color(DIM));
                    for (i, label) in ["Một lần", "Lặp", "Ngẫu nhiên"].iter().enumerate() {
                        if segment(ui, self.draft.loop_mode == i as u64, label).clicked() {
                            self.draft.loop_mode = i as u64;
                        }
                    }
                });
            });
            ui.add_space(8.0);
            egui::Frame::none().fill(Color32::from_rgb(20, 26, 34)).rounding(10.0)
                .inner_margin(Margin::symmetric(14.0, 10.0)).show(ui, |ui| {
                    ui.label(if self.draft.split {
                        "Chọn ô trái hoặc phải rồi chọn media bên dưới. Preview đang phát chỉ hiển thị ở Trang chủ."
                    } else {
                        "Chọn media bên dưới. Preview đang phát chỉ hiển thị ở Trang chủ."
                    });
                });
        } else {
            let mut presets: Vec<_> = self
                .state
                .files
                .iter()
                .filter(|p| !p.contains("/media/") && !p.contains("/user/"))
                .map(|p| file_of(p).to_owned())
                .collect();
            presets.sort_by_key(|p| (crate::presets::title(p).is_none(), p.clone()));
            if presets.is_empty() {
                presets = files.clone();
            }
            self.carousel(ui, &presets);
        }
        ui.separator();
        if self.display_tab {
            ui.separator();
            ui.heading("Thư viện media trên PANORAMA");
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        self.media_job.is_none(),
                        egui::Button::new("Thêm video / GIF").min_size(vec2(165., 40.)),
                    )
                    .clicked()
                {
                    self.pick_media(None);
                }
                if ui
                    .add_sized(
                        [250., 42.],
                        egui::Button::new("Mở thư viện Wallpaper Engine"),
                    )
                    .clicked()
                {
                    crate::spawn_self("--library");
                }
                if ui.button("GIPHY").clicked() {
                    crate::spawn_self("--giphy");
                }
                if let Some(id) = self.gif_id.as_ref() {
                    if ui
                        .add_enabled(
                            self.media_job.is_none(),
                            egui::Button::new("Nhập GIF đã chọn"),
                        )
                        .on_hover_text(id)
                        .clicked()
                    {
                        let id = id.clone();
                        let (tx, rx) = std::sync::mpsc::channel();
                        self.media_job = Some(rx);
                        self.media_note = "Đang tải GIF…".into();
                        std::thread::spawn(move || {
                            let _ = tx.send(crate::giphy::import_gif(&id));
                        });
                    }
                }
            });
            let cols = ((ui.available_width() + 8.) / 230.).floor().max(1.) as usize;
            let mut visible = if self.draft.split {
                vec![self.draft.left.clone(), self.draft.right.clone()]
            } else {
                vec![self.full.clone()]
            };
            egui::ScrollArea::vertical()
                .id_salt("custom-media")
                .min_scrolled_height(240.)
                .max_height(480.)
                .show_rows(ui, 142., files.len().div_ceil(cols), |ui, rows| {
                    for row in rows {
                        ui.horizontal(|ui| {
                            let width =
                                (ui.available_width() - 8. * (cols - 1) as f32) / cols as f32;
                            for file in files.iter().skip(row * cols).take(cols) {
                                let (rect, _) = ui.allocate_exact_size(vec2(width, 142.), Sense::hover());
                                let mut select_box = rect;
                                let active = if !self.draft.split {
                                    file == &self.full
                                } else if self.right_slot {
                                    file == &self.draft.right
                                } else {
                                    file == &self.draft.left
                                };
                                self.stage(ui, rect, file, false);
                                if active {
                                    ui.painter().rect_stroke(
                                        rect,
                                        8.,
                                        Stroke::new(2.0_f32, ACCENT),
                                    );
                                }
                                visible.push(file.clone());
                                if let Some(path) = self.state.files.iter().find(|p| {
                                    p.rsplit_once('/').is_some_and(|(dir, n)| {
                                        (dir.ends_with("/media") || dir.ends_with("/user") || n == config::DOWNLOAD_46) && n == file
                                    })
                                }) {
                                    let delete_box = Rect::from_min_size(
                                        rect.right_bottom() - vec2(69., 36.),
                                        vec2(63., 30.),
                                    );
                                    select_box.max.y = delete_box.min.y;
                                    let delete = ui.interact(
                                        delete_box,
                                        ui.id().with(("delete", file)),
                                        Sense::click(),
                                    );
                                    ui.painter().rect_filled(
                                        delete_box,
                                        5.,
                                        if delete.hovered() {
                                            Color32::DARK_RED
                                        } else {
                                            Color32::from_black_alpha(210)
                                        },
                                    );
                                    ui.painter().text(
                                        delete_box.center(),
                                        Align2::CENTER_CENTER,
                                        "Xóa",
                                        FontId::proportional(15.),
                                        Color32::WHITE,
                                    );
                                    if delete.clicked() {
                                        if !self.alive() || !self.state.connected {
                                            self.media_note = "Cần kết nối PANORAMA để xóa".into();
                                        } else if rfd::MessageDialog::new()
                                            .set_title("Xóa media trên PANORAMA")
                                            .set_description(format!(
                                                "Xóa {file} khỏi bộ nhớ màn hình?"
                                            ))
                                            .set_buttons(rfd::MessageButtons::YesNo)
                                            .show()
                                            == rfd::MessageDialogResult::Yes
                                        {
                                            self.media_note = config::request_removal(
                                                path.clone(),
                                                self.state
                                                    .info
                                                    .as_ref()
                                                    .map(|i| i.serial.clone())
                                                    .unwrap_or_default(),
                                            )
                                            .map(|_| "Đã gửi yêu cầu xóa".to_string())
                                            .unwrap_or_else(|e| e);
                                        }
                                    }
                                }
                                let select = ui.interact(select_box, ui.id().with(("select", file)), Sense::click());
                                if select.on_hover_text(file).clicked() {
                                    let slot = if !self.draft.split {
                                        &mut self.full
                                    } else if self.right_slot {
                                        &mut self.draft.right
                                    } else {
                                        &mut self.draft.left
                                    };
                                    slot.clone_from(file);
                                }
                            }
                        });
                    }
                });
            self.clips.retain(|k, _| visible.contains(k));
            ui.separator();
            ui.strong("Wallpaper Engine");
            ui.horizontal_wrapped(|ui| {
                ui.label("Màn hình PC");
                if ui
                    .add(egui::DragValue::new(&mut self.wallpaper_monitor).range(0..=31))
                    .changed()
                {
                    self.wallpaper = None;
                    self.wallpaper_query = None;
                    self.wallpaper_note.clear();
                }
                if ui
                    .add_enabled(
                        self.wallpaper_query.is_none(),
                        egui::Button::new("Kết nối / đọc wallpaper đang chạy"),
                    )
                    .clicked()
                {
                    let monitor = self.wallpaper_monitor;
                    let (tx, rx) = std::sync::mpsc::channel();
                    self.wallpaper_query = Some(rx);
                    self.wallpaper = None;
                    self.wallpaper_note = "Đang hỏi Wallpaper Engine…".into();
                    std::thread::spawn(move || {
                        let _ = tx.send(crate::media::active_wallpaper(monitor));
                    });
                }
                if let Some(active) = &self.wallpaper {
                    ui.label(&active.title);
                    if ui
                        .add_enabled(
                            self.media_job.is_none(),
                            egui::Button::new("Nhập vào PANORAMA"),
                        )
                        .clicked()
                    {
                        let path = active.source.clone();
                        let project = active.project;
                        let (tx, rx) = std::sync::mpsc::channel();
                        self.media_job = Some(rx);
                        self.media_note = "Đang chuẩn bị wallpaper…".into();
                        std::thread::spawn(move || {
                            let _ = tx.send(crate::media::import_wallpaper(path, project));
                        });
                    }
                }
            });

            if !self.wallpaper_note.is_empty() {
                ui.label(&self.wallpaper_note);
            }
            ui.separator();
        }
        ui.collapsing("Cài đặt màn hình", |ui| {
            self.display_card(ui);
            if ui.button("Làm mới thư viện").clicked() {
                config::send_command("refresh");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caption_below_carousel_at_desktop_and_compact_sizes() {
        for width in [470., 900.] {
            let ctx = egui::Context::default();
            let mut app = App::new();
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(width + 30., 760.),
                )),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| {
                        ui.set_width(width);
                        let top = ui.cursor().top();
                        app.carousel(
                            ui,
                            &[
                                "default_05.mp4.h264_2240x1080".into(),
                                "default_06.mp4.h264_2240x1080".into(),
                            ],
                        );
                        let bottom = top + (width * 0.32).clamp(155., 300.) + 20.;
                        assert!(ui.cursor().top() > bottom);
                    })
                    .inner
            });
        }
    }
}
