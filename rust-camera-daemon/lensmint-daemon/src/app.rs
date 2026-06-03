use eframe::egui;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI32, Ordering};
use crate::cmd::DaemonCmd;

pub struct LensMintApp {
    tx: tokio::sync::mpsc::Sender<DaemonCmd>,
    shared_frame: Arc<Mutex<Vec<u8>>>,
    shared_focus: Arc<AtomicI32>,
    local_focus: i32, // UI local state
    texture: Option<egui::TextureHandle>,
}

impl LensMintApp {
    pub fn new(
        tx: tokio::sync::mpsc::Sender<DaemonCmd>, 
        shared_frame: Arc<Mutex<Vec<u8>>>,
        shared_focus: Arc<AtomicI32>,
    ) -> Self {
        let local_focus = shared_focus.load(Ordering::Relaxed);
        Self { 
            tx, 
            shared_frame, 
            shared_focus,
            local_focus,
            texture: None 
        }
    }
}

impl eframe::App for LensMintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("LensMint OS: Live View & Lens Control");

            // Hardware Focus Slider
            let focus_slider = ui.add(
                egui::Slider::new(&mut self.local_focus, 0..=1023).text("Physical Focus")
            );

            // Send command immediately when value changes
            if focus_slider.changed() {
                let _ = self.tx.try_send(DaemonCmd::SetFocus(self.local_focus));
            }

            // Resilience logic: Snap back if not dragging and hardware rejected the value
            if !focus_slider.dragged() {
                self.local_focus = self.shared_focus.load(Ordering::Relaxed);
            }

            if let Ok(frame_data) = self.shared_frame.lock() {
                if frame_data.len() == 640 * 480 * 4 {
                    let image = egui::ColorImage::from_rgba_unmultiplied([640, 480], &frame_data);
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