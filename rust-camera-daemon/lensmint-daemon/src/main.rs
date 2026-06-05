mod keystore;
mod app;
mod backend;
mod cmd;

use app::LensMintApp;
use cmd::DaemonCmd;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicI32; // Added

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    let (tx, rx) = tokio::sync::mpsc::channel::<DaemonCmd>(4);
    
    let shared_frame = Arc::new(Mutex::new(vec![0; 640 * 480 * 4]));
    // Default focus value (0 to 1023 is typical for IMX708 VCM)
    let shared_focus = Arc::new(AtomicI32::new(0)); 

    let options = eframe::NativeOptions::default();
    
    eframe::run_native(
        "LensMint Camera Window",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let frame_clone = shared_frame.clone();
            let focus_clone = shared_focus.clone();
            
            tokio::spawn(async move {
                backend::run_backend(rx, frame_clone, focus_clone, ctx).await;
            });

            Ok(Box::new(LensMintApp::new(tx, shared_frame, shared_focus)))
        }),
    )
}