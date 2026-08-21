use crate::application::ports::RepositoryError;
use crate::application::services::AppServices;
use crate::application::settings_service::{SettingKey, SettingValue, SettingsError};
use crate::domain::entities::{DownloadJob, DownloadStatus, JobEvent};
use crate::domain::errors::{AppError, ErrorCode};
use crate::downloader::{ApplicationJobExecutor, BandwidthSnapshot, DownloadWorkerPool};
use crate::media::MediaProcessingConfiguration;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{Emitter, State, Window};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

static JOB_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Deserialize)]
pub struct CreateDownloadRequest {
    pub media_item_id: String,
    pub format_id: String,
    pub destination_path: String,
    pub filename: String,
    pub processing: Option<MediaProcessingConfiguration>,
}

fn now_utc() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

#[derive(Debug, Clone, Serialize)]
pub struct BandwidthStatus {
    pub limit_kbps: Option<u32>,
    pub current_kbps: u64,
    pub total_bytes: u64,
}

fn bandwidth_status(snapshot: BandwidthSnapshot) -> BandwidthStatus {
    BandwidthStatus {
        limit_kbps: snapshot
            .limit_bytes_per_sec
            .map(|value| (value / 1024).try_into().unwrap_or(u32::MAX)),
        current_kbps: snapshot.current_bytes_per_sec / 1024,
        total_bytes: snapshot.total_bytes,
    }
}

fn settings_error(error: SettingsError) -> AppError {
    AppError {
        code: ErrorCode::DatabaseUnavailable,
        message: "The bandwidth setting could not be saved.".to_owned(),
        retryable: true,
        user_action: Some("Try the bandwidth setting again.".to_owned()),
        diagnostic: Some(error.to_string()),
    }
}

fn invalid_request(message: &str) -> AppError {
    AppError {
        code: ErrorCode::UnknownError,
        message: message.to_owned(),
        retryable: false,
        user_action: Some(
            "Review the selected media, format, and destination, then try again.".to_owned(),
        ),
        diagnostic: None,
    }
}

fn repository_error(error: RepositoryError) -> AppError {
    let (code, message) = match error {
        RepositoryError::NotFound {
            entity: "media_item",
            ..
        } => (
            ErrorCode::MediaUnavailable,
            "The analyzed media item is no longer available.",
        ),
        RepositoryError::NotFound {
            entity: "media_format",
            ..
        } => (
            ErrorCode::FormatUnavailable,
            "The selected media format is no longer available.",
        ),
        RepositoryError::Storage { .. } => (
            ErrorCode::DatabaseUnavailable,
            "The download queue could not be saved.",
        ),
        RepositoryError::Conflict { .. } => (
            ErrorCode::DatabaseUnavailable,
            "The download queue changed concurrently; please retry.",
        ),
        RepositoryError::InvalidData { .. } | RepositoryError::NotFound { .. } => (
            ErrorCode::UnknownError,
            "The download request was rejected by the application.",
        ),
    };
    AppError {
        code,
        message: message.to_owned(),
        retryable: matches!(code, ErrorCode::DatabaseUnavailable),
        user_action: Some("Review the request and try again.".to_owned()),
        diagnostic: None,
    }
}

#[tauri::command]
pub fn get_bandwidth_status(
    pool: State<'_, DownloadWorkerPool<ApplicationJobExecutor>>,
) -> Result<BandwidthStatus, AppError> {
    Ok(bandwidth_status(pool.bandwidth_snapshot()))
}

#[tauri::command]
pub async fn set_bandwidth_limit(
    services: State<'_, AppServices>,
    pool: State<'_, DownloadWorkerPool<ApplicationJobExecutor>>,
    limit_kbps: u32,
) -> Result<BandwidthStatus, AppError> {
    services
        .settings
        .set(
            SettingKey::BandwidthLimitKbps,
            SettingValue::BandwidthLimitKbps(limit_kbps),
        )
        .await
        .map_err(settings_error)?;
    pool.set_bandwidth_limit_kbps(limit_kbps);
    Ok(bandwidth_status(pool.bandwidth_snapshot()))
}

#[tauri::command]
pub async fn create_download(
    services: State<'_, AppServices>,
    pool: State<'_, DownloadWorkerPool<ApplicationJobExecutor>>,
    request: CreateDownloadRequest,
) -> Result<DownloadJob, AppError> {
    if request.media_item_id.trim().is_empty()
        || request.format_id.trim().is_empty()
        || request.destination_path.trim().is_empty()
        || request.filename.trim().is_empty()
    {
        return Err(invalid_request(
            "Required download fields must not be empty.",
        ));
    }

    let plan = services
        .resolve_download_plan(
            &request.media_item_id,
            &request.format_id,
            Path::new(&request.destination_path),
            &request.filename,
        )
        .await
        .map_err(repository_error)?;
    if let Some(processing) = request.processing.as_ref() {
        let processing_filename = match processing {
            MediaProcessingConfiguration::MergeAudioVideo {
                output_filename, ..
            }
            | MediaProcessingConfiguration::ExtractAudio {
                output_filename, ..
            } => output_filename,
        };
        if processing_filename != &plan.destination.filename {
            return Err(invalid_request(
                "Processing output filename must match the download filename.",
            ));
        }
    }
    let processing_json = request
        .processing
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|_| invalid_request("Processing configuration could not be serialized."))?;
    let now = now_utc();
    let sequence = JOB_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let job_id = format!("download-{sequence}-{}", now.replace([':', '-', '.'], ""));
    let job = DownloadJob {
        id: job_id.clone(),
        media_item_id: request.media_item_id,
        format_id: Some(request.format_id),
        status: DownloadStatus::Queued,
        priority: 0,
        destination_path: plan.destination.root.to_string_lossy().to_string(),
        temp_path: Some(plan.destination.temporary.to_string_lossy().to_string()),
        filename: plan.destination.filename,
        total_bytes: plan.total_bytes,
        downloaded_bytes: 0,
        speed_bytes_per_sec: None,
        eta_seconds: None,
        retry_count: 0,
        max_retries: 3,
        processing_json,
        etag: None,
        last_modified: None,
        error_code: None,
        error_message: None,
        started_at: None,
        completed_at: None,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let event = JobEvent {
        id: format!("{job_id}-queued"),
        job_id: job_id.clone(),
        event_type: "queued".to_owned(),
        payload_json: Some(serde_json::json!({"source": "create_download"})),
        created_at: now,
    };
    services
        .create_download_job(&job, &event)
        .await
        .map_err(repository_error)?;

    let worker_pool = pool.inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = worker_pool.run_until_idle().await {
            tracing::error!(event = "download_worker_pool_failed", error = %error);
        }
    });
    Ok(job)
}

#[tauri::command]
pub fn cancel_download(
    pool: State<'_, DownloadWorkerPool<ApplicationJobExecutor>>,
    job_id: String,
) -> Result<bool, AppError> {
    if job_id.trim().is_empty() {
        return Err(invalid_request("A download job ID is required."));
    }
    Ok(pool.cancel_job(&job_id))
}

#[tauri::command]
pub async fn get_download_jobs(
    services: State<'_, AppServices>,
) -> Result<Vec<DownloadJob>, AppError> {
    services
        .list_download_jobs()
        .await
        .map_err(repository_error)
}

#[tauri::command]
pub fn subscribe_download_progress(
    window: Window,
    pool: State<'_, DownloadWorkerPool<ApplicationJobExecutor>>,
) -> Result<bool, AppError> {
    let mut receiver = pool.subscribe_progress();
    tauri::async_runtime::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if window.emit("download-progress", event).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::CreateDownloadRequest;

    #[test]
    fn request_shape_contains_destination_selection_fields() {
        let request = CreateDownloadRequest {
            media_item_id: "item-1".to_owned(),
            format_id: "format-1".to_owned(),
            destination_path: "/downloads".to_owned(),
            filename: "video.mp4".to_owned(),
            processing: None,
        };
        assert_eq!(request.destination_path, "/downloads");
        assert_eq!(request.filename, "video.mp4");
    }
}
