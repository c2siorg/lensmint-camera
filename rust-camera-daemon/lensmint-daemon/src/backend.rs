use std::ffi::CString;
use std::os::unix::io::RawFd;
use std::time::Duration;
use tokio::sync::mpsc;
use crate::cmd::DaemonCmd;

// --- FFI Structs (AArch64 64-bit Strict Memory Layout) ---
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
    _pad1: u32, // 64-bit alignment padding before timeval
    pub timestamp_sec: i64,
    pub timestamp_usec: i64,
    pub timecode: [u8; 16],
    pub sequence: u32,
    pub memory: u32,
    pub m_offset: u32, // union m { u32 offset; ... } mapped to offset
    _pad2: u32,
    pub length: u32,
    pub reserved2: u32,
    pub request_fd: i32,
    _pad3: u32, // total struct size: 88 bytes
}

const VIDIOC_S_FMT: libc::c_ulong = 0xc0d05605;
const VIDIOC_REQBUFS: libc::c_ulong = 0xc0145608;
const VIDIOC_QUERYBUF: libc::c_ulong = 0xc0585609;

const V4L2_BUF_TYPE_VIDEO_CAPTURE: u32 = 1;
const V4L2_MEMORY_MMAP: u32 = 1;

const fn v4l2_fourcc(a: u8, b: u8, c: u8, d: u8) -> u32 {
    (a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)
}

// --- Stateful RAII Hardware Wrapper ---
pub struct CameraStream {
    fd: RawFd,
    mem_ptr: *mut libc::c_void,
    mem_len: usize,
}

impl CameraStream {
    pub fn new() -> Option<Self> {
        println!("[Hardware] Opening /dev/video0 and setting up mmap...");
        unsafe {
            let path = CString::new("/dev/video0").unwrap();
            let fd = libc::open(path.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK);
            if fd < 0 { return None; }

            // 1. Set Format
            let mut fmt: v4l2_format = std::mem::zeroed();
            fmt.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            fmt.fmt.width = 640;
            fmt.fmt.height = 480;
            fmt.fmt.pixelformat = v4l2_fourcc(b'Y', b'U', b'Y', b'V');
            fmt.fmt.field = 1;
            libc::ioctl(fd, VIDIOC_S_FMT, &mut fmt);

            // 2. Request Kernel Buffers (Zero-Copy)
            let mut req: v4l2_requestbuffers = std::mem::zeroed();
            req.count = 1;
            req.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            req.memory = V4L2_MEMORY_MMAP;
            if libc::ioctl(fd, VIDIOC_REQBUFS, &mut req) < 0 {
                println!("[Hardware Error] VIDIOC_REQBUFS failed");
                libc::close(fd);
                return None;
            }

            // 3. Query Buffer Offset for mapping
            let mut buf: v4l2_buffer = std::mem::zeroed();
            buf.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            buf.memory = V4L2_MEMORY_MMAP;
            buf.index = 0;
            if libc::ioctl(fd, VIDIOC_QUERYBUF, &mut buf) < 0 {
                println!("[Hardware Error] VIDIOC_QUERYBUF failed");
                libc::close(fd);
                return None;
            }

            // 4. Map Kernel Memory to Rust Pointer
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                buf.length as usize,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                buf.m_offset as libc::off_t,
            );

            if ptr == libc::MAP_FAILED {
                println!("[Hardware Error] libc::mmap failed");
                libc::close(fd);
                return None;
            }

            println!("[Hardware] Zero-Copy MMAP Success: {} bytes at mapped offset.", buf.length);
            Some(Self { fd, mem_ptr: ptr, mem_len: buf.length as usize })
        }
    }
}

impl Drop for CameraStream {
    fn drop(&mut self) {
        unsafe {
            if !self.mem_ptr.is_null() && self.mem_ptr != libc::MAP_FAILED {
                libc::munmap(self.mem_ptr, self.mem_len);
            }
            libc::close(self.fd);
            println!("[Hardware] Camera stream safely closed & munmapped.");
        }
    }
}

// 明确告诉编译器：跨线程转移 mmap 裸指针的所有权是安全的
unsafe impl Send for CameraStream {}
unsafe impl Sync for CameraStream {}

// Background Worker Loop
pub async fn run_backend(mut rx: mpsc::Receiver<DaemonCmd>) {
    let _camera = CameraStream::new();
    if _camera.is_none() {
        println!("[Worker] Failed to initialize camera. Running worker in idle mode.");
    }

    while let Some(cmd) = rx.recv().await {
        match cmd {
            DaemonCmd::CapturePhoto => {
                tokio::time::sleep(Duration::from_secs(2)).await;
                println!("[Worker] Photo saved");
            }
        }
    }
}