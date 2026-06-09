use std::ffi::CString;
use std::os::unix::io::RawFd;
use tokio::sync::mpsc;
use std::sync::{Arc, Mutex};
use eframe::egui;
use crate::cmd::DaemonCmd;

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
    pub _pad0: u32, // Critical: 64-bit ABI alignment padding for AArch64
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

// #[repr(C, packed)]
// pub struct v4l2_ext_control {
//     pub id: u32,
//     pub size: u32,
//     pub reserved2: [u32; 1],
//     pub value: i32,
//     pub _padding: u32,
// }

// #[repr(C)]
// pub struct v4l2_ext_controls {
//     pub ctrl_class: u32,
//     pub count: u32,
//     pub error_idx: u32,
//     pub request_fd: i32,
//     pub reserved: [u32; 1],
//     pub _pad0: u32, // CRITICAL: 4-byte padding for 64-bit pointer alignment
//     pub controls: *mut v4l2_ext_control,
// }

// const VIDIOC_S_EXT_CTRLS: libc::c_ulong = 0xc0205648;
// const V4L2_CTRL_CLASS_CAMERA: u32 = 0x009a0000;
// const V4L2_CID_FOCUS_ABSOLUTE: u32 = V4L2_CTRL_CLASS_CAMERA | 0x000a;

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

    // pub fn set_focus(&self, value: i32) -> Result<(), std::io::Error> {
    //     let mut ctrl = v4l2_ext_control {
    //         id: V4L2_CID_FOCUS_ABSOLUTE,
    //         size: 0,
    //         reserved2: [0; 1],
    //         value,
    //         _padding: 0,
    //     };

    //     let mut ctrls = v4l2_ext_controls {
    //         ctrl_class: V4L2_CTRL_CLASS_CAMERA,
    //         count: 1,
    //         error_idx: 0,
    //         request_fd: 0,
    //         reserved: [0; 1],
    //         _pad0: 0,
    //         controls: &mut ctrl,
    //     };

    //     unsafe {
    //         let res = libc::ioctl(self.fd, VIDIOC_S_EXT_CTRLS, &mut ctrls);
    //         if res < 0 {
    //             return Err(std::io::Error::last_os_error());
    //         }
    //     }
    //     Ok(())
    // }

    pub fn set_focus(&self, value: i32) -> Result<(), std::io::Error> {
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

// in-place yuyv parsing. eliminates vec allocation in hot loop.
pub fn yuyv_to_rgba_in_place(yuyv: &[u8], rgba: &mut [u8], width: usize, height: usize, stride: usize) {
    for y in 0..height {
        let row_start = y * stride;
        
        for x in (0..width).step_by(2) {
            let i = row_start + x * 2;
            // bounds check to prevent panic during hardware tearing
            if i + 3 >= yuyv.len() { break; }

            let y0 = yuyv[i] as i32;
            let u  = yuyv[i + 1] as i32 - 128;
            let y1 = yuyv[i + 2] as i32;
            let v  = yuyv[i + 3] as i32 - 128;

            // fixed-point math for ARM. much faster than floats.
            let r_add = (104597 * v) >> 16;
            let g_sub = (25675 * u + 53279 * v) >> 16;
            let b_add = (132201 * u) >> 16;

            let out_idx = (y * width + x) * 4;

            // pixel 0
            rgba[out_idx]     = (y0 + r_add).clamp(0, 255) as u8;
            rgba[out_idx + 1] = (y0 - g_sub).clamp(0, 255) as u8;
            rgba[out_idx + 2] = (y0 + b_add).clamp(0, 255) as u8;
            rgba[out_idx + 3] = 255; 
            
            // pixel 1
            rgba[out_idx + 4] = (y1 + r_add).clamp(0, 255) as u8;
            rgba[out_idx + 5] = (y1 - g_sub).clamp(0, 255) as u8;
            rgba[out_idx + 6] = (y1 + b_add).clamp(0, 255) as u8;
            rgba[out_idx + 7] = 255; 
        }
    }
}

use std::sync::atomic::{AtomicI32, Ordering};

// Add process_and_store_image function
async fn process_and_store_image(
    uuid: uuid::Uuid, 
    rgba_data: Vec<u8>, 
    db: Arc<sled::Db>, 
    photos_dir: std::path::PathBuf
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Offload CPU-bound image encoding and scaling to blocking thread pool
    tokio::task::spawn_blocking(move || {
        let img = image::RgbaImage::from_raw(640, 480, rgba_data)
            .ok_or("Failed to construct RgbaImage")?;

        // Track 1: Full-res JPEG to Ext4 SD Card
        let file_path = photos_dir.join(format!("{}.jpg", uuid));
        img.save_with_format(&file_path, image::ImageFormat::Jpeg)?;

        // Track 2: Downscale thumbnail to Sled Memory-mapped DB
        // 256x192 maintains 4:3 aspect ratio. Triangle filter balances speed/quality on ARM.
        let thumbnail = image::imageops::resize(
            &img, 
            256, 
            192, 
            image::imageops::FilterType::Triangle 
        );

        let mut cursor = std::io::Cursor::new(Vec::new());
        thumbnail.write_to(&mut cursor, image::ImageFormat::Jpeg)?;
        
        // Sled key: 16-byte UUID. Value: JPEG bytes
        db.insert(uuid.as_bytes(), cursor.into_inner())?;
        db.flush()?;

        println!("[Storage] Dual-track save complete: {}", uuid);
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    }).await??;

    Ok(())
}

// 2. Update run_backend signature and loop logic
pub async fn run_backend(
    mut rx: mpsc::Receiver<DaemonCmd>, 
    shared_frame: Arc<Mutex<Vec<u8>>>,
    shared_focus: Arc<AtomicI32>, 
    db: Arc<sled::Db>,
    photos_dir: std::path::PathBuf,
    ctx: egui::Context,
) {
    let camera = CameraStream::new();
    if let Some(cam) = &camera {
        if !cam.start_stream() {
            println!("[Worker] Failed to start stream.");
        }
    }

    let mut local_rgba = vec![255u8; 640 * 480 * 4];
    let mut pending_capture: Option<uuid::Uuid> = None;

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
                }
                DaemonCmd::DeletePhoto(uuid) => println!("[Worker] Ready to delete: {}", uuid),
            }
        }

        if let Some(cam) = &camera {
            if cam.grab_frame() {
                let data_slice = unsafe { std::slice::from_raw_parts(cam.mem_ptr as *const u8, cam.mem_len) };
                
                yuyv_to_rgba_in_place(data_slice, &mut local_rgba, 640, 480, 1280);
                
                // Clone the freshest frame immediately if capture is requested
                if let Some(uuid) = pending_capture.take() {
                    let frame_clone = local_rgba.clone();
                    let db_clone = db.clone();
                    let dir_clone = photos_dir.clone();
                    
                    // Spawn non-blocking task to ensure V4L2 loop stays at 60FPS
                    tokio::spawn(async move {
                        if let Err(e) = process_and_store_image(uuid, frame_clone, db_clone, dir_clone).await {
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