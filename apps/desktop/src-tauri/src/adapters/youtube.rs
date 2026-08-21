use super::contract::{
    AdapterError, AnalysisResult, NormalizedSource, PlatformAdapter, PlatformCapabilities,
};
use crate::domain::entities::{MediaFormat, MediaItem};
use async_trait::async_trait;
use url::Url;

const PLATFORM_ID: &str = "youtube";

#[derive(Debug, Clone, Copy, Default)]
pub struct YouTubeAdapter;

impl YouTubeAdapter {
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlatformAdapter for YouTubeAdapter {
    fn id(&self) -> &'static str {
        PLATFORM_ID
    }

    fn detect(&self, url: &Url) -> bool {
        normalize_youtube_url(url).is_ok()
    }

    async fn normalize(&self, url: &Url) -> Result<NormalizedSource, AdapterError> {
        normalize_youtube_url(url)
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

fn normalize_youtube_url(url: &Url) -> Result<NormalizedSource, AdapterError> {
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
    let external_id = match host.as_str() {
        "youtube.com" | "www.youtube.com" | "m.youtube.com" => youtube_id_from_hosted_url(url)?,
        "youtu.be" => first_segment(url)?,
        _ => return Err(AdapterError::UnsupportedUrl),
    };

    let canonical_url = Url::parse(&format!("https://www.youtube.com/watch?v={external_id}"))
        .map_err(|_| AdapterError::InvalidUrl)?;

    Ok(NormalizedSource {
        platform_id: PLATFORM_ID.to_owned(),
        original_url: url.clone(),
        canonical_url,
        external_id,
    })
}

fn youtube_id_from_hosted_url(url: &Url) -> Result<String, AdapterError> {
    let segments: Vec<&str> = url
        .path_segments()
        .ok_or(AdapterError::InvalidUrl)?
        .filter(|segment| !segment.is_empty())
        .collect();

    let candidate = match segments.as_slice() {
        [path] if *path == "watch" => url
            .query_pairs()
            .find(|(key, _)| key == "v")
            .map(|(_, value)| value.into_owned()),
        [path, video_id] if matches!(*path, "shorts" | "embed" | "live") => {
            Some((*video_id).to_owned())
        }
        _ => None,
    }
    .ok_or(AdapterError::UnsupportedUrl)?;

    validate_video_id(&candidate)
}

fn first_segment(url: &Url) -> Result<String, AdapterError> {
    let segment = url
        .path_segments()
        .ok_or(AdapterError::InvalidUrl)?
        .find(|segment| !segment.is_empty())
        .ok_or(AdapterError::UnsupportedUrl)?;
    validate_video_id(segment)
}

fn validate_video_id(value: &str) -> Result<String, AdapterError> {
    if (6..=32).contains(&value.len())
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
    use super::YouTubeAdapter;
    use crate::adapters::contract::PlatformAdapter;
    use url::Url;

    #[test]
    fn detects_supported_public_url_shapes() {
        let adapter = YouTubeAdapter::new();
        for raw_url in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ?t=42",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
            "https://m.youtube.com/embed/dQw4w9WgXcQ",
        ] {
            assert!(adapter.detect(&Url::parse(raw_url).unwrap()), "{raw_url}");
        }
    }

    #[tokio::test]
    async fn canonicalizes_without_preserving_tracking_parameters() {
        let adapter = YouTubeAdapter::new();
        let normalized = adapter
            .normalize(&Url::parse("https://youtu.be/dQw4w9WgXcQ?si=tracking").unwrap())
            .await
            .unwrap();
        assert_eq!(normalized.external_id, "dQw4w9WgXcQ");
        assert_eq!(
            normalized.canonical_url.as_str(),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
    }

    #[test]
    fn rejects_insecure_or_non_video_urls() {
        let adapter = YouTubeAdapter::new();
        assert!(!adapter.detect(&Url::parse("http://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap()));
        assert!(!adapter.detect(&Url::parse("https://www.youtube.com/channel/UC123456").unwrap()));
        assert!(!adapter.detect(&Url::parse("https://www.youtube.com/watch?v=short").unwrap()));
        assert!(!adapter
            .detect(&Url::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ#private").unwrap()));
    }

    #[tokio::test]
    async fn fails_closed_without_a_public_media_byte_path() {
        let adapter = YouTubeAdapter::new();
        let source = adapter
            .normalize(&Url::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap())
            .await
            .unwrap();
        assert!(matches!(
            adapter.analyze(&source).await,
            Err(crate::adapters::AdapterError::PublicMediaUnavailable)
        ));
    }
}
