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
    
    is_recording: bool,
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
            is_recording: false,
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
                // Collect JPG entries and read file modification metadata
                let mut files: Vec<_> = entries
                    .flatten()
                    .filter_map(|entry| {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) != Some("jpg") { return None; }
                        let modified_time = entry.metadata().ok()?.modified().ok()?;
                        Some((path, modified_time))
                    })
                    .collect();

                // Sort by timestamp descending (newest images first)
                files.sort_by(|a, b| b.1.cmp(&a.1));

                // Process files sequentially based on chronological sort order
                for (path, _) in files {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(uuid) = uuid::Uuid::parse_str(stem) {
                            let img_bytes = match db.get(uuid.as_bytes()) {
                                Ok(Some(bytes)) => bytes.to_vec(),
                                _ => {
                                    if let Ok(raw) = image::open(&path) {
                                        // Fast 4:3 downsample via Triangle filter
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
                                
                                // Stream chronological updates to UI channel
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
        let frame = egui::Frame::none().fill(egui::Color32::BLACK).inner_margin(0.0);
        
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            let rect = ui.max_rect(); 
            
            if let Ok(frame_data) = self.shared_frame.lock() {
                if frame_data.len() == 640 * 480 * 4 {
                    let image = egui::ColorImage::from_rgba_unmultiplied([640, 480], &frame_data);
                    let tex = self.texture.get_or_insert_with(|| ctx.load_texture("camera", image.clone(), egui::TextureOptions::LINEAR));
                    tex.set(image, egui::TextureOptions::LINEAR);

                    // Prevent frame stretching
                    let aspect_cam = 640.0 / 480.0;
                    let aspect_screen = rect.width() / rect.height();
                    
                    let mut u_min = 0.0; let mut v_min = 0.0;
                    let mut u_max = 1.0; let mut v_max = 1.0;
                    
                    if aspect_screen > aspect_cam {
                        let crop = 1.0 - (aspect_cam / aspect_screen);
                        v_min = crop / 2.0;
                        v_max = 1.0 - (crop / 2.0);
                    } else {
                        let crop = 1.0 - (aspect_screen / aspect_cam);
                        u_min = crop / 2.0;
                        u_max = 1.0 - (crop / 2.0);
                    }

                    // Apply layout digital zoom
                    let z_offset = (1.0 - (1.0 / self.zoom_level)) / 2.0;
                    let uv_rect = egui::Rect::from_min_max(
                        egui::pos2(u_min + z_offset, v_min + z_offset),
                        egui::pos2(u_max - z_offset, v_max - z_offset)
                    );

                    // Painter rendering for edge-to-edge view
                    ui.painter().image(
                        tex.id(),
                        rect,
                        uv_rect,
                        egui::Color32::WHITE
                    );
                }
            }
        });

        // Top HUD (Status and Focus)
        egui::Area::new(egui::Id::new("top_hud"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(ctx.screen_rect().width());
                let alpha_bg = egui::Color32::from_black_alpha(120);
                egui::Frame::none().fill(alpha_bg).inner_margin(8.0).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("FOCUS: {}", self.local_focus)).color(egui::Color32::WHITE).size(14.0));
                        let focus_slider = ui.add(egui::Slider::new(&mut self.local_focus, 0..=1023).show_value(false));
                        if focus_slider.changed() { let _ = self.tx.try_send(DaemonCmd::SetFocus(self.local_focus)); }
                        if !focus_slider.dragged() { self.local_focus = self.shared_focus.load(Ordering::Relaxed); }
                    });
                });
            });

        // Bottom HUD (5-column hardware dock layout)
        egui::Area::new(egui::Id::new("bottom_hud"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(ctx.screen_rect().width());
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                
                let block_h = 70.0;
                let block_w = ui.available_width() / 5.0; 
                let font_size = 18.0;

                ui.horizontal(|ui| {
                    // 1. PHOTO
                    let btn_photo = egui::Button::new(egui::RichText::new("PHOTO").size(font_size).color(egui::Color32::WHITE).strong())
                        .fill(egui::Color32::from_rgb(46, 204, 113))
                        .min_size(egui::vec2(block_w, block_h))
                        .rounding(0.0);
                    if ui.add(btn_photo).clicked() {
                        let _ = self.tx.try_send(DaemonCmd::CapturePhoto(uuid::Uuid::new_v4()));
                    }

                    // 2. VIDEO
                    let video_color = if self.is_recording { egui::Color32::from_rgb(230, 126, 34) } else { egui::Color32::from_rgb(155, 89, 182) };
                    let video_text = if self.is_recording { "STOP" } else { "VIDEO" };
                    let btn_video = egui::Button::new(egui::RichText::new(video_text).size(font_size).color(egui::Color32::WHITE).strong())
                        .fill(video_color)
                        .min_size(egui::vec2(block_w, block_h))
                        .rounding(0.0);
                    if ui.add(btn_video).clicked() {
                        self.is_recording = !self.is_recording;
                    }

                    // 3. ZOOM
                    ui.vertical(|ui| {
                        let btn_zoom_in = egui::Button::new(egui::RichText::new("ZOOM +").size(font_size - 2.0).color(egui::Color32::WHITE).strong())
                            .fill(egui::Color32::from_rgb(52, 152, 219))
                            .min_size(egui::vec2(block_w, block_h / 2.0))
                            .rounding(0.0);
                        if ui.add(btn_zoom_in).clicked() { self.zoom_level = (self.zoom_level + 0.2).min(3.0); }

                        let btn_zoom_out = egui::Button::new(egui::RichText::new("ZOOM -").size(font_size - 2.0).color(egui::Color32::WHITE).strong())
                            .fill(egui::Color32::from_rgb(41, 128, 185))
                            .min_size(egui::vec2(block_w, block_h / 2.0))
                            .rounding(0.0);
                        if ui.add(btn_zoom_out).clicked() { self.zoom_level = (self.zoom_level - 0.2).max(1.0); }
                    });

                    // 4. GALLERY
                    let btn_gallery = egui::Button::new(egui::RichText::new("GALLERY").size(font_size).color(egui::Color32::WHITE).strong())
                        .fill(egui::Color32::from_rgb(142, 68, 173))
                        .min_size(egui::vec2(block_w, block_h))
                        .rounding(0.0);
                    if ui.add(btn_gallery).clicked() {
                        self.mode = AppMode::Gallery;
                        self.load_gallery(ctx.clone());
                    }

                    // 5. QUIT
                    let btn_quit = egui::Button::new(egui::RichText::new("QUIT").size(font_size).color(egui::Color32::WHITE).strong())
                        .fill(egui::Color32::from_rgb(231, 76, 60))
                        .min_size(egui::vec2(block_w, block_h))
                        .rounding(0.0);
                    if ui.add(btn_quit).clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
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

        let frame = egui::Frame::none().fill(egui::Color32::BLACK).inner_margin(0.0);
        
        egui::TopBottomPanel::top("gallery_top")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(30, 30, 30)).inner_margin(12.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(egui::RichText::new("BACK").color(egui::Color32::WHITE).size(16.0)).clicked() {
                        self.mode = AppMode::Camera;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(format!("{} PHOTOS", self.gallery_cache.len())).color(egui::Color32::GRAY).size(16.0));
                    });
                });
            });

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            // Force the scroll container to stretch to maximum width
            egui::ScrollArea::vertical()
                .auto_shrink([false, false]) 
                .show(ui, |ui| {
                    let spacing = 2.0; 
                    let columns = 3.0;
                    
                    // Width extraction directly inside scroll content to avoid wrapping errors
                    let cell_size = (ui.available_width() - (spacing * (columns - 1.0))) / columns - 0.1;

                    ui.style_mut().spacing.item_spacing = egui::vec2(spacing, spacing);
                    ui.horizontal_wrapped(|ui| {
                        let mut selected_uuid = None;
                        for (uuid, tex) in &self.gallery_cache {
                            // Square crop layout for 4:3 inputs
                            let img = egui::Image::new(tex)
                                .fit_to_exact_size(egui::vec2(cell_size, cell_size))
                                .uv(egui::Rect::from_min_max(egui::pos2(0.125, 0.0), egui::pos2(0.875, 1.0))); 
                            
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
        let frame = egui::Frame::none().fill(egui::Color32::BLACK).inner_margin(0.0);

        egui::TopBottomPanel::top("photo_top")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(30, 30, 30)).inner_margin(12.0))
            .show(ctx, |ui| {
                if ui.button(egui::RichText::new("BACK").color(egui::Color32::WHITE).size(16.0)).clicked() {
                    self.mode = AppMode::Gallery;
                }
            });

        egui::TopBottomPanel::bottom("photo_bot")
            .frame(egui::Frame::none().fill(egui::Color32::BLACK).inner_margin(0.0))
            .show(ctx, |ui| {
                let btn_del = egui::Button::new(egui::RichText::new("DELETE").size(18.0).color(egui::Color32::WHITE).strong())
                    .fill(egui::Color32::from_rgb(231, 76, 60))
                    .min_size(egui::vec2(ui.available_width(), 60.0))
                    .rounding(0.0);
                if ui.add(btn_del).clicked() {
                    let _ = self.tx.try_send(DaemonCmd::DeletePhoto(target_uuid));
                    self.mode = AppMode::Gallery; 
                }
            });

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            if let Some((_, tex)) = self.gallery_cache.iter().find(|(u, _)| *u == target_uuid) {
                let size = ui.available_size();
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new(tex).fit_to_exact_size(size).maintain_aspect_ratio(true));
                });
            } else {
                self.mode = AppMode::Gallery;
            }
        });
    }
}

impl eframe::App for LensMintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Enforce hard sharp corners globally across layout bounds
        let mut style = (*ctx.style()).clone();
        style.visuals.window_rounding = egui::Rounding::same(0.0);
        style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(0.0);
        style.visuals.widgets.inactive.rounding = egui::Rounding::same(0.0);
        style.visuals.widgets.hovered.rounding = egui::Rounding::same(0.0);
        style.visuals.widgets.active.rounding = egui::Rounding::same(0.0);
        ctx.set_style(style);

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