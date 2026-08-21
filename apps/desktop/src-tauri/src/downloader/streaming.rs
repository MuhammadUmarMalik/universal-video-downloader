use super::{
    finalize_part, DownloadPlan, FinalizationError, FinalizationResult, ProgressSampler,
    StreamProgress,
};
use crate::downloader::storage::{ensure_available_space, harden_file_permissions, StorageError};
use reqwest::{Client, Response, StatusCode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use url::Url;

const USER_AGENT: &str = "universal-media-downloader/0.1 public-media-stream";
pub const DEFAULT_MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StreamingError {
    #[error("the download target is not an approved HTTPS public-media URL")]
    InvalidTarget,
    #[error("the media response returned an unexpected HTTP status")]
    UnexpectedStatus { status: u16 },
    #[error("the media response exceeds the configured size limit")]
    ResponseTooLarge { limit: u64, observed: u64 },
    #[error("the media response length does not match the expected format size")]
    ContentLengthMismatch,
    #[error("the media response could not be requested")]
    RequestFailed { retryable: bool },
    #[error("the media response body could not be read")]
    BodyReadFailed { retryable: bool },
    #[error("the download was cancelled after a partial write")]
    Cancelled { bytes_written: u64 },
    #[error("the temporary media file could not be written")]
    FileWriteFailed,
    #[error("the destination directory is unavailable")]
    DestinationUnavailable,
    #[error("the destination directory is not writable")]
    PermissionDenied,
    #[error("the destination does not have enough free space")]
    DiskFull,
    #[error("the streamed part could not be finalized")]
    Finalization(#[from] FinalizationError),
    #[error("the server does not support a safe resume response")]
    ResumeNotSupported,
    #[error("the server returned an invalid byte range")]
    RangeMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamResult {
    pub bytes_written: u64,
    pub content_length: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumableStreamResult {
    pub stream: StreamResult,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub resumed_from: u64,
}

#[derive(Clone)]
pub struct StreamingEngine {
    client: Client,
    max_response_bytes: u64,
}

impl StreamingEngine {
    pub fn new() -> Result<Self, StreamingError> {
        Self::with_limits(DEFAULT_MAX_RESPONSE_BYTES, DEFAULT_TIMEOUT)
    }

    pub fn with_limits(max_response_bytes: u64, timeout: Duration) -> Result<Self, StreamingError> {
        if max_response_bytes == 0 || timeout.is_zero() {
            return Err(StreamingError::InvalidTarget);
        }
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout)
            .build()
            .map_err(|_| StreamingError::RequestFailed { retryable: false })?;
        Ok(Self {
            client,
            max_response_bytes,
        })
    }

    pub const fn max_response_bytes(&self) -> u64 {
        self.max_response_bytes
    }

    pub async fn stream_plan_and_finalize(
        &self,
        plan: &DownloadPlan,
        cancelled: &AtomicBool,
    ) -> Result<FinalizationResult, StreamingError> {
        let stream_result = self.stream_plan(plan, cancelled).await?;
        Ok(finalize_part(plan, &stream_result)?)
    }

    pub async fn stream_plan(
        &self,
        plan: &DownloadPlan,
        cancelled: &AtomicBool,
    ) -> Result<StreamResult, StreamingError> {
        self.stream_plan_with_progress(plan, cancelled, |_| {})
            .await
    }

    pub async fn stream_plan_with_progress<F>(
        &self,
        plan: &DownloadPlan,
        cancelled: &AtomicBool,
        on_progress: F,
    ) -> Result<StreamResult, StreamingError>
    where
        F: FnMut(StreamProgress),
    {
        validate_target(&plan.platform_id, &plan.source_url)?;
        if plan
            .total_bytes
            .is_some_and(|total| total < 0 || total as u64 > self.max_response_bytes)
        {
            return Err(StreamingError::ResponseTooLarge {
                limit: self.max_response_bytes,
                observed: plan.total_bytes.unwrap_or_default().max(0) as u64,
            });
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(StreamingError::Cancelled { bytes_written: 0 });
        }
        ensure_available_space(
            &plan.destination.root,
            plan.total_bytes.and_then(|value| u64::try_from(value).ok()),
            self.max_response_bytes,
        )
        .map_err(StreamingError::from)?;

        let response = self
            .client
            .get(plan.source_url.clone())
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(
                reqwest::header::ACCEPT,
                "video/mp4,application/octet-stream",
            )
            .send()
            .await
            .map_err(|error| StreamingError::RequestFailed {
                retryable: error.is_timeout() || error.is_connect(),
            })?;
        self.stream_response_to_part(plan, response, cancelled, on_progress)
            .await
    }

    pub async fn stream_plan_resumable<F>(
        &self,
        plan: &DownloadPlan,
        offset: u64,
        etag: Option<&str>,
        last_modified: Option<&str>,
        cancelled: &AtomicBool,
        on_progress: F,
    ) -> Result<ResumableStreamResult, StreamingError>
    where
        F: FnMut(StreamProgress),
    {
        validate_resumable_target(&plan.platform_id, &plan.source_url)?;
        if plan.total_bytes.is_some_and(|total| total < 0) {
            return Err(StreamingError::ContentLengthMismatch);
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(StreamingError::Cancelled {
                bytes_written: offset,
            });
        }
        let expected_remaining = plan
            .total_bytes
            .and_then(|value| u64::try_from(value).ok())
            .map(|total| total.saturating_sub(offset));
        ensure_available_space(
            &plan.destination.root,
            expected_remaining,
            self.max_response_bytes,
        )
        .map_err(StreamingError::from)?;
        let existing_length = if offset > 0 {
            let metadata = tokio::fs::metadata(&plan.destination.temporary)
                .await
                .map_err(|_| StreamingError::ResumeNotSupported)?;
            if !metadata.is_file() || metadata.len() != offset {
                return Err(StreamingError::ResumeNotSupported);
            }
            Some(metadata.len())
        } else {
            None
        };

        let mut request = self
            .client
            .get(plan.source_url.clone())
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(
                reqwest::header::ACCEPT,
                "video/mp4,application/octet-stream",
            );
        if offset > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
            if let Some(value) = etag.or(last_modified) {
                request = request.header(reqwest::header::IF_RANGE, value);
            }
        }
        let response = request
            .send()
            .await
            .map_err(|error| StreamingError::RequestFailed {
                retryable: error.is_timeout() || error.is_connect(),
            })?;
        let response_etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let response_last_modified = response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        if offset == 0 {
            if response.status() != StatusCode::OK {
                return Err(StreamingError::UnexpectedStatus {
                    status: response.status().as_u16(),
                });
            }
            let stream = self
                .stream_response_to_part(plan, response, cancelled, on_progress)
                .await?;
            return Ok(ResumableStreamResult {
                stream,
                etag: response_etag,
                last_modified: response_last_modified,
                resumed_from: 0,
            });
        }

        if response.status() != StatusCode::PARTIAL_CONTENT {
            return Err(StreamingError::ResumeNotSupported);
        }
        let content_range = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .ok_or(StreamingError::ResumeNotSupported)?;
        let (range_start, range_end, range_total) = parse_content_range(content_range)?;
        if range_start != offset || range_end < range_start {
            return Err(StreamingError::RangeMismatch);
        }
        if plan
            .total_bytes
            .and_then(|value| u64::try_from(value).ok())
            .is_some_and(|expected| range_total != Some(expected))
        {
            return Err(StreamingError::ContentLengthMismatch);
        }
        let body_length = response
            .content_length()
            .ok_or(StreamingError::ResumeNotSupported)?;
        if body_length != range_end - range_start + 1
            || body_length > self.max_response_bytes.saturating_sub(offset)
        {
            return Err(StreamingError::ContentLengthMismatch);
        }
        let mut file = OpenOptions::new()
            .append(true)
            .open(&plan.destination.temporary)
            .await
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::PermissionDenied => StreamingError::PermissionDenied,
                _ => StreamingError::ResumeNotSupported,
            })?;
        harden_file_permissions(&plan.destination.temporary).map_err(map_file_error)?;
        let mut response = response;
        let mut bytes_written = offset;
        let progress_total = plan
            .total_bytes
            .and_then(|value| u64::try_from(value).ok())
            .or(range_total);
        let mut sampler = ProgressSampler::new(Duration::from_millis(250));
        let mut on_progress = on_progress;
        while let Some(chunk) =
            response
                .chunk()
                .await
                .map_err(|error| StreamingError::BodyReadFailed {
                    retryable: error.is_timeout() || error.is_connect(),
                })?
        {
            if cancelled.load(Ordering::Acquire) {
                return Err(StreamingError::Cancelled { bytes_written });
            }
            let next_size = bytes_written.saturating_add(chunk.len() as u64);
            if next_size > self.max_response_bytes {
                return Err(StreamingError::ResponseTooLarge {
                    limit: self.max_response_bytes,
                    observed: next_size,
                });
            }
            ensure_available_space(
                &plan.destination.root,
                plan.total_bytes
                    .and_then(|value| u64::try_from(value).ok())
                    .map(|total| total.saturating_sub(next_size)),
                self.max_response_bytes,
            )
            .map_err(StreamingError::from)?;
            file.write_all(&chunk).await.map_err(map_file_error)?;
            bytes_written = next_size;
            if let Some(sample) = sampler.observe(Instant::now(), bytes_written, progress_total) {
                on_progress(sample);
            }
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(StreamingError::Cancelled { bytes_written });
        }
        if bytes_written != range_end + 1 {
            return Err(StreamingError::ContentLengthMismatch);
        }
        if let Some(sample) = sampler.finish(Instant::now(), bytes_written, progress_total) {
            on_progress(sample);
        }
        file.flush().await.map_err(map_file_error)?;
        file.sync_all().await.map_err(map_file_error)?;
        Ok(ResumableStreamResult {
            stream: StreamResult {
                bytes_written,
                content_length: range_total.or(Some(bytes_written)),
            },
            etag: response_etag,
            last_modified: response_last_modified,
            resumed_from: existing_length.unwrap_or_default(),
        })
    }

    async fn stream_response_to_part<F>(
        &self,
        plan: &DownloadPlan,
        response: Response,
        cancelled: &AtomicBool,
        mut on_progress: F,
    ) -> Result<StreamResult, StreamingError>
    where
        F: FnMut(StreamProgress),
    {
        if response.status() != StatusCode::OK {
            return Err(StreamingError::UnexpectedStatus {
                status: response.status().as_u16(),
            });
        }
        let content_length = response.content_length();
        if content_length.is_some_and(|length| length > self.max_response_bytes) {
            return Err(StreamingError::ResponseTooLarge {
                limit: self.max_response_bytes,
                observed: content_length.unwrap_or_default(),
            });
        }
        if let (Some(expected), Some(actual)) = (plan.total_bytes, content_length) {
            if expected < 0 || expected as u64 != actual {
                return Err(StreamingError::ContentLengthMismatch);
            }
        }

        if cancelled.load(Ordering::Acquire) {
            return Err(StreamingError::Cancelled { bytes_written: 0 });
        }
        ensure_available_space(
            &plan.destination.root,
            plan.total_bytes.and_then(|value| u64::try_from(value).ok()),
            self.max_response_bytes,
        )
        .map_err(StreamingError::from)?;
        let mut file = File::create(&plan.destination.temporary)
            .await
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    StreamingError::DestinationUnavailable
                } else {
                    map_file_error(error)
                }
            })?;
        harden_file_permissions(&plan.destination.temporary).map_err(map_file_error)?;
        let mut response = response;
        let mut bytes_written = 0_u64;
        let progress_total = plan
            .total_bytes
            .and_then(|value| u64::try_from(value).ok())
            .or(content_length);
        let mut sampler = ProgressSampler::new(Duration::from_millis(250));
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            if content_length.is_some_and(|expected| bytes_written < expected) {
                StreamingError::ContentLengthMismatch
            } else {
                StreamingError::BodyReadFailed {
                    retryable: error.is_timeout() || error.is_connect(),
                }
            }
        })? {
            if cancelled.load(Ordering::Acquire) {
                return Err(StreamingError::Cancelled { bytes_written });
            }
            let chunk_len = chunk.len() as u64;
            let next_size = bytes_written.saturating_add(chunk_len);
            if next_size > self.max_response_bytes {
                return Err(StreamingError::ResponseTooLarge {
                    limit: self.max_response_bytes,
                    observed: next_size,
                });
            }
            if plan
                .total_bytes
                .is_some_and(|expected| expected < 0 || next_size > expected as u64)
            {
                return Err(StreamingError::ContentLengthMismatch);
            }
            ensure_available_space(
                &plan.destination.root,
                plan.total_bytes
                    .and_then(|value| u64::try_from(value).ok())
                    .map(|total| total.saturating_sub(next_size)),
                self.max_response_bytes,
            )
            .map_err(StreamingError::from)?;
            file.write_all(&chunk).await.map_err(map_file_error)?;
            bytes_written = next_size;
            if let Some(sample) = sampler.observe(Instant::now(), bytes_written, progress_total) {
                on_progress(sample);
            }
        }

        if cancelled.load(Ordering::Acquire) {
            return Err(StreamingError::Cancelled { bytes_written });
        }
        if let Some(sample) = sampler.finish(Instant::now(), bytes_written, progress_total) {
            on_progress(sample);
        }
        if let Some(expected) = content_length {
            if bytes_written != expected {
                return Err(StreamingError::ContentLengthMismatch);
            }
        }
        if let Some(expected) = plan.total_bytes {
            if expected < 0 || bytes_written != expected as u64 {
                return Err(StreamingError::ContentLengthMismatch);
            }
        }
        file.flush().await.map_err(map_file_error)?;
        file.sync_all().await.map_err(map_file_error)?;
        Ok(StreamResult {
            bytes_written,
            content_length,
        })
    }
}

impl From<StorageError> for StreamingError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::DestinationUnavailable => StreamingError::DestinationUnavailable,
            StorageError::PermissionDenied => StreamingError::PermissionDenied,
            StorageError::DiskFull => StreamingError::DiskFull,
        }
    }
}

fn map_file_error(error: std::io::Error) -> StreamingError {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => StreamingError::PermissionDenied,
        _ if matches!(error.raw_os_error(), Some(28 | 39 | 112)) => StreamingError::DiskFull,
        _ => StreamingError::FileWriteFailed,
    }
}

fn parse_content_range(value: &str) -> Result<(u64, u64, Option<u64>), StreamingError> {
    let (unit, range) = value.split_once(' ').ok_or(StreamingError::RangeMismatch)?;
    if unit != "bytes" {
        return Err(StreamingError::RangeMismatch);
    }
    let (bounds, total) = range.split_once('/').ok_or(StreamingError::RangeMismatch)?;
    let (start, end) = bounds
        .split_once('-')
        .ok_or(StreamingError::RangeMismatch)?;
    let start = start
        .parse::<u64>()
        .map_err(|_| StreamingError::RangeMismatch)?;
    let end = end
        .parse::<u64>()
        .map_err(|_| StreamingError::RangeMismatch)?;
    let total = if total == "*" {
        None
    } else {
        Some(
            total
                .parse::<u64>()
                .map_err(|_| StreamingError::RangeMismatch)?,
        )
    };
    Ok((start, end, total))
}

fn validate_resumable_target(platform_id: &str, url: &Url) -> Result<(), StreamingError> {
    #[cfg(test)]
    if url.scheme() == "http"
        && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
    {
        return Ok(());
    }
    validate_target(platform_id, url)
}

fn validate_target(platform_id: &str, url: &Url) -> Result<(), StreamingError> {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(StreamingError::InvalidTarget);
    }
    let Some(host) = url.host_str() else {
        return Err(StreamingError::InvalidTarget);
    };
    let host = host.to_ascii_lowercase();
    match platform_id {
        "reddit" if host == "v.redd.it" || host.ends_with(".redd.it") => Ok(()),
        "direct" if is_direct_media_url(url) => Ok(()),
        _ => Err(StreamingError::InvalidTarget),
    }
}

fn is_direct_media_url(url: &Url) -> bool {
    const EXTENSIONS: &[&str] = &[
        "mp4", "webm", "mov", "m4v", "mkv", "mp3", "m4a", "wav", "ogg", "flac", "aac", "opus",
    ];
    let Some(segment) = url
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
    else {
        return false;
    };
    let Some((_, extension)) = segment.rsplit_once('.') else {
        return false;
    };
    EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::{StreamingEngine, StreamingError};
    use crate::downloader::{DestinationPaths, DownloadPlan};
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;
    use url::Url;

    fn plan(root: &std::path::Path, total_bytes: Option<i64>) -> DownloadPlan {
        DownloadPlan {
            media_item_id: "item-1".to_owned(),
            format_id: "format-1".to_owned(),
            platform_id: "reddit".to_owned(),
            source_url: Url::parse("https://v.redd.it/item-1/video.mp4").unwrap(),
            destination: DestinationPaths {
                root: root.to_owned(),
                destination: root.join("video.mp4"),
                temporary: root.join("video.mp4.part"),
                filename: "video.mp4".to_owned(),
            },
            total_bytes,
        }
    }

    async fn local_response(
        body: Vec<u8>,
        declared_length: Option<usize>,
        status: &'static str,
    ) -> reqwest::Response {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = std::io::Read::read(&mut stream, &mut request);
            let length_header = declared_length
                .map(|length| format!("Content-Length: {length}\r\n"))
                .unwrap_or_default();
            let headers = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/octet-stream\r\n{length_header}Connection: close\r\n\r\n"
            );
            stream.write_all(headers.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
            stream.flush().unwrap();
            thread::sleep(Duration::from_millis(10));
        });
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
            .get(format!("http://{address}/media"))
            .send()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn streams_bounded_response_to_a_part_file_and_syncs_it() {
        let directory = tempdir().unwrap();
        let engine = StreamingEngine::with_limits(32, Duration::from_secs(5)).unwrap();
        let response = local_response(b"hello world".to_vec(), Some(11), "200 OK").await;
        let result = engine
            .stream_response_to_part(
                &plan(directory.path(), Some(11)),
                response,
                &AtomicBool::new(false),
                |_| {},
            )
            .await
            .unwrap();
        assert_eq!(result.bytes_written, 11);
        assert_eq!(
            std::fs::read(directory.path().join("video.mp4.part")).unwrap(),
            b"hello world"
        );
    }

    #[tokio::test]
    async fn emits_progress_samples_after_part_writes() {
        let directory = tempdir().unwrap();
        let engine = StreamingEngine::with_limits(32, Duration::from_secs(5)).unwrap();
        let response = local_response(b"hello world".to_vec(), Some(11), "200 OK").await;
        let samples = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let samples_for_callback = std::sync::Arc::clone(&samples);
        engine
            .stream_response_to_part(
                &plan(directory.path(), Some(11)),
                response,
                &AtomicBool::new(false),
                move |sample| {
                    samples_for_callback.lock().unwrap().push(sample);
                },
            )
            .await
            .unwrap();
        let samples = samples.lock().unwrap();
        assert!(!samples.is_empty());
        assert_eq!(samples.last().unwrap().downloaded_bytes, 11);
        assert_eq!(samples.last().unwrap().total_bytes, Some(11));
    }

    #[tokio::test]
    async fn rejects_non_success_and_redirect_statuses_without_creating_a_part_file() {
        let directory = tempdir().unwrap();
        let engine = StreamingEngine::with_limits(32, Duration::from_secs(5)).unwrap();
        let response = local_response(Vec::new(), Some(0), "302 Found").await;
        let error = engine
            .stream_response_to_part(
                &plan(directory.path(), Some(0)),
                response,
                &AtomicBool::new(false),
                |_| {},
            )
            .await
            .unwrap_err();
        assert_eq!(error, StreamingError::UnexpectedStatus { status: 302 });
        assert!(!directory.path().join("video.mp4.part").exists());

        let response = local_response(Vec::new(), Some(0), "206 Partial Content").await;
        let error = engine
            .stream_response_to_part(
                &plan(directory.path(), Some(0)),
                response,
                &AtomicBool::new(false),
                |_| {},
            )
            .await
            .unwrap_err();
        assert_eq!(error, StreamingError::UnexpectedStatus { status: 206 });
        assert!(!directory.path().join("video.mp4.part").exists());
    }

    #[tokio::test]
    async fn rejects_unknown_length_bodies_when_the_stream_exceeds_the_limit() {
        let directory = tempdir().unwrap();
        let engine = StreamingEngine::with_limits(4, Duration::from_secs(5)).unwrap();
        let response = local_response(b"12345".to_vec(), None, "200 OK").await;
        let error = engine
            .stream_response_to_part(
                &plan(directory.path(), None),
                response,
                &AtomicBool::new(false),
                |_| {},
            )
            .await
            .unwrap_err();
        assert_eq!(
            error,
            StreamingError::ResponseTooLarge {
                limit: 4,
                observed: 5
            }
        );
        assert!(directory.path().join("video.mp4.part").exists());
    }

    #[tokio::test]
    async fn rejects_known_lengths_over_the_limit_before_writing() {
        let directory = tempdir().unwrap();
        let engine = StreamingEngine::with_limits(4, Duration::from_secs(5)).unwrap();
        let response = local_response(b"12345".to_vec(), Some(5), "200 OK").await;
        let error = engine
            .stream_response_to_part(
                &plan(directory.path(), None),
                response,
                &AtomicBool::new(false),
                |_| {},
            )
            .await
            .unwrap_err();
        assert_eq!(
            error,
            StreamingError::ResponseTooLarge {
                limit: 4,
                observed: 5
            }
        );
        assert!(!directory.path().join("video.mp4.part").exists());
    }

    #[tokio::test]
    async fn preserves_partial_file_when_body_is_truncated() {
        let directory = tempdir().unwrap();
        let engine = StreamingEngine::with_limits(32, Duration::from_secs(5)).unwrap();
        let response = local_response(b"short".to_vec(), Some(10), "200 OK").await;
        let error = engine
            .stream_response_to_part(
                &plan(directory.path(), Some(10)),
                response,
                &AtomicBool::new(false),
                |_| {},
            )
            .await
            .unwrap_err();
        assert_eq!(error, StreamingError::ContentLengthMismatch);
        assert_eq!(
            std::fs::read(directory.path().join("video.mp4.part")).unwrap(),
            b"short"
        );
    }

    #[tokio::test]
    async fn cancellation_stops_before_the_first_write_and_preserves_part_file() {
        let directory = tempdir().unwrap();
        let engine = StreamingEngine::with_limits(32, Duration::from_secs(5)).unwrap();
        let response = local_response(b"cancel me".to_vec(), Some(9), "200 OK").await;
        let cancelled = Arc::new(AtomicBool::new(true));
        let error = engine
            .stream_response_to_part(
                &plan(directory.path(), Some(9)),
                response,
                &cancelled,
                |_| {},
            )
            .await
            .unwrap_err();
        assert_eq!(error, StreamingError::Cancelled { bytes_written: 0 });
        assert!(!directory.path().join("video.mp4.part").exists());
    }

    #[tokio::test]
    async fn rejects_non_reddit_targets_before_requesting() {
        let directory = tempdir().unwrap();
        let engine = StreamingEngine::with_limits(32, Duration::from_secs(5)).unwrap();
        let mut invalid = plan(directory.path(), None);
        invalid.source_url = Url::parse("http://127.0.0.1:1/media").unwrap();
        let error = engine
            .stream_plan(&invalid, &AtomicBool::new(false))
            .await
            .unwrap_err();
        assert_eq!(error, StreamingError::InvalidTarget);
    }

    fn local_server(
        status: &'static str,
        headers: String,
        body: Vec<u8>,
    ) -> (String, Arc<std::sync::Mutex<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let request_log = Arc::new(std::sync::Mutex::new(String::new()));
        let request_log_for_thread = Arc::clone(&request_log);
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let bytes_read = std::io::Read::read(&mut stream, &mut request).unwrap();
            *request_log_for_thread.lock().unwrap() =
                String::from_utf8_lossy(&request[..bytes_read]).to_ascii_lowercase();
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
            stream.flush().unwrap();
        });
        (format!("http://{address}/media"), request_log)
    }

    fn local_plan(root: &std::path::Path, total_bytes: Option<i64>, url: String) -> DownloadPlan {
        let mut local = plan(root, total_bytes);
        local.source_url = Url::parse(&url).unwrap();
        local
    }

    #[tokio::test]
    async fn resumable_offset_zero_streams_and_captures_validators() {
        let directory = tempdir().unwrap();
        let (url, request_log) = local_server(
            "200 OK",
            "ETag: \"tag-1\"\r\nLast-Modified: Wed, 21 Oct 2015 07:28:00 GMT\r\n".to_owned(),
            b"hello world".to_vec(),
        );
        let mut download_plan = local_plan(directory.path(), Some(11), url);
        download_plan.destination.temporary = directory.path().join("video.mp4.part");
        let result = StreamingEngine::with_limits(32, Duration::from_secs(5))
            .unwrap()
            .stream_plan_resumable(
                &download_plan,
                0,
                None,
                None,
                &AtomicBool::new(false),
                |_| {},
            )
            .await
            .unwrap();
        assert_eq!(result.stream.bytes_written, 11);
        assert_eq!(result.etag.as_deref(), Some("\"tag-1\""));
        assert_eq!(
            result.last_modified.as_deref(),
            Some("Wed, 21 Oct 2015 07:28:00 GMT")
        );
        assert_eq!(result.resumed_from, 0);
        assert!(!request_log.lock().unwrap().contains("range:"));
    }

    #[tokio::test]
    async fn resumable_valid_206_appends_and_validates_offset_and_if_range() {
        let directory = tempdir().unwrap();
        let temporary = directory.path().join("video.mp4.part");
        std::fs::write(&temporary, b"hello").unwrap();
        let (url, request_log) = local_server(
            "206 Partial Content",
            "Content-Range: bytes 5-10/11\r\nETag: \"tag-1\"\r\n".to_owned(),
            b" world".to_vec(),
        );
        let mut download_plan = local_plan(directory.path(), Some(11), url);
        download_plan.destination.temporary = temporary.clone();
        let result = StreamingEngine::with_limits(32, Duration::from_secs(5))
            .unwrap()
            .stream_plan_resumable(
                &download_plan,
                5,
                Some("\"tag-1\""),
                None,
                &AtomicBool::new(false),
                |_| {},
            )
            .await
            .unwrap();
        assert_eq!(result.resumed_from, 5);
        assert_eq!(result.stream.bytes_written, 11);
        assert_eq!(result.etag.as_deref(), Some("\"tag-1\""));
        assert_eq!(std::fs::read(temporary).unwrap(), b"hello world");
        let request = request_log.lock().unwrap();
        assert!(request.contains("range: bytes=5-"));
        assert!(request.contains("if-range: \"tag-1\""));
    }

    #[tokio::test]
    async fn resumable_rejects_non_206_and_preserves_existing_part() {
        let directory = tempdir().unwrap();
        let temporary = directory.path().join("video.mp4.part");
        std::fs::write(&temporary, b"hello").unwrap();
        let (url, _) = local_server("200 OK", String::new(), b"hello world".to_vec());
        let mut download_plan = local_plan(directory.path(), Some(11), url);
        download_plan.destination.temporary = temporary.clone();
        let error = StreamingEngine::with_limits(32, Duration::from_secs(5))
            .unwrap()
            .stream_plan_resumable(
                &download_plan,
                5,
                None,
                None,
                &AtomicBool::new(false),
                |_| {},
            )
            .await
            .unwrap_err();
        assert_eq!(error, StreamingError::ResumeNotSupported);
        assert_eq!(std::fs::read(temporary).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn resumable_rejects_mismatched_content_range_start() {
        let directory = tempdir().unwrap();
        let temporary = directory.path().join("video.mp4.part");
        std::fs::write(&temporary, b"hello").unwrap();
        let (url, _) = local_server(
            "206 Partial Content",
            "Content-Range: bytes 4-10/11\r\n".to_owned(),
            b" world".to_vec(),
        );
        let mut download_plan = local_plan(directory.path(), Some(11), url);
        download_plan.destination.temporary = temporary;
        let error = StreamingEngine::with_limits(32, Duration::from_secs(5))
            .unwrap()
            .stream_plan_resumable(
                &download_plan,
                5,
                None,
                None,
                &AtomicBool::new(false),
                |_| {},
            )
            .await
            .unwrap_err();
        assert_eq!(error, StreamingError::RangeMismatch);
    }

    #[test]
    fn content_range_parser_accepts_explicit_total_and_rejects_malformed_ranges() {
        assert_eq!(
            super::parse_content_range("bytes 5-10/11").unwrap(),
            (5, 10, Some(11))
        );
        assert_eq!(
            super::parse_content_range("bytes 5-10/*").unwrap(),
            (5, 10, None)
        );
        assert_eq!(
            super::parse_content_range("bytes five-10/11").unwrap_err(),
            StreamingError::RangeMismatch
        );
        assert_eq!(
            super::parse_content_range("bytes 5-10/not-a-number").unwrap_err(),
            StreamingError::RangeMismatch
        );
    }
}
