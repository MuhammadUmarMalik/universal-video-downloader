use super::contract::{
    AdapterError, AnalysisResult, NormalizedSource, PlatformAdapter, PlatformCapabilities,
};
use crate::domain::entities::{MediaFormat, MediaItem};
use async_trait::async_trait;
use serde_json::json;
use url::Url;

const PLATFORM_ID: &str = "direct";
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "webm", "mov", "m4v", "mkv"];
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "wav", "ogg", "flac", "aac", "opus"];

#[derive(Debug, Clone, Copy, Default)]
pub struct DirectMediaAdapter;

impl DirectMediaAdapter {
    pub const fn new() -> Self {
        Self
    }

    pub fn is_supported_media_url(url: &Url) -> bool {
        normalize_direct_media_url(url).is_ok()
    }
}

#[async_trait]
impl PlatformAdapter for DirectMediaAdapter {
    fn id(&self) -> &'static str {
        PLATFORM_ID
    }

    fn detect(&self, url: &Url) -> bool {
        Self::is_supported_media_url(url)
    }

    async fn normalize(&self, url: &Url) -> Result<NormalizedSource, AdapterError> {
        normalize_direct_media_url(url)
    }

    async fn analyze(&self, source: &NormalizedSource) -> Result<AnalysisResult, AdapterError> {
        if source.platform_id != PLATFORM_ID {
            return Err(AdapterError::PlatformMismatch);
        }
        let extension = media_extension(&source.canonical_url)?;
        let is_video = VIDEO_EXTENSIONS.contains(&extension.as_str());
        let item_id = format!("direct:item:{}", source.external_id);
        let format_id = format!("direct:format:{}", source.external_id);
        let title = title_from_url(&source.canonical_url, &extension);
        let now = now_utc();

        let item = MediaItem {
            id: item_id.clone(),
            source_id: format!("direct:source:{}", source.external_id),
            collection_id: None,
            external_id: Some(source.external_id.clone()),
            canonical_url: source.canonical_url.to_string(),
            title,
            creator_name: None,
            creator_id: None,
            thumbnail_url: None,
            duration_ms: None,
            published_at: None,
            position: Some(0),
            metadata_json: Some(json!({
                "platform_id": PLATFORM_ID,
                "public_url": source.canonical_url,
                "source_kind": "direct_public_media"
            })),
            first_seen_at: now.clone(),
            last_seen_at: now.clone(),
        };

        let format = MediaFormat {
            id: format_id,
            media_item_id: item_id,
            external_format_id: Some(extension.clone()),
            container: Some(extension),
            video_codec: None,
            audio_codec: None,
            width: None,
            height: None,
            fps: None,
            bitrate: None,
            sample_rate: None,
            channels: None,
            file_size_bytes: None,
            is_video,
            is_audio: !is_video,
            is_progressive: true,
            metadata_json: Some(json!({
                "platform_id": PLATFORM_ID,
                "public_url": source.canonical_url,
                "kind": "direct_progressive"
            })),
            created_at: now,
        };

        Ok(AnalysisResult {
            source: source.into(),
            items: vec![item],
            formats: vec![format],
        })
    }

    async fn resolve_formats(&self, item: &MediaItem) -> Result<Vec<MediaFormat>, AdapterError> {
        let metadata = item
            .metadata_json
            .as_ref()
            .and_then(|value| value.get("public_url"))
            .and_then(serde_json::Value::as_str)
            .ok_or(AdapterError::PublicMediaUnavailable)?;
        let url = Url::parse(metadata).map_err(|_| AdapterError::PublicMediaUnavailable)?;
        let source = normalize_direct_media_url(&url)?;
        Ok(self.analyze(&source).await?.formats)
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            single_item: true,
            collections: false,
            audio_only: true,
            thumbnails: false,
            metadata: false,
            resume: true,
            scheduling: false,
        }
    }
}

fn normalize_direct_media_url(url: &Url) -> Result<NormalizedSource, AdapterError> {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return Err(AdapterError::InvalidUrl);
    }
    if !is_supported_media_path(url) {
        return Err(AdapterError::UnsupportedUrl);
    }

    Ok(NormalizedSource {
        platform_id: PLATFORM_ID.to_owned(),
        original_url: url.clone(),
        canonical_url: url.clone(),
        external_id: url.to_string(),
    })
}

fn is_supported_media_path(url: &Url) -> bool {
    media_extension(url)
        .map(|extension| {
            VIDEO_EXTENSIONS.contains(&extension.as_str())
                || AUDIO_EXTENSIONS.contains(&extension.as_str())
        })
        .unwrap_or(false)
}

fn media_extension(url: &Url) -> Result<String, AdapterError> {
    let segment = url
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .ok_or(AdapterError::UnsupportedUrl)?;
    let extension = segment
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .ok_or(AdapterError::UnsupportedUrl)?;
    if VIDEO_EXTENSIONS.contains(&extension.as_str())
        || AUDIO_EXTENSIONS.contains(&extension.as_str())
    {
        Ok(extension)
    } else {
        Err(AdapterError::UnsupportedUrl)
    }
}

fn title_from_url(url: &Url, extension: &str) -> String {
    let fallback = format!("direct-media.{extension}");
    let Some(segment) = url
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
    else {
        return fallback;
    };
    let title = segment
        .rsplit_once('.')
        .map(|(title, _)| title)
        .unwrap_or(segment)
        .replace(['_', '-'], " ");
    if title.trim().is_empty() {
        fallback
    } else {
        title
    }
}

fn now_utc() -> String {
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

#[cfg(test)]
mod tests {
    use super::DirectMediaAdapter;
    use crate::adapters::contract::PlatformAdapter;
    use url::Url;

    #[test]
    fn detects_only_direct_https_media_files() {
        let adapter = DirectMediaAdapter::new();
        assert!(adapter.detect(
            &Url::parse("https://cdn.example.test/media/video.mp4?token=short-lived").unwrap()
        ));
        assert!(adapter.detect(&Url::parse("https://cdn.example.test/audio/song.mp3").unwrap()));
        assert!(!adapter.detect(&Url::parse("https://example.test/video").unwrap()));
        assert!(!adapter
            .detect(&Url::parse("https://www.tiktok.com/@creator/video/1234567890").unwrap()));
        assert!(!adapter.detect(&Url::parse("https://www.instagram.com/reel/ABC123").unwrap()));
    }

    #[tokio::test]
    async fn analyzes_a_direct_media_url_without_fetching_or_scraping() {
        let adapter = DirectMediaAdapter::new();
        let source = adapter
            .normalize(
                &Url::parse("https://cdn.example.test/media/My_video.mp4?token=short-lived")
                    .unwrap(),
            )
            .await
            .unwrap();
        let analysis = adapter.analyze(&source).await.unwrap();
        assert_eq!(analysis.items[0].title, "My video");
        assert_eq!(analysis.formats.len(), 1);
        assert!(analysis.formats[0].is_video);
        assert!(analysis.formats[0].is_progressive);
        assert_eq!(
            analysis.formats[0]
                .metadata_json
                .as_ref()
                .and_then(|value| value.get("public_url"))
                .and_then(serde_json::Value::as_str),
            Some("https://cdn.example.test/media/My_video.mp4?token=short-lived")
        );
    }

    #[tokio::test]
    async fn rejects_unsafe_direct_urls() {
        let adapter = DirectMediaAdapter::new();
        for raw_url in [
            "http://cdn.example.test/video.mp4",
            "https://user:pass@cdn.example.test/video.mp4",
            "https://cdn.example.test/video.mp4#fragment",
        ] {
            assert!(adapter
                .normalize(&Url::parse(raw_url).unwrap())
                .await
                .is_err());
        }
    }
}
