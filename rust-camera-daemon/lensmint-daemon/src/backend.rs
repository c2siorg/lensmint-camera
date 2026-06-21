use std::ffi::CString;
use std::os::unix::io::RawFd;
use tokio::sync::mpsc;
use std::sync::{Arc, Mutex};
use eframe::egui;
use crate::cmd::{DaemonCmd, AppEvent, ChainTarget};
use std::sync::atomic::{AtomicI32, Ordering};
use sha2::{Sha256, Digest};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[repr(C)]
pub struct v4l2_pix_format {
    pub width: u32,
    pub height: u32,
    pub pixelformat: u32,
    pub field: u32,
    pub bytesperline: u32,
    pub sizeimage: u32,
    pub colorspace: u32,
    pub priv_data: u32,
    pub flags: u32,
    pub ycbcr_enc: u32,
    pub quantization: u32,
    pub xfer_func: u32,
}

#[repr(C)]
pub struct v4l2_format {
    pub type_: u32,
    pub _pad0: u32, 
    pub fmt: v4l2_pix_format,
    pub padding: [u8; 152],
}

#[repr(C)]
pub struct v4l2_requestbuffers {
    pub count: u32,
    pub type_: u32,
    pub memory: u32,
    pub capabilities: u32,
    pub reserved: [u32; 1],
}

#[repr(C)]
pub struct v4l2_buffer {
    pub index: u32,
    pub type_: u32,
    pub bytesused: u32,
    pub flags: u32,
    pub field: u32,
    _pad1: u32,
    pub timestamp_sec: i64,
    pub timestamp_usec: i64,
    pub timecode: [u8; 16],
    pub sequence: u32,
    pub memory: u32,
    pub m_offset: u32,
    _pad2: u32,
    pub length: u32,
    pub reserved2: u32,
    pub request_fd: i32,
    _pad3: u32,
}

#[repr(C)]
pub struct v4l2_control {
    pub id: u32,
    pub value: i32,
}

const VIDIOC_S_CTRL: libc::c_ulong = 0xc008561c;
const V4L2_CTRL_CLASS_CAMERA: u32 = 0x009a0000;
const V4L2_CID_FOCUS_ABSOLUTE: u32 = V4L2_CTRL_CLASS_CAMERA | 0x000a;

const VIDIOC_S_FMT: libc::c_ulong = 0xc0d05605;
const VIDIOC_REQBUFS: libc::c_ulong = 0xc0145608;
const VIDIOC_QUERYBUF: libc::c_ulong = 0xc0585609;
const VIDIOC_QBUF: libc::c_ulong = 0xc058560f;
const VIDIOC_DQBUF: libc::c_ulong = 0xc0585611;
const VIDIOC_STREAMON: libc::c_ulong = 0x40045612;
const VIDIOC_STREAMOFF: libc::c_ulong = 0x40045613;

const V4L2_BUF_TYPE_VIDEO_CAPTURE: u32 = 1;
const V4L2_MEMORY_MMAP: u32 = 1;

const fn v4l2_fourcc(a: u8, b: u8, c: u8, d: u8) -> u32 {
    (a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)
}

pub struct CameraStream {
    fd: RawFd,
    mem_ptr: *mut libc::c_void,
    mem_len: usize,
}

unsafe impl Send for CameraStream {}
unsafe impl Sync for CameraStream {}

impl CameraStream {
    pub fn new() -> Option<Self> {
        unsafe {
            let path = CString::new("/dev/video0").unwrap();
            let fd = libc::open(path.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK);
            if fd < 0 { return None; }

            let mut fmt: v4l2_format = std::mem::zeroed();
            fmt.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            fmt.fmt.width = 640;
            fmt.fmt.height = 480;
            fmt.fmt.pixelformat = v4l2_fourcc(b'Y', b'U', b'Y', b'V');
            fmt.fmt.field = 1;
            libc::ioctl(fd, VIDIOC_S_FMT, &mut fmt);

            let mut req: v4l2_requestbuffers = std::mem::zeroed();
            req.count = 1;
            req.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            req.memory = V4L2_MEMORY_MMAP;
            if libc::ioctl(fd, VIDIOC_REQBUFS, &mut req) < 0 { return None; }

            let mut buf: v4l2_buffer = std::mem::zeroed();
            buf.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            buf.memory = V4L2_MEMORY_MMAP;
            buf.index = 0;
            if libc::ioctl(fd, VIDIOC_QUERYBUF, &mut buf) < 0 { return None; }

            let ptr = libc::mmap(
                std::ptr::null_mut(),
                buf.length as usize,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                buf.m_offset as libc::off_t,
            );

            if ptr == libc::MAP_FAILED { return None; }
            Some(Self { fd, mem_ptr: ptr, mem_len: buf.length as usize })
        }
    }

    pub fn start_stream(&self) -> bool {
        unsafe {
            let mut buf: v4l2_buffer = std::mem::zeroed();
            buf.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            buf.memory = V4L2_MEMORY_MMAP;
            buf.index = 0;
            if libc::ioctl(self.fd, VIDIOC_QBUF, &mut buf) < 0 { return false; }

            let mut type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            if libc::ioctl(self.fd, VIDIOC_STREAMON, &mut type_) < 0 { return false; }
            true
        }
    }

    pub fn grab_frame(&self) -> bool {
        unsafe {
            let mut buf: v4l2_buffer = std::mem::zeroed();
            buf.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            buf.memory = V4L2_MEMORY_MMAP;

            if libc::ioctl(self.fd, VIDIOC_DQBUF, &mut buf) < 0 { return false; }
            libc::ioctl(self.fd, VIDIOC_QBUF, &mut buf);
            true
        }
    }

    pub fn set_focus(&self, value: i32) -> Result<(), std::io::Error> {
        #[allow(unused_mut)]
        let mut ctrl = v4l2_control {
            id: V4L2_CID_FOCUS_ABSOLUTE,
            value,
        };

        unsafe {
            let res = libc::ioctl(self.fd, VIDIOC_S_CTRL, &mut ctrl);
            if res < 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

impl Drop for CameraStream {
    fn drop(&mut self) {
        unsafe {
            let mut type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            libc::ioctl(self.fd, VIDIOC_STREAMOFF, &mut type_);
            if !self.mem_ptr.is_null() && self.mem_ptr != libc::MAP_FAILED {
                libc::munmap(self.mem_ptr, self.mem_len);
            }
            libc::close(self.fd);
        }
    }
}

pub fn yuyv_to_rgba_in_place(yuyv: &[u8], rgba: &mut [u8], width: usize, height: usize, stride: usize) {
    for y in 0..height {
        let row_start = y * stride;
        
        for x in (0..width).step_by(2) {
            let i = row_start + x * 2;
            if i + 3 >= yuyv.len() { break; }

            let y0 = yuyv[i] as i32;
            let u  = yuyv[i + 1] as i32 - 128;
            let y1 = yuyv[i + 2] as i32;
            let v  = yuyv[i + 3] as i32 - 128;

            let r_add = (104597 * v) >> 16;
            let g_sub = (25675 * u + 53279 * v) >> 16;
            let b_add = (132201 * u) >> 16;

            let out_idx = (y * width + x) * 4;

            rgba[out_idx]     = (y0 + r_add).clamp(0, 255) as u8;
            rgba[out_idx + 1] = (y0 - g_sub).clamp(0, 255) as u8;
            rgba[out_idx + 2] = (y0 + b_add).clamp(0, 255) as u8;
            rgba[out_idx + 3] = 255; 
            
            rgba[out_idx + 4] = (y1 + r_add).clamp(0, 255) as u8;
            rgba[out_idx + 5] = (y1 - g_sub).clamp(0, 255) as u8;
            rgba[out_idx + 6] = (y1 + b_add).clamp(0, 255) as u8;
            rgba[out_idx + 7] = 255; 
        }
    }
}

async fn process_and_store_image(
    uuid: uuid::Uuid, 
    rgba_data: Vec<u8>, 
    db: Arc<sled::Db>, 
    photos_dir: std::path::PathBuf
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tokio::task::spawn_blocking(move || {
        let img = image::RgbaImage::from_raw(640, 480, rgba_data)
            .ok_or("Failed to construct RgbaImage")?;

        let file_path = photos_dir.join(format!("{}.jpg", uuid));
        img.save_with_format(&file_path, image::ImageFormat::Jpeg)?;

        let thumbnail = image::imageops::resize(
            &img, 
            256, 
            192, 
            image::imageops::FilterType::Triangle 
        );

        let mut cursor = std::io::Cursor::new(Vec::new());
        thumbnail.write_to(&mut cursor, image::ImageFormat::Jpeg)?;
        
        db.insert(uuid.as_bytes(), cursor.into_inner())?;
        db.flush()?;

        println!("[Storage] Dual-track save complete: {}", uuid);
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    }).await??;

    Ok(())
}

pub struct ImageHashes {
    pub sha256: String,
    pub phash: String,
}

pub async fn compute_image_hashes(
    db: Arc<sled::Db>,
    uuid: uuid::Uuid,
) -> Result<ImageHashes, Box<dyn std::error::Error + Send + Sync>> {
    tokio::task::spawn_blocking(move || {
        let img_bytes = db.get(uuid.as_bytes())?
            .ok_or("missing image in sled cache")?;

        let mut sha256_hasher = Sha256::new();
        sha256_hasher.update(&img_bytes);
        let sha256_hex = hex::encode(sha256_hasher.finalize());

        let img = image::load_from_memory(&img_bytes)?;
        let p_hasher = image_hasher::HasherConfig::new()
            .hash_alg(image_hasher::HashAlg::Gradient)
            .to_hasher();
            
        let phash = p_hasher.hash_image(&img);
        let phash_hex = hex::encode(phash.as_bytes());

        Ok(ImageHashes {
            sha256: sha256_hex,
            phash: phash_hex,
        })
    })
    .await?
}

#[derive(Serialize)]
pub struct MetadataPayload {
    pub uuid: String,
    pub sha256: String,
    pub phash: String,
    pub pubkey: String,
    pub timestamp: u64,
    pub chain: String,
}

#[derive(Serialize)]
pub struct SignedEnvelope {
    pub payload_json: String,
    pub signature: String,
}

pub async fn run_backend(
    mut rx: mpsc::Receiver<DaemonCmd>, 
    shared_frame: Arc<Mutex<Vec<u8>>>,
    shared_focus: Arc<AtomicI32>, 
    db: Arc<sled::Db>,
    photos_dir: std::path::PathBuf,
    ctx: egui::Context,
    keystore: Arc<crate::keystore::LocalKeystore>, 
    event_tx: std::sync::mpsc::Sender<AppEvent>, 
) {
    let camera = CameraStream::new();
    if let Some(cam) = &camera {
        if !cam.start_stream() {
            println!("[Worker] Failed to start stream.");
        }
    }

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("Failed to build reqwest client");

    let mut local_rgba = vec![255u8; 640 * 480 * 4];
    let mut pending_capture: Option<uuid::Uuid> = None;

    let mut video_tx: Option<tokio::sync::mpsc::Sender<Vec<u8>>> = None;
    let mut child_process: Option<tokio::process::Child> = None;
    let mut current_video_uuid: Option<uuid::Uuid> = None;

    loop {
        if let Ok(cmd) = rx.try_recv() {
            match cmd {
                DaemonCmd::CapturePhoto(uuid) => {
                    pending_capture = Some(uuid);
                    println!("[Worker] Capture triggered: {}", uuid);
                },
                DaemonCmd::SetFocus(val) => {
                    if let Some(cam) = &camera {
                        if cam.set_focus(val).is_ok() {
                            shared_focus.store(val, Ordering::Relaxed);
                        }
                    }
                },
                DaemonCmd::DeletePhoto(uuid) => {
                    let db_clone = db.clone();
                    let file_path_jpg = photos_dir.join(format!("{}.jpg", uuid));
                    let file_path_mp4 = photos_dir.join(format!("{}.mp4", uuid));
                    
                    tokio::spawn(async move {
                        let _ = tokio::fs::remove_file(&file_path_jpg).await;
                        let _ = tokio::fs::remove_file(&file_path_mp4).await;
                        
                        let db_del = tokio::task::spawn_blocking(move || {
                            let res = db_clone.remove(uuid.as_bytes());
                            let _ = db_clone.flush();
                            res
                        });

                        if let Ok(Err(e)) = db_del.await {
                            eprintln!("[Storage] DB delete error for {}: {}", uuid, e);
                        } else {
                            println!("[Storage] Cascade deletion complete: {}", uuid);
                        }
                    });
                },
                DaemonCmd::Mint(uuid, target) => {
                    let db_clone = db.clone();
                    let key_clone = keystore.clone();
                    let client_clone = http_client.clone();
                    let tx_clone = event_tx.clone();
                    let ui_ctx = ctx.clone();
                    
                    tokio::spawn(async move {
                        match compute_image_hashes(db_clone, uuid).await {
                            Ok(hashes) => {
                                let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                                let chain_str = match target {
                                    ChainTarget::EVM => "evm".to_string(),
                                    ChainTarget::Solana => "solana".to_string(),
                                };
                                
                                let payload = MetadataPayload {
                                    uuid: uuid.to_string(),
                                    sha256: hashes.sha256,
                                    phash: hashes.phash,
                                    pubkey: key_clone.public_key_hex(),
                                    timestamp: ts,
                                    chain: chain_str, 
                                };

                                if let Ok(json_str) = serde_json::to_string(&payload) {
                                    let sig = key_clone.sign_payload_hex(json_str.as_bytes());
                                    let envelope = SignedEnvelope {
                                        payload_json: json_str,
                                        signature: sig,
                                    };
                                    
                                    let url = "https://httpbin.org/post";
                                    let res = client_clone.post(url).json(&envelope).send().await;
                                    
                                    match res {
                                        Ok(response) if response.status().is_success() => {
                                            println!("[Web3] Mint success for {}", uuid);
                                            let _ = tx_clone.send(AppEvent::MintSuccess(uuid, target));
                                        },
                                        Ok(bad_resp) => {
                                            let _ = tx_clone.send(AppEvent::MintFailed(uuid, target, bad_resp.status().to_string()));
                                        },
                                        Err(e) => {
                                            let _ = tx_clone.send(AppEvent::MintFailed(uuid, target, e.to_string()));
                                        }
                                    }
                                }
                            },
                            Err(e) => {
                                let _ = tx_clone.send(AppEvent::MintFailed(uuid, target, e.to_string()));
                            }
                        }
                        ui_ctx.request_repaint(); 
                    });
                },
                DaemonCmd::StartVideo(uuid) => {
                    let file_path = photos_dir.join(format!("{}.mp4", uuid));
                    let child_res = tokio::process::Command::new("ffmpeg")
                        .args(&[
                            "-y",
                            "-f", "rawvideo",
                            "-pix_fmt", "yuyv422",
                            "-s", "640x480",
                            "-framerate", "30",
                            "-i", "pipe:0",
                            "-c:v", "libx264",
                            "-preset", "ultrafast",
                            "-crf", "28",
                            file_path.to_str().unwrap(),
                        ])
                        .stdin(std::process::Stdio::piped())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn();

                    if let Ok(mut child) = child_res {
                        let mut stdin = child.stdin.take().expect("Failed to open stdin");
                        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(30);
                        video_tx = Some(tx);
                        child_process = Some(child);
                        current_video_uuid = Some(uuid);

                        tokio::spawn(async move {
                            use tokio::io::AsyncWriteExt;
                            while let Some(frame) = rx.recv().await {
                                if stdin.write_all(&frame).await.is_err() { break; }
                            }
                        });
                        println!("[Hardware] Started video recording: {}", uuid);
                    }
                },
                DaemonCmd::StopVideo => {
                    video_tx = None;
                    if let Some(mut child) = child_process.take() {
                        if let Some(uuid) = current_video_uuid.take() {
                            let dir_clone = photos_dir.clone();
                            let db_clone = db.clone();
                            tokio::spawn(async move {
                                let _ = child.wait().await;
                                println!("[Hardware] Video recording finalized: {}", uuid);
                                let video_path = dir_clone.join(format!("{}.mp4", uuid));
                                let thumb_output = tokio::process::Command::new("ffmpeg")
                                    .args(&[
                                        "-i", video_path.to_str().unwrap(),
                                        "-vframes", "1",
                                        "-f", "image2pipe",
                                        "-vcodec", "mjpeg",
                                        "-"
                                    ])
                                    .output()
                                    .await;

                                if let Ok(output) = thumb_output {
                                    if output.status.success() {
                                        if let Ok(img) = image::load_from_memory(&output.stdout) {
                                            let thumbnail = image::imageops::resize(&img, 256, 192, image::imageops::FilterType::Triangle);
                                            let mut cursor = std::io::Cursor::new(Vec::new());
                                            if thumbnail.write_to(&mut cursor, image::ImageFormat::Jpeg).is_ok() {
                                                let _ = db_clone.insert(uuid.as_bytes(), cursor.into_inner());
                                                let _ = db_clone.flush();
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }

        if let Some(cam) = &camera {
            if cam.grab_frame() {
                let data_slice = unsafe { std::slice::from_raw_parts(cam.mem_ptr as *const u8, cam.mem_len) };
                
                if let Some(tx) = &video_tx {
                    let _ = tx.try_send(data_slice.to_vec());
                }
                
                yuyv_to_rgba_in_place(data_slice, &mut local_rgba, 640, 480, 1280);
                
                if let Some(uuid) = pending_capture.take() {
                    let frame_clone = local_rgba.clone();
                    let db_clone = db.clone();
                    let dir_clone = photos_dir.clone();
                    
                    tokio::spawn(async move {
                        if let Err(e) = process_and_store_image(uuid, frame_clone, db_clone.clone(), dir_clone).await {
                            eprintln!("[Storage] Pipeline failed for {}: {}", uuid, e);
                        }
                    });
                }

                if let Ok(mut frame) = shared_frame.lock() {
                    if frame.len() != local_rgba.len() {
                        frame.resize(local_rgba.len(), 255);
                    }
                    frame.copy_from_slice(&local_rgba);
                }
                
                ctx.request_repaint();
            } else {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        } else {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }
}