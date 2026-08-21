use super::{
    AdapterError, FacebookAdapter, PlatformAdapter, RedditAdapter, TikTokAdapter, YouTubeAdapter,
};
use std::sync::Arc;
use thiserror::Error;
use url::Url;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("the URL is invalid")]
    InvalidUrl,
    #[error("no supported platform adapter detected for this URL")]
    UnsupportedPlatform,
    #[error("the selected platform does not match the URL")]
    PlatformMismatch,
    #[error("the selected platform adapter is not registered")]
    AdapterNotRegistered,
}

#[derive(Clone)]
pub struct AdapterRegistry {
    adapters: Vec<Arc<dyn PlatformAdapter>>,
}

impl AdapterRegistry {
    pub fn with_defaults() -> Result<Self, AdapterError> {
        Ok(Self::new(vec![
            Arc::new(RedditAdapter::new()?),
            Arc::new(TikTokAdapter::new()),
            Arc::new(YouTubeAdapter::new()),
            Arc::new(FacebookAdapter::new()),
        ]))
    }

    pub fn new(adapters: Vec<Arc<dyn PlatformAdapter>>) -> Self {
        Self { adapters }
    }

    pub fn list(&self) -> Vec<&'static str> {
        self.adapters.iter().map(|adapter| adapter.id()).collect()
    }

    pub fn select(
        &self,
        url: &Url,
        explicit_platform: Option<&str>,
    ) -> Result<Arc<dyn PlatformAdapter>, RegistryError> {
        if let Some(platform_id) = explicit_platform {
            let adapter = self
                .adapters
                .iter()
                .find(|adapter| adapter.id() == platform_id)
                .cloned()
                .ok_or(RegistryError::AdapterNotRegistered)?;
            return if adapter.detect(url) {
                Ok(adapter)
            } else {
                Err(RegistryError::PlatformMismatch)
            };
        }

        self.adapters
            .iter()
            .find(|adapter| adapter.detect(url))
            .cloned()
            .ok_or(RegistryError::UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests {
    use super::{AdapterRegistry, RegistryError};
    use url::Url;

    #[test]
    fn defaults_auto_detect_supported_reddit_urls() {
        let registry = AdapterRegistry::with_defaults().unwrap();
        let url = Url::parse("https://www.reddit.com/r/videos/comments/abc123/title").unwrap();
        assert_eq!(registry.select(&url, None).unwrap().id(), "reddit");
        assert_eq!(
            registry.list(),
            vec!["reddit", "tiktok", "youtube", "facebook"]
        );
        let tiktok_url = Url::parse("https://www.tiktok.com/@creator/video/1234567890").unwrap();
        assert_eq!(registry.select(&tiktok_url, None).unwrap().id(), "tiktok");
        let youtube_url = Url::parse("https://youtu.be/dQw4w9WgXcQ").unwrap();
        assert_eq!(registry.select(&youtube_url, None).unwrap().id(), "youtube");
        let facebook_url = Url::parse("https://www.facebook.com/reel/1234567890").unwrap();
        assert_eq!(
            registry.select(&facebook_url, None).unwrap().id(),
            "facebook"
        );
    }

    #[test]
    fn explicit_platform_selection_fails_closed_on_mismatch_or_unknown_id() {
        let registry = AdapterRegistry::with_defaults().unwrap();
        let reddit_url =
            Url::parse("https://www.reddit.com/r/videos/comments/abc123/title").unwrap();
        let other_url = Url::parse("https://example.com/video").unwrap();
        assert!(matches!(
            registry.select(&reddit_url, Some("missing")),
            Err(RegistryError::AdapterNotRegistered)
        ));
        assert!(matches!(
            registry.select(&other_url, Some("reddit")),
            Err(RegistryError::PlatformMismatch)
        ));
    }

    #[test]
    fn automatic_detection_rejects_unknown_platforms() {
        let registry = AdapterRegistry::with_defaults().unwrap();
        let url = Url::parse("https://example.com/video").unwrap();
        assert!(matches!(
            registry.select(&url, None),
            Err(RegistryError::UnsupportedPlatform)
        ));
    }
}
