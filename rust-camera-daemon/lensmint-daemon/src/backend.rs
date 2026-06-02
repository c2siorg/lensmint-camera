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
// 新增的 IOCTL 常量
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
        println!("[Hardware] Opening /dev/video0 and setting up mmap...");
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

            // 揭露内核真实分配的格式
            let pf = fmt.fmt.pixelformat;
            println!("[FFI] Kernel actually locked format to: {}x{} {}{}{}{}", 
                fmt.fmt.width, fmt.fmt.height, 
                (pf & 0xff) as u8 as char, ((pf >> 8) & 0xff) as u8 as char, 
                ((pf >> 16) & 0xff) as u8 as char, ((pf >> 24) & 0xff) as u8 as char);

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
            println!("[Hardware] Zero-Copy MMAP Success: {} bytes.", buf.length);
            
            Some(Self { fd, mem_ptr: ptr, mem_len: buf.length as usize })
        }
    }

    pub fn grab_frame(&self) -> bool {
        unsafe {
            let mut buf: v4l2_buffer = std::mem::zeroed();
            buf.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            buf.memory = V4L2_MEMORY_MMAP;

            let res = libc::ioctl(self.fd, VIDIOC_DQBUF, &mut buf);
            if res < 0 {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::EAGAIN) {
                    // 打印进度点，证明死循环正在拼命轮询 EAGAIN
                    use std::io::Write;
                    print!(".");
                    let _ = std::io::stdout().flush();
                    return false;
                }
                println!("\n[Hardware Error] DQBUF failed: {}", err);
                return false;
            }

            println!("\n[Stream] Got Frame! Index: {}, BytesUsed: {}, Raw Data: {:?}", 
                     buf.index, buf.bytesused, std::slice::from_raw_parts(self.mem_ptr as *const u8, 8));

            libc::ioctl(self.fd, VIDIOC_QBUF, &mut buf);
            true
        }
    }

    pub fn start_stream(&self) -> bool {
        unsafe {
            // 1. Enqueue buffer
            let mut buf: v4l2_buffer = std::mem::zeroed();
            buf.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            buf.memory = V4L2_MEMORY_MMAP;
            buf.index = 0;
            if libc::ioctl(self.fd, VIDIOC_QBUF, &mut buf) < 0 {
                println!("[Hardware Error] VIDIOC_QBUF failed");
                return false;
            }

            // 2. Stream ON
            let mut type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            if libc::ioctl(self.fd, VIDIOC_STREAMON, &mut type_) < 0 {
                println!("[Hardware Error] VIDIOC_STREAMON failed");
                return false;
            }
            println!("[Hardware] Camera Stream started successfully!");
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
            println!("[Hardware] Camera stream safely stopped & closed.");
        }
    }
}

pub async fn run_backend(mut rx: mpsc::Receiver<DaemonCmd>) {
    let camera = CameraStream::new();
    if let Some(cam) = &camera {
        if !cam.start_stream() {
            println!("[Worker] Failed to start stream.");
        }
    } else {
        println!("[Worker] Failed to initialize camera.");
    }

    // Tokio async loop: non-blocking frame grabbing
    loop {
        // Non-blocking UI command processing
        if let Ok(cmd) = rx.try_recv() {
            match cmd {
                DaemonCmd::CapturePhoto => println!("[Worker] Photo saved"),
            }
        }

        // Try grabbing a frame from hardware
        if let Some(cam) = &camera {
            if cam.grab_frame() {
                // If frame grabbed, sleep 33ms (~30 FPS) before next frame
                tokio::time::sleep(Duration::from_millis(33)).await;
            } else {
                // If EAGAIN (no frame), sleep 5ms to yield CPU, prevent 100% core usage
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        } else {
            // Idle loop if no camera
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}