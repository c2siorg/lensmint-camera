use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub enum ChainTarget {
    EVM,
    Solana,
}

#[derive(Debug, Clone)]
pub enum DaemonCmd {
    CapturePhoto(Uuid),
    SetFocus(i32),
    DeletePhoto(Uuid),
    StartVideo(Uuid),
    StopVideo,
    Mint(Uuid, ChainTarget),
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    MintSuccess(Uuid, ChainTarget),
    MintFailed(Uuid, ChainTarget, String),
}