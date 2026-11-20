use super::*;
use egui::{Align2, FontId, Rect, Sense};

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
                        std::fs::read(path.with_extension("preset.rgb"))
                            .ok()
                            .map(|f| (f, [640, 310]))
                    })?;
            let size = dimensions[0] * dimensions[1] * 3;
            if frames.is_empty() || frames.len() > size * 8 || frames.len() % size != 0 {
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
            let i = (clip.start.elapsed().as_millis() / 500) as usize % count;
            if i != clip.current {
                clip.texture.set(
                    egui::ColorImage::from_rgb(
                        clip.dimensions,
                        &clip.frames[i * size..(i + 1) * size],
                    ),
                    Default::default(),
                );
                clip.current = i;
            }
            ui.ctx().request_repaint_after(Duration::from_millis(500));
        }
        ui.painter().image(
            clip.texture.id(),
            rect,
            Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
            Color32::WHITE,
        );
        true
    }

    fn stage(&mut self, ui: &mut egui::Ui, rect: Rect, name: &str, focus: bool) {
        let p = ui.painter();
        p.rect_filled(rect, 8., Color32::from_rgb(14, 16, 19));
        if !self.artwork(ui, name, rect.shrink(3.), focus) {
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
            let band = Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.top() + 26.));
            ui.painter()
                .rect_filled(band, 4., Color32::from_black_alpha(176));
            ui.painter().text(
                band.left_center() + vec2(9., 0.),
                Align2::LEFT_CENTER,
                short(&title_for(name), (band.width() / 8.) as usize),
                FontId::proportional(14.),
                Color32::WHITE,
            );
        }
        ui.painter().rect_stroke(
            rect,
            8.,
            Stroke::new(1.0_f32, if focus { ACCENT } else { BORDER }),
        );
    }

    fn carousel(&mut self, ui: &mut egui::Ui, files: &[String]) {
        if files.is_empty() {
            ui.label("Kết nối màn hình để đọc danh sách preset.");
            return;
        }
        let mut i = files.iter().position(|f| f == &self.carousel).unwrap_or(0);
        let w = ui.available_width();
        let h = (w * 0.36).clamp(155., 350.);
        let (area, _) = ui.allocate_exact_size(vec2(w, h + 20.), Sense::hover());
        let mut shown = Vec::new();
        for offset in [-2isize, 2, -1, 1, 0] {
            if offset != 0 && files.len() <= offset.unsigned_abs() {
                continue;
            }
            let j = (i as isize + offset).rem_euclid(files.len() as isize) as usize;
            let depth = offset.unsigned_abs() as f32;
            let size = vec2(w * 0.66 - depth * w * 0.08, h * (1. - 0.12 * depth));
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
            if ui
                .put(
                    rect,
                    egui::Button::new(RichText::new(label).size(24.).color(BG))
                        .fill(Color32::WHITE)
                        .rounding(17.),
                )
                .clicked()
            {
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

    fn metrics(&mut self, ui: &mut egui::Ui) {
        ui.strong("System Information Display · tối đa 3 mục");
        ui.horizontal_wrapped(|ui| {
            for (key, label) in [
                ("CPU Temperature", "CPU Temperature"),
                ("CPU Frequency", "CPU Frequency"),
                ("CPU", "CPU Usage"),
                ("CPU Power", "CPU Power"),
                ("GPU Temperature", "GPU Temperature"),
                ("GPU Frequency", "GPU Frequency"),
                ("GPU Usage", "GPU Usage"),
                ("GPU Power", "GPU Power"),
                ("Memory Frequency", "Memory Frequency"),
                ("RAM", "Memory Utilization"),
                ("Date & Time", "Date & Time"),
            ] {
                let mut selected = self.draft.metrics.iter().any(|m| m == key);
                let enabled = selected || self.draft.metrics.len() < 3;
                if ui
                    .add_enabled(enabled, egui::Checkbox::new(&mut selected, label))
                    .on_hover_text(if matches!(key, "CPU" | "RAM" | "Date & Time") {
                        "Windows"
                    } else {
                        "Cần cảm biến từ LibreHardwareMonitor đang chạy"
                    })
                    .changed()
                {
                    self.draft.metrics.retain(|m| m != key);
                    if selected {
                        self.draft.metrics.push(key.into());
                    }
                }
            }
        });
        ui.add_enabled_ui(!self.draft.metrics.is_empty(), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.draft.dark_text, "Chữ đen");
                egui::ComboBox::from_id_salt("align")
                    .selected_text(
                        ["Trái", "Giữa", "Phải"][(self.draft.alignment.clamp(1, 3) - 1) as usize],
                    )
                    .show_ui(ui, |ui| {
                        for (i, v) in ["Trái", "Giữa", "Phải"].iter().enumerate() {
                            ui.selectable_value(&mut self.draft.alignment, i as u64 + 1, *v);
                        }
                    });
                ui.add(egui::Slider::new(&mut self.draft.font_size, 18..=64).text("Cỡ chữ"));
            })
        });
    }

    pub(super) fn panorama(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("PANORAMA");
            ui.selectable_value(&mut self.display_tab, false, "Preset");
            ui.selectable_value(&mut self.display_tab, true, "Customization");
            ui.checkbox(&mut self.draft.enabled, "Screen");
        });
        ui.separator();
        let files: Vec<String> = self
            .state
            .files
            .iter()
            .map(|p| file_of(p).to_owned())
            .collect();
        if self.display_tab {
            ui.horizontal_wrapped(|ui| {
                ui.radio_value(&mut self.draft.split, false, "Full Screen");
                ui.radio_value(&mut self.draft.split, true, "Screen Splitting");
                egui::ComboBox::from_label("Play Mode")
                    .selected_text(
                        ["Single", "Loop", "Shuffle"][self.draft.loop_mode.min(2) as usize],
                    )
                    .show_ui(ui, |ui| {
                        for (i, label) in ["Single", "Loop", "Shuffle"].iter().enumerate() {
                            ui.selectable_value(&mut self.draft.loop_mode, i as u64, *label);
                        }
                    });
            });
            let w = ui.available_width();
            let h = (w * 0.32).clamp(145., 280.);
            let (area, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
            let names = if self.draft.split {
                vec![self.draft.left.clone(), self.draft.right.clone()]
            } else {
                vec![self.full.clone()]
            };
            for (i, name) in names.iter().enumerate() {
                let width = w / names.len() as f32;
                let r = Rect::from_min_size(area.min + vec2(i as f32 * width, 0.), vec2(width, h))
                    .shrink(3.);
                self.stage(
                    ui,
                    r,
                    name,
                    !self.draft.split || (i == 1) == self.right_slot,
                );
                if ui
                    .interact(r, ui.id().with(("slot", i)), Sense::click())
                    .clicked()
                {
                    self.right_slot = i == 1;
                }
            }

            let target = if self.draft.split && self.right_slot {
                1
            } else {
                0
            };
            if let Some(name) = names.get(target).filter(|n| !n.is_empty()) {
                if self.clips.get(name).is_some_and(|p| p.is_none())
                    && ui
                        .add_enabled(
                            self.media_job.is_none(),
                            egui::Button::new("Ghép video để tạo preview"),
                        )
                        .clicked()
                {
                    self.pick_media(Some(name.clone()));
                }
            }
            ui.label(if self.draft.split {
                "Chọn ô trái hoặc phải, rồi chọn video trong thư viện."
            } else {
                "Chọn video toàn màn hình trong thư viện."
            });
        } else {
            let mut presets: Vec<_> = self
                .state
                .files
                .iter()
                .filter(|p| !p.contains("/media/"))
                .map(|p| file_of(p).to_owned())
                .collect();
            presets.sort_by_key(|p| (crate::presets::title(p).is_none(), p.clone()));
            if presets.is_empty() {
                presets = files.clone();
            }
            self.carousel(ui, &presets);
        }
        ui.separator();
        self.metrics(ui);
        let valid = if !self.display_tab {
            files.contains(&self.carousel)
        } else if self.draft.split {
            files.contains(&self.draft.left) && files.contains(&self.draft.right)
        } else {
            files.contains(&self.full)
        };
        ui.add_enabled_ui(valid, |ui| {
            if primary_button(ui, "Lưu vào màn hình").clicked() {
                self.cfg.layout = self.draft.clone();
                if self.display_tab {
                    self.cfg.playing = self.full.clone();
                } else {
                    self.cfg.layout.split = false;
                    self.cfg.playing = self.carousel.clone();
                }
                match config::save(&config::config_path(), &self.cfg) {
                    Ok(()) => self.media_note = "Đã lưu cấu hình.".into(),
                    Err(e) => self.media_note = e.to_string(),
                }
            }
        });
        if self.display_tab {
            ui.separator();
            ui.strong("Upload Media Library");
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        self.media_job.is_none(),
                        egui::Button::new("Thêm video / GIF"),
                    )
                    .clicked()
                {
                    self.pick_media(None);
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
                            let _ = tx.send(crate::workshop::import_gif(&id));
                        });
                    }
                }
                if ui.button("Steam Workshop").clicked() {
                    crate::spawn_self("--workshop");
                }
                if let Some(id) = self.workshop_id {
                    if ui
                        .add_enabled(
                            self.media_job.is_none(),
                            egui::Button::new(format!("Nhập mục {id}")),
                        )
                        .clicked()
                    {
                        let (tx, rx) = std::sync::mpsc::channel();
                        self.media_job = Some(rx);
                        self.media_note = "Đang chờ Steam tải wallpaper…".into();
                        std::thread::spawn(move || {
                            let _ = tx.send(crate::workshop::import(id));
                        });
                    }
                }
            });
            ui.small("Trong Workshop, đăng nhập Steam, tìm wallpaper và chọn Subscribe. Quay lại đây để nhập mục vừa chọn.");
            let cols = ((ui.available_width() + 8.) / 148.).floor().max(1.) as usize;
            let mut visible = if self.draft.split {
                vec![self.draft.left.clone(), self.draft.right.clone()]
            } else {
                vec![self.full.clone()]
            };
            egui::ScrollArea::vertical()
                .id_salt("custom-media")
                .max_height(210.)
                .show_rows(ui, 83., files.len().div_ceil(cols), |ui, rows| {
                    for row in rows {
                        ui.horizontal(|ui| {
                            let width =
                                (ui.available_width() - 8. * (cols - 1) as f32) / cols as f32;
                            for file in files.iter().skip(row * cols).take(cols) {
                                let (rect, response) =
                                    ui.allocate_exact_size(vec2(width, 78.), Sense::click());
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
                                if response.on_hover_text(file).clicked() {
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
        }
        if !self.media_note.is_empty() {
            ui.label(&self.media_note);
        }
        if !self.state.media_status.is_empty() {
            ui.label(&self.state.media_status);
        }
        ui.collapsing("Cài đặt màn hình", |ui| {
            self.display_card(ui);
            if ui.button("Làm mới thư viện").clicked() {
                config::send_command("refresh");
            }
        });
    }
}
