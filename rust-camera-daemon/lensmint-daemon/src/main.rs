mod keystore;
mod app;
mod backend;
mod cmd;

use app::LensMintApp;
use cmd::DaemonCmd;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    // 4-slot channel to handle IO backpressure safely
    let (tx, rx) = tokio::sync::mpsc::channel::<DaemonCmd>(4);
    
    // Shared frame buffer: 640 x 480 x 4 bytes (RGBA8888)
    let shared_frame = Arc::new(Mutex::new(vec![0; 640 * 480 * 4]));

    let options = eframe::NativeOptions::default();
    
    eframe::run_native(
        "LensMint Camera Window",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let frame_clone = shared_frame.clone();
            
            // Spawn detached async worker for hardware IO
            tokio::spawn(async move {
                backend::run_backend(rx, frame_clone, ctx).await;
            });

            Ok(Box::new(LensMintApp::new(tx, shared_frame)))
        }),
    )
}