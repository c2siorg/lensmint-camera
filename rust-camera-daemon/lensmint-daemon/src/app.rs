use eframe::egui;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI32, Ordering};
use std::path::PathBuf;
use crate::cmd::DaemonCmd;

#[derive(PartialEq)]
enum AppMode {
    Camera,
    Gallery,
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

    // --- Issue 8: Gallery States ---
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
            tx, 
            shared_frame, 
            shared_focus,
            local_focus,
            db,
            photos_dir,
            zoom_level: 1.0, 
            texture: None,
            mode: AppMode::Camera,
            gallery_cache: Vec::new(),
            thumb_rx: None,
        }
    }

    // Isolate CPU-bound loading thread to prevent UI freezing
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
                            
                            // Task 2: Cache Miss Fallback
                            let img_bytes = match db.get(uuid.as_bytes()) {
                                Ok(Some(bytes)) => bytes.to_vec(),
                                _ => {
                                    println!("[Gallery] Cache miss for {}. Regenerating...", uuid);
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

                            // Decode JPEG for rendering
                            if let Ok(img) = image::load_from_memory(&img_bytes) {
                                let rgba = img.into_rgba8();
                                let size = [rgba.width() as _, rgba.height() as _];
                                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                                let _ = tx.send((uuid, color_image));
                                ctx.request_repaint(); // Wake UI thread safely
                            }
                        }
                    }
                }
            }
        });
    }

    fn render_camera(&mut self, ctx: &egui::Context) {
        // --- Top Control Panel ---
        egui::TopBottomPanel::top("top_panel").frame(egui::Frame::none().fill(egui::Color32::from_black_alpha(200)).inner_margin(10.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("LensMint OS").color(egui::Color32::WHITE).strong());
                ui.add_space(20.0);
                
                let focus_slider = ui.add(egui::Slider::new(&mut self.local_focus, 0..=1023).text("Focus"));
                if focus_slider.changed() { let _ = self.tx.try_send(DaemonCmd::SetFocus(self.local_focus)); }
                if !focus_slider.dragged() { self.local_focus = self.shared_focus.load(Ordering::Relaxed); }

                ui.add_space(10.0);
                ui.add(egui::Slider::new(&mut self.zoom_level, 1.0..=3.0).text("Zoom"));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("❌ Exit").clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                });
            });
        });

        // --- Bottom Control Panel ---
        egui::TopBottomPanel::bottom("bottom_panel").exact_height(100.0).frame(egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 20))).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(20.0);

                if ui.add(egui::Button::new("🖼 Gallery").min_size(egui::vec2(80.0, 60.0))).clicked() {
                    self.mode = AppMode::Gallery;
                    self.load_gallery(ctx.clone());
                }

                let available_width = ui.available_width();
                ui.add_space((available_width / 2.0) - 40.0); 

                let shutter_btn = egui::Button::new("")
                    .fill(egui::Color32::WHITE)
                    .min_size(egui::vec2(80.0, 80.0))
                    .rounding(egui::Rounding::same(40.0));
                
                if ui.add(shutter_btn).clicked() {
                    let photo_id = uuid::Uuid::new_v4();
                    let _ = self.tx.try_send(DaemonCmd::CapturePhoto(photo_id));
                }
            });
        });

        // --- Viewfinder ---
        egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::BLACK)).show(ctx, |ui| {
            if let Ok(frame_data) = self.shared_frame.lock() {
                if frame_data.len() == 640 * 480 * 4 {
                    let image = egui::ColorImage::from_rgba_unmultiplied([640, 480], &frame_data);
                    let tex = self.texture.get_or_insert_with(|| ctx.load_texture("camera", image.clone(), egui::TextureOptions::LINEAR));
                    tex.set(image, egui::TextureOptions::LINEAR);

                    let offset = (1.0 - (1.0 / self.zoom_level)) / 2.0;
                    let min_uv = egui::pos2(offset, offset);
                    let max_uv = egui::pos2(1.0 - offset, 1.0 - offset);

                    let size = ui.available_size();
                    let aspect = 640.0 / 480.0;
                    let (img_w, img_h) = if size.x / size.y > aspect { (size.y * aspect, size.y) } else { (size.x, size.x / aspect) };

                    ui.centered_and_justified(|ui| {
                        ui.add(egui::Image::new((tex.id(), egui::vec2(img_w, img_h))).uv(egui::Rect::from_min_max(min_uv, max_uv)));
                    });
                }
            }
        });
    }

    fn render_gallery(&mut self, ctx: &egui::Context) {
        // Poll incoming thumbnails from loader thread
        if let Some(rx) = &self.thumb_rx {
            while let Ok((uuid, color_img)) = rx.try_recv() {
                let tex = ctx.load_texture(uuid.to_string(), color_img, egui::TextureOptions::LINEAR);
                self.gallery_cache.push((uuid, tex));
            }
        }

        // Task 4: Strict state sync. Retain only images that exist in DB.
        self.gallery_cache.retain(|(uuid, _)| self.db.contains_key(uuid.as_bytes()).unwrap_or(false));

        egui::TopBottomPanel::top("gallery_top").frame(egui::Frame::none().fill(egui::Color32::from_rgb(30, 30, 30)).inner_margin(15.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("⬅ Back to Camera").clicked() {
                    self.mode = AppMode::Camera;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("Photos: {}", self.gallery_cache.len())).color(egui::Color32::WHITE));
                });
            });
        });

        egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::BLACK)).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut deleted_target: Option<uuid::Uuid> = None;

                    for (uuid, tex) in &self.gallery_cache {
                        ui.allocate_ui(egui::vec2(256.0, 230.0), |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add(egui::Image::new((tex.id(), egui::vec2(256.0, 192.0))));
                                if ui.button("🗑 Delete").clicked() {
                                    deleted_target = Some(*uuid);
                                }
                            });
                        });
                    }

                    if let Some(target) = deleted_target {
                        let _ = self.tx.try_send(DaemonCmd::DeletePhoto(target));
                    }
                });
            });
        });
    }
}

impl eframe::App for LensMintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        match self.mode {
            AppMode::Camera => self.render_camera(ctx),
            AppMode::Gallery => self.render_gallery(ctx),
        }
    }
}