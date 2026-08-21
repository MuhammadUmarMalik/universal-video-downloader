use super::{
    finalize_part, CancellationRegistry, CancellationToken, DownloadProgress, FinalizationResult,
    LiveProgressEvent, ProgressBroadcaster, RetryDecision, RetryPolicy, StreamProgress,
    StreamingEngine, StreamingError,
};
use crate::application::services::AppServices;
use crate::domain::entities::{DownloadJob, DownloadStatus, JobEvent};
use crate::domain::errors::ErrorCode;
use crate::media::{MediaProcessingConfiguration, MediaProcessingPlan, MediaProcessor};
use async_trait::async_trait;
use serde_json::json;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::Semaphore;
use tokio::task::JoinError;
use tokio::time::{sleep, Duration};

const MAX_WORKER_COUNT: usize = 8;
const DEFAULT_WORKER_COUNT: usize = 3;

#[derive(Debug, Error)]
pub enum WorkerPoolError {
    #[error("worker concurrency must be between 1 and 8")]
    InvalidConcurrency,
    #[error("worker settings could not be loaded: {0}")]
    Settings(String),
    #[error("queue claim failed: {0}")]
    Queue(String),
    #[error("worker task failed to join: {0}")]
    Join(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkerPoolReport {
    pub claimed: usize,
    pub completed: usize,
    pub failed: usize,
    pub retried: usize,
    pub execution_errors: usize,
}

impl WorkerPoolReport {
    fn add(&mut self, other: Self) {
        self.claimed += other.claimed;
        self.completed += other.completed;
        self.failed += other.failed;
        self.retried += other.retried;
        self.execution_errors += other.execution_errors;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobExecutionOutcome {
    Completed,
    Failed,
    RetryScheduled,
}

#[derive(Debug, Clone)]
pub struct EventIdSource {
    sequence: Arc<AtomicU64>,
}

impl Default for EventIdSource {
    fn default() -> Self {
        Self {
            sequence: Arc::new(AtomicU64::new(1)),
        }
    }
}

impl EventIdSource {
    fn next(&self, worker_id: usize, phase: &str) -> String {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        format!("worker-{worker_id}-{sequence}-{phase}")
    }
}

#[async_trait]
pub trait ClaimedJobExecutor: Send + Sync + 'static {
    async fn execute(
        &self,
        job: DownloadJob,
        worker_id: usize,
        events: EventIdSource,
        cancellation: CancellationToken,
        progress: ProgressBroadcaster,
    ) -> Result<JobExecutionOutcome, String>;
}

pub struct DownloadWorkerPool<E> {
    services: Arc<AppServices>,
    executor: Arc<E>,
    concurrency: usize,
    events: EventIdSource,
    shutdown_requested: Arc<AtomicBool>,
    cancellations: CancellationRegistry,
    progress: ProgressBroadcaster,
}

impl<E> Clone for DownloadWorkerPool<E> {
    fn clone(&self) -> Self {
        Self {
            services: Arc::clone(&self.services),
            executor: Arc::clone(&self.executor),
            concurrency: self.concurrency,
            events: self.events.clone(),
            shutdown_requested: Arc::clone(&self.shutdown_requested),
            cancellations: self.cancellations.clone(),
            progress: self.progress.clone(),
        }
    }
}

impl<E> DownloadWorkerPool<E>
where
    E: ClaimedJobExecutor,
{
    pub fn new(
        services: Arc<AppServices>,
        executor: Arc<E>,
        concurrency: usize,
    ) -> Result<Self, WorkerPoolError> {
        if !(1..=MAX_WORKER_COUNT).contains(&concurrency) {
            return Err(WorkerPoolError::InvalidConcurrency);
        }
        Ok(Self {
            services,
            executor,
            concurrency,
            events: EventIdSource::default(),
            shutdown_requested: Arc::new(AtomicBool::new(false)),
            cancellations: CancellationRegistry::default(),
            progress: ProgressBroadcaster::default(),
        })
    }

    pub fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::Release);
    }

    pub fn cancel_job(&self, job_id: &str) -> bool {
        self.cancellations.cancel(job_id)
    }

    pub fn subscribe_progress(&self) -> tokio::sync::broadcast::Receiver<LiveProgressEvent> {
        self.progress.subscribe()
    }

    pub async fn run_until_idle(&self) -> Result<WorkerPoolReport, WorkerPoolError> {
        let mut workers = Vec::with_capacity(self.concurrency);
        for worker_id in 0..self.concurrency {
            let pool = self.clone();
            workers.push(tokio::spawn(
                async move { pool.run_worker(worker_id).await },
            ));
        }

        let mut report = WorkerPoolReport::default();
        for worker in workers {
            let worker_report = worker
                .await
                .map_err(|error: JoinError| WorkerPoolError::Join(error.to_string()))??;
            report.add(worker_report);
        }
        Ok(report)
    }

    async fn run_worker(&self, worker_id: usize) -> Result<WorkerPoolReport, WorkerPoolError> {
        let mut report = WorkerPoolReport::default();
        loop {
            if self.shutdown_requested.load(Ordering::Acquire) {
                break;
            }
            let event_id = self.events.next(worker_id, "claim");
            let Some(job) = self
                .services
                .claim_next_queued_job(&event_id, Some(json!({"worker_id": worker_id})), &now_utc())
                .await
                .map_err(|error| WorkerPoolError::Queue(error.to_string()))?
            else {
                break;
            };
            report.claimed += 1;
            let job_id = job.id.clone();
            let cancellation = self.cancellations.register(job_id.clone());
            let outcome = self
                .executor
                .execute(
                    job,
                    worker_id,
                    self.events.clone(),
                    cancellation,
                    self.progress.clone(),
                )
                .await;
            self.cancellations.remove(&job_id);
            match outcome {
                Ok(JobExecutionOutcome::Completed) => report.completed += 1,
                Ok(JobExecutionOutcome::Failed) => report.failed += 1,
                Ok(JobExecutionOutcome::RetryScheduled) => report.retried += 1,
                Err(_) => report.execution_errors += 1,
            }
        }
        Ok(report)
    }
}

pub async fn execute_jobs_bounded<E>(
    jobs: Vec<DownloadJob>,
    executor: Arc<E>,
    concurrency: usize,
    events: EventIdSource,
) -> Result<WorkerPoolReport, WorkerPoolError>
where
    E: ClaimedJobExecutor,
{
    if !(1..=MAX_WORKER_COUNT).contains(&concurrency) {
        return Err(WorkerPoolError::InvalidConcurrency);
    }
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut handles = Vec::with_capacity(jobs.len());
    for (index, job) in jobs.into_iter().enumerate() {
        let semaphore = Arc::clone(&semaphore);
        let executor = Arc::clone(&executor);
        let events = events.clone();
        let cancellation = CancellationToken::default();
        let progress = ProgressBroadcaster::default();
        handles.push(tokio::spawn(async move {
            let permit = semaphore
                .acquire_owned()
                .await
                .map_err(|error| WorkerPoolError::Join(error.to_string()))?;
            let result = executor
                .execute(job, index % concurrency, events, cancellation, progress)
                .await;
            drop(permit);
            result.map_err(WorkerPoolError::Queue)
        }));
    }
    let mut report = WorkerPoolReport::default();
    for handle in handles {
        report.claimed += 1;
        match handle
            .await
            .map_err(|error| WorkerPoolError::Join(error.to_string()))?
        {
            Ok(JobExecutionOutcome::Completed) => report.completed += 1,
            Ok(JobExecutionOutcome::Failed) => report.failed += 1,
            Ok(JobExecutionOutcome::RetryScheduled) => report.retried += 1,
            Err(_) => report.execution_errors += 1,
        }
    }
    Ok(report)
}

impl DownloadWorkerPool<ApplicationJobExecutor> {
    pub async fn from_services(
        services: Arc<AppServices>,
        engine: Arc<StreamingEngine>,
    ) -> Result<Self, WorkerPoolError> {
        let concurrency = match services
            .settings
            .get_or_default(crate::application::settings_service::SettingKey::ConcurrentJobs)
            .await
            .map_err(|error| WorkerPoolError::Settings(error.to_string()))?
        {
            Some(crate::application::settings_service::SettingValue::ConcurrentJobs(value)) => {
                usize::from(value)
            }
            _ => DEFAULT_WORKER_COUNT,
        };
        let retry_backoff = match services
            .settings
            .get_or_default(crate::application::settings_service::SettingKey::RetryBackoff)
            .await
            .map_err(|error| WorkerPoolError::Settings(error.to_string()))?
        {
            Some(crate::application::settings_service::SettingValue::RetryBackoff(value)) => value,
            _ => crate::application::settings_service::RetryBackoff {
                base_seconds: 2,
                max_seconds: 900,
            },
        };
        let retry_policy = RetryPolicy::new(retry_backoff.base_seconds, retry_backoff.max_seconds)
            .map_err(|error| WorkerPoolError::Settings(error.to_string()))?;
        let bandwidth_limit_kbps = match services
            .settings
            .get_or_default(crate::application::settings_service::SettingKey::BandwidthLimitKbps)
            .await
            .map_err(|error| WorkerPoolError::Settings(error.to_string()))?
        {
            Some(crate::application::settings_service::SettingValue::BandwidthLimitKbps(value))
                if value > 0 =>
            {
                Some(u64::from(value))
            }
            _ => None,
        };
        engine.set_bandwidth_limit_bytes_per_sec(
            bandwidth_limit_kbps.map(|value| value.saturating_mul(1024)),
        );
        let processor = match MediaProcessor::from_system().await {
            Ok(processor) => Some(Arc::new(processor)),
            Err(error) => {
                tracing::warn!(
                    event = "ffmpeg_unavailable",
                    error = %error,
                    "media processing will fail closed until FFmpeg is installed"
                );
                None
            }
        };
        Self::new(
            Arc::clone(&services),
            Arc::new(ApplicationJobExecutor {
                services,
                engine,
                retry_policy,
                processor,
            }),
            concurrency,
        )
    }
}

pub struct ApplicationJobExecutor {
    services: Arc<AppServices>,
    engine: Arc<StreamingEngine>,
    retry_policy: RetryPolicy,
    processor: Option<Arc<MediaProcessor>>,
}

impl ApplicationJobExecutor {
    pub fn bandwidth_snapshot(&self) -> crate::downloader::BandwidthSnapshot {
        self.engine.bandwidth_snapshot()
    }

    pub fn set_bandwidth_limit_kbps(&self, limit_kbps: u32) {
        self.engine.set_bandwidth_limit_bytes_per_sec(
            (limit_kbps > 0).then(|| u64::from(limit_kbps).saturating_mul(1024)),
        );
    }
}

impl DownloadWorkerPool<ApplicationJobExecutor> {
    pub fn bandwidth_snapshot(&self) -> crate::downloader::BandwidthSnapshot {
        self.executor.bandwidth_snapshot()
    }

    pub fn set_bandwidth_limit_kbps(&self, limit_kbps: u32) {
        self.executor.set_bandwidth_limit_kbps(limit_kbps);
    }
}

#[async_trait]
impl ClaimedJobExecutor for ApplicationJobExecutor {
    async fn execute(
        &self,
        job: DownloadJob,
        worker_id: usize,
        events: EventIdSource,
        cancellation: CancellationToken,
        progress: ProgressBroadcaster,
    ) -> Result<JobExecutionOutcome, String> {
        let Some(format_id) = job.format_id.clone() else {
            return self
                .fail_job(job, worker_id, events, ErrorCode::FormatUnavailable)
                .await;
        };
        let plan = match self
            .services
            .resolve_download_plan(
                &job.media_item_id,
                &format_id,
                std::path::Path::new(&job.destination_path),
                &job.filename,
            )
            .await
        {
            Ok(plan) => plan,
            Err(_) => {
                return self
                    .fail_job(job, worker_id, events, ErrorCode::MediaUnavailable)
                    .await;
            }
        };

        let mut downloading = job.clone();
        downloading.status = DownloadStatus::Downloading;
        downloading.total_bytes = plan.total_bytes;
        downloading.temp_path = Some(plan.destination.temporary.to_string_lossy().to_string());
        downloading.updated_at = now_utc();
        if let Err(error) = self
            .services
            .transition_download_job(
                &downloading,
                &job_event(
                    &job.id,
                    events.next(worker_id, "downloading"),
                    "downloading",
                    json!({"worker_id": worker_id}),
                ),
            )
            .await
        {
            return Err(error.to_string());
        }

        let resume_offset = match u64::try_from(downloading.downloaded_bytes) {
            Ok(value) => value,
            Err(_) => {
                return self
                    .fail_job(downloading, worker_id, events, ErrorCode::UnknownError)
                    .await;
            }
        };
        let latest_progress = Arc::new(Mutex::new(None::<StreamProgress>));
        let latest_progress_for_callback = Arc::clone(&latest_progress);
        let job_id_for_callback = job.id.clone();
        let result = match self
            .engine
            .stream_plan_resumable(
                &plan,
                resume_offset,
                downloading.etag.as_deref(),
                downloading.last_modified.as_deref(),
                cancellation.as_atomic(),
                move |sample| {
                    if let Ok(mut latest) = latest_progress_for_callback.lock() {
                        *latest = Some(sample);
                    }
                    progress.publish(LiveProgressEvent {
                        job_id: job_id_for_callback.clone(),
                        downloaded_bytes: sample.downloaded_bytes,
                        total_bytes: sample.total_bytes,
                        speed_bytes_per_sec: sample.speed_bytes_per_sec,
                        eta_seconds: sample.eta_seconds,
                        bandwidth: self.engine.bandwidth_snapshot(),
                    });
                },
            )
            .await
        {
            Ok(result) => result,
            Err(StreamingError::Cancelled { .. }) => {
                return self.cancel_job(downloading, worker_id, events).await;
            }
            Err(error) => {
                let durable_bytes = tokio::fs::metadata(&plan.destination.temporary)
                    .await
                    .ok()
                    .filter(|metadata| metadata.is_file())
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                if let Ok(downloaded_bytes) = i64::try_from(durable_bytes) {
                    downloading.downloaded_bytes = downloaded_bytes;
                    downloading.total_bytes = plan.total_bytes.or(downloading.total_bytes);
                    downloading.speed_bytes_per_sec = None;
                    downloading.eta_seconds = None;
                    let _ = self
                        .services
                        .persist_download_progress_with_validators(
                            &DownloadProgress {
                                job_id: downloading.id.clone(),
                                downloaded_bytes,
                                total_bytes: downloading.total_bytes,
                                speed_bytes_per_sec: None,
                                eta_seconds: None,
                                updated_at: now_utc(),
                            },
                            &job_event(
                                &downloading.id,
                                events.next(worker_id, "retry_progress"),
                                "progress",
                                json!({"downloaded_bytes": downloaded_bytes}),
                            ),
                            downloading.etag.as_deref(),
                            downloading.last_modified.as_deref(),
                        )
                        .await;
                }
                return self
                    .fail_or_retry(
                        downloading,
                        worker_id,
                        events,
                        cancellation,
                        streaming_error_code(&error),
                    )
                    .await;
            }
        };
        let final_sample = latest_progress.lock().ok().and_then(|latest| *latest);
        let total_bytes = plan.total_bytes.or_else(|| {
            result
                .stream
                .content_length
                .and_then(|value| i64::try_from(value).ok())
        });
        let downloaded_bytes = match i64::try_from(result.stream.bytes_written) {
            Ok(value) => value,
            Err(_) => {
                return self
                    .fail_job(downloading, worker_id, events, ErrorCode::UnknownError)
                    .await;
            }
        };
        if let Err(error) = self
            .services
            .persist_download_progress_with_validators(
                &DownloadProgress {
                    job_id: job.id.clone(),
                    downloaded_bytes,
                    total_bytes,
                    speed_bytes_per_sec: final_sample
                        .and_then(|sample| sample.speed_bytes_per_sec)
                        .and_then(|value| i64::try_from(value).ok()),
                    eta_seconds: final_sample
                        .and_then(|sample| sample.eta_seconds)
                        .and_then(|value| i64::try_from(value).ok()),
                    updated_at: now_utc(),
                },
                &job_event(
                    &job.id,
                    events.next(worker_id, "progress"),
                    "progress",
                    json!({"downloaded_bytes": downloaded_bytes}),
                ),
                result.etag.as_deref(),
                result.last_modified.as_deref(),
            )
            .await
        {
            return Err(error.to_string());
        }

        let finalization = if let Some(processing_json) = downloading.processing_json.clone() {
            let Some(processor) = self.processor.as_ref() else {
                return self
                    .fail_job(downloading, worker_id, events, ErrorCode::FfmpegFailed)
                    .await;
            };
            let configuration =
                match serde_json::from_value::<MediaProcessingConfiguration>(processing_json) {
                    Ok(configuration) => configuration,
                    Err(_) => {
                        return self
                            .fail_job(downloading, worker_id, events, ErrorCode::FfmpegFailed)
                            .await;
                    }
                };
            let processing_plan = match MediaProcessingPlan::resolve(
                &plan.destination.root,
                configuration.into_request(),
            ) {
                Ok(processing_plan)
                    if processing_plan.output.final_path == plan.destination.destination =>
                {
                    processing_plan
                }
                _ => {
                    return self
                        .fail_job(downloading, worker_id, events, ErrorCode::FfmpegFailed)
                        .await;
                }
            };
            let mut processing_job = downloading.clone();
            processing_job.status = DownloadStatus::Processing;
            processing_job.temp_path = Some(
                processing_plan
                    .output
                    .temporary_path
                    .to_string_lossy()
                    .to_string(),
            );
            processing_job.total_bytes = None;
            processing_job.downloaded_bytes = 0;
            processing_job.speed_bytes_per_sec = None;
            processing_job.eta_seconds = None;
            processing_job.updated_at = now_utc();
            if let Err(error) = self
                .services
                .transition_download_job(
                    &processing_job,
                    &job_event(
                        &job.id,
                        events.next(worker_id, "processing"),
                        "processing",
                        json!({"operation": "typed_media_processing"}),
                    ),
                )
                .await
            {
                return Err(error.to_string());
            }
            match processor
                .execute(&processing_plan, cancellation.as_atomic())
                .await
            {
                Ok(processed) => FinalizationResult {
                    final_path: processed.final_path,
                    bytes_finalized: processed.bytes_finalized,
                },
                Err(error) if error.is_cancelled() => {
                    let _ = tokio::fs::remove_file(&processing_plan.output.temporary_path).await;
                    return self.cancel_job(processing_job, worker_id, events).await;
                }
                Err(_) => {
                    let _ = tokio::fs::remove_file(&processing_plan.output.temporary_path).await;
                    return self
                        .fail_job(processing_job, worker_id, events, ErrorCode::FfmpegFailed)
                        .await;
                }
            }
        } else {
            match finalize_part(&plan, &result.stream) {
                Ok(finalization) => finalization,
                Err(_) => {
                    return self
                        .fail_job(downloading, worker_id, events, ErrorCode::UnknownError)
                        .await;
                }
            }
        };
        if let Err(error) = self
            .services
            .complete_streamed_download_with_validators(
                &job.id,
                &finalization,
                &now_utc(),
                &job_event(
                    &job.id,
                    events.next(worker_id, "completed"),
                    "completed",
                    json!({"bytes_finalized": finalization.bytes_finalized}),
                ),
                result.etag.as_deref(),
                result.last_modified.as_deref(),
            )
            .await
        {
            return Err(error.to_string());
        }
        let _ = self
            .services
            .record_history_for_job(
                &job.id,
                DownloadStatus::Completed,
                i64::try_from(finalization.bytes_finalized).ok(),
                None,
                None,
                &now_utc(),
            )
            .await;
        Ok(JobExecutionOutcome::Completed)
    }
}

impl ApplicationJobExecutor {
    async fn fail_job(
        &self,
        mut job: DownloadJob,
        worker_id: usize,
        events: EventIdSource,
        code: ErrorCode,
    ) -> Result<JobExecutionOutcome, String> {
        job.status = DownloadStatus::Failed;
        job.error_code = Some(code.to_string());
        job.error_message = Some(code.to_string());
        job.updated_at = now_utc();
        self.services
            .transition_download_job(
                &job,
                &job_event(
                    &job.id,
                    events.next(worker_id, "failed"),
                    "failed",
                    json!({"error_code": code.to_string()}),
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        let _ = self
            .services
            .record_history_for_job(
                &job.id,
                DownloadStatus::Failed,
                None,
                Some(code.to_string()),
                Some(code.to_string()),
                &job.updated_at,
            )
            .await;
        Ok(JobExecutionOutcome::Failed)
    }

    async fn fail_or_retry(
        &self,
        mut job: DownloadJob,
        worker_id: usize,
        events: EventIdSource,
        cancellation: CancellationToken,
        code: ErrorCode,
    ) -> Result<JobExecutionOutcome, String> {
        job.status = DownloadStatus::Failed;
        job.error_code = Some(code.to_string());
        job.error_message = Some(code.to_string());
        job.updated_at = now_utc();
        self.services
            .transition_download_job(
                &job,
                &job_event(
                    &job.id,
                    events.next(worker_id, "failed"),
                    "failed",
                    json!({"error_code": code.to_string()}),
                ),
            )
            .await
            .map_err(|error| error.to_string())?;

        let jitter = deterministic_jitter(&job.id, job.retry_count);
        let decision = self
            .retry_policy
            .decision(code, job.retry_count, job.max_retries, jitter)
            .map_err(|error| error.to_string())?;
        let RetryDecision::Retry {
            next_retry_count,
            delay_seconds,
        } = decision
        else {
            let _ = self
                .services
                .record_history_for_job(
                    &job.id,
                    DownloadStatus::Failed,
                    None,
                    Some(code.to_string()),
                    Some(code.to_string()),
                    &job.updated_at,
                )
                .await;
            return Ok(JobExecutionOutcome::Failed);
        };
        if cancellation.is_cancelled() {
            return Ok(JobExecutionOutcome::Failed);
        }
        sleep(Duration::from_secs(delay_seconds)).await;
        if cancellation.is_cancelled() {
            return Ok(JobExecutionOutcome::Failed);
        }
        job.status = DownloadStatus::Queued;
        job.retry_count = next_retry_count;
        job.error_code = None;
        job.error_message = None;
        job.updated_at = now_utc();
        self.services
            .transition_download_job(
                &job,
                &job_event(
                    &job.id,
                    events.next(worker_id, "retry"),
                    "retry_scheduled",
                    json!({
                        "retry_count": next_retry_count,
                        "delay_seconds": delay_seconds,
                    }),
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(JobExecutionOutcome::RetryScheduled)
    }

    async fn cancel_job(
        &self,
        mut job: DownloadJob,
        worker_id: usize,
        events: EventIdSource,
    ) -> Result<JobExecutionOutcome, String> {
        job.status = DownloadStatus::Cancelled;
        job.error_code = None;
        job.error_message = Some("cancelled".to_owned());
        job.updated_at = now_utc();
        self.services
            .transition_download_job(
                &job,
                &job_event(
                    &job.id,
                    events.next(worker_id, "cancelled"),
                    "cancelled",
                    json!({"cancelled": true}),
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        let _ = self
            .services
            .record_history_for_job(
                &job.id,
                DownloadStatus::Cancelled,
                Some(job.downloaded_bytes),
                None,
                Some("cancelled".to_owned()),
                &job.updated_at,
            )
            .await;
        Ok(JobExecutionOutcome::Failed)
    }
}

fn job_event(
    job_id: &str,
    id: String,
    event_type: &str,
    payload_json: serde_json::Value,
) -> JobEvent {
    JobEvent {
        id,
        job_id: job_id.to_owned(),
        event_type: event_type.to_owned(),
        payload_json: Some(payload_json),
        created_at: now_utc(),
    }
}

fn streaming_error_code(error: &StreamingError) -> ErrorCode {
    match error {
        StreamingError::RequestFailed { retryable: true }
        | StreamingError::BodyReadFailed { retryable: true } => ErrorCode::NetworkError,
        StreamingError::UnexpectedStatus { status: 429 } => ErrorCode::RateLimited,
        StreamingError::UnexpectedStatus { status } if *status >= 500 => ErrorCode::NetworkError,
        StreamingError::DiskFull => ErrorCode::DiskFull,
        StreamingError::PermissionDenied
        | StreamingError::Finalization(
            crate::downloader::FinalizationError::FinalFilePermissionFailed,
        ) => ErrorCode::PermissionDenied,
        _ => ErrorCode::UnknownError,
    }
}

fn deterministic_jitter(job_id: &str, retry_count: i64) -> u16 {
    let mut hasher = DefaultHasher::new();
    job_id.hash(&mut hasher);
    retry_count.hash(&mut hasher);
    (hasher.finish() % 1_001) as u16
}

fn now_utc() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        execute_jobs_bounded, CancellationToken, ClaimedJobExecutor, DownloadWorkerPool,
        EventIdSource, JobExecutionOutcome, ProgressBroadcaster, WorkerPoolError,
    };
    use crate::application::services::AppServices;
    use crate::domain::entities::DownloadJob;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tempfile::tempdir;

    struct FakeExecutor {
        active: AtomicUsize,
        maximum: AtomicUsize,
        completed: AtomicUsize,
        fail: bool,
    }

    #[async_trait]
    impl ClaimedJobExecutor for FakeExecutor {
        async fn execute(
            &self,
            _job: DownloadJob,
            _worker_id: usize,
            _events: EventIdSource,
            _cancellation: CancellationToken,
            _progress: ProgressBroadcaster,
        ) -> Result<JobExecutionOutcome, String> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.maximum.fetch_max(active, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            if self.fail {
                Err("simulated execution failure".to_owned())
            } else {
                self.completed.fetch_add(1, Ordering::SeqCst);
                Ok(JobExecutionOutcome::Completed)
            }
        }
    }

    #[tokio::test]
    async fn rejects_invalid_worker_counts() {
        let directory = tempdir().unwrap();
        let database = crate::persistence::Database::from_app_data_dir(directory.path())
            .await
            .unwrap();
        let services = Arc::new(AppServices::from_database(&database));
        let executor = Arc::new(FakeExecutor {
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            fail: false,
        });
        assert!(matches!(
            DownloadWorkerPool::new(services, executor, 0),
            Err(WorkerPoolError::InvalidConcurrency)
        ));
    }

    fn test_job(id: usize) -> DownloadJob {
        DownloadJob {
            id: format!("job-{id}"),
            media_item_id: "item-1".to_owned(),
            format_id: Some("format-1".to_owned()),
            status: crate::domain::entities::DownloadStatus::Queued,
            priority: 0,
            destination_path: "/downloads".to_owned(),
            temp_path: Some(format!("/downloads/job-{id}.part")),
            filename: format!("job-{id}.mp4"),
            total_bytes: Some(10),
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
    async fn requested_shutdown_prevents_new_queue_claims() {
        let directory = tempdir().unwrap();
        let database = crate::persistence::Database::from_app_data_dir(directory.path())
            .await
            .unwrap();
        let services = Arc::new(AppServices::from_database(&database));
        let executor = Arc::new(FakeExecutor {
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            fail: false,
        });
        let pool = DownloadWorkerPool::new(services, executor, 2).unwrap();
        pool.request_shutdown();
        let report = pool
            .run_until_idle()
            .await
            .expect("shutdown should be graceful");
        assert_eq!(report, super::WorkerPoolReport::default());
    }

    #[tokio::test]
    async fn bounded_batch_never_exceeds_configured_concurrency() {
        let executor = Arc::new(FakeExecutor {
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            fail: false,
        });
        let jobs = (0..12).map(test_job).collect();
        let report = execute_jobs_bounded(jobs, Arc::clone(&executor), 3, EventIdSource::default())
            .await
            .expect("bounded batch should complete");
        assert_eq!(report.claimed, 12);
        assert_eq!(report.completed, 12);
        assert_eq!(report.execution_errors, 0);
        assert!(executor.maximum.load(Ordering::SeqCst) <= 3);
        assert!(executor.maximum.load(Ordering::SeqCst) > 1);
    }

    #[test]
    fn retry_jitter_is_deterministic_and_bounded() {
        let first = super::deterministic_jitter("job-1", 0);
        let second = super::deterministic_jitter("job-1", 0);
        assert_eq!(first, second);
        assert!(first <= 1_000);
        assert!(super::deterministic_jitter("job-1", 1) <= 1_000);
    }

    #[test]
    fn streaming_error_mapping_preserves_retryable_categories() {
        assert_eq!(
            super::streaming_error_code(&super::StreamingError::RequestFailed { retryable: true }),
            crate::domain::errors::ErrorCode::NetworkError
        );
        assert_eq!(
            super::streaming_error_code(&super::StreamingError::UnexpectedStatus { status: 429 }),
            crate::domain::errors::ErrorCode::RateLimited
        );
        assert_eq!(
            super::streaming_error_code(&super::StreamingError::UnexpectedStatus { status: 404 }),
            crate::domain::errors::ErrorCode::UnknownError
        );
        assert_eq!(
            super::streaming_error_code(&super::StreamingError::DiskFull),
            crate::domain::errors::ErrorCode::DiskFull
        );
        assert_eq!(
            super::streaming_error_code(&super::StreamingError::PermissionDenied),
            crate::domain::errors::ErrorCode::PermissionDenied
        );
    }

    struct RetryExecutor;

    #[async_trait]
    impl ClaimedJobExecutor for RetryExecutor {
        async fn execute(
            &self,
            _job: DownloadJob,
            _worker_id: usize,
            _events: EventIdSource,
            _cancellation: CancellationToken,
            _progress: ProgressBroadcaster,
        ) -> Result<JobExecutionOutcome, String> {
            Ok(JobExecutionOutcome::RetryScheduled)
        }
    }

    #[tokio::test]
    async fn bounded_batch_reports_retry_scheduling_under_concurrency() {
        let jobs = (0..7).map(test_job).collect();
        let report =
            execute_jobs_bounded(jobs, Arc::new(RetryExecutor), 3, EventIdSource::default())
                .await
                .expect("retry outcomes should be aggregated");
        assert_eq!(report.claimed, 7);
        assert_eq!(report.retried, 7);
        assert_eq!(report.completed, 0);
        assert_eq!(report.execution_errors, 0);
    }

    #[tokio::test]
    async fn one_executor_failure_does_not_stop_other_jobs() {
        let executor = Arc::new(FakeExecutor {
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            fail: true,
        });
        let jobs = (0..5).map(test_job).collect();
        let report = execute_jobs_bounded(jobs, executor, 2, EventIdSource::default())
            .await
            .expect("executor errors should be isolated");
        assert_eq!(report.claimed, 5);
        assert_eq!(report.completed, 0);
        assert_eq!(report.execution_errors, 5);
    }
}
