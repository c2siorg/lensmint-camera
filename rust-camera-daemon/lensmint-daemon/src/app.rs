use eframe::egui;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI32, Ordering};
use std::path::PathBuf;
use crate::cmd::DaemonCmd;

#[derive(PartialEq, Clone)]
enum AppMode {
    Camera,
    Gallery,
    PhotoView(uuid::Uuid),
}

pub struct LensMintApp {
    tx: tokio::sync::mpsc::Sender<DaemonCmd>,
    shared_frame: Arc<Mutex<Vec<u8>>>,
    shared_focus: Arc<AtomicI32>,
    db: Arc<sled::Db>,
    photos_dir: PathBuf,
    
    local_focus: i32, 
    zoom_level: f32,  
    texture: Option<egui::TextureHandle>,

    mode: AppMode,
    gallery_cache: Vec<(uuid::Uuid, egui::TextureHandle)>,
    thumb_rx: Option<std::sync::mpsc::Receiver<(uuid::Uuid, egui::ColorImage)>>,
}

impl LensMintApp {
    pub fn new(
        tx: tokio::sync::mpsc::Sender<DaemonCmd>, 
        shared_frame: Arc<Mutex<Vec<u8>>>,
        shared_focus: Arc<AtomicI32>,
        db: Arc<sled::Db>,
        photos_dir: PathBuf,
    ) -> Self {
        let local_focus = shared_focus.load(Ordering::Relaxed);
        Self { 
            tx, shared_frame, shared_focus, local_focus, db, photos_dir,
            zoom_level: 1.0, 
            texture: None,
            mode: AppMode::Camera,
            gallery_cache: Vec::new(),
            thumb_rx: None,
        }
    }

    fn load_gallery(&mut self, ctx: egui::Context) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.thumb_rx = Some(rx);
        self.gallery_cache.clear();

        let db = self.db.clone();
        let photos_dir = self.photos_dir.clone();

        std::thread::spawn(move || {
            if let Ok(entries) = std::fs::read_dir(&photos_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("jpg") { continue; }
                    
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(uuid) = uuid::Uuid::parse_str(stem) {
                            let img_bytes = match db.get(uuid.as_bytes()) {
                                Ok(Some(bytes)) => bytes.to_vec(),
                                _ => {
                                    if let Ok(raw) = image::open(&path) {
                                        let thumb = image::imageops::resize(&raw, 256, 192, image::imageops::FilterType::Triangle);
                                        let mut buf = std::io::Cursor::new(Vec::new());
                                        if thumb.write_to(&mut buf, image::ImageFormat::Jpeg).is_ok() {
                                            let bytes = buf.into_inner();
                                            let _ = db.insert(uuid.as_bytes(), bytes.clone());
                                            let _ = db.flush();
                                            bytes
                                        } else { continue; }
                                    } else { continue; }
                                }
                            };

                            if let Ok(img) = image::load_from_memory(&img_bytes) {
                                let rgba = img.into_rgba8();
                                let size = [rgba.width() as _, rgba.height() as _];
                                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                                let _ = tx.send((uuid, color_image));
                                ctx.request_repaint(); 
                            }
                        }
                    }
                }
            }
        });
    }

    fn render_camera(&mut self, ctx: &egui::Context) {
        // --- Fullscreen Viewfinder ---
        egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::BLACK)).show(ctx, |ui| {
            if let Ok(frame_data) = self.shared_frame.lock() {
                if frame_data.len() == 640 * 480 * 4 {
                    let image = egui::ColorImage::from_rgba_unmultiplied([640, 480], &frame_data);
                    let tex = self.texture.get_or_insert_with(|| ctx.load_texture("camera", image.clone(), egui::TextureOptions::LINEAR));
                    tex.set(image, egui::TextureOptions::LINEAR);

                    let offset = (1.0 - (1.0 / self.zoom_level)) / 2.0;
                    let min_uv = egui::pos2(offset, offset);
                    let max_uv = egui::pos2(1.0 - offset, 1.0 - offset);

                    // Fit entirely to screen ignoring aspect ratio (true fullscreen on small displays)
                    let size = ui.available_size();
                    ui.add(egui::Image::new(&*tex).fit_to_exact_size(size).uv(egui::Rect::from_min_max(min_uv, max_uv)));
                }
            }
        });

        // --- Floating HUD Top ---
        egui::Area::new(egui::Id::new("top_hud"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(ctx.screen_rect().width());
                egui::Frame::none().fill(egui::Color32::from_black_alpha(150)).inner_margin(10.0).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let focus_slider = ui.add(egui::Slider::new(&mut self.local_focus, 0..=1023).text("Focus").show_value(false));
                        if focus_slider.changed() { let _ = self.tx.try_send(DaemonCmd::SetFocus(self.local_focus)); }
                        if !focus_slider.dragged() { self.local_focus = self.shared_focus.load(Ordering::Relaxed); }

                        ui.add_space(10.0);
                        ui.add(egui::Slider::new(&mut self.zoom_level, 1.0..=3.0).text("Zoom").show_value(false));

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("❌").clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                        });
                    });
                });
            });

        // --- Floating HUD Bottom ---
        egui::Area::new(egui::Id::new("bottom_hud"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -20.0))
            .show(ctx, |ui| {
                ui.set_width(ctx.screen_rect().width());
                ui.horizontal_centered(|ui| {
                    ui.add_space(20.0);
                    if ui.add(egui::Button::new("🖼").min_size(egui::vec2(60.0, 60.0))).clicked() {
                        self.mode = AppMode::Gallery;
                        self.load_gallery(ctx.clone());
                    }

                    ui.add_space((ui.available_width() / 2.0) - 30.0); 

                    let shutter_btn = egui::Button::new("")
                        .fill(egui::Color32::WHITE)
                        .min_size(egui::vec2(70.0, 70.0))
                        .rounding(egui::Rounding::same(35.0));
                    
                    if ui.add(shutter_btn).clicked() {
                        let photo_id = uuid::Uuid::new_v4();
                        let _ = self.tx.try_send(DaemonCmd::CapturePhoto(photo_id));
                    }
                });
            });
    }

    fn render_gallery(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.thumb_rx {
            while let Ok((uuid, color_img)) = rx.try_recv() {
                let tex = ctx.load_texture(uuid.to_string(), color_img, egui::TextureOptions::LINEAR);
                self.gallery_cache.push((uuid, tex));
            }
        }

        self.gallery_cache.retain(|(uuid, _)| self.db.contains_key(uuid.as_bytes()).unwrap_or(false));

        egui::TopBottomPanel::top("gallery_top").frame(egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 20)).inner_margin(10.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("⬅ Camera").clicked() { self.mode = AppMode::Camera; }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("{} Photos", self.gallery_cache.len())).color(egui::Color32::WHITE));
                });
            });
        });

        // Mobile Style 3x3 Grid
        egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::BLACK)).show(ctx, |ui| {
            let spacing = 2.0;
            let columns = 3.0;
            let cell_size = (ui.available_width() - (spacing * (columns - 1.0))) / columns;

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.style_mut().spacing.item_spacing = egui::vec2(spacing, spacing);
                ui.horizontal_wrapped(|ui| {
                    let mut selected_uuid = None;

                    for (uuid, tex) in &self.gallery_cache {
                        // Create square image button
                        let img = egui::Image::new(tex)
                            .fit_to_exact_size(egui::vec2(cell_size, cell_size))
                            .uv(egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)));
                        let btn = egui::ImageButton::new(img).frame(false);

                        if ui.add(btn).clicked() {
                            selected_uuid = Some(*uuid);
                        }
                    }

                    if let Some(uuid) = selected_uuid {
                        self.mode = AppMode::PhotoView(uuid);
                    }
                });
            });
        });
    }

    fn render_photo_view(&mut self, ctx: &egui::Context, target_uuid: uuid::Uuid) {
        // Top HUD for Back button
        egui::TopBottomPanel::top("photo_view_top").frame(egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 20)).inner_margin(10.0)).show(ctx, |ui| {
            if ui.button("⬅ Gallery").clicked() {
                self.mode = AppMode::Gallery;
            }
        });

        // Bottom HUD for Delete button
        egui::TopBottomPanel::bottom("photo_view_bot").frame(egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 20)).inner_margin(10.0)).show(ctx, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("🗑 Delete").clicked() {
                    let _ = self.tx.try_send(DaemonCmd::DeletePhoto(target_uuid));
                    self.mode = AppMode::Gallery; // Go back to gallery instantly
                }
            });
        });

        // Central image (Scale up thumbnail to save RAM)
        egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::BLACK)).show(ctx, |ui| {
            if let Some((_, tex)) = self.gallery_cache.iter().find(|(u, _)| *u == target_uuid) {
                let size = ui.available_size();
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new(tex).fit_to_exact_size(size));
                });
            } else {
                // If it was deleted somehow
                self.mode = AppMode::Gallery;
            }
        });
    }
}

impl eframe::App for LensMintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        match self.mode.clone() {
            AppMode::Camera => self.render_camera(ctx),
            AppMode::Gallery => self.render_gallery(ctx),
            AppMode::PhotoView(uuid) => self.render_photo_view(ctx, uuid),
        }
    }
}