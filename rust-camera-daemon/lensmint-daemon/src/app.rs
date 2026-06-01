use crate::cmd::DaemonCmd;
use eframe::egui;
use tokio::sync::mpsc;

pub struct LensMintApp {
    tx: mpsc::Sender<DaemonCmd>,
}

impl LensMintApp {
    pub fn new(tx: mpsc::Sender<DaemonCmd>) -> Self {
        Self { tx }
    }
}

impl eframe::App for LensMintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("LensMint Camera Daemon");
            ui.label("Hardware MVP: AArch64 Native Render (60FPS Target)"); // 新增：用于在实机确认运行版本

            if ui.button("Capture Photo").clicked() {
                match self.tx.try_send(DaemonCmd::CapturePhoto) {
                    Ok(_) => {
                        println!("CapturePhoto event queued successfully.");
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        eprintln!("WARNING: Channel full! Dropped CapturePhoto event to prevent lag/memory loops (Backpressure in effect).");
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        eprintln!("CRITICAL ERROR: Background worker channel is closed!");
                    }
                }
            }
        });

        // 核心注入：强制 UI 线程不休眠，每帧重绘，用于测试树莓派 GPU/CPU 负载
        ctx.request_repaint();
    }
}