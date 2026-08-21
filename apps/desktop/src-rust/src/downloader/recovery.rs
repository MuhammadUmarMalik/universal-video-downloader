use crate::application::ports::{DownloadJobRepository, RepositoryError};
use crate::application::services::AppServices;
use crate::domain::entities::{DownloadJob, DownloadStatus, JobEvent};
use crate::downloader::path_safety::validate_path_within_root;
use crate::downloader::{
    harden_file_permissions, validate_destination, FinalizationResult, DEFAULT_MAX_RESPONSE_BYTES,
};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::fs;

const RECOVERY_FAILURE_CODE: &str = "RECOVERY_UNRECOVERABLE";
const RECOVERY_FAILURE_MESSAGE: &str =
    "The download could not be safely recovered after application restart.";

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("recovery repository operation failed: {0}")]
    Repository(String),
    #[error("recovery filesystem operation failed")]
    Filesystem,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct RecoveryReport {
    pub inspected: usize,
    pub requeued: usize,
    pub completed: usize,
    pub failed: usize,
}

#[derive(Clone)]
pub struct StartupRecoveryCoordinator {
    services: Arc<AppServices>,
    max_response_bytes: u64,
}

impl StartupRecoveryCoordinator {
    pub fn new(services: Arc<AppServices>) -> Self {
        Self {
            services,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        }
    }

    pub async fn recover(&self) -> Result<RecoveryReport, RecoveryError> {
        let jobs = self
            .services
            .repositories
            .download_jobs
            .list_all()
            .await
            .map_err(repository_error)?;
        let recoverable = jobs
            .into_iter()
            .filter(|job| !is_terminal(job.status.clone()))
            .collect::<Vec<_>>();
        let mut report = RecoveryReport {
            inspected: recoverable.len(),
            ..RecoveryReport::default()
        };

        for job in recoverable {
            match self.recover_job(job).await {
                Ok(RecoveryAction::Requeued) => report.requeued += 1,
                Ok(RecoveryAction::Completed) => report.completed += 1,
                Ok(RecoveryAction::Failed) => report.failed += 1,
                Err(error) => {
                    tracing::error!(
                        event = "startup_recovery_job_failed",
                        error = %error
                    );
                }
            }
        }
        Ok(report)
    }

    async fn recover_job(&self, job: DownloadJob) -> Result<RecoveryAction, RecoveryError> {
        let paths = match validate_destination(Path::new(&job.destination_path), &job.filename) {
            Ok(paths) => paths,
            Err(_) => return self.fail_unrecoverable(job, "destination_policy").await,
        };

        let finalization = match self.valid_final_file(&job, &paths).await {
            Ok(finalization) => finalization,
            Err(RecoveryError::Filesystem) => {
                return self.fail_unrecoverable(job, "unsafe_final_file").await;
            }
            Err(error) => return Err(error),
        };
        if let Some(finalization) = finalization {
            let event = recovery_event(&job.id, "completed", "reconciled_final_file");
            self.services
                .recover_completed_download(&job.id, &finalization, &now_utc(), &event)
                .await
                .map_err(repository_error)?;
            let _ = self
                .services
                .record_history_for_job(
                    &job.id,
                    DownloadStatus::Completed,
                    i64::try_from(finalization.bytes_finalized).ok(),
                    None,
                    None,
                    &event.created_at,
                )
                .await;
            return Ok(RecoveryAction::Completed);
        }

        let part_length = if job.status == DownloadStatus::Processing {
            if let Err(error) = self.clean_processing_artifact(&job).await {
                if matches!(error, RecoveryError::Filesystem) {
                    return self
                        .fail_unrecoverable(job, "unsafe_processing_artifact")
                        .await;
                }
                return Err(error);
            }
            0
        } else {
            match self.safe_part_length(&paths.temporary).await {
                Ok(length) => length,
                Err(RecoveryError::Filesystem) => {
                    return self.fail_unrecoverable(job, "unsafe_part_file").await;
                }
                Err(error) => return Err(error),
            }
        };
        if let Some(total) = job.total_bytes {
            if total < 0 || u64::try_from(total).unwrap_or(u64::MAX) > self.max_response_bytes {
                return self.fail_unrecoverable(job, "invalid_total").await;
            }
            if part_length > u64::try_from(total).unwrap_or(u64::MAX) {
                return self.fail_unrecoverable(job, "part_exceeds_total").await;
            }
        }

        if job.retry_count >= job.max_retries {
            return self.fail_unrecoverable(job, "retry_budget_exhausted").await;
        }

        let downloaded_bytes = i64::try_from(part_length).map_err(|_| RecoveryError::Filesystem)?;
        let mut queued = job.clone();
        queued.status = DownloadStatus::Queued;
        queued.temp_path = Some(paths.temporary.to_string_lossy().to_string());
        queued.downloaded_bytes = downloaded_bytes;
        queued.speed_bytes_per_sec = None;
        queued.eta_seconds = None;
        queued.retry_count = queued.retry_count.saturating_add(1);
        queued.error_code = None;
        queued.error_message = None;
        queued.started_at = None;
        queued.completed_at = None;
        queued.updated_at = now_utc();
        let event = recovery_event(
            &queued.id,
            "queued",
            if part_length > 0 {
                "resume_candidate"
            } else {
                "restart_candidate"
            },
        );
        self.services
            .recover_job_to_queued(&queued, &event)
            .await
            .map_err(repository_error)?;
        Ok(RecoveryAction::Requeued)
    }

    async fn valid_final_file(
        &self,
        job: &DownloadJob,
        paths: &crate::downloader::DestinationPaths,
    ) -> Result<Option<FinalizationResult>, RecoveryError> {
        let metadata = match fs::symlink_metadata(&paths.destination).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(RecoveryError::Filesystem),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(RecoveryError::Filesystem);
        }
        let bytes = metadata.len();
        if bytes > self.max_response_bytes
            || job
                .total_bytes
                .is_some_and(|total| total < 0 || u64::try_from(total).unwrap_or(u64::MAX) != bytes)
        {
            return Err(RecoveryError::Filesystem);
        }
        harden_file_permissions(&paths.destination).map_err(|_| RecoveryError::Filesystem)?;
        Ok(Some(FinalizationResult {
            final_path: paths.destination.clone(),
            bytes_finalized: bytes,
        }))
    }

    async fn safe_part_length(&self, part_path: &Path) -> Result<u64, RecoveryError> {
        let metadata = match fs::symlink_metadata(part_path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(_) => return Err(RecoveryError::Filesystem),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(RecoveryError::Filesystem);
        }
        let length = metadata.len();
        if length > self.max_response_bytes {
            return Err(RecoveryError::Filesystem);
        }
        Ok(length)
    }

    async fn clean_processing_artifact(&self, job: &DownloadJob) -> Result<(), RecoveryError> {
        let Some(temp_path) = job.temp_path.as_deref() else {
            return Ok(());
        };
        let path = PathBuf::from(temp_path);
        validate_path_within_root(Path::new(&job.destination_path), &path)
            .map_err(|_| RecoveryError::Filesystem)?;
        let metadata = match fs::symlink_metadata(&path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(RecoveryError::Filesystem),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(RecoveryError::Filesystem);
        }
        fs::remove_file(path)
            .await
            .map_err(|_| RecoveryError::Filesystem)
    }

    async fn fail_unrecoverable(
        &self,
        mut job: DownloadJob,
        reason: &str,
    ) -> Result<RecoveryAction, RecoveryError> {
        job.status = DownloadStatus::Failed;
        job.error_code = Some(RECOVERY_FAILURE_CODE.to_owned());
        job.error_message = Some(RECOVERY_FAILURE_MESSAGE.to_owned());
        job.speed_bytes_per_sec = None;
        job.eta_seconds = None;
        job.completed_at = None;
        job.updated_at = now_utc();
        let event = recovery_event(&job.id, "failed", reason);
        self.services
            .recover_job_to_failed(&job, &event)
            .await
            .map_err(repository_error)?;
        let _ = self
            .services
            .record_history_for_job(
                &job.id,
                DownloadStatus::Failed,
                None,
                job.error_code.clone(),
                job.error_message.clone(),
                &event.created_at,
            )
            .await;
        Ok(RecoveryAction::Failed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryAction {
    Requeued,
    Completed,
    Failed,
}

fn is_terminal(status: DownloadStatus) -> bool {
    matches!(
        status,
        DownloadStatus::Completed | DownloadStatus::Cancelled | DownloadStatus::Failed
    )
}

fn recovery_event(job_id: &str, state: &str, reason: &str) -> JobEvent {
    let created_at = now_utc();
    JobEvent {
        id: format!(
            "recovery-{job_id}-{}-{state}",
            created_at.replace([':', '-', '.'], "")
        ),
        job_id: job_id.to_owned(),
        event_type: format!("recovery_{state}"),
        payload_json: Some(serde_json::json!({
            "reason": reason,
            "startup": true,
        })),
        created_at,
    }
}

fn repository_error(error: RepositoryError) -> RecoveryError {
    RecoveryError::Repository(error.to_string())
}

fn now_utc() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{is_terminal, StartupRecoveryCoordinator};
    use crate::application::ports::{
        DownloadJobRepository, MediaFormatRepository, MediaItemRepository, MediaSourceRepository,
        PlatformRepository,
    };
    use crate::application::services::AppServices;
    use crate::domain::entities::{
        DownloadJob, DownloadStatus, JobEvent, MediaFormat, MediaItem, MediaSource, Platform,
        SourceType,
    };
    use crate::persistence::Database;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn terminal_states_are_not_recovered() {
        assert!(is_terminal(DownloadStatus::Completed));
        assert!(is_terminal(DownloadStatus::Cancelled));
        assert!(is_terminal(DownloadStatus::Failed));
        assert!(!is_terminal(DownloadStatus::Queued));
        assert!(!is_terminal(DownloadStatus::Resolving));
        assert!(!is_terminal(DownloadStatus::Downloading));
        assert!(!is_terminal(DownloadStatus::Processing));
    }

    async fn fixture() -> (tempfile::TempDir, Arc<AppServices>, DownloadJob) {
        let directory = tempdir().unwrap();
        let database = Database::from_app_data_dir(directory.path()).await.unwrap();
        let services = Arc::new(AppServices::from_database(&database));
        services
            .repositories
            .platforms
            .upsert(&Platform {
                id: "platform-1".to_owned(),
                slug: "generic".to_owned(),
                name: "Generic".to_owned(),
                enabled: true,
                adapter_version: None,
                created_at: "2026-01-01T00:00:00Z".to_owned(),
                updated_at: "2026-01-01T00:00:00Z".to_owned(),
            })
            .await
            .unwrap();
        services
            .repositories
            .media_sources
            .upsert(&MediaSource {
                id: "source-1".to_owned(),
                platform_id: "platform-1".to_owned(),
                source_url: "https://example.test/source".to_owned(),
                normalized_url: "https://example.test/source".to_owned(),
                source_type: SourceType::Single,
                title: None,
                creator_name: None,
                creator_id: None,
                thumbnail_url: None,
                item_count: None,
                discovered_at: "2026-01-01T00:00:00Z".to_owned(),
                last_analyzed_at: None,
                metadata_json: None,
            })
            .await
            .unwrap();
        services
            .repositories
            .media_items
            .upsert(&MediaItem {
                id: "item-1".to_owned(),
                source_id: "source-1".to_owned(),
                collection_id: None,
                external_id: None,
                canonical_url: "https://example.test/item".to_owned(),
                title: "Item".to_owned(),
                creator_name: Some("Creator".to_owned()),
                creator_id: None,
                thumbnail_url: None,
                duration_ms: None,
                published_at: None,
                position: None,
                metadata_json: None,
                first_seen_at: "2026-01-01T00:00:00Z".to_owned(),
                last_seen_at: "2026-01-01T00:00:00Z".to_owned(),
            })
            .await
            .unwrap();
        services
            .repositories
            .media_formats
            .upsert(&MediaFormat {
                id: "format-1".to_owned(),
                media_item_id: "item-1".to_owned(),
                external_format_id: None,
                container: Some("mp4".to_owned()),
                video_codec: None,
                audio_codec: None,
                width: None,
                height: None,
                fps: None,
                bitrate: None,
                sample_rate: None,
                channels: None,
                file_size_bytes: Some(5),
                is_video: true,
                is_audio: false,
                is_progressive: true,
                metadata_json: Some(json!({"public_url": "https://v.redd.it/public/video.mp4"})),
                created_at: "2026-01-01T00:00:00Z".to_owned(),
            })
            .await
            .unwrap();
        let job = DownloadJob {
            id: "job-1".to_owned(),
            media_item_id: "item-1".to_owned(),
            format_id: Some("format-1".to_owned()),
            status: DownloadStatus::Queued,
            priority: 0,
            destination_path: directory.path().to_string_lossy().to_string(),
            temp_path: Some(
                directory
                    .path()
                    .join("video.mp4.part")
                    .to_string_lossy()
                    .to_string(),
            ),
            filename: "video.mp4".to_owned(),
            total_bytes: Some(5),
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
        };
        services
            .create_download_job(
                &job,
                &JobEvent {
                    id: "job-1-created".to_owned(),
                    job_id: "job-1".to_owned(),
                    event_type: "queued".to_owned(),
                    payload_json: None,
                    created_at: job.created_at.clone(),
                },
            )
            .await
            .unwrap();
        (directory, services, job)
    }

    async fn mark_downloading(services: &AppServices, job: &DownloadJob) {
        let mut resolving = job.clone();
        resolving.status = DownloadStatus::Resolving;
        resolving.updated_at = "2026-01-01T00:00:01Z".to_owned();
        services
            .transition_download_job(
                &resolving,
                &JobEvent {
                    id: "job-1-resolving".to_owned(),
                    job_id: job.id.clone(),
                    event_type: "resolving".to_owned(),
                    payload_json: None,
                    created_at: resolving.updated_at.clone(),
                },
            )
            .await
            .unwrap();
        let mut downloading = resolving;
        downloading.status = DownloadStatus::Downloading;
        downloading.updated_at = "2026-01-01T00:00:02Z".to_owned();
        services
            .transition_download_job(
                &downloading,
                &JobEvent {
                    id: "job-1-downloading".to_owned(),
                    job_id: job.id.clone(),
                    event_type: "downloading".to_owned(),
                    payload_json: None,
                    created_at: downloading.updated_at.clone(),
                },
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn requeues_downloading_job_with_safe_part_offset() {
        let (directory, services, job) = fixture().await;
        mark_downloading(&services, &job).await;
        tokio::fs::write(directory.path().join("video.mp4.part"), b"hello")
            .await
            .unwrap();
        let report = StartupRecoveryCoordinator::new(Arc::clone(&services))
            .recover()
            .await
            .unwrap();
        assert_eq!(report.requeued, 1);
        let recovered = services
            .repositories
            .download_jobs
            .get("job-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(recovered.status, DownloadStatus::Queued);
        assert_eq!(recovered.downloaded_bytes, 5);
        assert_eq!(recovered.retry_count, 1);
    }

    #[tokio::test]
    async fn reconciles_a_final_file_as_completed() {
        let (directory, services, job) = fixture().await;
        mark_downloading(&services, &job).await;
        tokio::fs::write(directory.path().join("video.mp4"), b"hello")
            .await
            .unwrap();
        let report = StartupRecoveryCoordinator::new(Arc::clone(&services))
            .recover()
            .await
            .unwrap();
        assert_eq!(report.completed, 1);
        assert_eq!(
            services
                .repositories
                .download_jobs
                .get("job-1")
                .await
                .unwrap()
                .unwrap()
                .status,
            DownloadStatus::Completed
        );
    }

    #[tokio::test]
    async fn marks_invalid_destination_as_unrecoverable() {
        let (_directory, services, mut job) = fixture().await;
        job.destination_path = "relative".to_owned();
        services
            .repositories
            .download_jobs
            .update(&job)
            .await
            .unwrap();
        let report = StartupRecoveryCoordinator::new(Arc::clone(&services))
            .recover()
            .await
            .unwrap();
        assert_eq!(report.failed, 1);
        assert_eq!(
            services
                .repositories
                .download_jobs
                .get("job-1")
                .await
                .unwrap()
                .unwrap()
                .status,
            DownloadStatus::Failed
        );
    }
}
