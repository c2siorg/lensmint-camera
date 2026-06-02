use eframe::egui;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use crate::cmd::DaemonCmd;

pub struct LensMintApp {
    tx: tokio::sync::mpsc::Sender<DaemonCmd>,
    shared_frame: Arc<Mutex<Vec<u8>>>,
    shared_stride: Arc<AtomicUsize>,
    texture: Option<egui::TextureHandle>,
    local_stride: usize, // UI 滑块的状态
}

impl LensMintApp {
    pub fn new(
        tx: tokio::sync::mpsc::Sender<DaemonCmd>, 
        shared_frame: Arc<Mutex<Vec<u8>>>,
        shared_stride: Arc<AtomicUsize>,
    ) -> Self {
        let local_stride = shared_stride.load(Ordering::Relaxed);
        Self { 
            tx, 
            shared_frame,
            shared_stride,
            texture: None,
            local_stride,
        }
    }
}

impl eframe::App for LensMintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("LensMint OS: FFI Hardware Calibration");

            // 硬件校准滑块 (拖动它，寻找魔法数字)
            if ui.add(egui::Slider::new(&mut self.local_stride, 1280..=4096).text("Stride (Bytes/Line)")).changed() {
                self.shared_stride.store(self.local_stride, Ordering::Relaxed);
            }

            if let Ok(frame_data) = self.shared_frame.lock() {
                if frame_data.len() == 640 * 480 * 4 {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [640, 480],
                        &frame_data,
                    );
                    
                    let tex = self.texture.get_or_insert_with(|| {
                        ctx.load_texture("camera_stream", image.clone(), egui::TextureOptions::LINEAR)
                    });
                    tex.set(image, egui::TextureOptions::LINEAR);

                    ui.image(&*tex);
                }
            }
            
            if ui.button("Take Photo").clicked() {
                let _ = self.tx.try_send(DaemonCmd::CapturePhoto);
            }
        });
    }
}