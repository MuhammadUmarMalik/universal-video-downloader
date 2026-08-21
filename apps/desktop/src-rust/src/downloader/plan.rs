use super::path_safety::{validate_destination, DestinationPathError, DestinationPaths};
use crate::domain::entities::{MediaFormat, MediaItem};
use serde_json::Value;
use std::path::Path;
use thiserror::Error;
use url::Url;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DownloadPlanError {
    #[error("the selected format does not belong to the selected media item")]
    FormatItemMismatch,
    #[error("the selected format is not a progressive download")]
    FormatNotProgressive,
    #[error("the selected format has invalid metadata")]
    InvalidFormatMetadata,
    #[error("the selected platform is not enabled for direct progressive downloads")]
    UnsupportedPlatform,
    #[error("the adapter did not provide a public media URL")]
    MissingPublicUrl,
    #[error("the adapter-provided media URL is invalid")]
    InvalidPublicUrl,
    #[error("the adapter-provided media URL is not on an approved public host")]
    UnapprovedPublicHost,
    #[error("the destination path is unsafe: {0}")]
    Destination(#[from] DestinationPathError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadPlan {
    pub media_item_id: String,
    pub format_id: String,
    pub platform_id: String,
    pub source_url: Url,
    pub destination: DestinationPaths,
    pub total_bytes: Option<i64>,
}

impl DownloadPlan {
    pub fn resolve(
        platform_id: &str,
        item: &MediaItem,
        format: &MediaFormat,
        destination_root: &Path,
        filename: &str,
    ) -> Result<Self, DownloadPlanError> {
        if item.id.is_empty() || format.id.is_empty() || format.media_item_id != item.id {
            return Err(DownloadPlanError::FormatItemMismatch);
        }
        if !format.is_progressive {
            return Err(DownloadPlanError::FormatNotProgressive);
        }
        if format.file_size_bytes.is_some_and(|bytes| bytes < 0) {
            return Err(DownloadPlanError::InvalidFormatMetadata);
        }

        if !matches!(platform_id, "reddit" | "direct") {
            return Err(DownloadPlanError::UnsupportedPlatform);
        }
        let source_url = public_url_from_format(platform_id, format)?;
        let destination = validate_destination(destination_root, filename)?;

        Ok(Self {
            media_item_id: item.id.clone(),
            format_id: format.id.clone(),
            platform_id: platform_id.to_owned(),
            source_url,
            destination,
            total_bytes: format.file_size_bytes,
        })
    }
}

fn public_url_from_format(
    platform_id: &str,
    format: &MediaFormat,
) -> Result<Url, DownloadPlanError> {
    let metadata = format
        .metadata_json
        .as_ref()
        .and_then(Value::as_object)
        .ok_or(DownloadPlanError::InvalidFormatMetadata)?;
    let value = metadata
        .get("public_url")
        .and_then(Value::as_str)
        .ok_or(DownloadPlanError::MissingPublicUrl)?;
    let url = Url::parse(value).map_err(|_| DownloadPlanError::InvalidPublicUrl)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(DownloadPlanError::InvalidPublicUrl);
    }
    let host = url
        .host_str()
        .ok_or(DownloadPlanError::InvalidPublicUrl)?
        .to_ascii_lowercase();
    match platform_id {
        "reddit" if host == "v.redd.it" || host.ends_with(".redd.it") => Ok(url),
        "direct" if is_direct_media_url(&url) => Ok(url),
        "reddit" | "direct" => Err(DownloadPlanError::UnapprovedPublicHost),
        _ => Err(DownloadPlanError::UnsupportedPlatform),
    }
}

fn is_direct_media_url(url: &Url) -> bool {
    const EXTENSIONS: &[&str] = &[
        "mp4", "webm", "mov", "m4v", "mkv", "mp3", "m4a", "wav", "ogg", "flac", "aac", "opus",
    ];
    url.path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .and_then(|segment| segment.rsplit_once('.').map(|(_, extension)| extension))
        .is_some_and(|extension| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
}

#[cfg(test)]
mod tests {
    use super::{DownloadPlan, DownloadPlanError};
    use crate::domain::entities::{MediaFormat, MediaItem};
    use serde_json::json;
    use std::path::Path;

    fn item() -> MediaItem {
        MediaItem {
            id: "reddit:item:abc123".to_owned(),
            source_id: "reddit:source:abc123".to_owned(),
            collection_id: None,
            external_id: Some("abc123".to_owned()),
            canonical_url: "https://www.reddit.com/comments/abc123/".to_owned(),
            title: "Public video".to_owned(),
            creator_name: Some("creator".to_owned()),
            creator_id: None,
            thumbnail_url: None,
            duration_ms: Some(1_000),
            published_at: None,
            position: Some(0),
            metadata_json: Some(json!({"platform_id": "reddit"})),
            first_seen_at: "2026-01-01T00:00:00Z".to_owned(),
            last_seen_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    fn format(progressive: bool, public_url: &str) -> MediaFormat {
        MediaFormat {
            id: "reddit:format:abc123:fallback".to_owned(),
            media_item_id: "reddit:item:abc123".to_owned(),
            external_format_id: Some("fallback".to_owned()),
            container: Some("mp4".to_owned()),
            video_codec: None,
            audio_codec: None,
            width: Some(1920),
            height: Some(1080),
            fps: None,
            bitrate: None,
            sample_rate: None,
            channels: None,
            file_size_bytes: Some(1_000),
            is_video: true,
            is_audio: false,
            is_progressive: progressive,
            metadata_json: Some(json!({"public_url": public_url, "kind": "fallback"})),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn resolves_a_valid_reddit_progressive_format() {
        let plan = DownloadPlan::resolve(
            "reddit",
            &item(),
            &format(true, "https://v.redd.it/abc123/DASH_720.mp4"),
            Path::new("/downloads"),
            "creator - Public video.mp4",
        )
        .unwrap();
        assert_eq!(plan.platform_id, "reddit");
        assert_eq!(plan.media_item_id, "reddit:item:abc123");
        assert_eq!(plan.format_id, "reddit:format:abc123:fallback");
        assert_eq!(plan.source_url.host_str(), Some("v.redd.it"));
        assert_eq!(plan.total_bytes, Some(1_000));
        assert_eq!(
            plan.destination.destination,
            Path::new("/downloads/creator - Public video.mp4")
        );
    }

    #[test]
    fn rejects_non_progressive_formats_before_using_their_url() {
        let error = DownloadPlan::resolve(
            "reddit",
            &item(),
            &format(false, "https://v.redd.it/abc123/playlist.m3u8"),
            Path::new("/downloads"),
            "video.mp4",
        )
        .expect_err("DASH/HLS formats are deferred to media processing");
        assert_eq!(error, DownloadPlanError::FormatNotProgressive);
    }

    #[test]
    fn rejects_mismatched_items_and_missing_metadata() {
        let mut mismatched = format(true, "https://v.redd.it/abc123/video.mp4");
        mismatched.media_item_id = "other-item".to_owned();
        assert_eq!(
            DownloadPlan::resolve(
                "reddit",
                &item(),
                &mismatched,
                Path::new("/downloads"),
                "video.mp4"
            ),
            Err(DownloadPlanError::FormatItemMismatch)
        );

        let mut missing = format(true, "https://v.redd.it/abc123/video.mp4");
        missing.metadata_json = None;
        assert_eq!(
            DownloadPlan::resolve(
                "reddit",
                &item(),
                &missing,
                Path::new("/downloads"),
                "video.mp4"
            ),
            Err(DownloadPlanError::InvalidFormatMetadata)
        );
    }

    #[test]
    fn rejects_non_https_credentials_fragments_and_unapproved_hosts() {
        for url in [
            "http://v.redd.it/abc123/video.mp4",
            "https://user:pass@v.redd.it/abc123/video.mp4",
            "https://v.redd.it/abc123/video.mp4#fragment",
        ] {
            let error = DownloadPlan::resolve(
                "reddit",
                &item(),
                &format(true, url),
                Path::new("/downloads"),
                "video.mp4",
            )
            .expect_err("unsafe URL should be rejected");
            assert_eq!(error, DownloadPlanError::InvalidPublicUrl);
        }
        assert_eq!(
            DownloadPlan::resolve(
                "reddit",
                &item(),
                &format(true, "https://example.com/video.mp4"),
                Path::new("/downloads"),
                "video.mp4",
            ),
            Err(DownloadPlanError::UnapprovedPublicHost)
        );
    }

    #[test]
    fn resolves_a_direct_public_media_url() {
        let mut media_item = item();
        media_item.id = "direct:item:video".to_owned();
        media_item.source_id = "direct:source:video".to_owned();
        media_item.metadata_json = Some(json!({"platform_id": "direct"}));
        let mut direct_format =
            format(true, "https://cdn.example.test/video.mp4?token=short-lived");
        direct_format.id = "direct:format:video".to_owned();
        direct_format.media_item_id = media_item.id.clone();
        direct_format.metadata_json =
            Some(json!({"public_url": "https://cdn.example.test/video.mp4?token=short-lived"}));

        let plan = DownloadPlan::resolve(
            "direct",
            &media_item,
            &direct_format,
            Path::new("/downloads"),
            "video.mp4",
        )
        .unwrap();
        assert_eq!(plan.platform_id, "direct");
        assert_eq!(plan.source_url.host_str(), Some("cdn.example.test"));
    }

    #[test]
    fn rejects_non_reddit_platform_metadata() {
        let mut media_item = item();
        media_item.metadata_json = Some(json!({"platform_id": "tiktok"}));
        assert_eq!(
            DownloadPlan::resolve(
                "tiktok",
                &media_item,
                &format(true, "https://v.redd.it/abc123/video.mp4"),
                Path::new("/downloads"),
                "video.mp4",
            ),
            Err(DownloadPlanError::UnsupportedPlatform)
        );
        assert_eq!(
            DownloadPlan::resolve(
                "direct",
                &media_item,
                &format(true, "https://example.com/page"),
                Path::new("/downloads"),
                "video.mp4",
            ),
            Err(DownloadPlanError::UnapprovedPublicHost)
        );
    }
}
