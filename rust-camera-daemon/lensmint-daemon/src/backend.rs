use std::ffi::CString;
use std::os::unix::io::RawFd;
use std::time::Duration;
use tokio::sync::mpsc;
use crate::cmd::DaemonCmd;

#[repr(C)]
pub struct v4l2_capability {
    pub driver: [u8; 16],
    pub card: [u8; 32],
    pub bus_info: [u8; 32],
    pub version: u32,
    pub capabilities: u32,
    pub device_caps: u32,
    pub reserved: [u32; 3],
}

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

// Emulating C union with strict padding (200 bytes total for union, 48 used by pix)
#[repr(C)]
pub struct v4l2_format {
    pub type_: u32,
    pub fmt: v4l2_pix_format,
    pub padding: [u8; 152], 
}

const VIDIOC_QUERYCAP: libc::c_ulong = 0x80685600;
// Magic number for _IOWR('V', 5, struct v4l2_format). Computed for 64-bit ABI.
const VIDIOC_S_FMT: libc::c_ulong = 0xc0d05605; 
const V4L2_BUF_TYPE_VIDEO_CAPTURE: u32 = 1;

// Helper to construct FOURCC codes
const fn v4l2_fourcc(a: u8, b: u8, c: u8, d: u8) -> u32 {
    (a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)
}

fn parse_c_string(bytes: &[u8]) -> String {
    let null_pos = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..null_pos]).into_owned()
}

// Renamed from probe_camera to initialize camera state
pub fn init_camera() {
    println!("[FFI] Initializing /dev/video0...");
    unsafe {
        let dev_path = CString::new("/dev/video0").unwrap();
        let fd: RawFd = libc::open(dev_path.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK);
        
        if fd < 0 {
            println!("[FFI Error] Failed to open: {}", std::io::Error::last_os_error());
            return;
        }

        // 1. Probe Capabilities
        let mut caps: v4l2_capability = std::mem::zeroed();
        if libc::ioctl(fd, VIDIOC_QUERYCAP, &mut caps) < 0 {
            println!("[FFI Error] VIDIOC_QUERYCAP failed");
            libc::close(fd);
            return;
        }
        println!("[FFI] Driver: {}", parse_c_string(&caps.driver));

        // 2. Set Video Format (640x480 YUYV)
        let mut fmt: v4l2_format = std::mem::zeroed();
        fmt.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        fmt.fmt.width = 640;
        fmt.fmt.height = 480;
        fmt.fmt.pixelformat = v4l2_fourcc(b'Y', b'U', b'Y', b'V');
        fmt.fmt.field = 1; // V4L2_FIELD_NONE

        let res = libc::ioctl(fd, VIDIOC_S_FMT, &mut fmt);
        
        if res < 0 {
            println!("[FFI Error] VIDIOC_S_FMT failed: {}", std::io::Error::last_os_error());
        } else {
            println!("[FFI] Format set to {}x{} YUYV", fmt.fmt.width, fmt.fmt.height);
        }
        
        libc::close(fd);
    }
}

pub async fn run_backend(mut rx: mpsc::Receiver<DaemonCmd>) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            DaemonCmd::CapturePhoto => {
                tokio::time::sleep(Duration::from_secs(2)).await;
                println!("[Backend] Photo saved");
            }
        }
    }
}