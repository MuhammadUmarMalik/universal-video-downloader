use super::contract::{
    AdapterError, AnalysisResult, NormalizedSource, PlatformAdapter, PlatformCapabilities,
};
use crate::domain::entities::{MediaFormat, MediaItem};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

const PLATFORM_ID: &str = "reddit";
const USER_AGENT: &str = "universal-media-downloader/0.1 public-media-adapter";
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone)]
pub struct RedditAdapter {
    client: Client,
}

impl RedditAdapter {
    pub fn new() -> Result<Self, AdapterError> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::limited(3))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| AdapterError::Network { retryable: false })?;
        Ok(Self { client })
    }

    fn endpoint_for(source: &NormalizedSource) -> Url {
        Url::parse(&format!(
            "https://www.reddit.com/comments/{}.json",
            source.external_id
        ))
        .expect("the normalized Reddit endpoint template is valid")
    }

    async fn fetch_post(&self, source: &NormalizedSource) -> Result<RedditPost, AdapterError> {
        let response = self
            .client
            .get(Self::endpoint_for(source))
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|_| AdapterError::Network { retryable: true })?;

        let status = response.status();
        if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            return Err(AdapterError::ResourceUnavailable { retryable: true });
        }
        if !status.is_success() {
            return Err(AdapterError::ResourceUnavailable { retryable: false });
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|_| AdapterError::Network { retryable: true })?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(AdapterError::MalformedResponse {
                details: "response exceeds the adapter size limit",
            });
        }

        let listings: Vec<RedditListing<RedditPost>> =
            serde_json::from_slice(&bytes).map_err(|_| AdapterError::MalformedResponse {
                details: "expected a Reddit listing response",
            })?;
        listings
            .into_iter()
            .flat_map(|listing| listing.data.children)
            .map(|child| child.data)
            .find(|post| post.id == source.external_id)
            .ok_or(AdapterError::MalformedResponse {
                details: "the requested post was not present in the listing",
            })
    }

    fn build_analysis(
        source: &NormalizedSource,
        post: RedditPost,
    ) -> Result<AnalysisResult, AdapterError> {
        if post.over_18.unwrap_or(false) || post.removed_by_category.is_some() {
            return Err(AdapterError::ResourceUnavailable { retryable: false });
        }
        let video = post
            .secure_media
            .as_ref()
            .and_then(|media| media.reddit_video.as_ref())
            .or_else(|| {
                post.media
                    .as_ref()
                    .and_then(|media| media.reddit_video.as_ref())
            })
            .ok_or(AdapterError::PublicMediaUnavailable)?;
        let descriptor = RedditMediaDescriptor::from(video);
        let formats = descriptor.formats(&post.id)?;
        let item_id = format!("reddit:item:{}", post.id);
        let metadata = RedditItemMetadata {
            subreddit: post.subreddit,
            post_hint: post.post_hint,
            media: descriptor,
        };
        let metadata_json =
            serde_json::to_value(metadata).map_err(|_| AdapterError::MalformedResponse {
                details: "Reddit metadata could not be represented",
            })?;

        let item = MediaItem {
            id: item_id.clone(),
            source_id: format!("reddit:source:{}", source.external_id),
            collection_id: None,
            external_id: Some(post.id.clone()),
            canonical_url: source.canonical_url.to_string(),
            title: post.title,
            creator_name: post.author,
            creator_id: post.author_fullname,
            thumbnail_url: normalize_thumbnail(post.thumbnail.as_deref()),
            duration_ms: video.duration.map(|seconds| seconds.saturating_mul(1000)),
            published_at: post.created_utc.map(format_unix_timestamp),
            position: Some(0),
            metadata_json: Some(metadata_json),
            first_seen_at: now_utc(),
            last_seen_at: now_utc(),
        };

        Ok(AnalysisResult {
            source: source.into(),
            items: vec![item],
            formats,
        })
    }
}

#[async_trait]
impl PlatformAdapter for RedditAdapter {
    fn id(&self) -> &'static str {
        PLATFORM_ID
    }

    fn detect(&self, url: &Url) -> bool {
        normalize_reddit_url(url).is_ok()
    }

    async fn normalize(&self, url: &Url) -> Result<NormalizedSource, AdapterError> {
        normalize_reddit_url(url)
    }

    async fn analyze(&self, source: &NormalizedSource) -> Result<AnalysisResult, AdapterError> {
        if source.platform_id != PLATFORM_ID {
            return Err(AdapterError::PlatformMismatch);
        }
        let post = self.fetch_post(source).await?;
        Self::build_analysis(source, post)
    }

    async fn resolve_formats(&self, item: &MediaItem) -> Result<Vec<MediaFormat>, AdapterError> {
        let metadata = item
            .metadata_json
            .clone()
            .ok_or(AdapterError::PublicMediaUnavailable)?;
        let metadata: RedditItemMetadata =
            serde_json::from_value(metadata).map_err(|_| AdapterError::MalformedResponse {
                details: "media item metadata is not a Reddit adapter record",
            })?;
        metadata.media.formats(item.external_id.as_deref().ok_or(
            AdapterError::MalformedResponse {
                details: "media item is missing its Reddit external id",
            },
        )?)
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            single_item: true,
            collections: false,
            audio_only: false,
            thumbnails: true,
            metadata: true,
            resume: false,
            scheduling: false,
        }
    }
}

fn normalize_reddit_url(url: &Url) -> Result<NormalizedSource, AdapterError> {
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
        "reddit.com" | "www.reddit.com" | "old.reddit.com" | "new.reddit.com"
    ) {
        return Err(AdapterError::UnsupportedUrl);
    }

    let segments: Vec<&str> = url
        .path_segments()
        .ok_or(AdapterError::InvalidUrl)?
        .filter(|segment| !segment.is_empty())
        .collect();
    let comments_index = segments
        .iter()
        .position(|segment| *segment == "comments")
        .ok_or(AdapterError::UnsupportedUrl)?;
    let external_id = segments
        .get(comments_index + 1)
        .filter(|id| {
            !id.is_empty()
                && id
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
        .ok_or(AdapterError::UnsupportedUrl)?
        .to_ascii_lowercase();
    let canonical_url = Url::parse(&format!("https://www.reddit.com/comments/{external_id}/"))
        .map_err(|_| AdapterError::InvalidUrl)?;

    Ok(NormalizedSource {
        platform_id: PLATFORM_ID.to_owned(),
        original_url: url.clone(),
        canonical_url,
        external_id,
    })
}

#[derive(Debug, Deserialize)]
struct RedditListing<T> {
    data: RedditListingData<T>,
}

#[derive(Debug, Deserialize)]
struct RedditListingData<T> {
    children: Vec<RedditChild<T>>,
}

#[derive(Debug, Deserialize)]
struct RedditChild<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
struct RedditPost {
    id: String,
    title: String,
    author: Option<String>,
    author_fullname: Option<String>,
    thumbnail: Option<String>,
    created_utc: Option<f64>,
    over_18: Option<bool>,
    removed_by_category: Option<String>,
    subreddit: Option<String>,
    post_hint: Option<String>,
    media: Option<RedditMedia>,
    secure_media: Option<RedditMedia>,
}

#[derive(Debug, Deserialize)]
struct RedditMedia {
    reddit_video: Option<RedditVideo>,
}

#[derive(Debug, Clone, Deserialize)]
struct RedditVideo {
    fallback_url: Option<String>,
    dash_url: Option<String>,
    hls_url: Option<String>,
    duration: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    bitrate_kbps: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RedditMediaDescriptor {
    fallback_url: Option<String>,
    dash_url: Option<String>,
    hls_url: Option<String>,
    duration: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    bitrate_kbps: Option<i64>,
}

impl From<&RedditVideo> for RedditMediaDescriptor {
    fn from(video: &RedditVideo) -> Self {
        Self {
            fallback_url: video.fallback_url.clone(),
            dash_url: video.dash_url.clone(),
            hls_url: video.hls_url.clone(),
            duration: video.duration,
            width: video.width,
            height: video.height,
            bitrate_kbps: video.bitrate_kbps,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RedditItemMetadata {
    subreddit: Option<String>,
    post_hint: Option<String>,
    media: RedditMediaDescriptor,
}

impl RedditMediaDescriptor {
    fn formats(&self, post_id: &str) -> Result<Vec<MediaFormat>, AdapterError> {
        let mut formats = Vec::new();
        for (kind, candidate) in [
            ("fallback", self.fallback_url.as_deref()),
            ("dash", self.dash_url.as_deref()),
            ("hls", self.hls_url.as_deref()),
        ] {
            let Some(candidate) = candidate else { continue };
            let url = validate_public_media_url(candidate)?;
            formats.push(MediaFormat {
                id: format!("reddit:format:{post_id}:{kind}"),
                media_item_id: format!("reddit:item:{post_id}"),
                external_format_id: Some(kind.to_owned()),
                container: Some(match kind {
                    "fallback" => "mp4".to_owned(),
                    "dash" => "mpd".to_owned(),
                    _ => "m3u8".to_owned(),
                }),
                video_codec: None,
                audio_codec: None,
                width: self.width,
                height: self.height,
                fps: None,
                bitrate: self.bitrate_kbps.map(|value| value.saturating_mul(1000)),
                sample_rate: None,
                channels: None,
                file_size_bytes: None,
                is_video: true,
                is_audio: false,
                is_progressive: kind == "fallback",
                metadata_json: Some(serde_json::json!({
                    "public_url": url.as_str(),
                    "kind": kind,
                    "duration_seconds": self.duration,
                })),
                created_at: now_utc(),
            });
        }
        if formats.is_empty() {
            return Err(AdapterError::PublicMediaUnavailable);
        }
        Ok(formats)
    }
}

fn validate_public_media_url(value: &str) -> Result<Url, AdapterError> {
    let url = Url::parse(value).map_err(|_| AdapterError::PublicMediaUnavailable)?;
    if url.scheme() != "https" {
        return Err(AdapterError::PublicMediaUnavailable);
    }
    let host = url
        .host_str()
        .ok_or(AdapterError::PublicMediaUnavailable)?
        .to_ascii_lowercase();
    if host != "v.redd.it" && !host.ends_with(".redd.it") {
        return Err(AdapterError::PublicMediaUnavailable);
    }
    Ok(url)
}

fn normalize_thumbnail(value: Option<&str>) -> Option<String> {
    let url = value.and_then(|value| Url::parse(value).ok())?;
    let host = url.host_str()?.to_ascii_lowercase();
    let is_reddit_host =
        host.ends_with(".redd.it") || host == "reddit.com" || host.ends_with(".reddit.com");
    (url.scheme() == "https" && is_reddit_host).then(|| url.to_string())
}

fn format_unix_timestamp(seconds: f64) -> String {
    if !seconds.is_finite() {
        return "1970-01-01T00:00:00Z".to_owned();
    }
    let seconds = seconds.trunc() as i64;
    let Ok(timestamp) = time::OffsetDateTime::from_unix_timestamp(seconds) else {
        return "1970-01-01T00:00:00Z".to_owned();
    };
    timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

fn now_utc() -> String {
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{normalize_reddit_url, validate_public_media_url};
    use crate::adapters::contract::PlatformAdapter;
    use url::Url;

    #[test]
    fn normalizes_supported_reddit_post_urls() {
        let input =
            Url::parse("https://www.reddit.com/r/videos/comments/AbC123/example/?utm_source=test")
                .unwrap();
        let normalized = normalize_reddit_url(&input).unwrap();
        assert_eq!(normalized.external_id, "abc123");
        assert_eq!(
            normalized.canonical_url.as_str(),
            "https://www.reddit.com/comments/abc123/"
        );
    }

    #[test]
    fn rejects_non_reddit_and_non_https_urls() {
        assert!(normalize_reddit_url(
            &Url::parse("http://www.reddit.com/r/x/comments/abc/title").unwrap()
        )
        .is_err());
        assert!(normalize_reddit_url(
            &Url::parse("https://example.com/r/x/comments/abc/title").unwrap()
        )
        .is_err());
        assert!(
            normalize_reddit_url(&Url::parse("https://www.reddit.com/r/x/submitted").unwrap())
                .is_err()
        );
    }

    #[test]
    fn accepts_only_explicit_reddit_media_hosts() {
        assert!(validate_public_media_url("https://v.redd.it/abc/DASH_720.mp4").is_ok());
        assert!(validate_public_media_url("https://cdn.example.com/video.mp4").is_err());
        assert!(validate_public_media_url("http://v.redd.it/abc/video.mp4").is_err());
    }

    #[test]
    fn fixture_produces_item_and_public_formats() {
        let listings: Vec<super::RedditListing<super::RedditPost>> =
            serde_json::from_str(include_str!("fixtures/reddit_video.json")).unwrap();
        let post = listings
            .into_iter()
            .flat_map(|listing| listing.data.children)
            .next()
            .unwrap()
            .data;
        assert_eq!(post.id, "abc123");
        assert_eq!(post.subreddit.as_deref(), Some("videos"));
        assert_eq!(post.post_hint.as_deref(), Some("hosted:video"));
        assert_eq!(post.over_18, Some(false));
        assert!(post.removed_by_category.is_none());
        assert!(post.secure_media.as_ref().unwrap().reddit_video.is_some());
        let source = normalize_reddit_url(
            &Url::parse("https://www.reddit.com/r/videos/comments/abc123/a_public_test_video/")
                .unwrap(),
        )
        .unwrap();
        let analysis = super::RedditAdapter::build_analysis(&source, post).unwrap();
        assert_eq!(analysis.items.len(), 1);
        assert_eq!(analysis.formats.len(), 3);
        assert!(analysis
            .formats
            .iter()
            .all(|format| format.media_item_id == "reddit:item:abc123"));
        assert_eq!(
            analysis.items[0].thumbnail_url.as_deref(),
            Some("https://preview.redd.it/abc123/thumb.jpg?width=640&crop=smart&auto=webp")
        );
    }

    #[test]
    fn malformed_or_untrusted_media_is_rejected() {
        let video = super::RedditVideo {
            fallback_url: Some("https://cdn.example.com/video.mp4".to_owned()),
            dash_url: None,
            hls_url: None,
            duration: Some(1),
            width: Some(1),
            height: Some(1),
            bitrate_kbps: None,
        };
        let descriptor = super::RedditMediaDescriptor::from(&video);
        assert!(descriptor.formats("abc123").is_err());
    }

    #[test]
    fn capabilities_are_narrow_and_fail_closed() {
        let adapter = super::RedditAdapter::new().unwrap();
        let capabilities = adapter.capabilities();
        assert!(capabilities.single_item);
        assert!(capabilities.metadata);
        assert!(capabilities.thumbnails);
        assert!(!capabilities.collections);
        assert!(!capabilities.scheduling);
        assert!(!capabilities.resume);
    }
}
