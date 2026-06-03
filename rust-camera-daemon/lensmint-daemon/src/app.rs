use eframe::egui;
use std::sync::{Arc, Mutex};
use crate::cmd::DaemonCmd;

pub struct LensMintApp {
    tx: tokio::sync::mpsc::Sender<DaemonCmd>,
    shared_frame: Arc<Mutex<Vec<u8>>>,
    texture: Option<egui::TextureHandle>,
}

impl LensMintApp {
    pub fn new(
        tx: tokio::sync::mpsc::Sender<DaemonCmd>, 
        shared_frame: Arc<Mutex<Vec<u8>>>,
    ) -> Self {
        Self { 
            tx, 
            shared_frame, 
            texture: None 
        }
    }
}

impl eframe::App for LensMintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("LensMint OS: Live View (Hardware ISP)");

            // Pull the latest processed RGBA frame from shared memory
            if let Ok(frame_data) = self.shared_frame.lock() {
                if frame_data.len() == 640 * 480 * 4 {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [640, 480], 
                        &frame_data
                    );
                    
                    // Update GPU texture
                    let tex = self.texture.get_or_insert_with(|| {
                        ctx.load_texture("camera_stream", image.clone(), egui::TextureOptions::LINEAR)
                    });
                    tex.set(image, egui::TextureOptions::LINEAR);
                    
                    ui.image(&*tex);
                }
            }
            
            if ui.button("Mint Photo (Take)").clicked() {
                let _ = self.tx.try_send(DaemonCmd::CapturePhoto);
            }
        });
    }
}