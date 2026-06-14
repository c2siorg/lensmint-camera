use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum DaemonCmd {
    CapturePhoto(Uuid),
    SetFocus(i32),
    DeletePhoto(Uuid),
    StartVideo(Uuid),
    StopVideo,
}