use eframe::egui;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI32, Ordering};
use crate::cmd::DaemonCmd;

pub struct LensMintApp {
    tx: tokio::sync::mpsc::Sender<DaemonCmd>,
    shared_frame: Arc<Mutex<Vec<u8>>>,
    shared_focus: Arc<AtomicI32>,
    db: Arc<sled::Db>,
    local_focus: i32, 
    zoom_level: f32,  
    texture: Option<egui::TextureHandle>,
}

impl LensMintApp {
    pub fn new(
        tx: tokio::sync::mpsc::Sender<DaemonCmd>, 
        shared_frame: Arc<Mutex<Vec<u8>>>,
        shared_focus: Arc<AtomicI32>,
        db: Arc<sled::Db>,
    ) -> Self {
        let local_focus = shared_focus.load(Ordering::Relaxed);
        Self { 
            tx, 
            shared_frame, 
            shared_focus,
            local_focus,
            db,
            zoom_level: 1.0, 
            texture: None 
        }
    }
}

impl eframe::App for LensMintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Esc 快捷键退出 (需要屏幕有焦点)
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // --- 1. Top Control Panel ---
        egui::TopBottomPanel::top("top_panel")
            .frame(egui::Frame::none().fill(egui::Color32::from_black_alpha(200)).inner_margin(10.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("LensMint OS").color(egui::Color32::WHITE).strong());
                    ui.add_space(20.0);
                    
                    let focus_slider = ui.add(
                        egui::Slider::new(&mut self.local_focus, 0..=1023).text("Focus")
                    );
                    if focus_slider.changed() {
                        let _ = self.tx.try_send(DaemonCmd::SetFocus(self.local_focus));
                    }
                    if !focus_slider.dragged() {
                        self.local_focus = self.shared_focus.load(Ordering::Relaxed);
                    }

                    ui.add_space(10.0);
                    ui.add(egui::Slider::new(&mut self.zoom_level, 1.0..=3.0).text("Zoom"));

                    // 触控安全退出按钮 (靠右对齐)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("❌ Exit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
            });

        // --- 2. Bottom Control Panel (iPhone Camera Style) ---
        egui::TopBottomPanel::bottom("bottom_panel")
            .exact_height(100.0)
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 20)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(20.0);

                    // Gallery 入口占位
                    let gallery_btn = egui::Button::new("🖼 Gallery\n(Issue 8)")
                        .min_size(egui::vec2(80.0, 60.0));
                    if ui.add(gallery_btn).clicked() {
                        println!("[UI] Gallery clicked");
                    }

                    // 动态计算剩余空间，将快门按钮居中
                    let available_width = ui.available_width();
                    ui.add_space((available_width / 2.0) - 40.0); 

                    // A solid white circle (80x80 size with 40 radius)
                    let shutter_btn = egui::Button::new("")
                        .fill(egui::Color32::WHITE)
                        .min_size(egui::vec2(80.0, 80.0))
                        .rounding(egui::Rounding::same(40.0)); 

                    if ui.add(shutter_btn).clicked() {
                        let photo_id = uuid::Uuid::new_v4();
                        let _ = self.tx.try_send(DaemonCmd::CapturePhoto(photo_id));
                        println!("[UI] Shutter triggered, UUID: {}", photo_id);
                    }
                });
            });

        // --- 3. Main Camera Viewfinder ---
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::BLACK))
            .show(ctx, |ui| {
                if let Ok(frame_data) = self.shared_frame.lock() {
                    if frame_data.len() == 640 * 480 * 4 {
                        let image = egui::ColorImage::from_rgba_unmultiplied([640, 480], &frame_data);
                        let tex = self.texture.get_or_insert_with(|| {
                            ctx.load_texture("camera_stream", image.clone(), egui::TextureOptions::LINEAR)
                        });
                        tex.set(image, egui::TextureOptions::LINEAR);

                        let offset = (1.0 - (1.0 / self.zoom_level)) / 2.0;
                        let min_uv = egui::pos2(offset, offset);
                        let max_uv = egui::pos2(1.0 - offset, 1.0 - offset);

                        let available_size = ui.available_size();
                        let aspect_ratio = 640.0 / 480.0;
                        
                        // 动态等比缩放，确保在任意小屏幕上都不会撑爆 UI
                        let (img_w, img_h) = if available_size.x / available_size.y > aspect_ratio {
                            (available_size.y * aspect_ratio, available_size.y)
                        } else {
                            (available_size.x, available_size.x / aspect_ratio)
                        };

                        let sized_image = egui::Image::new((tex.id(), egui::vec2(img_w, img_h)))
                            .uv(egui::Rect::from_min_max(min_uv, max_uv));
                        
                        ui.centered_and_justified(|ui| {
                            ui.add(sized_image);
                        });
                    }
                }
            });
    }
}