use super::contract::{
    AdapterError, AnalysisResult, NormalizedSource, PlatformAdapter, PlatformCapabilities,
};
use crate::domain::entities::{MediaFormat, MediaItem};
use async_trait::async_trait;
use url::Url;

const PLATFORM_ID: &str = "instagram";

#[derive(Debug, Clone, Copy, Default)]
pub struct InstagramAdapter;

impl InstagramAdapter {
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlatformAdapter for InstagramAdapter {
    fn id(&self) -> &'static str {
        PLATFORM_ID
    }

    fn detect(&self, url: &Url) -> bool {
        normalize_instagram_url(url).is_ok()
    }

    async fn normalize(&self, url: &Url) -> Result<NormalizedSource, AdapterError> {
        normalize_instagram_url(url)
    }

    async fn analyze(&self, _source: &NormalizedSource) -> Result<AnalysisResult, AdapterError> {
        Err(AdapterError::PublicMediaUnavailable)
    }

    async fn resolve_formats(&self, _item: &MediaItem) -> Result<Vec<MediaFormat>, AdapterError> {
        Err(AdapterError::PublicMediaUnavailable)
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            single_item: true,
            collections: false,
            audio_only: false,
            thumbnails: false,
            metadata: false,
            resume: false,
            scheduling: false,
        }
    }
}

fn normalize_instagram_url(url: &Url) -> Result<NormalizedSource, AdapterError> {
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
    if !matches!(
        host.as_str(),
        "instagram.com" | "www.instagram.com" | "m.instagram.com"
    ) {
        return Err(AdapterError::UnsupportedUrl);
    }

    let segments: Vec<&str> = url
        .path_segments()
        .ok_or(AdapterError::InvalidUrl)?
        .filter(|segment| !segment.is_empty())
        .collect();
    let [kind, external_id] = segments.as_slice() else {
        return Err(AdapterError::UnsupportedUrl);
    };
    if !matches!(*kind, "p" | "reel" | "reels" | "tv") {
        return Err(AdapterError::UnsupportedUrl);
    }
    validate_media_id(external_id)?;

    let canonical_url = Url::parse(&format!("https://www.instagram.com/{kind}/{external_id}/"))
        .map_err(|_| AdapterError::InvalidUrl)?;

    Ok(NormalizedSource {
        platform_id: PLATFORM_ID.to_owned(),
        original_url: url.clone(),
        canonical_url,
        external_id: (*external_id).to_owned(),
    })
}

fn validate_media_id(value: &str) -> Result<(), AdapterError> {
    if (2..=128).contains(&value.len())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        Ok(())
    } else {
        Err(AdapterError::UnsupportedUrl)
    }
}

#[cfg(test)]
mod tests {
    use super::InstagramAdapter;
    use crate::adapters::contract::PlatformAdapter;
    use url::Url;

    #[test]
    fn detects_public_post_reel_and_tv_url_shapes() {
        let adapter = InstagramAdapter::new();
        for raw_url in [
            "https://www.instagram.com/p/ABC123_-",
            "https://www.instagram.com/reel/ABC123_-",
            "https://www.instagram.com/reels/ABC123_-",
            "https://www.instagram.com/tv/ABC123_-",
        ] {
            assert!(adapter.detect(&Url::parse(raw_url).unwrap()), "{raw_url}");
        }
    }

    #[tokio::test]
    async fn canonicalizes_public_media_page_urls() {
        let adapter = InstagramAdapter::new();
        let normalized = adapter
            .normalize(
                &Url::parse("https://m.instagram.com/reel/ABC123_-/?utm_source=test").unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(normalized.external_id, "ABC123_-");
        assert_eq!(
            normalized.canonical_url.as_str(),
            "https://www.instagram.com/reel/ABC123_-/"
        );
    }

    #[test]
    fn rejects_insecure_or_non_media_page_urls() {
        let adapter = InstagramAdapter::new();
        assert!(!adapter.detect(&Url::parse("http://www.instagram.com/p/ABC123").unwrap()));
        assert!(!adapter.detect(&Url::parse("https://www.instagram.com/accounts/login/").unwrap()));
        assert!(!adapter.detect(&Url::parse("https://www.instagram.com/p/ABC123#private").unwrap()));
    }

    #[tokio::test]
    async fn fails_closed_without_an_official_public_media_byte_path() {
        let adapter = InstagramAdapter::new();
        let source = adapter
            .normalize(&Url::parse("https://www.instagram.com/p/ABC123").unwrap())
            .await
            .unwrap();
        assert!(matches!(
            adapter.analyze(&source).await,
            Err(crate::adapters::AdapterError::PublicMediaUnavailable)
        ));
    }
}
