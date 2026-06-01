use std::ffi::CString;
use std::os::unix::io::RawFd;
use std::time::Duration;
use tokio::sync::mpsc;
use crate::cmd::DaemonCmd;

// V4L2 memory layout for ioctl compatibility
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

// VIDIOC_QUERYCAP magic number: _IOR('V', 0, struct v4l2_capability)
const VIDIOC_QUERYCAP: libc::c_ulong = 0x80685600;

// Safely parse null-terminated C string to Rust String
fn parse_c_string(bytes: &[u8]) -> String {
    let null_pos = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..null_pos]).into_owned()
}

pub fn probe_camera() {
    println!("[FFI] Probing /dev/video0...");
    unsafe {
        let dev_path = CString::new("/dev/video0").unwrap();
        // O_RDWR | O_NONBLOCK
        let fd: RawFd = libc::open(dev_path.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK);
        
        if fd < 0 {
            let err = std::io::Error::last_os_error();
            println!("[FFI Error] Failed to open /dev/video0: {}", err);
            return;
        }

        let mut caps: v4l2_capability = std::mem::zeroed();
        let res = libc::ioctl(fd, VIDIOC_QUERYCAP, &mut caps);
        
        if res < 0 {
            let err = std::io::Error::last_os_error();
            println!("[FFI Error] VIDIOC_QUERYCAP ioctl failed: {}", err);
        } else {
            let driver_name = parse_c_string(&caps.driver);
            let card_name = parse_c_string(&caps.card);
            
            println!("--- Camera Probed Successfully ---");
            println!("Driver: {}", driver_name);
            println!("Card:   {}", card_name);
            println!("----------------------------------");
        }
        
        libc::close(fd);
    }
}

pub async fn run_backend(mut rx: mpsc::Receiver<DaemonCmd>) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            DaemonCmd::CapturePhoto => {
                println!("[Backend] Processing CapturePhoto command...");
                // Mocking the heavy I/O delay
                tokio::time::sleep(Duration::from_secs(2)).await;
                println!("[Backend] Photo saved");
            }
        }
    }
}
