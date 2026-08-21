use crate::domain::entities::{MediaFormat, MediaItem};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedSource {
    pub platform_id: String,
    pub original_url: Url,
    pub canonical_url: Url,
    pub external_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformCapabilities {
    pub single_item: bool,
    pub collections: bool,
    pub audio_only: bool,
    pub thumbnails: bool,
    pub metadata: bool,
    pub resume: bool,
    pub scheduling: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalysisResult {
    pub source: NormalizedSourceDto,
    pub items: Vec<MediaItem>,
    pub formats: Vec<MediaFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedSourceDto {
    pub platform_id: String,
    pub canonical_url: String,
    pub external_id: String,
}

impl From<&NormalizedSource> for NormalizedSourceDto {
    fn from(source: &NormalizedSource) -> Self {
        Self {
            platform_id: source.platform_id.clone(),
            canonical_url: source.canonical_url.to_string(),
            external_id: source.external_id.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("the URL is not a supported Reddit post URL")]
    UnsupportedUrl,
    #[error("the URL is invalid")]
    InvalidUrl,
    #[error("the selected platform does not match the URL")]
    PlatformMismatch,
    #[error("the Reddit response was malformed")]
    MalformedResponse { details: &'static str },
    #[error("the Reddit resource is unavailable")]
    ResourceUnavailable { retryable: bool },
    #[error("the Reddit media resource is not explicitly public and downloadable")]
    PublicMediaUnavailable,
    #[error("the Reddit request failed")]
    Network { retryable: bool },
    #[error("this platform requires an authorized API integration")]
    AuthenticationRequired,
}

impl AdapterError {
    pub const fn retryable(&self) -> bool {
        match self {
            Self::ResourceUnavailable { retryable } | Self::Network { retryable } => *retryable,
            Self::UnsupportedUrl
            | Self::InvalidUrl
            | Self::PlatformMismatch
            | Self::MalformedResponse { .. }
            | Self::PublicMediaUnavailable
            | Self::AuthenticationRequired => false,
        }
    }
}

#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn detect(&self, url: &Url) -> bool;
    async fn normalize(&self, url: &Url) -> Result<NormalizedSource, AdapterError>;
    async fn analyze(&self, source: &NormalizedSource) -> Result<AnalysisResult, AdapterError>;
    async fn resolve_formats(&self, item: &MediaItem) -> Result<Vec<MediaFormat>, AdapterError>;
    fn capabilities(&self) -> PlatformCapabilities;
}

impl Display for NormalizedSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}:{}", self.platform_id, self.external_id)
    }
}
