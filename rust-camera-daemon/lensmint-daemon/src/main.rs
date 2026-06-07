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
    // 1. 初始化通信管道
    let (tx, rx) = tokio::sync::mpsc::channel::<DaemonCmd>(32); // 扩大缓冲区，防止高频连拍阻塞
    
    // 2. 初始化 Sled 本地数据库 (使用 XDG 标准路径 ~/.local/share/lensmint/)
    let proj_dirs = directories::ProjectDirs::from("", "", "lensmint")
        .expect("Failed to resolve app directories");
    let cache_dir = proj_dirs.data_dir().join("cache_db");
    std::fs::create_dir_all(&cache_dir).expect("Failed to create cache directory");
    
    // 打开数据库，并用 Arc 包装以便跨线程共享
    let db = sled::open(cache_dir).expect("Failed to open Sled database");
    let shared_db = Arc::new(db);

    // 3. 初始化共享内存状态
    let shared_frame = Arc::new(Mutex::new(vec![0; 640 * 480 * 4]));
    let shared_focus = Arc::new(AtomicI32::new(0)); 

    // 4. 配置 eframe 窗口参数 (解决导师的全屏需求)
    let mut options = eframe::NativeOptions::default();
    options.viewport = egui::ViewportBuilder::default().with_fullscreen(true);
    
    eframe::run_native(
        "LensMint OS",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let frame_clone = shared_frame.clone();
            let focus_clone = shared_focus.clone();
            let db_backend = shared_db.clone();
            let db_app = shared_db.clone();
            
            // 启动后台硬件和 I/O 引擎
            tokio::spawn(async move {
                // 注意：我们之后需要在 run_backend 的签名中加入 db_backend 参数
                backend::run_backend(rx, frame_clone, focus_clone, db_backend, ctx).await;
            });

            // 启动前台 UI
            Ok(Box::new(LensMintApp::new(tx, shared_frame, shared_focus, db_app)))
        }),
    )
}