mod keystore;
mod app;
mod backend;
mod cmd;

use app::LensMintApp;
use cmd::DaemonCmd;

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    // Scaffold bounded channel (4 slots max for strict backpressure on SBC)
    let (tx, rx) = tokio::sync::mpsc::channel::<DaemonCmd>(4);

    // Spawn the detached async hardware IO worker loop
    tokio::spawn(async move {
        backend::run_backend(rx).await;
    });

    // Start egui UI strictly isolated from backend worker
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "LensMint Camera Window",
        options,
        Box::new(|_cc| Ok(Box::new(LensMintApp::new(tx)))),
    )
}
