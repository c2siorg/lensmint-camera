use eframe::egui;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI32, Ordering};
use std::path::PathBuf;
use std::collections::HashMap;
use crate::cmd::{DaemonCmd, AppEvent, ChainTarget};

#[derive(PartialEq, Clone)]
enum AppMode {
    Camera,
    Gallery,
    PhotoView(uuid::Uuid),
}

#[derive(Clone, PartialEq)]
enum MintStatus {
    Minting,
    Success,
    Failed,
}

pub struct LensMintApp {
    tx: tokio::sync::mpsc::Sender<DaemonCmd>,
    event_rx: std::sync::mpsc::Receiver<AppEvent>,
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
    mint_states: HashMap<uuid::Uuid, MintStatus>,
}

impl LensMintApp {
    pub fn new(
        tx: tokio::sync::mpsc::Sender<DaemonCmd>, 
        event_rx: std::sync::mpsc::Receiver<AppEvent>,
        shared_frame: Arc<Mutex<Vec<u8>>>,
        shared_focus: Arc<AtomicI32>,
        db: Arc<sled::Db>,
        photos_dir: PathBuf,
    ) -> Self {
        let local_focus = shared_focus.load(Ordering::Relaxed);
        Self { 
            tx, event_rx, shared_frame, shared_focus, local_focus, db, photos_dir,
            zoom_level: 1.0, 
            texture: None,
            mode: AppMode::Camera,
            gallery_cache: Vec::new(),
            thumb_rx: None,
            is_recording: false,
            mint_states: HashMap::new(),
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
                let mut files: Vec<_> = entries
                    .flatten()
                    .filter_map(|entry| {
                        let path = entry.path();
                        let ext = path.extension().and_then(|e| e.to_str())?;
                        if ext != "jpg" && ext != "mp4" { return None; }
                        let modified_time = entry.metadata().ok()?.modified().ok()?;
                        Some((path, modified_time))
                    })
                    .collect();

                files.sort_by(|a, b| b.1.cmp(&a.1));

                for (path, _) in files {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(uuid) = uuid::Uuid::parse_str(stem) {
                            let img_bytes = match db.get(uuid.as_bytes()) {
                                Ok(Some(bytes)) => bytes.to_vec(),
                                _ => {
                                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                                    if ext == "jpg" {
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
                                    } else {
                                        continue;
                                    }
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
        let zoom_delta = ctx.input(|i| i.zoom_delta());
        if zoom_delta != 1.0 {
            self.zoom_level = (self.zoom_level * zoom_delta).clamp(1.0, 3.0);
        }

        let frame = egui::Frame::none().fill(egui::Color32::BLACK).inner_margin(0.0);
        
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            let rect = ui.max_rect(); 
            
            if let Ok(frame_data) = self.shared_frame.lock() {
                if frame_data.len() == 640 * 480 * 4 {
                    let image = egui::ColorImage::from_rgba_unmultiplied([640, 480], &frame_data);
                    let tex = self.texture.get_or_insert_with(|| ctx.load_texture("camera", image.clone(), egui::TextureOptions::LINEAR));
                    tex.set(image, egui::TextureOptions::LINEAR);

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

                    let z_offset = (1.0 - (1.0 / self.zoom_level)) / 2.0;
                    let uv_rect = egui::Rect::from_min_max(
                        egui::pos2(u_min + z_offset, v_min + z_offset),
                        egui::pos2(u_max - z_offset, v_max - z_offset)
                    );

                    ui.painter().image(
                        tex.id(),
                        rect,
                        uv_rect,
                        egui::Color32::WHITE
                    );
                }
            }
        });

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

        egui::Area::new(egui::Id::new("bottom_hud"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(ctx.screen_rect().width());
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                
                let block_h = 70.0;
                let block_w = ui.available_width() / 5.0; 
                let font_size = 18.0;

                ui.horizontal(|ui| {
                    let btn_photo = egui::Button::new(egui::RichText::new("PHOTO").size(font_size).color(egui::Color32::WHITE).strong())
                        .fill(egui::Color32::from_rgb(46, 204, 113))
                        .min_size(egui::vec2(block_w, block_h))
                        .rounding(0.0);
                    if ui.add(btn_photo).clicked() {
                        let _ = self.tx.try_send(DaemonCmd::CapturePhoto(uuid::Uuid::new_v4()));
                    }

                    let video_color = if self.is_recording { egui::Color32::from_rgb(230, 126, 34) } else { egui::Color32::from_rgb(155, 89, 182) };
                    let video_text = if self.is_recording { "STOP" } else { "VIDEO" };
                    let btn_video = egui::Button::new(egui::RichText::new(video_text).size(font_size).color(egui::Color32::WHITE).strong())
                        .fill(video_color)
                        .min_size(egui::vec2(block_w, block_h))
                        .rounding(0.0);
                        
                    if ui.add(btn_video).clicked() {
                        if self.is_recording {
                            let _ = self.tx.try_send(DaemonCmd::StopVideo);
                        } else {
                            let _ = self.tx.try_send(DaemonCmd::StartVideo(uuid::Uuid::new_v4()));
                        }
                        self.is_recording = !self.is_recording;
                    }

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

                    let btn_gallery = egui::Button::new(egui::RichText::new("GALLERY").size(font_size).color(egui::Color32::WHITE).strong())
                        .fill(egui::Color32::from_rgb(142, 68, 173))
                        .min_size(egui::vec2(block_w, block_h))
                        .rounding(0.0);
                    if ui.add(btn_gallery).clicked() {
                        self.mode = AppMode::Gallery;
                        self.load_gallery(ctx.clone());
                    }

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
            egui::ScrollArea::vertical()
                .auto_shrink([false, false]) 
                .show(ui, |ui| {
                    let spacing = 2.0; 
                    let columns = 3.0;
                    let cell_size = (ui.available_width() - (spacing * (columns - 1.0))) / columns - 0.1;

                    ui.style_mut().spacing.item_spacing = egui::vec2(spacing, spacing);
                    ui.horizontal_wrapped(|ui| {
                        let mut selected_uuid = None;
                        for (uuid, tex) in &self.gallery_cache {
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
                ui.horizontal(|ui| {
                    if ui.button(egui::RichText::new("BACK").color(egui::Color32::WHITE).size(16.0)).clicked() {
                        self.mode = AppMode::Gallery;
                    }
                    
                    if let Some(status) = self.mint_states.get(&target_uuid) {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            match status {
                                MintStatus::Minting => {
                                    ui.label(egui::RichText::new("MINTING...").color(egui::Color32::YELLOW).strong());
                                }
                                MintStatus::Success => {
                                    ui.label(egui::RichText::new("ON-CHAIN").color(egui::Color32::GREEN).strong());
                                }
                                MintStatus::Failed => {
                                    ui.label(egui::RichText::new("FAILED").color(egui::Color32::RED).strong());
                                }
                            }
                        });
                    }
                });
            });

        egui::TopBottomPanel::bottom("photo_bot")
            .frame(egui::Frame::none().fill(egui::Color32::BLACK).inner_margin(0.0))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                let block_h = 60.0;
                let block_w = ui.available_width() / 3.0;
                let font_size = 16.0;

                ui.horizontal(|ui| {
                    let btn_evm = egui::Button::new(egui::RichText::new("MINT EVM").size(font_size).color(egui::Color32::WHITE).strong())
                        .fill(egui::Color32::from_rgb(41, 128, 185))
                        .min_size(egui::vec2(block_w, block_h))
                        .rounding(0.0);
                    if ui.add(btn_evm).clicked() {
                        self.mint_states.insert(target_uuid, MintStatus::Minting);
                        let _ = self.tx.try_send(DaemonCmd::Mint(target_uuid, ChainTarget::EVM));
                    }

                    let btn_sol = egui::Button::new(egui::RichText::new("MINT SOL").size(font_size).color(egui::Color32::WHITE).strong())
                        .fill(egui::Color32::from_rgb(142, 68, 173))
                        .min_size(egui::vec2(block_w, block_h))
                        .rounding(0.0);
                    if ui.add(btn_sol).clicked() {
                        self.mint_states.insert(target_uuid, MintStatus::Minting);
                        let _ = self.tx.try_send(DaemonCmd::Mint(target_uuid, ChainTarget::Solana));
                    }

                    let btn_del = egui::Button::new(egui::RichText::new("DELETE").size(font_size).color(egui::Color32::WHITE).strong())
                        .fill(egui::Color32::from_rgb(192, 57, 43))
                        .min_size(egui::vec2(block_w, block_h))
                        .rounding(0.0);
                    if ui.add(btn_del).clicked() {
                        let _ = self.tx.try_send(DaemonCmd::DeletePhoto(target_uuid));
                        self.mode = AppMode::Gallery;
                    }
                });
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
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AppEvent::MintSuccess(uuid, target, tx_hash) => {
                    println!("[UI] Mint success on {:?}, tx_hash: {}", target, tx_hash);
                    self.mint_states.insert(uuid, MintStatus::Success);
                },
                AppEvent::MintFailed(uuid, target, err_msg) => {
                    eprintln!("[UI] Mint failed on {:?}: {}", target, err_msg);
                    self.mint_states.insert(uuid, MintStatus::Failed);
                }
            }
        }

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

        match self.mode {
            AppMode::Camera => self.render_camera(ctx),
            AppMode::Gallery => self.render_gallery(ctx),
            AppMode::PhotoView(uuid) => self.render_photo_view(ctx, uuid),
        }
    }
}