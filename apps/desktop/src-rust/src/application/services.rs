use super::settings_service::SettingsService;
use crate::application::ports::{
    DownloadJobRepository, HistoryRepository, MediaFormatRepository, MediaItemRepository,
    MediaSourceRepository, PlatformRepository, RepositoryError, RepositoryResult,
    ScheduleRepository,
};
use crate::domain::entities::{
    Collection, DownloadJob, HistoryEntry, JobEvent, LicenseState, MediaFormat, MediaItem,
    MediaSource, Platform, Schedule,
};
use crate::downloader::{
    DownloadPlan, DownloadPlanError, DownloadProgress, DownloadStateMachine, FinalizationResult,
};

use crate::persistence::sqlite::repositories::{SqliteRepositories, SqliteSettingsRepository};
use crate::persistence::sqlite::SqliteTransactionCoordinator;
use crate::persistence::Database;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SnapshotItem {
    pub item: MediaItem,
    pub formats: Vec<MediaFormat>,
}

#[derive(Debug, Clone)]
pub struct AnalysisSnapshot {
    pub platform: Platform,
    pub source: MediaSource,
    pub collections: Vec<Collection>,
    pub items: Vec<SnapshotItem>,
}

#[derive(Clone)]
pub struct AppServices {
    pub repositories: SqliteRepositories,
    pub settings: SettingsService<SqliteSettingsRepository>,
    pub transactions: SqliteTransactionCoordinator,
}

impl AppServices {
    pub fn from_database(database: &Database) -> Self {
        let repositories = SqliteRepositories::new(database.pool());
        Self {
            settings: SettingsService::new(repositories.settings.clone()),
            transactions: SqliteTransactionCoordinator::new(database.pool()),
            repositories,
        }
    }

    pub async fn save_analysis_snapshot(
        &self,
        snapshot: &AnalysisSnapshot,
    ) -> RepositoryResult<()> {
        if snapshot.source.platform_id != snapshot.platform.id {
            return Err(RepositoryError::InvalidData {
                details: "snapshot source must reference its snapshot platform".to_owned(),
            });
        }
        if snapshot
            .collections
            .iter()
            .any(|collection| collection.source_id != snapshot.source.id)
        {
            return Err(RepositoryError::InvalidData {
                details: "snapshot collections must reference its snapshot source".to_owned(),
            });
        }
        if snapshot.items.iter().any(|entry| {
            entry.item.source_id != snapshot.source.id
                || entry
                    .formats
                    .iter()
                    .any(|format| format.media_item_id != entry.item.id)
        }) {
            return Err(RepositoryError::InvalidData {
                details: "snapshot items and formats have inconsistent ownership".to_owned(),
            });
        }

        self.transactions.save_analysis_snapshot(snapshot).await
    }

    pub async fn create_download_job(
        &self,
        job: &DownloadJob,
        initial_event: &JobEvent,
    ) -> RepositoryResult<()> {
        DownloadStateMachine::validate_new_job(job).map_err(|error| {
            RepositoryError::InvalidData {
                details: error.to_string(),
            }
        })?;
        if initial_event.job_id != job.id {
            return Err(RepositoryError::InvalidData {
                details: "initial job event must reference its job".to_owned(),
            });
        }
        if self
            .repositories
            .media_items
            .get(&job.media_item_id)
            .await?
            .is_none()
        {
            return Err(RepositoryError::NotFound {
                entity: "media_item",
                id: job.media_item_id.clone(),
            });
        }
        if let Some(format_id) = job.format_id.as_deref() {
            let format_exists = self
                .repositories
                .media_formats
                .list_by_item(&job.media_item_id)
                .await?
                .into_iter()
                .any(|format| format.id == format_id);
            if !format_exists {
                return Err(RepositoryError::InvalidData {
                    details: "download format must belong to the selected media item".to_owned(),
                });
            }
        }
        self.transactions
            .create_job_with_event(job, initial_event)
            .await
    }

    pub async fn transition_download_job(
        &self,
        job: &DownloadJob,
        event: &JobEvent,
    ) -> RepositoryResult<()> {
        let current = self
            .repositories
            .download_jobs
            .get(&job.id)
            .await?
            .ok_or_else(|| RepositoryError::NotFound {
                entity: "download_job",
                id: job.id.clone(),
            })?;
        if current.media_item_id != job.media_item_id || current.format_id != job.format_id {
            return Err(RepositoryError::InvalidData {
                details: "download job ownership cannot change during a transition".to_owned(),
            });
        }
        DownloadStateMachine::validate_transition(current.status, job.status.clone()).map_err(
            |error| RepositoryError::InvalidData {
                details: error.to_string(),
            },
        )?;
        DownloadStateMachine::validate_job(job).map_err(|error| RepositoryError::InvalidData {
            details: error.to_string(),
        })?;
        self.transactions.update_job_with_event(job, event).await
    }

    pub async fn recover_job_to_queued(
        &self,
        job: &DownloadJob,
        event: &JobEvent,
    ) -> RepositoryResult<()> {
        self.transactions.recover_job_to_queued(job, event).await
    }

    pub async fn recover_job_to_failed(
        &self,
        job: &DownloadJob,
        event: &JobEvent,
    ) -> RepositoryResult<()> {
        self.transactions.recover_job_to_failed(job, event).await
    }

    pub async fn recover_completed_download(
        &self,
        job_id: &str,
        finalization: &FinalizationResult,
        completed_at: &str,
        event: &JobEvent,
    ) -> RepositoryResult<()> {
        let bytes_finalized = i64::try_from(finalization.bytes_finalized).map_err(|_| {
            RepositoryError::InvalidData {
                details: "recovery finalized byte count exceeds the database integer range"
                    .to_owned(),
            }
        })?;
        self.transactions
            .recover_completed_download(
                job_id,
                &finalization.final_path.to_string_lossy(),
                bytes_finalized,
                completed_at,
                event,
            )
            .await
    }

    pub async fn list_download_jobs(&self) -> RepositoryResult<Vec<DownloadJob>> {
        self.repositories.download_jobs.list_all().await
    }

    pub async fn claim_next_queued_job(
        &self,
        event_id: &str,
        payload_json: Option<serde_json::Value>,
        created_at: &str,
    ) -> RepositoryResult<Option<DownloadJob>> {
        let Some(job_id) = self
            .transactions
            .claim_next_queued_job(event_id, "resolving", &payload_json, created_at)
            .await?
        else {
            return Ok(None);
        };
        self.repositories.download_jobs.get(&job_id).await
    }

    pub async fn resolve_download_plan(
        &self,
        media_item_id: &str,
        format_id: &str,
        destination_root: &Path,
        filename: &str,
    ) -> RepositoryResult<DownloadPlan> {
        let item = self
            .repositories
            .media_items
            .get(media_item_id)
            .await?
            .ok_or_else(|| RepositoryError::NotFound {
                entity: "media_item",
                id: media_item_id.to_owned(),
            })?;
        let source = self
            .repositories
            .media_sources
            .get(&item.source_id)
            .await?
            .ok_or_else(|| RepositoryError::NotFound {
                entity: "media_source",
                id: item.source_id.clone(),
            })?;
        let platform = self
            .repositories
            .platforms
            .get(&source.platform_id)
            .await?
            .ok_or_else(|| RepositoryError::NotFound {
                entity: "platform",
                id: source.platform_id.clone(),
            })?;
        let format = self
            .repositories
            .media_formats
            .list_by_item(media_item_id)
            .await?
            .into_iter()
            .find(|candidate| candidate.id == format_id)
            .ok_or_else(|| RepositoryError::NotFound {
                entity: "media_format",
                id: format_id.to_owned(),
            })?;
        DownloadPlan::resolve(&platform.id, &item, &format, destination_root, filename).map_err(
            |error: DownloadPlanError| RepositoryError::InvalidData {
                details: error.to_string(),
            },
        )
    }

    pub async fn persist_download_progress(
        &self,
        progress: &DownloadProgress,
        event: &JobEvent,
    ) -> RepositoryResult<()> {
        self.persist_download_progress_with_validators(progress, event, None, None)
            .await
    }

    pub async fn persist_download_progress_with_validators(
        &self,
        progress: &DownloadProgress,
        event: &JobEvent,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> RepositoryResult<()> {
        self.transactions
            .persist_download_progress_with_validators(progress, event, etag, last_modified)
            .await
    }

    pub async fn complete_streamed_download(
        &self,
        job_id: &str,
        finalization: &FinalizationResult,
        completed_at: &str,
        event: &JobEvent,
    ) -> RepositoryResult<()> {
        self.complete_streamed_download_with_validators(
            job_id,
            finalization,
            completed_at,
            event,
            None,
            None,
        )
        .await
    }

    pub async fn complete_streamed_download_with_validators(
        &self,
        job_id: &str,
        finalization: &FinalizationResult,
        completed_at: &str,
        event: &JobEvent,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> RepositoryResult<()> {
        let bytes_finalized = i64::try_from(finalization.bytes_finalized).map_err(|_| {
            RepositoryError::InvalidData {
                details: "finalized byte count exceeds the database integer range".to_owned(),
            }
        })?;
        let final_path = finalization.final_path.to_string_lossy().to_string();
        self.transactions
            .complete_download_job_with_validators(
                job_id,
                &final_path,
                bytes_finalized,
                completed_at,
                event,
                (etag, last_modified),
            )
            .await
    }

    pub async fn record_history_for_job(
        &self,
        job_id: &str,
        status: crate::domain::entities::DownloadStatus,
        size_bytes: Option<i64>,
        error_code: Option<String>,
        error_message: Option<String>,
        finished_at: &str,
    ) -> RepositoryResult<()> {
        if !matches!(
            status,
            crate::domain::entities::DownloadStatus::Completed
                | crate::domain::entities::DownloadStatus::Failed
                | crate::domain::entities::DownloadStatus::Cancelled
        ) {
            return Err(RepositoryError::InvalidData {
                details: "history entries require a terminal job status".to_owned(),
            });
        }
        let job = self
            .repositories
            .download_jobs
            .get(job_id)
            .await?
            .ok_or_else(|| RepositoryError::NotFound {
                entity: "download_job",
                id: job_id.to_owned(),
            })?;
        let item = self
            .repositories
            .media_items
            .get(&job.media_item_id)
            .await?
            .ok_or_else(|| RepositoryError::NotFound {
                entity: "media_item",
                id: job.media_item_id.clone(),
            })?;
        let source = self
            .repositories
            .media_sources
            .get(&item.source_id)
            .await?
            .ok_or_else(|| RepositoryError::NotFound {
                entity: "media_source",
                id: item.source_id.clone(),
            })?;
        let platform = self
            .repositories
            .platforms
            .get(&source.platform_id)
            .await?
            .ok_or_else(|| RepositoryError::NotFound {
                entity: "platform",
                id: source.platform_id.clone(),
            })?;
        self.repositories
            .history
            .upsert(&HistoryEntry {
                id: format!("history-{job_id}"),
                job_id: job.id,
                media_item_id: job.media_item_id,
                format_id: job.format_id,
                platform_id: platform.id,
                platform_name: platform.name,
                source_url: source.normalized_url,
                title: item.title,
                creator_name: item.creator_name,
                destination_path: job.destination_path,
                filename: job.filename,
                status,
                size_bytes,
                error_code,
                error_message,
                created_at: job.created_at,
                finished_at: finished_at.to_owned(),
            })
            .await
    }

    pub async fn list_history(&self, query: Option<&str>) -> RepositoryResult<Vec<HistoryEntry>> {
        self.repositories.history.list(query).await
    }

    pub async fn delete_history_entry(&self, id: &str) -> RepositoryResult<bool> {
        self.repositories.history.delete(id).await
    }

    pub async fn clear_history(&self) -> RepositoryResult<u64> {
        self.repositories.history.clear().await
    }

    pub async fn upsert_history_entry(&self, entry: &HistoryEntry) -> RepositoryResult<()> {
        self.repositories.history.upsert(entry).await
    }

    pub async fn list_schedules(&self) -> RepositoryResult<Vec<Schedule>> {
        self.repositories.schedules.list_all().await
    }

    pub async fn get_schedule(&self, id: &str) -> RepositoryResult<Option<Schedule>> {
        self.repositories.schedules.get(id).await
    }

    pub async fn save_schedule(&self, schedule: &Schedule) -> RepositoryResult<()> {
        if self
            .repositories
            .media_sources
            .get(&schedule.source_id)
            .await?
            .is_none()
        {
            return Err(RepositoryError::NotFound {
                entity: "media_source",
                id: schedule.source_id.clone(),
            });
        }
        self.repositories.schedules.upsert(schedule).await
    }

    pub async fn delete_schedule(&self, id: &str) -> RepositoryResult<bool> {
        self.repositories.schedules.delete(id).await
    }

    pub async fn get_license_state(&self) -> RepositoryResult<Option<LicenseState>> {
        use crate::application::ports::LicenseStateRepository;
        self.repositories.license_state.get().await
    }
}

#[cfg(test)]
mod tests {
    use super::{AnalysisSnapshot, AppServices, SnapshotItem};
    use crate::application::ports::{
        DownloadJobRepository, JobEventRepository, MediaItemRepository, MediaSourceRepository,
    };
    use crate::application::settings_service::{SettingKey, SettingValue};
    use crate::domain::entities::{
        DownloadJob, DownloadStatus, JobEvent, MediaFormat, MediaItem, MediaSource, Platform,
        SourceType,
    };
    use crate::downloader::{DownloadProgress, FinalizationResult};
    use crate::persistence::Database;
    use serde_json::json;
    use tempfile::tempdir;

    fn snapshot() -> AnalysisSnapshot {
        AnalysisSnapshot {
            platform: Platform {
                id: "platform-1".to_owned(),
                slug: "generic".to_owned(),
                name: "Generic".to_owned(),
                enabled: true,
                adapter_version: None,
                created_at: "2026-01-01T00:00:00Z".to_owned(),
                updated_at: "2026-01-01T00:00:00Z".to_owned(),
            },
            source: MediaSource {
                id: "source-1".to_owned(),
                platform_id: "platform-1".to_owned(),
                source_url: "https://example.test/source".to_owned(),
                normalized_url: "https://example.test/source".to_owned(),
                source_type: SourceType::Generic,
                title: Some("Source".to_owned()),
                creator_name: None,
                creator_id: None,
                thumbnail_url: None,
                item_count: Some(1),
                discovered_at: "2026-01-01T00:00:00Z".to_owned(),
                last_analyzed_at: Some("2026-01-01T00:00:00Z".to_owned()),
                metadata_json: None,
            },
            collections: Vec::new(),
            items: vec![SnapshotItem {
                item: MediaItem {
                    id: "item-1".to_owned(),
                    source_id: "source-1".to_owned(),
                    collection_id: None,
                    external_id: None,
                    canonical_url: "https://example.test/item".to_owned(),
                    title: "Item".to_owned(),
                    creator_name: None,
                    creator_id: None,
                    thumbnail_url: None,
                    duration_ms: None,
                    published_at: None,
                    position: Some(0),
                    metadata_json: None,
                    first_seen_at: "2026-01-01T00:00:00Z".to_owned(),
                    last_seen_at: "2026-01-01T00:00:00Z".to_owned(),
                },
                formats: Vec::new(),
            }],
        }
    }

    fn job() -> DownloadJob {
        DownloadJob {
            id: "job-1".to_owned(),
            media_item_id: "item-1".to_owned(),
            format_id: None,
            status: DownloadStatus::Queued,
            priority: 0,
            destination_path: "/tmp/umd".to_owned(),
            temp_path: None,
            filename: "item.mp4".to_owned(),
            total_bytes: None,
            downloaded_bytes: 0,
            speed_bytes_per_sec: None,
            eta_seconds: None,
            retry_count: 0,
            max_retries: 3,
            processing_json: None,
            etag: None,
            last_modified: None,
            error_code: None,
            error_message: None,
            started_at: None,
            completed_at: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[tokio::test]
    async fn app_services_wire_snapshot_job_event_and_settings_operations() {
        let directory = tempdir().expect("temporary directory should be created");
        let database = Database::from_app_data_dir(directory.path())
            .await
            .expect("database should initialize");
        let services = AppServices::from_database(&database);

        services
            .save_analysis_snapshot(&snapshot())
            .await
            .expect("snapshot should persist");
        assert!(services
            .repositories
            .media_sources
            .get("source-1")
            .await
            .unwrap()
            .is_some());
        assert!(services
            .repositories
            .media_items
            .get("item-1")
            .await
            .unwrap()
            .is_some());

        let event = JobEvent {
            id: "event-1".to_owned(),
            job_id: "job-1".to_owned(),
            event_type: "queued".to_owned(),
            payload_json: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
        };
        services
            .create_download_job(&job(), &event)
            .await
            .expect("job and event should persist");
        assert!(services
            .repositories
            .download_jobs
            .get("job-1")
            .await
            .unwrap()
            .is_some());
        assert_eq!(
            services
                .repositories
                .job_events
                .list_by_job("job-1")
                .await
                .unwrap()
                .len(),
            1
        );

        let claimed = services
            .claim_next_queued_job(
                "event-claim-1",
                Some(json!({"worker": "test-worker"})),
                "2026-01-01T00:01:00Z",
            )
            .await
            .expect("queue claim should succeed")
            .expect("one queued job should be claimable");
        assert_eq!(claimed.id, "job-1");
        assert_eq!(claimed.status, DownloadStatus::Resolving);

        let mut downloading = claimed.clone();
        downloading.status = DownloadStatus::Downloading;
        downloading.total_bytes = Some(5);
        downloading.updated_at = "2026-01-01T00:01:30Z".to_owned();
        services
            .transition_download_job(
                &downloading,
                &JobEvent {
                    id: "event-downloading".to_owned(),
                    job_id: "job-1".to_owned(),
                    event_type: "downloading".to_owned(),
                    payload_json: None,
                    created_at: "2026-01-01T00:01:30Z".to_owned(),
                },
            )
            .await
            .expect("resolving to downloading should persist");
        services
            .persist_download_progress(
                &DownloadProgress {
                    job_id: "job-1".to_owned(),
                    downloaded_bytes: 5,
                    total_bytes: Some(5),
                    speed_bytes_per_sec: Some(5),
                    eta_seconds: Some(0),
                    updated_at: "2026-01-01T00:01:31Z".to_owned(),
                },
                &JobEvent {
                    id: "event-progress".to_owned(),
                    job_id: "job-1".to_owned(),
                    event_type: "progress".to_owned(),
                    payload_json: Some(json!({"downloaded_bytes": 5})),
                    created_at: "2026-01-01T00:01:31Z".to_owned(),
                },
            )
            .await
            .expect("progress should persist");
        let finalization = FinalizationResult {
            final_path: std::path::PathBuf::from("/tmp/umd/item.mp4"),
            bytes_finalized: 5,
        };
        services
            .complete_streamed_download(
                "job-1",
                &finalization,
                "2026-01-01T00:01:32Z",
                &JobEvent {
                    id: "event-completed".to_owned(),
                    job_id: "job-1".to_owned(),
                    event_type: "completed".to_owned(),
                    payload_json: Some(json!({"bytes_finalized": 5})),
                    created_at: "2026-01-01T00:01:32Z".to_owned(),
                },
            )
            .await
            .expect("completion should persist");
        let completed = services
            .repositories
            .download_jobs
            .get("job-1")
            .await
            .unwrap()
            .expect("completed job should remain available");
        assert_eq!(completed.status, DownloadStatus::Completed);
        assert_eq!(completed.destination_path, "/tmp/umd");
        assert_eq!(completed.temp_path, None);
        assert_eq!(completed.downloaded_bytes, 5);

        services
            .settings
            .set(SettingKey::ConcurrentJobs, SettingValue::ConcurrentJobs(4))
            .await
            .expect("typed setting should persist");
        assert_eq!(
            services
                .settings
                .get(SettingKey::ConcurrentJobs)
                .await
                .unwrap(),
            Some(SettingValue::ConcurrentJobs(4))
        );
    }

    #[tokio::test]
    async fn resolve_download_plan_uses_persisted_ownership_and_path_policy() {
        let directory = tempdir().expect("temporary directory should be created");
        let database = Database::from_app_data_dir(directory.path())
            .await
            .expect("database should initialize");
        let services = AppServices::from_database(&database);
        let mut persisted = snapshot();
        persisted.platform.id = "reddit".to_owned();
        persisted.platform.slug = "reddit".to_owned();
        persisted.source.platform_id = "reddit".to_owned();
        persisted.items[0].formats.push(MediaFormat {
            id: "format-1".to_owned(),
            media_item_id: "item-1".to_owned(),
            external_format_id: Some("fallback".to_owned()),
            container: Some("mp4".to_owned()),
            video_codec: None,
            audio_codec: None,
            width: Some(1280),
            height: Some(720),
            fps: None,
            bitrate: None,
            sample_rate: None,
            channels: None,
            file_size_bytes: Some(500),
            is_video: true,
            is_audio: false,
            is_progressive: true,
            metadata_json: Some(json!({
                "public_url": "https://v.redd.it/item-1/video.mp4"
            })),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
        });
        services
            .save_analysis_snapshot(&persisted)
            .await
            .expect("snapshot should persist");

        let root = directory.path().join("downloads");
        let plan = services
            .resolve_download_plan("item-1", "format-1", &root, "Item.mp4")
            .await
            .expect("persisted progressive format should resolve");
        assert_eq!(plan.platform_id, "reddit");
        assert_eq!(plan.source_url.host_str(), Some("v.redd.it"));
        assert_eq!(plan.destination.destination, root.join("Item.mp4"));
        assert_eq!(plan.destination.temporary, root.join("Item.mp4.part"));
    }

    #[tokio::test]
    async fn create_download_rejects_a_format_that_does_not_belong_to_the_item() {
        let directory = tempdir().expect("temporary directory should be created");
        let database = Database::from_app_data_dir(directory.path())
            .await
            .expect("database should initialize");
        let services = AppServices::from_database(&database);
        services
            .save_analysis_snapshot(&snapshot())
            .await
            .expect("snapshot should persist");

        let mut invalid_job = job();
        invalid_job.format_id = Some("format-from-another-item".to_owned());
        let event = JobEvent {
            id: "event-invalid-format".to_owned(),
            job_id: invalid_job.id.clone(),
            event_type: "queued".to_owned(),
            payload_json: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
        };
        let error = services
            .create_download_job(&invalid_job, &event)
            .await
            .expect_err("format ownership must be validated");
        assert!(matches!(
            error,
            crate::application::ports::RepositoryError::InvalidData { .. }
        ));
    }
}
