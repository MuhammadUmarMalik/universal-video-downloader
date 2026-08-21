use crate::adapters::{
    AdapterError, AdapterRegistry, AnalysisResult, NormalizedSourceDto, PlatformCapabilities,
    RegistryError,
};
use crate::application::ports::{RepositoryError, RepositoryResult};
use crate::application::services::{AnalysisSnapshot, AppServices, SnapshotItem};
use crate::domain::entities::{MediaFormat, MediaItem, MediaSource, Platform, SourceType};
use crate::domain::errors::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use url::Url;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AnalyzeRequest {
    pub url: String,
    #[serde(default)]
    pub platform_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AnalyzeResponse {
    pub platform_id: String,
    pub capabilities: PlatformCapabilities,
    pub source: NormalizedSourceDto,
    pub items: Vec<MediaItem>,
    pub formats: Vec<MediaFormat>,
}

#[derive(Clone)]
pub struct AnalyzerService {
    app_services: AppServices,
    registry: AdapterRegistry,
}

impl AnalyzerService {
    pub fn with_defaults(app_services: AppServices) -> Result<Self, AdapterError> {
        Ok(Self {
            app_services,
            registry: AdapterRegistry::with_defaults()?,
        })
    }

    pub fn new(app_services: AppServices, registry: AdapterRegistry) -> Self {
        Self {
            app_services,
            registry,
        }
    }

    pub fn registry(&self) -> &AdapterRegistry {
        &self.registry
    }

    pub async fn analyze(&self, request: AnalyzeRequest) -> Result<AnalyzeResponse, AppError> {
        let input = request.url.trim();
        if input.is_empty() {
            return Err(AnalyzerError::InvalidUrl.into_app_error());
        }
        let url = Url::parse(input).map_err(|_| AnalyzerError::InvalidUrl.into_app_error())?;
        let adapter = self
            .registry
            .select(&url, request.platform_id.as_deref())
            .map_err(|error| AnalyzerError::Registry(error).into_app_error())?;
        let normalized = adapter
            .normalize(&url)
            .await
            .map_err(|error| AnalyzerError::Adapter(error).into_app_error())?;
        let result = adapter
            .analyze(&normalized)
            .await
            .map_err(|error| AnalyzerError::Adapter(error).into_app_error())?;

        self.persist_snapshot(&result, &normalized, adapter.id())
            .await
            .map_err(|error| AnalyzerError::Persistence(error).into_app_error())?;

        Ok(AnalyzeResponse {
            platform_id: adapter.id().to_owned(),
            capabilities: adapter.capabilities(),
            source: result.source,
            items: result.items,
            formats: result.formats,
        })
    }

    async fn persist_snapshot(
        &self,
        result: &AnalysisResult,
        normalized: &crate::adapters::NormalizedSource,
        adapter_id: &str,
    ) -> RepositoryResult<()> {
        let timestamp = now_utc();
        let platform = Platform {
            id: adapter_id.to_owned(),
            slug: adapter_id.to_owned(),
            name: platform_name(adapter_id),
            enabled: true,
            adapter_version: Some("phase3-core".to_owned()),
            created_at: timestamp.clone(),
            updated_at: timestamp.clone(),
        };
        let source_id = format!("{adapter_id}:source:{}", normalized.external_id);
        let source = MediaSource {
            id: source_id.clone(),
            platform_id: adapter_id.to_owned(),
            source_url: normalized.original_url.to_string(),
            normalized_url: normalized.canonical_url.to_string(),
            source_type: SourceType::Single,
            title: result.items.first().map(|item| item.title.clone()),
            creator_name: result
                .items
                .first()
                .and_then(|item| item.creator_name.clone()),
            creator_id: result
                .items
                .first()
                .and_then(|item| item.creator_id.clone()),
            thumbnail_url: result
                .items
                .first()
                .and_then(|item| item.thumbnail_url.clone()),
            item_count: Some(result.items.len() as i64),
            discovered_at: timestamp.clone(),
            last_analyzed_at: Some(timestamp.clone()),
            metadata_json: Some(serde_json::json!({
                "adapter_id": adapter_id,
                "external_id": normalized.external_id,
            })),
        };

        let mut snapshot_items = Vec::with_capacity(result.items.len());
        for item in &result.items {
            if item.source_id != source_id {
                return Err(RepositoryError::InvalidData {
                    details: "adapter item source ownership does not match normalized source"
                        .to_owned(),
                });
            }
            let formats = result
                .formats
                .iter()
                .filter(|format| format.media_item_id == item.id)
                .cloned()
                .collect::<Vec<_>>();
            snapshot_items.push(SnapshotItem {
                item: item.clone(),
                formats,
            });
        }
        if result.formats.iter().any(|format| {
            !result
                .items
                .iter()
                .any(|item| item.id == format.media_item_id)
        }) {
            return Err(RepositoryError::InvalidData {
                details: "adapter format ownership does not match an analyzed item".to_owned(),
            });
        }

        self.app_services
            .save_analysis_snapshot(&AnalysisSnapshot {
                platform,
                source,
                collections: Vec::new(),
                items: snapshot_items,
            })
            .await
    }
}

#[derive(Debug)]
enum AnalyzerError {
    InvalidUrl,
    Registry(RegistryError),
    Adapter(AdapterError),
    Persistence(RepositoryError),
}

impl AnalyzerError {
    fn into_app_error(self) -> AppError {
        match self {
            Self::InvalidUrl => AppError {
                code: ErrorCode::InvalidUrl,
                message: "Enter a valid public media URL.".to_owned(),
                retryable: false,
                user_action: Some("Check the URL and try again.".to_owned()),
                diagnostic: None,
            },
            Self::Registry(error) => match error {
                RegistryError::InvalidUrl => AppError {
                    code: ErrorCode::InvalidUrl,
                    message: "Enter a valid public media URL.".to_owned(),
                    retryable: false,
                    user_action: Some("Check the URL and try again.".to_owned()),
                    diagnostic: None,
                },
                RegistryError::PlatformMismatch => AppError {
                    code: ErrorCode::UnsupportedPlatform,
                    message: "The selected platform does not match this URL.".to_owned(),
                    retryable: false,
                    user_action: Some(
                        "Choose the matching platform or use automatic detection.".to_owned(),
                    ),
                    diagnostic: None,
                },
                RegistryError::UnsupportedPlatform | RegistryError::AdapterNotRegistered => {
                    AppError {
                        code: ErrorCode::UnsupportedPlatform,
                        message: "This URL is not supported by an available platform adapter."
                            .to_owned(),
                        retryable: false,
                        user_action: Some(
                            "Use a supported public URL or choose a registered platform."
                                .to_owned(),
                        ),
                        diagnostic: None,
                    }
                }
            },
            Self::Adapter(error) => adapter_error(error),
            Self::Persistence(error) => persistence_error(error),
        }
    }
}

fn adapter_error(error: AdapterError) -> AppError {
    let retryable = error.retryable();
    match error {
        AdapterError::UnsupportedUrl | AdapterError::InvalidUrl => AppError {
            code: ErrorCode::InvalidUrl,
            message: "Enter a supported public media URL.".to_owned(),
            retryable: false,
            user_action: Some("Check the URL and try again.".to_owned()),
            diagnostic: None,
        },
        AdapterError::PlatformMismatch => AppError {
            code: ErrorCode::UnsupportedPlatform,
            message: "The selected platform does not match this URL.".to_owned(),
            retryable: false,
            user_action: Some("Choose the matching platform or use automatic detection.".to_owned()),
            diagnostic: None,
        },
        AdapterError::AuthenticationRequired => AppError {
            code: ErrorCode::AuthRequired,
            message: "This platform requires an authorized API integration.".to_owned(),
            retryable: false,
            user_action: Some(
                "Use a supported public URL or configure an approved integration."
                    .to_owned(),
            ),
            diagnostic: None,
        },
        AdapterError::PublicMediaUnavailable => AppError {
            code: ErrorCode::MediaUnavailable,
            message: "This platform was detected, but no official public media download is available.".to_owned(),
            retryable: false,
            user_action: Some(
                "Use a direct public media URL or choose a platform with an available download path."
                    .to_owned(),
            ),
            diagnostic: None,
        },
        AdapterError::MalformedResponse { .. } => AppError {
            code: ErrorCode::MediaUnavailable,
            message: "The public media response could not be understood.".to_owned(),
            retryable: false,
            user_action: Some("Try another public URL or try again later.".to_owned()),
            diagnostic: None,
        },
        AdapterError::ResourceUnavailable { .. } | AdapterError::Network { .. } => AppError {
            code: ErrorCode::NetworkError,
            message: "The platform could not be reached right now.".to_owned(),
            retryable,
            user_action: Some("Check your connection and try again.".to_owned()),
            diagnostic: None,
        },
    }
}

fn persistence_error(error: RepositoryError) -> AppError {
    match error {
        RepositoryError::Storage { .. } => AppError {
            code: ErrorCode::DatabaseUnavailable,
            message: "The analysis result could not be saved locally.".to_owned(),
            retryable: true,
            user_action: Some("Check available disk space and try again.".to_owned()),
            diagnostic: None,
        },
        RepositoryError::Conflict { .. } | RepositoryError::InvalidData { .. } => AppError {
            code: ErrorCode::DatabaseCorrupt,
            message: "The analysis result could not be stored because local data is inconsistent."
                .to_owned(),
            retryable: false,
            user_action: Some("Restart the application and try again.".to_owned()),
            diagnostic: None,
        },
        RepositoryError::NotFound { .. } => AppError {
            code: ErrorCode::DatabaseUnavailable,
            message: "The local analysis record could not be found.".to_owned(),
            retryable: false,
            user_action: Some("Analyze the URL again.".to_owned()),
            diagnostic: None,
        },
    }
}

fn platform_name(adapter_id: &str) -> String {
    let mut characters = adapter_id.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => "Unknown".to_owned(),
    }
}

fn now_utc() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{AnalyzeRequest, AnalyzerService};
    use crate::adapters::AdapterRegistry;
    use crate::application::services::AppServices;
    use crate::persistence::Database;
    use tempfile::tempdir;

    #[tokio::test]
    async fn invalid_url_is_rejected_before_network_or_persistence() {
        let directory = tempdir().unwrap();
        let database = Database::from_app_data_dir(directory.path()).await.unwrap();
        let services = AppServices::from_database(&database);
        let analyzer = AnalyzerService::new(services, AdapterRegistry::new(Vec::new()));
        let error = analyzer
            .analyze(AnalyzeRequest {
                url: "not a url".to_owned(),
                platform_id: None,
            })
            .await
            .unwrap_err();
        assert_eq!(error.code.to_string(), "INVALID_URL");
    }
}

#[cfg(test)]
mod integration_tests {
    use super::{AnalyzeRequest, AnalyzeResponse, AnalyzerService};
    use crate::adapters::{
        AdapterError, AnalysisResult, NormalizedSource, PlatformAdapter, PlatformCapabilities,
    };
    use crate::application::ports::MediaSourceRepository;
    use crate::application::services::AppServices;
    use crate::domain::entities::{MediaFormat, MediaItem};
    use async_trait::async_trait;
    use std::sync::Arc;
    use tempfile::tempdir;
    use url::Url;

    struct StaticAdapter;

    #[async_trait]
    impl PlatformAdapter for StaticAdapter {
        fn id(&self) -> &'static str {
            "fixture"
        }

        fn detect(&self, url: &Url) -> bool {
            url.host_str() == Some("fixture.test")
        }

        async fn normalize(&self, url: &Url) -> Result<NormalizedSource, AdapterError> {
            if !self.detect(url) {
                return Err(AdapterError::UnsupportedUrl);
            }
            Ok(NormalizedSource {
                platform_id: "fixture".to_owned(),
                original_url: url.clone(),
                canonical_url: Url::parse("https://fixture.test/item").unwrap(),
                external_id: "item-1".to_owned(),
            })
        }

        async fn analyze(&self, source: &NormalizedSource) -> Result<AnalysisResult, AdapterError> {
            let item = MediaItem {
                id: "fixture:item:item-1".to_owned(),
                source_id: "fixture:source:item-1".to_owned(),
                collection_id: None,
                external_id: Some("item-1".to_owned()),
                canonical_url: source.canonical_url.to_string(),
                title: "Fixture item".to_owned(),
                creator_name: None,
                creator_id: None,
                thumbnail_url: None,
                duration_ms: Some(1000),
                published_at: None,
                position: Some(0),
                metadata_json: None,
                first_seen_at: "2026-01-01T00:00:00Z".to_owned(),
                last_seen_at: "2026-01-01T00:00:00Z".to_owned(),
            };
            let format = MediaFormat {
                id: "fixture:format:item-1:mp4".to_owned(),
                media_item_id: item.id.clone(),
                external_format_id: Some("mp4".to_owned()),
                container: Some("mp4".to_owned()),
                video_codec: None,
                audio_codec: None,
                width: Some(640),
                height: Some(360),
                fps: None,
                bitrate: None,
                sample_rate: None,
                channels: None,
                file_size_bytes: None,
                is_video: true,
                is_audio: false,
                is_progressive: true,
                metadata_json: None,
                created_at: "2026-01-01T00:00:00Z".to_owned(),
            };
            Ok(AnalysisResult {
                source: source.into(),
                items: vec![item],
                formats: vec![format],
            })
        }

        async fn resolve_formats(
            &self,
            _item: &MediaItem,
        ) -> Result<Vec<MediaFormat>, AdapterError> {
            Ok(Vec::new())
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

    #[tokio::test]
    async fn detection_only_tiktok_profile_returns_explicit_media_unavailable_error() {
        let directory = tempdir().unwrap();
        let database = crate::persistence::Database::from_app_data_dir(directory.path())
            .await
            .unwrap();
        let services = AppServices::from_database(&database);
        let analyzer = AnalyzerService::with_defaults(services).unwrap();

        let error = analyzer
            .analyze(AnalyzeRequest {
                url: "https://www.tiktok.com/@stave087".to_owned(),
                platform_id: None,
            })
            .await
            .unwrap_err();

        assert_eq!(
            error.code,
            crate::domain::errors::ErrorCode::MediaUnavailable
        );
        assert!(!error.retryable);
        assert!(error
            .message
            .contains("no official public media download is available"));
    }

    #[tokio::test]
    async fn analyzer_selects_adapter_persists_snapshot_and_shapes_response() {
        let directory = tempdir().unwrap();
        let database = crate::persistence::Database::from_app_data_dir(directory.path())
            .await
            .unwrap();
        let services = AppServices::from_database(&database);
        let registry = crate::adapters::AdapterRegistry::new(vec![Arc::new(StaticAdapter)]);
        let analyzer = AnalyzerService::new(services.clone(), registry);

        let response: AnalyzeResponse = analyzer
            .analyze(AnalyzeRequest {
                url: "https://fixture.test/item?tracking=ignored".to_owned(),
                platform_id: None,
            })
            .await
            .unwrap();

        assert_eq!(response.platform_id, "fixture");
        assert_eq!(response.items.len(), 1);
        assert_eq!(response.formats.len(), 1);
        assert!(services
            .repositories
            .media_sources
            .find_by_normalized_url("fixture", "https://fixture.test/item")
            .await
            .unwrap()
            .is_some());
    }
}
