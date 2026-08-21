use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FoundationStatus {
    pub app_name: &'static str,
    pub phase: &'static str,
    pub tauri: bool,
    pub message: &'static str,
}

impl FoundationStatus {
    pub const fn ready() -> Self {
        Self {
            app_name: "Universal Media Downloader",
            phase: "foundation",
            tauri: true,
            message: "The Tauri foundation is connected.",
        }
    }
}
