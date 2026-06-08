use eframe::egui;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI32, Ordering};
use crate::cmd::DaemonCmd;

pub struct LensMintApp {
    tx: tokio::sync::mpsc::Sender<DaemonCmd>,
    shared_frame: Arc<Mutex<Vec<u8>>>,
    shared_focus: Arc<AtomicI32>,
    db: Arc<sled::Db>,
    local_focus: i32, // Hardware focus UI state
    zoom_level: f32,  // Software zoom UI state
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
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("LensMint OS: Live View & Lens Control");

            // --- 1. Hardware Focus (Issue 5) ---
            let focus_slider = ui.add(
                egui::Slider::new(&mut self.local_focus, 0..=1023).text("Physical Focus")
            );

            if focus_slider.changed() {
                let _ = self.tx.try_send(DaemonCmd::SetFocus(self.local_focus));
            }
            if !focus_slider.dragged() {
                self.local_focus = self.shared_focus.load(Ordering::Relaxed);
            }

            // --- 2. Digital Zoom (Issue 6) ---
            ui.add(egui::Slider::new(&mut self.zoom_level, 1.0..=3.0).text("Digital Zoom"));

            // --- 3. Render Pipeline ---
            if let Ok(frame_data) = self.shared_frame.lock() {
                if frame_data.len() == 640 * 480 * 4 {
                    let image = egui::ColorImage::from_rgba_unmultiplied([640, 480], &frame_data);
                    let tex = self.texture.get_or_insert_with(|| {
                        ctx.load_texture("camera_stream", image.clone(), egui::TextureOptions::LINEAR)
                    });
                    tex.set(image, egui::TextureOptions::LINEAR);

                    // UV Math for Zero-Overhead Zoom
                    let offset = (1.0 - (1.0 / self.zoom_level)) / 2.0;
                    let min_uv = egui::pos2(offset, offset);
                    let max_uv = egui::pos2(1.0 - offset, 1.0 - offset);

                    let sized_image = egui::Image::new((tex.id(), egui::vec2(640.0, 480.0)))
                        .uv(egui::Rect::from_min_max(min_uv, max_uv));
                    
                    ui.add(sized_image);
                }
            }
            
            ui.separator();
            
            if ui.button("Capture & Mint (Ed25519)").clicked() {
                let photo_id = uuid::Uuid::new_v4();
                let _ = self.tx.try_send(DaemonCmd::CapturePhoto(photo_id));
            }

            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
}