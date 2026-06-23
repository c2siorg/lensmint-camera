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
    Settings,
    PhotoView(uuid::Uuid),
}

#[derive(Clone, PartialEq)]
enum MintStatus {
    Minting,
    Success,
    Failed,
}

#[derive(PartialEq, Clone)]
enum SelectedChain {
    EVM,
    Solana,
}

#[derive(PartialEq, Clone)]
enum CaptureMode {
    Photo,
    Video,
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
    gallery_cache: Vec<(uuid::Uuid, egui::TextureHandle, bool)>,
    thumb_rx: Option<std::sync::mpsc::Receiver<(uuid::Uuid, egui::ColorImage, bool)>>,
    capture_mode: CaptureMode,
    is_recording: bool,
    mint_states: HashMap<uuid::Uuid, MintStatus>,
    default_chain: SelectedChain,
    master_wallet: String,
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
            capture_mode: CaptureMode::Photo,
            is_recording: false,
            mint_states: HashMap::new(),
            default_chain: SelectedChain::EVM,
            master_wallet: "0xLensMint...Camera".to_string(),
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
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        let is_mp4 = ext == "mp4";
                        let is_jpg = ext == "jpg";
                        if !is_jpg && !is_mp4 { return None; }
                        let modified_time = entry.metadata().ok()?.modified().ok()?;
                        Some((path, modified_time, is_mp4))
                    })
                    .collect();

                files.sort_by(|a, b| b.1.cmp(&a.1));

                for (path, _, is_video) in files {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(uuid) = uuid::Uuid::parse_str(stem) {
                            let img_bytes = match db.get(uuid.as_bytes()) {
                                Ok(Some(bytes)) => bytes.to_vec(),
                                _ => {
                                    if !is_video {
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
                                let _ = tx.send((uuid, color_image, is_video));
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

                    ui.painter().image(tex.id(), rect, uv_rect, egui::Color32::WHITE);
                }
            }
        });

        egui::Area::new(egui::Id::new("top_right_osd"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 16.0))
            .show(ctx, |ui| {
                if ui.add(egui::Button::new(egui::RichText::new("QUIT").size(12.0).strong().color(egui::Color32::WHITE))
                    .fill(egui::Color32::from_black_alpha(150))
                    .rounding(16.0)).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

        egui::Area::new(egui::Id::new("rec_indicator"))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 32.0))
            .show(ctx, |ui| {
                if self.is_recording {
                    egui::Frame::none()
                        .fill(egui::Color32::from_black_alpha(150))
                        .rounding(16.0)
                        .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                                ui.painter().circle_filled(rect.center(), 5.0, egui::Color32::RED);
                                ui.label(egui::RichText::new("REC").color(egui::Color32::WHITE).strong());
                            });
                        });
                }
            });

        egui::Area::new(egui::Id::new("bottom_osd"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -16.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(20.0, 0.0);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        ui.horizontal(|ui| {
                            if ui.selectable_label(self.capture_mode == CaptureMode::Photo, egui::RichText::new("PHOTO").size(12.0).strong()).clicked() {
                                self.capture_mode = CaptureMode::Photo;
                            }
                            if ui.selectable_label(self.capture_mode == CaptureMode::Video, egui::RichText::new("VIDEO").size(12.0).strong()).clicked() {
                                self.capture_mode = CaptureMode::Video;
                            }
                        });
                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            ui.allocate_ui(egui::vec2(80.0, 64.0), |ui| {
                                ui.centered_and_justified(|ui| {
                                    if ui.add(egui::Button::new(egui::RichText::new("GALLERY").size(12.0).strong())
                                        .fill(egui::Color32::from_black_alpha(150))
                                        .rounding(24.0)).clicked() {
                                        self.mode = AppMode::Gallery;
                                        self.load_gallery(ctx.clone());
                                    }
                                });
                            });

                            ui.add_space(20.0);

                            let shutter_size = egui::vec2(72.0, 72.0);
                            let (rect, response) = ui.allocate_exact_size(shutter_size, egui::Sense::click());
                            let center = rect.center();
                            ui.painter().circle_stroke(center, 34.0, egui::Stroke::new(3.0, egui::Color32::WHITE));
                            
                            let inner_radius = if response.is_pointer_button_down_on() { 26.0 } else { 30.0 };
                            let inner_color = if self.capture_mode == CaptureMode::Video {
                                egui::Color32::from_rgb(231, 76, 60)
                            } else {
                                egui::Color32::WHITE
                            };
                            ui.painter().circle_filled(center, inner_radius, inner_color);

                            if response.clicked() {
                                match self.capture_mode {
                                    CaptureMode::Photo => {
                                        let _ = self.tx.try_send(DaemonCmd::CapturePhoto(uuid::Uuid::new_v4()));
                                    },
                                    CaptureMode::Video => {
                                        if self.is_recording {
                                            let _ = self.tx.try_send(DaemonCmd::StopVideo);
                                        } else {
                                            let _ = self.tx.try_send(DaemonCmd::StartVideo(uuid::Uuid::new_v4()));
                                        }
                                        self.is_recording = !self.is_recording;
                                    }
                                }
                            }

                            ui.add_space(20.0);

                            ui.allocate_ui(egui::vec2(80.0, 64.0), |ui| {
                                ui.centered_and_justified(|ui| {
                                    if ui.add(egui::Button::new(egui::RichText::new("SETTINGS").size(12.0).strong())
                                        .fill(egui::Color32::from_black_alpha(150))
                                        .rounding(24.0)).clicked() {
                                        self.mode = AppMode::Settings;
                                    }
                                });
                            });
                        });
                    });
                });
            });
    }

    fn render_gallery(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.thumb_rx {
            while let Ok((uuid, color_img, is_video)) = rx.try_recv() {
                let tex = ctx.load_texture(uuid.to_string(), color_img, egui::TextureOptions::LINEAR);
                self.gallery_cache.push((uuid, tex, is_video));
            }
        }
        self.gallery_cache.retain(|(uuid, _, _)| self.db.contains_key(uuid.as_bytes()).unwrap_or(false));

        let bg_frame = egui::Frame::none().fill(egui::Color32::from_rgb(18, 18, 20)).inner_margin(0.0);
        
        egui::TopBottomPanel::top("gallery_top")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(25, 25, 28)).inner_margin(egui::Margin::symmetric(16.0, 12.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(egui::RichText::new("◀ CAMERA").color(egui::Color32::WHITE).size(14.0)).fill(egui::Color32::TRANSPARENT)).clicked() {
                        self.mode = AppMode::Camera;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(format!("{} MEDIA", self.gallery_cache.len())).color(egui::Color32::from_gray(150)).size(14.0).strong());
                    });
                });
            });

        egui::CentralPanel::default().frame(bg_frame).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let spacing = 4.0;
                let columns = 3.0;
                let available_w = ui.available_width() - (spacing * (columns - 1.0));
                let cell_size = (available_w / columns).floor();
                let mut selected_uuid = None;
                
                for chunk in self.gallery_cache.chunks(3) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(spacing, spacing);
                        for (uuid, tex, is_video) in chunk {
                            let (rect, response) = ui.allocate_exact_size(egui::vec2(cell_size, cell_size), egui::Sense::click());
                            
                            if ui.is_rect_visible(rect) {
                                let uv = egui::Rect::from_min_max(egui::pos2(0.125, 0.0), egui::pos2(0.875, 1.0));
                                ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);
                                
                                if *is_video {
                                    let center = rect.center();
                                    ui.painter().circle_filled(center, 18.0, egui::Color32::from_black_alpha(150));
                                    let p1 = center + egui::vec2(-4.0, -6.0);
                                    let p2 = center + egui::vec2(-4.0, 6.0);
                                    let p3 = center + egui::vec2(6.0, 0.0);
                                    ui.painter().add(egui::Shape::convex_polygon(vec![p1, p2, p3], egui::Color32::WHITE, egui::Stroke::NONE));
                                }
                            }
                            if response.clicked() {
                                selected_uuid = Some(*uuid);
                            }
                        }
                    });
                    ui.add_space(spacing);
                }
                if let Some(uuid) = selected_uuid {
                    self.mode = AppMode::PhotoView(uuid);
                }
            });
        });
    }

    fn render_photo_view(&mut self, ctx: &egui::Context, target_uuid: uuid::Uuid) {
        let frame = egui::Frame::none().fill(egui::Color32::BLACK).inner_margin(0.0);

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            if let Some((_, tex, is_video)) = self.gallery_cache.iter().find(|(u, _, _)| *u == target_uuid) {
                ui.centered_and_justified(|ui| {
                    let rect = ui.max_rect();
                    ui.painter().image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0,0.0), egui::pos2(1.0,1.0)), egui::Color32::WHITE);
                    
                    if *is_video {
                        let center = rect.center();
                        ui.painter().circle_filled(center, 32.0, egui::Color32::from_black_alpha(180));
                        let p1 = center + egui::vec2(-8.0, -12.0);
                        let p2 = center + egui::vec2(-8.0, 12.0);
                        let p3 = center + egui::vec2(12.0, 0.0);
                        ui.painter().add(egui::Shape::convex_polygon(vec![p1, p2, p3], egui::Color32::WHITE, egui::Stroke::NONE));
                    }
                });
            } else {
                self.mode = AppMode::Gallery;
            }
        });

        egui::Area::new(egui::Id::new("photo_top_osd"))
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(16.0, 16.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(egui::RichText::new("◀ BACK").size(14.0).strong())
                        .fill(egui::Color32::from_black_alpha(180))
                        .rounding(16.0)).clicked() {
                        self.mode = AppMode::Gallery;
                    }
                    
                    if let Some(status) = self.mint_states.get(&target_uuid) {
                        ui.add_space(10.0);
                        let (text, color) = match status {
                            MintStatus::Minting => ("MINTING...", egui::Color32::YELLOW),
                            MintStatus::Success => ("ON-CHAIN", egui::Color32::from_rgb(46, 204, 113)),
                            MintStatus::Failed => ("FAILED", egui::Color32::RED),
                        };
                        egui::Frame::none().fill(egui::Color32::from_black_alpha(180)).rounding(16.0).inner_margin(egui::Margin::symmetric(12.0, 6.0)).show(ui, |ui| {
                            ui.label(egui::RichText::new(text).color(color).strong().size(12.0));
                        });
                    }
                });
            });

        egui::Area::new(egui::Id::new("photo_bot_osd"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -20.0))
            .show(ctx, |ui| {
                egui::Frame::none().fill(egui::Color32::from_black_alpha(180)).rounding(24.0).inner_margin(egui::Margin::symmetric(16.0, 8.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let btn_mint = egui::Button::new(egui::RichText::new("MINT").size(14.0).color(egui::Color32::WHITE).strong())
                            .fill(egui::Color32::TRANSPARENT)
                            .min_size(egui::vec2(80.0, 32.0));
                            
                        if ui.add(btn_mint).clicked() {
                            self.mint_states.insert(target_uuid, MintStatus::Minting);
                            let target = if self.default_chain == SelectedChain::EVM { ChainTarget::EVM } else { ChainTarget::Solana };
                            let _ = self.tx.try_send(DaemonCmd::Mint(target_uuid, target));
                        }

                        ui.add(egui::Separator::default().vertical());

                        let btn_del = egui::Button::new(egui::RichText::new("DELETE").size(14.0).color(egui::Color32::from_rgb(231, 76, 60)).strong())
                            .fill(egui::Color32::TRANSPARENT)
                            .min_size(egui::vec2(80.0, 32.0));
                            
                        if ui.add(btn_del).clicked() {
                            let _ = self.tx.try_send(DaemonCmd::DeletePhoto(target_uuid));
                            self.mode = AppMode::Gallery;
                        }
                    });
                });
            });
    }

    fn render_settings(&mut self, ctx: &egui::Context) {
        let frame = egui::Frame::none().fill(egui::Color32::from_rgb(18, 18, 20)).inner_margin(0.0);
        
        egui::TopBottomPanel::top("settings_top")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(25, 25, 28)).inner_margin(egui::Margin::symmetric(16.0, 12.0)))
            .show(ctx, |ui| {
                if ui.add(egui::Button::new(egui::RichText::new("◀ CAMERA").color(egui::Color32::WHITE).size(14.0)).fill(egui::Color32::TRANSPARENT)).clicked() {
                    self.mode = AppMode::Camera;
                }
            });

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(20.0);
                egui::Frame::none().inner_margin(egui::Margin::symmetric(24.0, 0.0)).show(ui, |ui| {
                    ui.label(egui::RichText::new("Hardware Controls").color(egui::Color32::WHITE).size(20.0).strong());
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("MANUAL LENS FOCUS").color(egui::Color32::from_gray(150)).size(12.0));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let focus_slider = ui.add(egui::Slider::new(&mut self.local_focus, 0..=1023).trailing_fill(true));
                        if focus_slider.changed() { let _ = self.tx.try_send(DaemonCmd::SetFocus(self.local_focus)); }
                        if !focus_slider.dragged() { self.local_focus = self.shared_focus.load(Ordering::Relaxed); }
                    });

                    ui.add_space(30.0);
                    
                    ui.label(egui::RichText::new("Blockchain Settings").color(egui::Color32::WHITE).size(20.0).strong());
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("DEFAULT MINTING CHAIN").color(egui::Color32::from_gray(150)).size(12.0));
                    ui.add_space(4.0);
                    egui::ComboBox::from_id_salt("chain_combo")
                        .selected_text(if self.default_chain == SelectedChain::EVM { "Ethereum (EVM)" } else { "Solana" })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.default_chain, SelectedChain::EVM, "Ethereum (EVM)");
                            ui.selectable_value(&mut self.default_chain, SelectedChain::Solana, "Solana");
                        });

                    ui.add_space(20.0);
                    ui.label(egui::RichText::new("MASTER WALLET").color(egui::Color32::from_gray(150)).size(12.0));
                    ui.add_space(4.0);
                    ui.add(egui::TextEdit::singleline(&mut self.master_wallet).margin(egui::vec2(10.0, 10.0)));
                });
            });
        });
    }
}

impl eframe::App for LensMintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AppEvent::MintSuccess(uuid, _target) => {
                    self.mint_states.insert(uuid, MintStatus::Success);
                },
                AppEvent::MintFailed(uuid, _target, _err_msg) => {
                    self.mint_states.insert(uuid, MintStatus::Failed);
                }
            }
        }

        let mut visuals = egui::Visuals::dark();
        visuals.window_rounding = egui::Rounding::same(12.0);
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(30, 30, 32);
        visuals.widgets.noninteractive.rounding = egui::Rounding::same(8.0);
        visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);
        visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);
        visuals.widgets.active.rounding = egui::Rounding::same(8.0);
        ctx.set_visuals(visuals);

        let mut style = (*ctx.style()).clone();
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        style.spacing.interact_size = egui::vec2(48.0, 48.0);
        ctx.set_style(style);

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        match self.mode {
            AppMode::Camera => self.render_camera(ctx),
            AppMode::Gallery => self.render_gallery(ctx),
            AppMode::Settings => self.render_settings(ctx), 
            AppMode::PhotoView(uuid) => self.render_photo_view(ctx, uuid),
        }
    }
}