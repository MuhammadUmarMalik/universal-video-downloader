use super::repositories::{serialize_json, storage};
use crate::application::ports::{RepositoryError, RepositoryResult};
use crate::application::services::AnalysisSnapshot;
use crate::domain::entities::{DownloadJob, DownloadStatus, JobEvent, MediaFormat};
use crate::downloader::{DownloadProgress, DownloadStateError, DownloadStateMachine};
use sqlx::{Sqlite, SqlitePool, Transaction};

#[derive(Clone)]
pub struct SqliteTransactionCoordinator {
    pool: SqlitePool,
}

impl SqliteTransactionCoordinator {
    pub(crate) fn new(pool: &SqlitePool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn save_analysis_snapshot(
        &self,
        snapshot: &AnalysisSnapshot,
    ) -> RepositoryResult<()> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| storage("analysis_snapshot.begin", error))?;

        sqlx::query("INSERT INTO platforms (id, slug, name, enabled, adapter_version, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET slug = excluded.slug, name = excluded.name, enabled = excluded.enabled, adapter_version = excluded.adapter_version, updated_at = excluded.updated_at")
            .bind(&snapshot.platform.id)
            .bind(&snapshot.platform.slug)
            .bind(&snapshot.platform.name)
            .bind(i64::from(snapshot.platform.enabled))
            .bind(&snapshot.platform.adapter_version)
            .bind(&snapshot.platform.created_at)
            .bind(&snapshot.platform.updated_at)
            .execute(&mut *transaction)
            .await
            .map_err(|error| storage("analysis_snapshot.platform", error))?;

        let metadata_json = serialize_json(
            "media_sources.metadata_json",
            &snapshot.source.metadata_json,
        )?;
        sqlx::query("INSERT INTO media_sources (id, platform_id, source_url, normalized_url, source_type, title, creator_name, creator_id, thumbnail_url, item_count, discovered_at, last_analyzed_at, metadata_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET platform_id = excluded.platform_id, source_url = excluded.source_url, normalized_url = excluded.normalized_url, source_type = excluded.source_type, title = excluded.title, creator_name = excluded.creator_name, creator_id = excluded.creator_id, thumbnail_url = excluded.thumbnail_url, item_count = excluded.item_count, discovered_at = excluded.discovered_at, last_analyzed_at = excluded.last_analyzed_at, metadata_json = excluded.metadata_json")
            .bind(&snapshot.source.id)
            .bind(&snapshot.source.platform_id)
            .bind(&snapshot.source.source_url)
            .bind(&snapshot.source.normalized_url)
            .bind(snapshot.source.source_type.as_str())
            .bind(&snapshot.source.title)
            .bind(&snapshot.source.creator_name)
            .bind(&snapshot.source.creator_id)
            .bind(&snapshot.source.thumbnail_url)
            .bind(snapshot.source.item_count)
            .bind(&snapshot.source.discovered_at)
            .bind(&snapshot.source.last_analyzed_at)
            .bind(metadata_json)
            .execute(&mut *transaction)
            .await
            .map_err(|error| storage("analysis_snapshot.source", error))?;

        for collection in &snapshot.collections {
            sqlx::query("INSERT INTO collections (id, source_id, external_id, title, creator_name, item_count, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET source_id = excluded.source_id, external_id = excluded.external_id, title = excluded.title, creator_name = excluded.creator_name, item_count = excluded.item_count, updated_at = excluded.updated_at")
                .bind(&collection.id)
                .bind(&collection.source_id)
                .bind(&collection.external_id)
                .bind(&collection.title)
                .bind(&collection.creator_name)
                .bind(collection.item_count)
                .bind(&collection.created_at)
                .bind(&collection.updated_at)
                .execute(&mut *transaction)
                .await
                .map_err(|error| storage("analysis_snapshot.collection", error))?;
        }

        for entry in &snapshot.items {
            let metadata_json =
                serialize_json("media_items.metadata_json", &entry.item.metadata_json)?;
            sqlx::query("INSERT INTO media_items (id, source_id, collection_id, external_id, canonical_url, title, creator_name, creator_id, thumbnail_url, duration_ms, published_at, position, metadata_json, first_seen_at, last_seen_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET source_id = excluded.source_id, collection_id = excluded.collection_id, external_id = excluded.external_id, canonical_url = excluded.canonical_url, title = excluded.title, creator_name = excluded.creator_name, creator_id = excluded.creator_id, thumbnail_url = excluded.thumbnail_url, duration_ms = excluded.duration_ms, published_at = excluded.published_at, position = excluded.position, metadata_json = excluded.metadata_json, last_seen_at = excluded.last_seen_at")
                .bind(&entry.item.id)
                .bind(&entry.item.source_id)
                .bind(&entry.item.collection_id)
                .bind(&entry.item.external_id)
                .bind(&entry.item.canonical_url)
                .bind(&entry.item.title)
                .bind(&entry.item.creator_name)
                .bind(&entry.item.creator_id)
                .bind(&entry.item.thumbnail_url)
                .bind(entry.item.duration_ms)
                .bind(&entry.item.published_at)
                .bind(entry.item.position)
                .bind(metadata_json)
                .bind(&entry.item.first_seen_at)
                .bind(&entry.item.last_seen_at)
                .execute(&mut *transaction)
                .await
                .map_err(|error| storage("analysis_snapshot.item", error))?;

            sqlx::query("DELETE FROM media_formats WHERE media_item_id = ?")
                .bind(&entry.item.id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| storage("analysis_snapshot.formats.delete", error))?;
            for format in &entry.formats {
                insert_format(&mut transaction, format).await?;
            }
        }

        transaction
            .commit()
            .await
            .map_err(|error| storage("analysis_snapshot.commit", error))?;
        Ok(())
    }

    pub async fn create_job_with_event(
        &self,
        job: &DownloadJob,
        event: &JobEvent,
    ) -> RepositoryResult<()> {
        validate_new_job(job)?;
        validate_event_job(job, event)?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| storage("download_job.create.begin", error))?;
        insert_job(&mut transaction, job, "download_job.create.job").await?;
        insert_event(&mut transaction, event, "download_job.create.event").await?;
        transaction
            .commit()
            .await
            .map_err(|error| storage("download_job.create.commit", error))?;
        Ok(())
    }

    pub async fn update_job_with_event(
        &self,
        job: &DownloadJob,
        event: &JobEvent,
    ) -> RepositoryResult<()> {
        DownloadStateMachine::validate_job(job).map_err(invalid_state)?;
        validate_event_job(job, event)?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| storage("download_job.update.begin", error))?;
        let current_status: Option<String> =
            sqlx::query_scalar("SELECT status FROM download_jobs WHERE id = ?")
                .bind(&job.id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|error| storage("download_job.update.current_status", error))?;
        let current_status = current_status.ok_or_else(|| RepositoryError::NotFound {
            entity: "download_job",
            id: job.id.clone(),
        })?;
        let current_status =
            DownloadStatus::try_from(current_status.as_str()).map_err(|error| {
                RepositoryError::InvalidData {
                    details: error.to_string(),
                }
            })?;
        DownloadStateMachine::validate_transition(current_status, job.status.clone())
            .map_err(invalid_state)?;

        let result = sqlx::query("UPDATE download_jobs SET media_item_id = ?, format_id = ?, status = ?, priority = ?, destination_path = ?, temp_path = ?, filename = ?, total_bytes = ?, downloaded_bytes = ?, speed_bytes_per_sec = ?, eta_seconds = ?, retry_count = ?, max_retries = ?, processing_json = ?, etag = ?, last_modified = ?, error_code = ?, error_message = ?, started_at = ?, completed_at = ?, updated_at = ? WHERE id = ?")
            .bind(&job.media_item_id)
            .bind(&job.format_id)
            .bind(job.status.as_str())
            .bind(job.priority)
            .bind(&job.destination_path)
            .bind(&job.temp_path)
            .bind(&job.filename)
            .bind(job.total_bytes)
            .bind(job.downloaded_bytes)
            .bind(job.speed_bytes_per_sec)
            .bind(job.eta_seconds)
            .bind(job.retry_count)
            .bind(job.max_retries)
            .bind(serialize_json("download_jobs.processing_json", &job.processing_json)?)
            .bind(&job.etag)
            .bind(&job.last_modified)
            .bind(&job.error_code)
            .bind(&job.error_message)
            .bind(&job.started_at)
            .bind(&job.completed_at)
            .bind(&job.updated_at)
            .bind(&job.id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| storage("download_job.update.job", error))?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::Conflict {
                details: "download job changed before its transition could be committed".to_owned(),
            });
        }
        insert_event(&mut transaction, event, "download_job.update.event").await?;
        transaction
            .commit()
            .await
            .map_err(|error| storage("download_job.update.commit", error))?;
        Ok(())
    }

    pub async fn recover_job_to_queued(
        &self,
        job: &DownloadJob,
        event: &JobEvent,
    ) -> RepositoryResult<()> {
        if job.status != DownloadStatus::Queued {
            return Err(RepositoryError::InvalidData {
                details: "recovery requeue target must be queued".to_owned(),
            });
        }
        DownloadStateMachine::validate_job(job).map_err(invalid_state)?;
        validate_event_job(job, event)?;
        self.recover_job_update(job, event).await
    }

    pub async fn recover_job_to_failed(
        &self,
        job: &DownloadJob,
        event: &JobEvent,
    ) -> RepositoryResult<()> {
        if job.status != DownloadStatus::Failed {
            return Err(RepositoryError::InvalidData {
                details: "recovery failure target must be failed".to_owned(),
            });
        }
        DownloadStateMachine::validate_job(job).map_err(invalid_state)?;
        validate_event_job(job, event)?;
        self.recover_job_update(job, event).await
    }

    async fn recover_job_update(
        &self,
        job: &DownloadJob,
        event: &JobEvent,
    ) -> RepositoryResult<()> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| storage("download_job.recovery.begin", error))?;
        let current: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT status, media_item_id, format_id FROM download_jobs WHERE id = ?",
        )
        .bind(&job.id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| storage("download_job.recovery.current", error))?;
        let Some((current_status, media_item_id, format_id)) = current else {
            return Err(RepositoryError::NotFound {
                entity: "download_job",
                id: job.id.clone(),
            });
        };
        if media_item_id != job.media_item_id || format_id != job.format_id {
            return Err(RepositoryError::InvalidData {
                details: "recovery cannot change download ownership".to_owned(),
            });
        }
        let allowed_statuses = if job.status == DownloadStatus::Queued {
            &["queued", "resolving", "downloading", "processing", "failed"][..]
        } else {
            &["queued", "resolving", "downloading", "processing"][..]
        };
        if !allowed_statuses.contains(&current_status.as_str()) {
            return Err(RepositoryError::Conflict {
                details: "download job changed before recovery could be committed".to_owned(),
            });
        }
        let query = if job.status == DownloadStatus::Queued {
            "UPDATE download_jobs SET status = ?, priority = ?, destination_path = ?, temp_path = ?, filename = ?, total_bytes = ?, downloaded_bytes = ?, speed_bytes_per_sec = ?, eta_seconds = ?, retry_count = ?, max_retries = ?, processing_json = ?, etag = ?, last_modified = ?, error_code = ?, error_message = ?, started_at = ?, completed_at = ?, updated_at = ? WHERE id = ? AND status IN ('queued', 'resolving', 'downloading', 'processing', 'failed')"
        } else {
            "UPDATE download_jobs SET status = ?, priority = ?, destination_path = ?, temp_path = ?, filename = ?, total_bytes = ?, downloaded_bytes = ?, speed_bytes_per_sec = ?, eta_seconds = ?, retry_count = ?, max_retries = ?, processing_json = ?, etag = ?, last_modified = ?, error_code = ?, error_message = ?, started_at = ?, completed_at = ?, updated_at = ? WHERE id = ? AND status IN ('queued', 'resolving', 'downloading', 'processing')"
        };
        let update = sqlx::query(query)
            .bind(job.status.as_str())
            .bind(job.priority)
            .bind(&job.destination_path)
            .bind(&job.temp_path)
            .bind(&job.filename)
            .bind(job.total_bytes)
            .bind(job.downloaded_bytes)
            .bind(job.speed_bytes_per_sec)
            .bind(job.eta_seconds)
            .bind(job.retry_count)
            .bind(job.max_retries)
            .bind(serialize_json(
                "download_jobs.recovery.processing_json",
                &job.processing_json,
            )?)
            .bind(&job.etag)
            .bind(&job.last_modified)
            .bind(&job.error_code)
            .bind(&job.error_message)
            .bind(&job.started_at)
            .bind(&job.completed_at)
            .bind(&job.updated_at)
            .bind(&job.id);
        let result = update
            .execute(&mut *transaction)
            .await
            .map_err(|error| storage("download_job.recovery.update", error))?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::Conflict {
                details: "download job changed before recovery could be committed".to_owned(),
            });
        }
        insert_event(&mut transaction, event, "download_job.recovery.event").await?;
        transaction
            .commit()
            .await
            .map_err(|error| storage("download_job.recovery.commit", error))?;
        Ok(())
    }

    pub async fn recover_completed_download(
        &self,
        job_id: &str,
        final_path: &str,
        bytes_finalized: i64,
        completed_at: &str,
        event: &JobEvent,
    ) -> RepositoryResult<()> {
        validate_event_job_id(job_id, event)?;
        if final_path.is_empty() || completed_at.is_empty() || bytes_finalized < 0 {
            return Err(RepositoryError::InvalidData {
                details: "recovery completion fields are invalid".to_owned(),
            });
        }
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| storage("download_job.recovery_complete.begin", error))?;
        let current: Option<(String, Option<i64>, String, String)> = sqlx::query_as(
            "SELECT status, total_bytes, destination_path, filename FROM download_jobs WHERE id = ?",
        )
        .bind(job_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| storage("download_job.recovery_complete.current", error))?;
        let Some((current_status, total_bytes, destination_path, filename)) = current else {
            return Err(RepositoryError::NotFound {
                entity: "download_job",
                id: job_id.to_owned(),
            });
        };
        if !["resolving", "downloading", "processing"].contains(&current_status.as_str()) {
            return Err(RepositoryError::Conflict {
                details: "download job is not recoverable as completed".to_owned(),
            });
        }
        if std::path::Path::new(final_path)
            != std::path::Path::new(&destination_path).join(filename)
        {
            return Err(RepositoryError::InvalidData {
                details: "recovery final path does not match the persisted destination".to_owned(),
            });
        }
        if total_bytes.is_some_and(|total| total != bytes_finalized) {
            return Err(RepositoryError::InvalidData {
                details: "recovery final file size does not match the persisted total".to_owned(),
            });
        }
        let result = sqlx::query("UPDATE download_jobs SET status = 'completed', temp_path = NULL, downloaded_bytes = ?, eta_seconds = 0, error_code = NULL, error_message = NULL, completed_at = ?, updated_at = ? WHERE id = ? AND status IN ('resolving', 'downloading', 'processing')")
            .bind(bytes_finalized)
            .bind(completed_at)
            .bind(completed_at)
            .bind(job_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| storage("download_job.recovery_complete.update", error))?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::Conflict {
                details: "download job changed before recovery completion could be committed"
                    .to_owned(),
            });
        }
        insert_event(
            &mut transaction,
            event,
            "download_job.recovery_complete.event",
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| storage("download_job.recovery_complete.commit", error))?;
        Ok(())
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
        DownloadStateMachine::validate_progress(progress.downloaded_bytes, progress.total_bytes)
            .map_err(invalid_state)?;
        validate_event_job_id(&progress.job_id, event)?;
        if progress.speed_bytes_per_sec.is_some_and(|value| value < 0)
            || progress.eta_seconds.is_some_and(|value| value < 0)
        {
            return Err(RepositoryError::InvalidData {
                details: "download progress rate and ETA must not be negative".to_owned(),
            });
        }
        if progress.updated_at.is_empty() {
            return Err(RepositoryError::InvalidData {
                details: "download progress timestamp must not be empty".to_owned(),
            });
        }
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| storage("download_job.progress.begin", error))?;
        let current_status: Option<String> =
            sqlx::query_scalar("SELECT status FROM download_jobs WHERE id = ?")
                .bind(&progress.job_id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|error| storage("download_job.progress.current_status", error))?;
        let current_status = current_status.ok_or_else(|| RepositoryError::NotFound {
            entity: "download_job",
            id: progress.job_id.clone(),
        })?;
        if current_status != DownloadStatus::Downloading.as_str() {
            return Err(RepositoryError::InvalidData {
                details: "durable progress requires a downloading job".to_owned(),
            });
        }
        let result = sqlx::query("UPDATE download_jobs SET total_bytes = ?, downloaded_bytes = ?, speed_bytes_per_sec = ?, eta_seconds = ?, etag = COALESCE(?, etag), last_modified = COALESCE(?, last_modified), updated_at = ? WHERE id = ? AND status = 'downloading'")
            .bind(progress.total_bytes)
            .bind(progress.downloaded_bytes)
            .bind(progress.speed_bytes_per_sec)
            .bind(progress.eta_seconds)
            .bind(etag)
            .bind(last_modified)
            .bind(&progress.updated_at)
            .bind(&progress.job_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| storage("download_job.progress.update", error))?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::Conflict {
                details: "download job changed before progress could be committed".to_owned(),
            });
        }
        insert_event(&mut transaction, event, "download_job.progress.event").await?;
        transaction
            .commit()
            .await
            .map_err(|error| storage("download_job.progress.commit", error))?;
        Ok(())
    }

    pub async fn complete_download_job(
        &self,
        job_id: &str,
        final_path: &str,
        bytes_finalized: i64,
        completed_at: &str,
        event: &JobEvent,
    ) -> RepositoryResult<()> {
        self.complete_download_job_with_validators(
            job_id,
            final_path,
            bytes_finalized,
            completed_at,
            event,
            (None, None),
        )
        .await
    }

    pub async fn complete_download_job_with_validators(
        &self,
        job_id: &str,
        final_path: &str,
        bytes_finalized: i64,
        completed_at: &str,
        event: &JobEvent,
        validators: (Option<&str>, Option<&str>),
    ) -> RepositoryResult<()> {
        validate_event_job_id(job_id, event)?;
        if final_path.is_empty() || completed_at.is_empty() || bytes_finalized < 0 {
            return Err(RepositoryError::InvalidData {
                details: "download completion fields are invalid".to_owned(),
            });
        }
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| storage("download_job.complete.begin", error))?;
        let current: Option<(String, Option<i64>, String, String)> = sqlx::query_as(
            "SELECT status, total_bytes, destination_path, filename FROM download_jobs WHERE id = ?",
        )
                .bind(job_id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|error| storage("download_job.complete.current", error))?;
        let Some((current_status, total_bytes, destination_path, filename)) = current else {
            return Err(RepositoryError::NotFound {
                entity: "download_job",
                id: job_id.to_owned(),
            });
        };
        let current_status =
            DownloadStatus::try_from(current_status.as_str()).map_err(|error| {
                RepositoryError::InvalidData {
                    details: error.to_string(),
                }
            })?;
        DownloadStateMachine::validate_transition(current_status, DownloadStatus::Completed)
            .map_err(invalid_state)?;
        let expected_final_path = std::path::Path::new(&destination_path).join(filename);
        if expected_final_path != std::path::Path::new(final_path) {
            return Err(RepositoryError::InvalidData {
                details: "finalized path does not match the persisted destination".to_owned(),
            });
        }
        DownloadStateMachine::validate_progress(bytes_finalized, total_bytes)
            .map_err(invalid_state)?;
        if total_bytes.is_some_and(|total| bytes_finalized != total) {
            return Err(RepositoryError::InvalidData {
                details: "completed download bytes must equal the known total".to_owned(),
            });
        }
        let result = sqlx::query("UPDATE download_jobs SET status = 'completed', temp_path = NULL, downloaded_bytes = ?, eta_seconds = 0, error_code = NULL, error_message = NULL, etag = COALESCE(?, etag), last_modified = COALESCE(?, last_modified), completed_at = ?, updated_at = ? WHERE id = ? AND status IN ('downloading', 'processing')")
            .bind(bytes_finalized)
            .bind(validators.0)
            .bind(validators.1)
            .bind(completed_at)
            .bind(completed_at)
            .bind(job_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| storage("download_job.complete.update", error))?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::Conflict {
                details: "download job changed before completion could be committed".to_owned(),
            });
        }
        insert_event(&mut transaction, event, "download_job.complete.event").await?;
        transaction
            .commit()
            .await
            .map_err(|error| storage("download_job.complete.commit", error))?;
        Ok(())
    }

    pub async fn claim_next_queued_job(
        &self,
        event_id: &str,
        event_type: &str,
        payload_json: &Option<serde_json::Value>,
        created_at: &str,
    ) -> RepositoryResult<Option<String>> {
        if event_id.is_empty() || event_type.is_empty() || created_at.is_empty() {
            return Err(RepositoryError::InvalidData {
                details: "queue claim event fields must not be empty".to_owned(),
            });
        }
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| storage("download_job.claim.begin", error))?;
        let candidate: Option<String> = sqlx::query_scalar("UPDATE download_jobs SET status = 'resolving', updated_at = ? WHERE id = (SELECT id FROM download_jobs WHERE status = 'queued' ORDER BY priority DESC, created_at, id LIMIT 1) AND status = 'queued' RETURNING id")
            .bind(created_at)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| storage("download_job.claim.update", error))?;
        let Some(job_id) = candidate else {
            transaction
                .commit()
                .await
                .map_err(|error| storage("download_job.claim.empty_commit", error))?;
            return Ok(None);
        };

        let payload_json = serialize_json("download_job.claim.payload", payload_json)?;
        sqlx::query("INSERT INTO job_events (id, job_id, event_type, payload_json, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(event_id)
            .bind(&job_id)
            .bind(event_type)
            .bind(payload_json)
            .bind(created_at)
            .execute(&mut *transaction)
            .await
            .map_err(|error| storage("download_job.claim.event", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| storage("download_job.claim.commit", error))?;
        Ok(Some(job_id))
    }
}

fn validate_new_job(job: &DownloadJob) -> RepositoryResult<()> {
    DownloadStateMachine::validate_new_job(job).map_err(invalid_state)
}

fn invalid_state(error: DownloadStateError) -> RepositoryError {
    RepositoryError::InvalidData {
        details: error.to_string(),
    }
}

fn validate_event_job_id(job_id: &str, event: &JobEvent) -> RepositoryResult<()> {
    if job_id.is_empty() || event.job_id != job_id {
        return Err(RepositoryError::InvalidData {
            details: "job event must reference the same non-empty job".to_owned(),
        });
    }
    if event.id.is_empty() || event.event_type.is_empty() || event.created_at.is_empty() {
        return Err(RepositoryError::InvalidData {
            details: "job event fields must not be empty".to_owned(),
        });
    }
    Ok(())
}

fn validate_event_job(job: &DownloadJob, event: &JobEvent) -> RepositoryResult<()> {
    if event.job_id != job.id {
        return Err(RepositoryError::InvalidData {
            details: "job event must reference the same job".to_owned(),
        });
    }
    if event.id.is_empty() || event.event_type.is_empty() || event.created_at.is_empty() {
        return Err(RepositoryError::InvalidData {
            details: "job event fields must not be empty".to_owned(),
        });
    }
    Ok(())
}

async fn insert_job(
    transaction: &mut Transaction<'_, Sqlite>,
    job: &DownloadJob,
    operation: &'static str,
) -> RepositoryResult<()> {
    sqlx::query("INSERT INTO download_jobs (id, media_item_id, format_id, status, priority, destination_path, temp_path, filename, total_bytes, downloaded_bytes, speed_bytes_per_sec, eta_seconds, retry_count, max_retries, processing_json, etag, last_modified, error_code, error_message, started_at, completed_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&job.id)
        .bind(&job.media_item_id)
        .bind(&job.format_id)
        .bind(job.status.as_str())
        .bind(job.priority)
        .bind(&job.destination_path)
        .bind(&job.temp_path)
        .bind(&job.filename)
        .bind(job.total_bytes)
        .bind(job.downloaded_bytes)
        .bind(job.speed_bytes_per_sec)
        .bind(job.eta_seconds)
        .bind(job.retry_count)
        .bind(job.max_retries)
        .bind(serialize_json("download_jobs.processing_json", &job.processing_json)?)
        .bind(&job.etag)
        .bind(&job.last_modified)
        .bind(&job.error_code)
        .bind(&job.error_message)
        .bind(&job.started_at)
        .bind(&job.completed_at)
        .bind(&job.created_at)
        .bind(&job.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(|error| storage(operation, error))?;
    Ok(())
}

async fn insert_event(
    transaction: &mut Transaction<'_, Sqlite>,
    event: &JobEvent,
    operation: &'static str,
) -> RepositoryResult<()> {
    let payload_json = serialize_json("job_events.payload_json", &event.payload_json)?;
    sqlx::query("INSERT INTO job_events (id, job_id, event_type, payload_json, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(&event.id)
        .bind(&event.job_id)
        .bind(&event.event_type)
        .bind(payload_json)
        .bind(&event.created_at)
        .execute(&mut **transaction)
        .await
        .map_err(|error| storage(operation, error))?;
    Ok(())
}

async fn insert_format(
    transaction: &mut Transaction<'_, Sqlite>,
    format: &MediaFormat,
) -> RepositoryResult<()> {
    let metadata_json = serialize_json("media_formats.metadata_json", &format.metadata_json)?;
    sqlx::query("INSERT INTO media_formats (id, media_item_id, external_format_id, container, video_codec, audio_codec, width, height, fps, bitrate, sample_rate, channels, file_size_bytes, is_video, is_audio, is_progressive, metadata_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&format.id)
        .bind(&format.media_item_id)
        .bind(&format.external_format_id)
        .bind(&format.container)
        .bind(&format.video_codec)
        .bind(&format.audio_codec)
        .bind(format.width)
        .bind(format.height)
        .bind(format.fps)
        .bind(format.bitrate)
        .bind(format.sample_rate)
        .bind(format.channels)
        .bind(format.file_size_bytes)
        .bind(i64::from(format.is_video))
        .bind(i64::from(format.is_audio))
        .bind(i64::from(format.is_progressive))
        .bind(metadata_json)
        .bind(&format.created_at)
        .execute(&mut **transaction)
        .await
        .map_err(|error| storage("analysis_snapshot.formats.insert", error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::SqliteTransactionCoordinator;
    use crate::application::services::{AnalysisSnapshot, SnapshotItem};
    use crate::domain::entities::{
        Collection, DownloadJob, DownloadStatus, JobEvent, MediaFormat, MediaItem, MediaSource,
        Platform, SourceType,
    };
    use crate::downloader::DownloadProgress;
    use serde_json::json;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::SqlitePool;

    async fn test_pool() -> SqlitePool {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .in_memory(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .connect_with(options)
            .await
            .expect("pool should initialize");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations should apply");
        pool
    }

    fn snapshot(duplicate_format: bool) -> AnalysisSnapshot {
        let format = MediaFormat {
            id: "format-1".to_owned(),
            media_item_id: "item-1".to_owned(),
            external_format_id: Some("external-format-1".to_owned()),
            container: Some("mp4".to_owned()),
            video_codec: Some("h264".to_owned()),
            audio_codec: Some("aac".to_owned()),
            width: Some(1920),
            height: Some(1080),
            fps: Some(30.0),
            bitrate: Some(1_000_000),
            sample_rate: Some(48_000),
            channels: Some(2),
            file_size_bytes: Some(1_000),
            is_video: true,
            is_audio: true,
            is_progressive: true,
            metadata_json: Some(json!({"source": "test"})),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
        };
        AnalysisSnapshot {
            platform: Platform {
                id: "platform-1".to_owned(),
                slug: "generic".to_owned(),
                name: "Generic".to_owned(),
                enabled: true,
                adapter_version: Some("0.1.0".to_owned()),
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
                creator_name: Some("Creator".to_owned()),
                creator_id: None,
                thumbnail_url: None,
                item_count: Some(1),
                discovered_at: "2026-01-01T00:00:00Z".to_owned(),
                last_analyzed_at: Some("2026-01-01T00:00:00Z".to_owned()),
                metadata_json: Some(json!({"analyzed": true})),
            },
            collections: vec![Collection {
                id: "collection-1".to_owned(),
                source_id: "source-1".to_owned(),
                external_id: Some("collection-external-1".to_owned()),
                title: Some("Collection".to_owned()),
                creator_name: Some("Creator".to_owned()),
                item_count: Some(1),
                created_at: "2026-01-01T00:00:00Z".to_owned(),
                updated_at: "2026-01-01T00:00:00Z".to_owned(),
            }],
            items: vec![SnapshotItem {
                item: MediaItem {
                    id: "item-1".to_owned(),
                    source_id: "source-1".to_owned(),
                    collection_id: Some("collection-1".to_owned()),
                    external_id: Some("item-external-1".to_owned()),
                    canonical_url: "https://example.test/item".to_owned(),
                    title: "Item".to_owned(),
                    creator_name: Some("Creator".to_owned()),
                    creator_id: None,
                    thumbnail_url: None,
                    duration_ms: Some(1_000),
                    published_at: Some("2026-01-01T00:00:00Z".to_owned()),
                    position: Some(0),
                    metadata_json: None,
                    first_seen_at: "2026-01-01T00:00:00Z".to_owned(),
                    last_seen_at: "2026-01-01T00:00:00Z".to_owned(),
                },
                formats: if duplicate_format {
                    vec![format.clone(), format]
                } else {
                    vec![format]
                },
            }],
        }
    }

    type CompletedRow = (
        String,
        String,
        Option<String>,
        i64,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
    );

    fn job(id: &str) -> DownloadJob {
        DownloadJob {
            id: id.to_owned(),
            media_item_id: "item-1".to_owned(),
            format_id: Some("format-1".to_owned()),
            status: DownloadStatus::Queued,
            priority: 0,
            destination_path: "/tmp/umd".to_owned(),
            temp_path: Some("/tmp/umd/item.part".to_owned()),
            filename: "item.mp4".to_owned(),
            total_bytes: Some(1_000),
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

    fn event(id: &str, job_id: &str) -> JobEvent {
        JobEvent {
            id: id.to_owned(),
            job_id: job_id.to_owned(),
            event_type: "queued".to_owned(),
            payload_json: Some(json!({"reason": "test"})),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[tokio::test]
    async fn analysis_snapshot_commits_all_related_rows() {
        let pool = test_pool().await;
        let coordinator = SqliteTransactionCoordinator::new(&pool);
        coordinator
            .save_analysis_snapshot(&snapshot(false))
            .await
            .expect("valid snapshot should commit");

        let counts: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM platforms), (SELECT COUNT(*) FROM media_sources), (SELECT COUNT(*) FROM media_items), (SELECT COUNT(*) FROM media_formats)",
        )
        .fetch_one(&pool)
        .await
        .expect("count query should succeed");
        assert_eq!(counts, (1, 1, 1, 1));
    }

    #[tokio::test]
    async fn invalid_snapshot_rolls_back_platform_source_item_and_formats() {
        let pool = test_pool().await;
        let coordinator = SqliteTransactionCoordinator::new(&pool);
        assert!(coordinator
            .save_analysis_snapshot(&snapshot(true))
            .await
            .is_err());

        let counts: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM platforms), (SELECT COUNT(*) FROM media_sources), (SELECT COUNT(*) FROM media_items), (SELECT COUNT(*) FROM media_formats)",
        )
        .fetch_one(&pool)
        .await
        .expect("count query should succeed");
        assert_eq!(counts, (0, 0, 0, 0));
    }

    #[tokio::test]
    async fn rejects_illegal_job_transition_before_writing() {
        let pool = test_pool().await;
        let coordinator = SqliteTransactionCoordinator::new(&pool);
        coordinator
            .save_analysis_snapshot(&snapshot(false))
            .await
            .expect("snapshot should commit");
        coordinator
            .create_job_with_event(&job("job-1"), &event("event-1", "job-1"))
            .await
            .expect("job should commit");

        let mut invalid = job("job-1");
        invalid.status = DownloadStatus::Downloading;
        let error = coordinator
            .update_job_with_event(&invalid, &event("event-2", "job-1"))
            .await
            .expect_err("queued jobs must resolve before downloading");
        assert!(matches!(
            error,
            crate::application::ports::RepositoryError::InvalidData { .. }
        ));
        let status: String =
            sqlx::query_scalar("SELECT status FROM download_jobs WHERE id = 'job-1'")
                .fetch_one(&pool)
                .await
                .expect("status query should succeed");
        assert_eq!(status, "queued");
    }

    #[tokio::test]
    async fn queue_claim_selects_priority_order_and_records_resolving_events() {
        let pool = test_pool().await;
        let coordinator = SqliteTransactionCoordinator::new(&pool);
        coordinator
            .save_analysis_snapshot(&snapshot(false))
            .await
            .expect("snapshot should commit");

        let mut first = job("job-1");
        first.created_at = "2026-01-01T00:00:00Z".to_owned();
        coordinator
            .create_job_with_event(&first, &event("event-1", "job-1"))
            .await
            .expect("first job should commit");
        let mut second = job("job-2");
        second.priority = 5;
        second.created_at = "2026-01-01T00:01:00Z".to_owned();
        coordinator
            .create_job_with_event(&second, &event("event-2", "job-2"))
            .await
            .expect("second job should commit");

        assert_eq!(
            coordinator
                .claim_next_queued_job(
                    "claim-event-1",
                    "resolving",
                    &Some(json!({"worker": "test-1"})),
                    "2026-01-01T00:02:00Z",
                )
                .await
                .expect("first claim should succeed"),
            Some("job-2".to_owned())
        );
        assert_eq!(
            coordinator
                .claim_next_queued_job("claim-event-2", "resolving", &None, "2026-01-01T00:03:00Z",)
                .await
                .expect("second claim should succeed"),
            Some("job-1".to_owned())
        );
        assert_eq!(
            coordinator
                .claim_next_queued_job("claim-event-3", "resolving", &None, "2026-01-01T00:04:00Z",)
                .await
                .expect("empty claim should succeed"),
            None
        );

        let statuses: Vec<(String, String)> =
            sqlx::query_as("SELECT id, status FROM download_jobs ORDER BY id")
                .fetch_all(&pool)
                .await
                .expect("status query should succeed");
        assert_eq!(
            statuses,
            vec![
                ("job-1".to_owned(), "resolving".to_owned()),
                ("job-2".to_owned(), "resolving".to_owned())
            ]
        );
        let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM job_events")
            .fetch_one(&pool)
            .await
            .expect("event count query should succeed");
        assert_eq!(event_count, 4);
    }

    #[tokio::test]
    async fn concurrent_claimers_claim_one_job_only_once() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let database = crate::persistence::Database::from_app_data_dir(directory.path())
            .await
            .expect("database should initialize");
        let coordinator = SqliteTransactionCoordinator::new(database.pool());
        coordinator
            .save_analysis_snapshot(&snapshot(false))
            .await
            .expect("snapshot should commit");
        coordinator
            .create_job_with_event(&job("job-1"), &event("event-1", "job-1"))
            .await
            .expect("job should commit");

        let (first, second) = tokio::join!(
            coordinator.claim_next_queued_job(
                "claim-event-1",
                "resolving",
                &None,
                "2026-01-01T00:02:00Z",
            ),
            coordinator.claim_next_queued_job(
                "claim-event-2",
                "resolving",
                &None,
                "2026-01-01T00:02:01Z",
            )
        );
        let first = first.expect("first concurrent claim should not fail");
        let second = second.expect("second concurrent claim should not fail");
        assert_eq!(
            [first, second].into_iter().flatten().collect::<Vec<_>>(),
            vec!["job-1".to_owned()]
        );
        let event_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM job_events WHERE job_id = 'job-1'")
                .fetch_one(database.pool())
                .await
                .expect("event count query should succeed");
        assert_eq!(event_count, 2);
    }

    #[tokio::test]
    async fn progress_and_completion_commit_job_fields_and_events_atomically() {
        let pool = test_pool().await;
        let coordinator = SqliteTransactionCoordinator::new(&pool);
        coordinator
            .save_analysis_snapshot(&snapshot(false))
            .await
            .expect("snapshot should commit");
        let mut queued = job("job-1");
        queued.total_bytes = Some(5);
        coordinator
            .create_job_with_event(&queued, &event("event-1", "job-1"))
            .await
            .expect("job should commit");

        let mut resolving = queued.clone();
        resolving.status = DownloadStatus::Resolving;
        coordinator
            .update_job_with_event(&resolving, &event("event-2", "job-1"))
            .await
            .expect("resolving transition should commit");
        let mut downloading = resolving;
        downloading.status = DownloadStatus::Downloading;
        coordinator
            .update_job_with_event(&downloading, &event("event-3", "job-1"))
            .await
            .expect("downloading transition should commit");

        coordinator
            .persist_download_progress_with_validators(
                &DownloadProgress {
                    job_id: "job-1".to_owned(),
                    downloaded_bytes: 5,
                    total_bytes: Some(5),
                    speed_bytes_per_sec: Some(100),
                    eta_seconds: Some(0),
                    updated_at: "2026-01-01T00:01:00Z".to_owned(),
                },
                &event("event-4", "job-1"),
                Some("\"etag-1\""),
                Some("Wed, 21 Oct 2015 07:28:00 GMT"),
            )
            .await
            .expect("progress should commit");
        let progress: (
            String,
            i64,
            Option<i64>,
            Option<i64>,
            Option<String>,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT status, downloaded_bytes, speed_bytes_per_sec, eta_seconds, etag, last_modified FROM download_jobs WHERE id = 'job-1'",
        )
        .fetch_one(&pool)
        .await
        .expect("progress query should succeed");
        assert_eq!(
            progress,
            (
                "downloading".to_owned(),
                5,
                Some(100),
                Some(0),
                Some("\"etag-1\"".to_owned()),
                Some("Wed, 21 Oct 2015 07:28:00 GMT".to_owned())
            )
        );

        coordinator
            .complete_download_job_with_validators(
                "job-1",
                "/tmp/umd/item.mp4",
                5,
                "2026-01-01T00:01:01Z",
                &event("event-5", "job-1"),
                (Some("\"etag-2\""), Some("Wed, 22 Oct 2015 07:28:00 GMT")),
            )
            .await
            .expect("completion should commit");
        let completed: CompletedRow = sqlx::query_as(
            "SELECT status, destination_path, temp_path, downloaded_bytes, eta_seconds, completed_at, etag, last_modified FROM download_jobs WHERE id = 'job-1'",
        )
        .fetch_one(&pool)
        .await
        .expect("completion query should succeed");
        assert_eq!(
            completed,
            (
                "completed".to_owned(),
                "/tmp/umd".to_owned(),
                None,
                5,
                0,
                Some("2026-01-01T00:01:01Z".to_owned()),
                Some("\"etag-2\"".to_owned()),
                Some("Wed, 22 Oct 2015 07:28:00 GMT".to_owned())
            )
        );
        let event_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM job_events WHERE job_id = 'job-1'")
                .fetch_one(&pool)
                .await
                .expect("event count query should succeed");
        assert_eq!(event_count, 5);
    }

    #[tokio::test]
    async fn processing_transition_and_completion_commit_processing_state_atomically() {
        let pool = test_pool().await;
        let coordinator = SqliteTransactionCoordinator::new(&pool);
        coordinator
            .save_analysis_snapshot(&snapshot(false))
            .await
            .expect("snapshot should commit");
        coordinator
            .create_job_with_event(&job("job-1"), &event("event-1", "job-1"))
            .await
            .expect("job should commit");
        let mut resolving = job("job-1");
        resolving.status = DownloadStatus::Resolving;
        coordinator
            .update_job_with_event(&resolving, &event("event-2", "job-1"))
            .await
            .expect("resolving transition should commit");
        let mut downloading = resolving;
        downloading.status = DownloadStatus::Downloading;
        downloading.total_bytes = Some(5);
        coordinator
            .update_job_with_event(&downloading, &event("event-3", "job-1"))
            .await
            .expect("downloading transition should commit");
        let mut processing = downloading;
        processing.status = DownloadStatus::Processing;
        processing.temp_path = Some("/tmp/umd/item.mp4.processing.part".to_owned());
        processing.total_bytes = None;
        processing.downloaded_bytes = 0;
        processing.processing_json = Some(json!({
            "operation": "extract_audio",
            "input": "/tmp/umd/item.mp4.part",
            "output_filename": "item.mp4"
        }));
        coordinator
            .update_job_with_event(&processing, &event("event-4", "job-1"))
            .await
            .expect("processing transition should commit");
        let persisted: (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT status, temp_path, processing_json FROM download_jobs WHERE id = 'job-1'",
        )
        .fetch_one(&pool)
        .await
        .expect("processing row should be readable");
        assert_eq!(persisted.0, "processing");
        assert_eq!(
            persisted.1,
            Some("/tmp/umd/item.mp4.processing.part".to_owned())
        );
        assert!(persisted.2.unwrap().contains("extract_audio"));
        coordinator
            .complete_download_job(
                "job-1",
                "/tmp/umd/item.mp4",
                5,
                "2026-01-01T00:01:01Z",
                &event("event-5", "job-1"),
            )
            .await
            .expect("processing job should complete");
        let status: String =
            sqlx::query_scalar("SELECT status FROM download_jobs WHERE id = 'job-1'")
                .fetch_one(&pool)
                .await
                .expect("status should be readable");
        assert_eq!(status, "completed");
    }

    #[tokio::test]
    async fn progress_and_completion_reject_invalid_job_states_without_events() {
        let pool = test_pool().await;
        let coordinator = SqliteTransactionCoordinator::new(&pool);
        coordinator
            .save_analysis_snapshot(&snapshot(false))
            .await
            .expect("snapshot should commit");
        coordinator
            .create_job_with_event(&job("job-1"), &event("event-1", "job-1"))
            .await
            .expect("job should commit");

        let progress_error = coordinator
            .persist_download_progress(
                &DownloadProgress {
                    job_id: "job-1".to_owned(),
                    downloaded_bytes: 1,
                    total_bytes: Some(1),
                    speed_bytes_per_sec: None,
                    eta_seconds: None,
                    updated_at: "2026-01-01T00:01:00Z".to_owned(),
                },
                &event("event-2", "job-1"),
            )
            .await
            .expect_err("queued jobs must not accept progress");
        assert!(matches!(
            progress_error,
            crate::application::ports::RepositoryError::InvalidData { .. }
        ));
        let completion_error = coordinator
            .complete_download_job(
                "job-1",
                "/downloads/video.mp4",
                1,
                "2026-01-01T00:01:01Z",
                &event("event-3", "job-1"),
            )
            .await
            .expect_err("queued jobs must not complete");
        assert!(matches!(
            completion_error,
            crate::application::ports::RepositoryError::InvalidData { .. }
        ));
        let event_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM job_events WHERE job_id = 'job-1'")
                .fetch_one(&pool)
                .await
                .expect("event count query should succeed");
        assert_eq!(event_count, 1);
    }

    #[tokio::test]
    async fn job_and_event_commit_together_and_roll_back_together() {
        let pool = test_pool().await;
        let coordinator = SqliteTransactionCoordinator::new(&pool);
        coordinator
            .save_analysis_snapshot(&snapshot(false))
            .await
            .expect("snapshot should commit");

        coordinator
            .create_job_with_event(&job("job-1"), &event("event-1", "job-1"))
            .await
            .expect("job and event should commit");
        let committed: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM download_jobs WHERE id = 'job-1'), (SELECT COUNT(*) FROM job_events WHERE id = 'event-1')",
        )
        .fetch_one(&pool)
        .await
        .expect("count query should succeed");
        assert_eq!(committed, (1, 1));

        let mut resolving_job = job("job-1");
        resolving_job.status = DownloadStatus::Resolving;
        resolving_job.updated_at = "2026-01-01T00:00:30Z".to_owned();
        coordinator
            .update_job_with_event(&resolving_job, &event("event-2", "job-1"))
            .await
            .expect("queued to resolving transition should commit");

        let mut updated_job = resolving_job;
        updated_job.status = DownloadStatus::Downloading;
        updated_job.downloaded_bytes = 250;
        updated_job.updated_at = "2026-01-01T00:01:00Z".to_owned();
        coordinator
            .update_job_with_event(&updated_job, &event("event-3", "job-1"))
            .await
            .expect("resolving to downloading transition should commit");
        let updated_status: String =
            sqlx::query_scalar("SELECT status FROM download_jobs WHERE id = 'job-1'")
                .fetch_one(&pool)
                .await
                .expect("status query should succeed");
        let event_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM job_events WHERE job_id = 'job-1'")
                .fetch_one(&pool)
                .await
                .expect("event count query should succeed");
        assert_eq!(updated_status, "downloading");
        assert_eq!(event_count, 3);

        assert!(coordinator
            .create_job_with_event(&job("job-2"), &event("event-1", "job-2"))
            .await
            .is_err());
        let rolled_back: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM download_jobs WHERE id = 'job-2'")
                .fetch_one(&pool)
                .await
                .expect("rollback query should succeed");
        assert_eq!(rolled_back, 0);
    }
}
