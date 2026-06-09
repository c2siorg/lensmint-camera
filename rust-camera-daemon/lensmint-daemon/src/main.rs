mod keystore;
mod app;
mod backend;
mod cmd;

use app::LensMintApp;
use cmd::DaemonCmd;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicI32;
use eframe::egui;

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    let (tx, rx) = tokio::sync::mpsc::channel::<DaemonCmd>(32);
    
    let proj_dirs = directories::ProjectDirs::from("", "", "lensmint")
        .expect("Failed to resolve app directories");
    
    let cache_dir = proj_dirs.data_dir().join("cache_db");
    let photos_dir = proj_dirs.data_dir().join("photos");
    
    std::fs::create_dir_all(&cache_dir).expect("Failed to create cache directory");
    std::fs::create_dir_all(&photos_dir).expect("Failed to create photos directory");
    
    let db = sled::open(cache_dir).expect("Failed to open Sled database");
    let shared_db = Arc::new(db);

    let shared_frame = Arc::new(Mutex::new(vec![0; 640 * 480 * 4]));
    let shared_focus = Arc::new(AtomicI32::new(0)); 

    // 4. Configure eframe window parameters for absolute fullscreen Kiosk mode
    let mut options = eframe::NativeOptions::default();
    options.viewport = egui::ViewportBuilder::default()
        .with_fullscreen(true)
        .with_decorations(false) // Strip title bars and borders
        .with_maximized(true);
    
    eframe::run_native(
        "LensMint OS",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let frame_clone = shared_frame.clone();
            let focus_clone = shared_focus.clone();
            let db_backend = shared_db.clone();
            let db_app = shared_db.clone();
            let photos_dir_clone = photos_dir.clone();
            
            tokio::spawn(async move {
                // Pass photos_dir to backend
                backend::run_backend(rx, frame_clone, focus_clone, db_backend, photos_dir_clone, ctx).await;
            });

            Ok(Box::new(LensMintApp::new(tx, shared_frame, shared_focus, db_app)))
        }),
    )
}