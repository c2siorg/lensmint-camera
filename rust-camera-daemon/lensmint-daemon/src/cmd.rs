#[derive(Debug, Clone)]
pub enum DaemonCmd {
    CapturePhoto,
    SetFocus(i32),
}