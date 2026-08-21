pub mod contract;
pub mod reddit;
pub mod registry;
pub mod tiktok;

pub use contract::{
    AdapterError, AnalysisResult, NormalizedSource, NormalizedSourceDto, PlatformAdapter,
    PlatformCapabilities,
};
pub use reddit::RedditAdapter;
pub use registry::{AdapterRegistry, RegistryError};
pub use tiktok::TikTokAdapter;
