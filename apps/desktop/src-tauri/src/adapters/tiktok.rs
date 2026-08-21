use super::contract::{
    AdapterError, AnalysisResult, NormalizedSource, PlatformAdapter, PlatformCapabilities,
};
use crate::domain::entities::{MediaFormat, MediaItem};
use async_trait::async_trait;
use url::Url;

const PLATFORM_ID: &str = "tiktok";

#[derive(Debug, Clone, Copy, Default)]
pub struct TikTokAdapter;

impl TikTokAdapter {
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlatformAdapter for TikTokAdapter {
    fn id(&self) -> &'static str {
        PLATFORM_ID
    }

    fn detect(&self, url: &Url) -> bool {
        normalize_tiktok_url(url).is_ok()
    }

    async fn normalize(&self, url: &Url) -> Result<NormalizedSource, AdapterError> {
        normalize_tiktok_url(url)
    }

    async fn analyze(&self, _source: &NormalizedSource) -> Result<AnalysisResult, AdapterError> {
        Err(AdapterError::AuthenticationRequired)
    }

    async fn resolve_formats(&self, _item: &MediaItem) -> Result<Vec<MediaFormat>, AdapterError> {
        Err(AdapterError::AuthenticationRequired)
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            single_item: true,
            collections: false,
            audio_only: false,
            thumbnails: false,
            metadata: true,
            resume: false,
            scheduling: false,
        }
    }
}

fn normalize_tiktok_url(url: &Url) -> Result<NormalizedSource, AdapterError> {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(AdapterError::InvalidUrl);
    }
    let host = url
        .host_str()
        .ok_or(AdapterError::InvalidUrl)?
        .to_ascii_lowercase();
    if host != "www.tiktok.com" && host != "tiktok.com" {
        return Err(AdapterError::UnsupportedUrl);
    }
    let segments: Vec<&str> = url
        .path_segments()
        .ok_or(AdapterError::InvalidUrl)?
        .filter(|segment| !segment.is_empty())
        .collect();
    let username = segments
        .first()
        .filter(|segment| segment.starts_with('@') && segment.len() > 1)
        .ok_or(AdapterError::UnsupportedUrl)?;
    if segments.get(1) != Some(&"video") {
        return Err(AdapterError::UnsupportedUrl);
    }
    let external_id = segments
        .get(2)
        .filter(|id| !id.is_empty() && id.chars().all(|character| character.is_ascii_digit()))
        .ok_or(AdapterError::UnsupportedUrl)?;
    let canonical_url = Url::parse(&format!(
        "https://www.tiktok.com/{username}/video/{external_id}"
    ))
    .map_err(|_| AdapterError::InvalidUrl)?;

    Ok(NormalizedSource {
        platform_id: PLATFORM_ID.to_owned(),
        original_url: url.clone(),
        canonical_url,
        external_id: (*external_id).to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::TikTokAdapter;
    use crate::adapters::contract::PlatformAdapter;
    use url::Url;

    #[tokio::test]
    async fn detects_and_normalizes_canonical_video_urls() {
        let adapter = TikTokAdapter::new();
        let url = Url::parse("https://www.tiktok.com/@creator/video/1234567890?lang=en").unwrap();
        assert!(adapter.detect(&url));
        let normalized = adapter.normalize(&url).await.unwrap();
        assert_eq!(normalized.external_id, "1234567890");
        assert_eq!(
            normalized.canonical_url.as_str(),
            "https://www.tiktok.com/@creator/video/1234567890"
        );
    }

    #[test]
    fn rejects_shortlinks_and_noncanonical_paths() {
        let adapter = TikTokAdapter::new();
        assert!(!adapter.detect(&Url::parse("https://vm.tiktok.com/abc123/").unwrap()));
        assert!(!adapter.detect(&Url::parse("https://www.tiktok.com/@creator").unwrap()));
        assert!(!adapter.detect(&Url::parse("http://www.tiktok.com/@creator/video/123").unwrap()));
    }

    #[tokio::test]
    async fn fails_closed_without_credentials_or_media_access_workarounds() {
        let adapter = TikTokAdapter::new();
        let url = Url::parse("https://www.tiktok.com/@creator/video/1234567890").unwrap();
        let normalized = adapter.normalize(&url).await.unwrap();
        let error = adapter.analyze(&normalized).await.unwrap_err();
        assert!(matches!(
            error,
            crate::adapters::AdapterError::AuthenticationRequired
        ));
    }
}
