use super::contract::{
    AdapterError, AnalysisResult, NormalizedSource, PlatformAdapter, PlatformCapabilities,
};
use crate::domain::entities::{MediaFormat, MediaItem};
use async_trait::async_trait;
use url::Url;

const PLATFORM_ID: &str = "facebook";

#[derive(Debug, Clone, Copy, Default)]
pub struct FacebookAdapter;

impl FacebookAdapter {
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlatformAdapter for FacebookAdapter {
    fn id(&self) -> &'static str {
        PLATFORM_ID
    }

    fn detect(&self, url: &Url) -> bool {
        normalize_facebook_url(url).is_ok()
    }

    async fn normalize(&self, url: &Url) -> Result<NormalizedSource, AdapterError> {
        normalize_facebook_url(url)
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

fn normalize_facebook_url(url: &Url) -> Result<NormalizedSource, AdapterError> {
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
        "facebook.com" | "www.facebook.com" | "m.facebook.com"
    ) {
        return Err(AdapterError::UnsupportedUrl);
    }

    let segments: Vec<&str> = url
        .path_segments()
        .ok_or(AdapterError::InvalidUrl)?
        .filter(|segment| !segment.is_empty())
        .collect();

    let (canonical_path, external_id) = match segments.as_slice() {
        [kind, id] if matches!(*kind, "reel" | "watch") => {
            (format!("/{kind}/{id}"), validate_media_id(id)?)
        }
        ["watch"] => {
            let id = url
                .query_pairs()
                .find(|(key, _)| key == "v")
                .map(|(_, value)| value.into_owned())
                .ok_or(AdapterError::UnsupportedUrl)?;
            (format!("/watch?v={id}"), validate_media_id(&id)?)
        }
        [page, "videos", id] => (format!("/{page}/videos/{id}"), validate_media_id(id)?),
        _ => return Err(AdapterError::UnsupportedUrl),
    };

    let canonical_url = Url::parse(&format!("https://www.facebook.com{canonical_path}"))
        .map_err(|_| AdapterError::InvalidUrl)?;

    Ok(NormalizedSource {
        platform_id: PLATFORM_ID.to_owned(),
        original_url: url.clone(),
        canonical_url,
        external_id,
    })
}

fn validate_media_id(value: &str) -> Result<String, AdapterError> {
    if (2..=128).contains(&value.len())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        Ok(value.to_owned())
    } else {
        Err(AdapterError::UnsupportedUrl)
    }
}

#[cfg(test)]
mod tests {
    use super::FacebookAdapter;
    use crate::adapters::contract::PlatformAdapter;
    use url::Url;

    #[test]
    fn detects_supported_public_video_url_shapes() {
        let adapter = FacebookAdapter::new();
        for raw_url in [
            "https://www.facebook.com/reel/1234567890",
            "https://www.facebook.com/watch/?v=1234567890",
            "https://www.facebook.com/examplepage/videos/1234567890",
        ] {
            assert!(adapter.detect(&Url::parse(raw_url).unwrap()), "{raw_url}");
        }
    }

    #[tokio::test]
    async fn canonicalizes_facebook_video_urls() {
        let adapter = FacebookAdapter::new();
        let normalized = adapter
            .normalize(
                &Url::parse("https://m.facebook.com/watch/?v=1234567890&utm_source=test").unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(normalized.external_id, "1234567890");
        assert_eq!(
            normalized.canonical_url.as_str(),
            "https://www.facebook.com/watch?v=1234567890"
        );
    }

    #[test]
    fn rejects_insecure_or_non_video_urls() {
        let adapter = FacebookAdapter::new();
        assert!(!adapter.detect(&Url::parse("http://www.facebook.com/reel/1234567890").unwrap()));
        assert!(
            !adapter.detect(&Url::parse("https://www.facebook.com/profile.php?id=123").unwrap())
        );
        assert!(!adapter.detect(&Url::parse("https://www.facebook.com/reel/1#private").unwrap()));
    }

    #[tokio::test]
    async fn fails_closed_without_a_public_media_byte_path() {
        let adapter = FacebookAdapter::new();
        let source = adapter
            .normalize(&Url::parse("https://www.facebook.com/reel/1234567890").unwrap())
            .await
            .unwrap();
        assert!(matches!(
            adapter.analyze(&source).await,
            Err(crate::adapters::AdapterError::PublicMediaUnavailable)
        ));
    }
}
