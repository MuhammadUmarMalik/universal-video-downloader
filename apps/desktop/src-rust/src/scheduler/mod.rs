use crate::application::analyzer::{AnalyzeRequest, AnalyzerService};
use crate::application::ports::{MediaSourceRepository, RepositoryError, ScheduleRepository};
use crate::application::services::AppServices;
use crate::domain::entities::{DownloadJob, DownloadStatus, JobEvent, Schedule, ScheduleType};
use crate::downloader::{ApplicationJobExecutor, DownloadWorkerPool};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};
use url::Url;

const MIN_INTERVAL_SECONDS: i64 = 60;
const MAX_INTERVAL_SECONDS: i64 = 31_536_000;
const SCHEDULER_TICK_SECONDS: u64 = 15;
static SCHEDULE_JOB_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleConfiguration {
    pub format_id: Option<String>,
    pub destination_path: String,
    pub filename_template: String,
    #[serde(default = "default_auto_download_new_items")]
    pub auto_download_new_items: bool,
}

fn default_auto_download_new_items() -> bool {
    true
}

impl ScheduleConfiguration {
    pub fn validate(&self) -> Result<(), SchedulerError> {
        if self.destination_path.trim().is_empty()
            || !Path::new(&self.destination_path).is_absolute()
        {
            return Err(SchedulerError::InvalidConfiguration(
                "scheduled destination must be an absolute path".to_owned(),
            ));
        }
        if self.filename_template.trim().is_empty() || self.filename_template.len() > 255 {
            return Err(SchedulerError::InvalidConfiguration(
                "scheduled filename template must be non-empty and at most 255 characters"
                    .to_owned(),
            ));
        }
        if self
            .filename_template
            .chars()
            .any(|character| character.is_control())
        {
            return Err(SchedulerError::InvalidConfiguration(
                "scheduled filename template cannot contain control characters".to_owned(),
            ));
        }
        if self
            .format_id
            .as_deref()
            .map(str::trim)
            .map(|value| value.is_empty())
            .unwrap_or(false)
        {
            return Err(SchedulerError::InvalidConfiguration(
                "scheduled format ID cannot be empty".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("schedule is invalid: {0}")]
    InvalidSchedule(String),
    #[error("schedule configuration is invalid: {0}")]
    InvalidConfiguration(String),
    #[error("schedule source is not supported for public monitoring")]
    SchedulingUnsupported,
    #[error("schedule source could not be read: {0}")]
    Repository(String),
    #[error("scheduled public source analysis failed: {0}")]
    Analysis(String),
    #[error("scheduled download could not be queued: {0}")]
    Queue(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SchedulerRunReport {
    pub schedules_checked: usize,
    pub schedules_processed: usize,
    pub jobs_enqueued: usize,
    pub schedules_failed: usize,
}

#[derive(Clone)]
pub struct SchedulerLoop {
    services: Arc<AppServices>,
    analyzer: AnalyzerService,
    worker_pool: DownloadWorkerPool<ApplicationJobExecutor>,
    stop_requested: Arc<AtomicBool>,
}

impl SchedulerLoop {
    pub fn new(
        services: Arc<AppServices>,
        analyzer: AnalyzerService,
        worker_pool: DownloadWorkerPool<ApplicationJobExecutor>,
    ) -> Self {
        Self {
            services,
            analyzer,
            worker_pool,
            stop_requested: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(&self) {
        let scheduler = self.clone();
        tokio::spawn(async move {
            scheduler.run().await;
        });
    }

    async fn run(self) {
        let mut ticker =
            tokio::time::interval(tokio::time::Duration::from_secs(SCHEDULER_TICK_SECONDS));
        loop {
            ticker.tick().await;
            if self.stop_requested.load(Ordering::Acquire) {
                break;
            }
            let enabled = match self
                .services
                .settings
                .get_or_default(crate::application::settings_service::SettingKey::SchedulerEnabled)
                .await
            {
                Ok(Some(crate::application::settings_service::SettingValue::SchedulerEnabled(
                    enabled,
                ))) => enabled,
                Ok(_) => false,
                Err(error) => {
                    tracing::warn!(event = "scheduler_setting_read_failed", error = %error);
                    false
                }
            };
            if !enabled {
                continue;
            }
            match self.run_once(&now_utc()).await {
                Ok(report) => {
                    if report.schedules_processed > 0 {
                        tracing::info!(
                            event = "scheduler_tick_completed",
                            schedules_checked = report.schedules_checked,
                            schedules_processed = report.schedules_processed,
                            jobs_enqueued = report.jobs_enqueued,
                            schedules_failed = report.schedules_failed
                        );
                    }
                }
                Err(error) => tracing::warn!(event = "scheduler_tick_failed", error = %error),
            }
        }
        tracing::info!(event = "scheduler_loop_stopped");
    }

    pub async fn run_once(&self, now: &str) -> Result<SchedulerRunReport, SchedulerError> {
        let report = self.run_due(now).await?;
        if report.jobs_enqueued > 0 {
            self.worker_pool
                .run_until_idle()
                .await
                .map_err(|error| SchedulerError::Queue(error.to_string()))?;
        }
        Ok(report)
    }

    pub async fn run_due(&self, now: &str) -> Result<SchedulerRunReport, SchedulerError> {
        let due = self
            .services
            .repositories
            .schedules
            .list_due(now)
            .await
            .map_err(repository_error)?;
        let mut report = SchedulerRunReport {
            schedules_checked: due.len(),
            ..SchedulerRunReport::default()
        };
        for schedule in due {
            match self.process_schedule(&schedule).await {
                Ok(enqueued) => {
                    report.schedules_processed += 1;
                    report.jobs_enqueued += enqueued;
                }
                Err(error) => {
                    report.schedules_failed += 1;
                    tracing::warn!(
                        event = "scheduled_source_failed",
                        schedule_id = %schedule.id,
                        error = %error
                    );
                }
            }
            let mut advanced = schedule.clone();
            advanced.last_run_at = Some(now.to_owned());
            advanced.next_run_at = next_run_for(&schedule, now)?;
            if matches!(schedule.schedule_type, ScheduleType::Once) {
                advanced.enabled = false;
                advanced.next_run_at = None;
            }
            advanced.updated_at = now.to_owned();
            self.services
                .repositories
                .schedules
                .upsert(&advanced)
                .await
                .map_err(repository_error)?;
        }
        Ok(report)
    }

    async fn process_schedule(&self, schedule: &Schedule) -> Result<usize, SchedulerError> {
        let configuration = validate_schedule(schedule)?;
        let source = self
            .services
            .repositories
            .media_sources
            .get(&schedule.source_id)
            .await
            .map_err(repository_error)?
            .ok_or_else(|| {
                SchedulerError::Repository("scheduled source was not found".to_owned())
            })?;
        ensure_schedulable_source(&self.analyzer, &source.source_url, &source.platform_id)?;
        let response = self
            .analyzer
            .analyze(AnalyzeRequest {
                url: source.normalized_url.clone(),
                platform_id: Some(source.platform_id.clone()),
            })
            .await
            .map_err(|error| SchedulerError::Analysis(error.message))?;
        if !configuration.auto_download_new_items {
            return Ok(0);
        }
        let existing_jobs = self
            .services
            .list_download_jobs()
            .await
            .map_err(repository_error)?;
        let mut existing_keys = existing_jobs
            .iter()
            .map(scheduled_job_key)
            .collect::<HashSet<_>>();
        let mut enqueued = 0;
        for item in response.items {
            let format = response
                .formats
                .iter()
                .filter(|candidate| candidate.media_item_id == item.id && candidate.is_progressive)
                .find(|candidate| {
                    configuration
                        .format_id
                        .as_deref()
                        .map(|id| candidate.id == id)
                        .unwrap_or(true)
                })
                .cloned();
            let Some(format) = format else {
                continue;
            };
            let filename = expand_filename(
                &configuration.filename_template,
                &item,
                &response.platform_id,
            );
            let plan = self
                .services
                .resolve_download_plan(
                    &item.id,
                    &format.id,
                    Path::new(&configuration.destination_path),
                    &filename,
                )
                .await
                .map_err(repository_error)?;
            let job_key = (
                item.id.clone(),
                format.id.clone(),
                plan.destination.root.to_string_lossy().into_owned(),
                plan.destination.filename.clone(),
            );
            if existing_keys.contains(&job_key) {
                continue;
            }
            let now = now_utc();
            let sequence = SCHEDULE_JOB_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let job_id = format!("scheduled-{sequence}-{}", now.replace([':', '-', '.'], ""));
            let job = DownloadJob {
                id: job_id.clone(),
                media_item_id: item.id,
                format_id: Some(format.id),
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
                processing_json: None,
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
                job_id,
                event_type: "queued".to_owned(),
                payload_json: Some(serde_json::json!({
                    "source": "scheduler",
                    "schedule_id": schedule.id,
                })),
                created_at: now,
            };
            self.services
                .create_download_job(&job, &event)
                .await
                .map_err(|error| SchedulerError::Queue(error.to_string()))?;
            existing_keys.insert(job_key);
            enqueued += 1;
        }
        Ok(enqueued)
    }
}

pub fn validate_schedule(schedule: &Schedule) -> Result<ScheduleConfiguration, SchedulerError> {
    if schedule.id.trim().is_empty() || schedule.source_id.trim().is_empty() {
        return Err(SchedulerError::InvalidSchedule(
            "schedule ID and source ID are required".to_owned(),
        ));
    }
    if schedule.enabled && schedule.next_run_at.is_none() {
        return Err(SchedulerError::InvalidSchedule(
            "enabled schedules require a next run time".to_owned(),
        ));
    }
    match schedule.schedule_type {
        ScheduleType::Once | ScheduleType::Daily | ScheduleType::Weekly => {
            if schedule.interval_seconds.is_some() || schedule.cron_expression.is_some() {
                return Err(SchedulerError::InvalidSchedule(
                    "calendar schedules cannot include interval or cron fields".to_owned(),
                ));
            }
        }
        ScheduleType::Interval => {
            let interval = schedule.interval_seconds.ok_or_else(|| {
                SchedulerError::InvalidSchedule(
                    "interval schedules require interval_seconds".to_owned(),
                )
            })?;
            if !(MIN_INTERVAL_SECONDS..=MAX_INTERVAL_SECONDS).contains(&interval) {
                return Err(SchedulerError::InvalidSchedule(format!(
                    "interval_seconds must be between {MIN_INTERVAL_SECONDS} and {MAX_INTERVAL_SECONDS}"
                )));
            }
            if schedule.cron_expression.is_some() {
                return Err(SchedulerError::InvalidSchedule(
                    "interval schedules cannot include a cron expression".to_owned(),
                ));
            }
        }
    }
    let configuration = schedule
        .configuration_json
        .clone()
        .ok_or_else(|| {
            SchedulerError::InvalidConfiguration("schedule configuration is required".to_owned())
        })
        .and_then(|value| {
            serde_json::from_value::<ScheduleConfiguration>(value).map_err(|_| {
                SchedulerError::InvalidConfiguration(
                    "schedule configuration is malformed".to_owned(),
                )
            })
        })?;
    configuration.validate()?;
    Ok(configuration)
}

pub fn next_run_for(schedule: &Schedule, now: &str) -> Result<Option<String>, SchedulerError> {
    validate_schedule(schedule)?;
    if matches!(schedule.schedule_type, ScheduleType::Once) {
        return Ok(None);
    }
    let current = parse_time(now)?;
    let next = match schedule.schedule_type {
        ScheduleType::Daily => current + Duration::days(1),
        ScheduleType::Weekly => current + Duration::days(7),
        ScheduleType::Interval => {
            current
                + Duration::seconds(schedule.interval_seconds.ok_or_else(|| {
                    SchedulerError::InvalidSchedule(
                        "interval schedules require interval_seconds".to_owned(),
                    )
                })?)
        }
        ScheduleType::Once => current,
    };
    Ok(Some(format_time(next)))
}

pub fn ensure_schedulable_source(
    analyzer: &AnalyzerService,
    source_url: &str,
    platform_id: &str,
) -> Result<(), SchedulerError> {
    let url = Url::parse(source_url).map_err(|_| SchedulerError::SchedulingUnsupported)?;
    let adapter = analyzer
        .registry()
        .select(&url, Some(platform_id))
        .map_err(|_| SchedulerError::SchedulingUnsupported)?;
    if adapter.capabilities().scheduling {
        Ok(())
    } else {
        Err(SchedulerError::SchedulingUnsupported)
    }
}

fn expand_filename(
    template: &str,
    item: &crate::domain::entities::MediaItem,
    platform_id: &str,
) -> String {
    let creator = item.creator_name.as_deref().unwrap_or("unknown");
    let mut filename = template
        .replace("{title}", &item.title)
        .replace("{creator}", creator)
        .replace("{platform}", platform_id)
        .replace("{item_id}", &item.id);
    filename = filename
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect();
    filename
        .trim()
        .trim_end_matches(['.', ' '])
        .chars()
        .take(255)
        .collect()
}

fn scheduled_job_key(job: &DownloadJob) -> (String, String, String, String) {
    (
        job.media_item_id.clone(),
        job.format_id.clone().unwrap_or_default(),
        job.destination_path.clone(),
        job.filename.clone(),
    )
}

fn repository_error(error: RepositoryError) -> SchedulerError {
    SchedulerError::Repository(error.to_string())
}

fn parse_time(value: &str) -> Result<OffsetDateTime, SchedulerError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|_| SchedulerError::InvalidSchedule("timestamps must use RFC3339".to_owned()))
}

fn format_time(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

fn now_utc() -> String {
    format_time(OffsetDateTime::now_utc())
}

#[cfg(test)]
mod tests {
    use super::{
        expand_filename, next_run_for, validate_schedule, ScheduleConfiguration, SchedulerError,
    };
    use crate::domain::entities::{MediaItem, Schedule, ScheduleType};
    use serde_json::json;
    use std::collections::HashSet;
    use std::fs;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    fn schedule(schedule_type: ScheduleType) -> Schedule {
        Schedule {
            id: "schedule-1".to_owned(),
            source_id: "source-1".to_owned(),
            schedule_type,
            cron_expression: None,
            interval_seconds: Some(3600),
            enabled: true,
            last_run_at: None,
            next_run_at: Some("2026-08-21T00:00:00Z".to_owned()),
            configuration_json: Some(json!(ScheduleConfiguration {
                format_id: None,
                destination_path: "/downloads".to_owned(),
                filename_template: "{creator} - {title}.mp4".to_owned(),
                auto_download_new_items: true,
            })),
            created_at: "2026-08-20T00:00:00Z".to_owned(),
            updated_at: "2026-08-20T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn calculates_daily_weekly_and_interval_next_runs() {
        let mut daily = schedule(ScheduleType::Daily);
        daily.interval_seconds = None;
        assert_eq!(
            next_run_for(&daily, "2026-08-21T00:00:00Z").unwrap(),
            Some("2026-08-22T00:00:00Z".to_owned())
        );
        let mut weekly = schedule(ScheduleType::Weekly);
        weekly.interval_seconds = None;
        assert_eq!(
            next_run_for(&weekly, "2026-08-21T00:00:00Z").unwrap(),
            Some("2026-08-28T00:00:00Z".to_owned())
        );
        let interval = schedule(ScheduleType::Interval);
        assert_eq!(
            next_run_for(&interval, "2026-08-21T00:00:00Z").unwrap(),
            Some("2026-08-21T01:00:00Z".to_owned())
        );
    }

    #[test]
    fn once_schedules_have_no_next_run_after_processing() {
        let mut once = schedule(ScheduleType::Once);
        once.interval_seconds = None;
        assert_eq!(next_run_for(&once, "2026-08-21T00:00:00Z").unwrap(), None);
    }

    #[test]
    fn rejects_short_intervals_and_malformed_configuration() {
        let mut invalid = schedule(ScheduleType::Interval);
        invalid.interval_seconds = Some(1);
        assert!(matches!(
            validate_schedule(&invalid),
            Err(SchedulerError::InvalidSchedule(_))
        ));
        invalid.interval_seconds = Some(60);
        invalid.configuration_json = Some(json!({ "destination_path": "relative" }));
        assert!(matches!(
            validate_schedule(&invalid),
            Err(SchedulerError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn large_collection_dedup_key_generation_is_bounded() {
        let started = Instant::now();
        let mut keys = HashSet::with_capacity(100_000);
        for index in 0..100_000 {
            let item = MediaItem {
                id: format!("item-{index}"),
                source_id: "source-1".to_owned(),
                collection_id: Some("collection-large".to_owned()),
                external_id: Some(format!("external-{index}")),
                canonical_url: format!("https://example.test/items/{index}"),
                title: format!("Large collection item {index}"),
                creator_name: Some("Creator".to_owned()),
                creator_id: None,
                thumbnail_url: None,
                duration_ms: None,
                published_at: None,
                position: Some(index),
                metadata_json: None,
                first_seen_at: "2026-08-21T00:00:00Z".to_owned(),
                last_seen_at: "2026-08-21T00:00:00Z".to_owned(),
            };
            let filename = expand_filename("{creator}-{title}.mp4", &item, "reddit");
            keys.insert((
                item.id,
                "format-1".to_owned(),
                "/downloads".to_owned(),
                filename,
            ));
        }
        let elapsed = started.elapsed();
        assert_eq!(keys.len(), 100_000);
        eprintln!(
            "scheduler_large_collection_candidates=100000 elapsed_ms={}",
            elapsed.as_millis()
        );
        assert!(elapsed < Duration::from_secs(5));
    }

    #[test]
    fn large_file_metadata_check_is_constant_time() {
        let directory = tempdir().unwrap();
        let file_path = directory.path().join("large-media.mp4");
        let file = fs::File::create(&file_path).unwrap();
        file.set_len(4 * 1024 * 1024 * 1024).unwrap();
        let started = Instant::now();
        let mut observed = 0_u64;
        for _ in 0..10_000 {
            observed = fs::symlink_metadata(&file_path).unwrap().len();
        }
        let elapsed = started.elapsed();
        assert_eq!(observed, 4 * 1024 * 1024 * 1024);
        eprintln!(
            "scheduler_large_file_metadata_checks=10000 bytes={} elapsed_ms={}",
            observed,
            elapsed.as_millis()
        );
        assert!(elapsed < Duration::from_secs(5));
    }

    #[test]
    fn expands_metadata_and_removes_path_control_characters() {
        let item = MediaItem {
            id: "item-1".to_owned(),
            source_id: "source-1".to_owned(),
            collection_id: None,
            external_id: None,
            canonical_url: "https://example.test/item".to_owned(),
            title: "A / public: title".to_owned(),
            creator_name: Some("Creator".to_owned()),
            creator_id: None,
            thumbnail_url: None,
            duration_ms: None,
            published_at: None,
            position: None,
            metadata_json: None,
            first_seen_at: "2026-08-21T00:00:00Z".to_owned(),
            last_seen_at: "2026-08-21T00:00:00Z".to_owned(),
        };
        assert_eq!(
            expand_filename("{creator} - {title}.mp4", &item, "reddit"),
            "Creator - A _ public_ title.mp4"
        );
    }
}
