pub mod contract;
pub mod direct;
pub mod facebook;
pub mod instagram;
pub mod reddit;
pub mod registry;
pub mod tiktok;
pub mod youtube;

pub use contract::{
    AdapterError, AnalysisResult, NormalizedSource, NormalizedSourceDto, PlatformAdapter,
    PlatformCapabilities,
};
pub use direct::DirectMediaAdapter;
pub use facebook::FacebookAdapter;
pub use instagram::InstagramAdapter;
pub use reddit::RedditAdapter;
pub use registry::{AdapterRegistry, RegistryError};
pub use tiktok::TikTokAdapter;
pub use youtube::YouTubeAdapter;
