mod keystore;
mod app;
mod backend;
mod cmd;

use app::LensMintApp;
use cmd::DaemonCmd;

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    // Scaffold bounded channel
    let (tx, rx) = tokio::sync::mpsc::channel::<DaemonCmd>(4);

    // Spawn the detached async hardware IO worker loop
    tokio::spawn(async move {
        backend::run_backend(rx).await;
    });

    // 临时测试代码：挂起主线程，不启动 eframe，等待后台相机的瀑布流
    println!("[Main] UI temporarily disabled for FFI testing. Press Ctrl+C to exit.");
    tokio::signal::ctrl_c().await.unwrap();
    println!("[Main] Exiting...");

    Ok(())
    
    /* 
    // 原有的 UI 启动代码，暂时注释
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "LensMint Camera Window",
        options,
        Box::new(|_cc| Ok(Box::new(LensMintApp::new(tx)))),
    )
    */
}