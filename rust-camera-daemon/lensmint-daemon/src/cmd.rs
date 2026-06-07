use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum DaemonCmd {
    CapturePhoto,
    SetFocus(i32),
    DeletePhoto(Uuid),
}