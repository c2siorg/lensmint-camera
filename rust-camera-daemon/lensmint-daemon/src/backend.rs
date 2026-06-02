use eframe::egui;
use std::ffi::CString;
use std::os::unix::io::RawFd;
use std::time::Duration;
use tokio::sync::mpsc;
use crate::cmd::DaemonCmd;

// --- FFI Structs ---
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
    pub mem_ptr: *mut libc::c_void,
    pub mem_len: usize,
    pub width: usize,
    pub height: usize,
    pub stride: usize, // 新增：保存硬件真实的内存跨距
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

            let stride = fmt.fmt.bytesperline as usize;
            let width = fmt.fmt.width as usize;
            let height = fmt.fmt.height as usize;
            println!("[Hardware] Negotiated Format: {}x{}, Stride: {} bytes/line", width, height, stride);

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
            
            Some(Self { fd, mem_ptr: ptr, mem_len: buf.length as usize, width, height, stride })
        }
    }

    pub fn grab_frame(&self) -> bool {
        unsafe {
            let mut buf: v4l2_buffer = std::mem::zeroed();
            buf.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            buf.memory = V4L2_MEMORY_MMAP;
            let res = libc::ioctl(self.fd, VIDIOC_DQBUF, &mut buf);
            if res < 0 { return false; }
            libc::ioctl(self.fd, VIDIOC_QBUF, &mut buf);
            true
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

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// 支持动态 Stride 的二维 YUYV 到 RGBA 转换
pub fn yuyv_to_rgba(yuyv: &[u8], width: usize, height: usize, stride: usize) -> Vec<u8> {
    let mut rgba = vec![255; width * height * 4];

    for y in 0..height {
        let row_start = y * stride;
        
        for x in (0..width).step_by(2) {
            let i = row_start + x * 2;
            
            // 内存安全锁：如果跨距拉得过大，防止访问越界
            if i + 3 >= yuyv.len() { break; }

            let y0 = yuyv[i] as i32;
            let u  = yuyv[i + 1] as i32 - 128;
            let y1 = yuyv[i + 2] as i32;
            let v  = yuyv[i + 3] as i32 - 128;

            let r_add = (104597 * v) >> 16;
            let g_sub = (25675 * u + 53279 * v) >> 16;
            let b_add = (132201 * u) >> 16;

            let out_idx = (y * width + x) * 4;

            rgba[out_idx] = (y0 + r_add).clamp(0, 255) as u8;
            rgba[out_idx + 1] = (y0 - g_sub).clamp(0, 255) as u8;
            rgba[out_idx + 2] = (y0 + b_add).clamp(0, 255) as u8;
            
            rgba[out_idx + 4] = (y1 + r_add).clamp(0, 255) as u8;
            rgba[out_idx + 5] = (y1 - g_sub).clamp(0, 255) as u8;
            rgba[out_idx + 6] = (y1 + b_add).clamp(0, 255) as u8;
        }
    }
    rgba
}

pub async fn run_backend(
    mut rx: mpsc::Receiver<DaemonCmd>, 
    shared_frame: Arc<Mutex<Vec<u8>>>,
    shared_stride: Arc<AtomicUsize>, // 接收 UI 传来的实时滑块数据
    ctx: egui::Context,
) {
    let camera = CameraStream::new();
    if let Some(cam) = &camera {
        if !cam.start_stream() {
            println!("[Worker] Failed to start stream.");
        }
    }

    loop {
        if let Ok(cmd) = rx.try_recv() {
            match cmd {
                DaemonCmd::CapturePhoto => println!("[Worker] Photo saved"),
            }
        }

        if let Some(cam) = &camera {
            if cam.grab_frame() {
                // 读取实时跨距
                let current_stride = shared_stride.load(Ordering::Relaxed);
                
                // 将整个 15MB 物理内存传给算法，防止读取越界
                let data_slice = unsafe { std::slice::from_raw_parts(cam.mem_ptr as *const u8, cam.mem_len) };
                
                let rgba_data = yuyv_to_rgba(data_slice, 640, 480, current_stride);
                
                if let Ok(mut frame) = shared_frame.lock() {
                    *frame = rgba_data;
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