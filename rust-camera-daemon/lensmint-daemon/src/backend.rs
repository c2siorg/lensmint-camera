use crate::cmd::DaemonCmd;
use std::time::Duration;
use tokio::sync::mpsc;

pub async fn run_backend(mut rx: mpsc::Receiver<DaemonCmd>) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            DaemonCmd::CapturePhoto => {
                println!("Processing CapturePhoto command...");
                // Mocking the heavy I/O delay
                tokio::time::sleep(Duration::from_secs(2)).await;
                println!("Photo saved");
            }
        }
    }
}
