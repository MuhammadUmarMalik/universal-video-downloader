use crate::application::analyzer::AnalyzerService;
use crate::application::ports::{MediaSourceRepository, RepositoryError};
use crate::application::services::AppServices;
use crate::domain::entities::{Schedule, ScheduleType};
use crate::domain::errors::{AppError, ErrorCode};
use crate::scheduler::{
    ensure_schedulable_source, validate_schedule, ScheduleConfiguration, SchedulerError,
    SchedulerLoop,
};
use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

static SCHEDULE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Deserialize)]
pub struct CreateScheduleRequest {
    pub source_id: String,
    pub schedule_type: ScheduleType,
    pub interval_seconds: Option<i64>,
    pub next_run_at: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub format_id: Option<String>,
    pub destination_path: String,
    pub filename_template: String,
    #[serde(default = "default_auto_download_new_items")]
    pub auto_download_new_items: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateScheduleRequest {
    pub id: String,
    pub source_id: String,
    pub schedule_type: ScheduleType,
    pub interval_seconds: Option<i64>,
    pub next_run_at: Option<String>,
    pub enabled: bool,
    pub format_id: Option<String>,
    pub destination_path: String,
    pub filename_template: String,
    #[serde(default = "default_auto_download_new_items")]
    pub auto_download_new_items: bool,
}

fn default_enabled() -> bool {
    true
}

fn default_auto_download_new_items() -> bool {
    true
}

pub async fn get_schedules_core(services: &AppServices) -> Result<Vec<Schedule>, AppError> {
    services.list_schedules().await.map_err(repository_error)
}

pub async fn create_schedule_core(
    services: &AppServices,
    analyzer: &AnalyzerService,
    request: CreateScheduleRequest,
) -> Result<Schedule, AppError> {
    let now = now_utc();
    let id = format!(
        "schedule-{}-{}",
        SCHEDULE_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        now.replace([':', '-', '.'], "")
    );
    let schedule = Schedule {
        id,
        source_id: request.source_id,
        schedule_type: request.schedule_type,
        cron_expression: None,
        interval_seconds: request.interval_seconds,
        enabled: request.enabled,
        last_run_at: None,
        next_run_at: if request.enabled {
            Some(request.next_run_at)
        } else {
            None
        },
        configuration_json: Some(configuration_value(
            request.format_id,
            request.destination_path,
            request.filename_template,
            request.auto_download_new_items,
        )?),
        created_at: now.clone(),
        updated_at: now,
    };
    save_validated_schedule(services, analyzer, schedule).await
}

pub async fn update_schedule_core(
    services: &AppServices,
    analyzer: &AnalyzerService,
    request: UpdateScheduleRequest,
) -> Result<Schedule, AppError> {
    let current = services
        .get_schedule(request.id.trim())
        .await
        .map_err(repository_error)?
        .ok_or_else(|| not_found("schedule"))?;
    let schedule = Schedule {
        id: current.id,
        source_id: request.source_id,
        schedule_type: request.schedule_type,
        cron_expression: None,
        interval_seconds: request.interval_seconds,
        enabled: request.enabled,
        last_run_at: current.last_run_at,
        next_run_at: if request.enabled {
            request.next_run_at
        } else {
            None
        },
        configuration_json: Some(configuration_value(
            request.format_id,
            request.destination_path,
            request.filename_template,
            request.auto_download_new_items,
        )?),
        created_at: current.created_at,
        updated_at: now_utc(),
    };
    save_validated_schedule(services, analyzer, schedule).await
}

pub async fn delete_schedule_core(services: &AppServices, id: String) -> Result<bool, AppError> {
    if id.trim().is_empty() {
        return Err(invalid_request("A schedule ID is required."));
    }
    services
        .delete_schedule(id.trim())
        .await
        .map_err(repository_error)
}

pub async fn get_scheduler_enabled_core(services: &AppServices) -> Result<bool, AppError> {
    match services
        .settings
        .get_or_default(crate::application::settings_service::SettingKey::SchedulerEnabled)
        .await
        .map_err(|_| AppError {
            code: ErrorCode::DatabaseUnavailable,
            message: "The scheduler setting could not be read locally.".to_owned(),
            retryable: true,
            user_action: Some("Check available disk space and try again.".to_owned()),
            diagnostic: None,
        })? {
        Some(crate::application::settings_service::SettingValue::SchedulerEnabled(enabled)) => {
            Ok(enabled)
        }
        _ => Ok(false),
    }
}

pub async fn set_scheduler_enabled_core(
    services: &AppServices,
    enabled: bool,
) -> Result<bool, AppError> {
    services
        .settings
        .set(
            crate::application::settings_service::SettingKey::SchedulerEnabled,
            crate::application::settings_service::SettingValue::SchedulerEnabled(enabled),
        )
        .await
        .map_err(|_| AppError {
            code: ErrorCode::DatabaseUnavailable,
            message: "The scheduler setting could not be saved locally.".to_owned(),
            retryable: true,
            user_action: Some("Check available disk space and try again.".to_owned()),
            diagnostic: None,
        })?;
    Ok(enabled)
}

pub async fn run_scheduler_now_core(
    scheduler: &SchedulerLoop,
) -> Result<crate::scheduler::SchedulerRunReport, AppError> {
    scheduler
        .run_once(&now_utc())
        .await
        .map_err(scheduler_error)
}

async fn save_validated_schedule(
    services: &AppServices,
    analyzer: &AnalyzerService,
    schedule: Schedule,
) -> Result<Schedule, AppError> {
    validate_schedule(&schedule).map_err(scheduler_error)?;
    let source = services
        .repositories
        .media_sources
        .get(&schedule.source_id)
        .await
        .map_err(repository_error)?
        .ok_or_else(|| not_found("media_source"))?;
    ensure_schedulable_source(analyzer, &source.source_url, &source.platform_id)
        .map_err(scheduler_error)?;
    services
        .save_schedule(&schedule)
        .await
        .map_err(repository_error)?;
    Ok(schedule)
}

fn configuration_value(
    format_id: Option<String>,
    destination_path: String,
    filename_template: String,
    auto_download_new_items: bool,
) -> Result<serde_json::Value, AppError> {
    serde_json::to_value(ScheduleConfiguration {
        format_id,
        destination_path,
        filename_template,
        auto_download_new_items,
    })
    .map_err(|_| invalid_request("Schedule configuration could not be serialized."))
}

fn scheduler_error(error: SchedulerError) -> AppError {
    match error {
        SchedulerError::InvalidSchedule(message)
        | SchedulerError::InvalidConfiguration(message) => invalid_request(&message),
        SchedulerError::SchedulingUnsupported => AppError {
            code: ErrorCode::UnsupportedPlatform,
            message: "This public platform does not support scheduled monitoring.".to_owned(),
            retryable: false,
            user_action: Some("Choose a platform adapter with scheduling capability.".to_owned()),
            diagnostic: None,
        },
        SchedulerError::Repository(_) | SchedulerError::Queue(_) => AppError {
            code: ErrorCode::DatabaseUnavailable,
            message: "The scheduler could not update the local queue.".to_owned(),
            retryable: true,
            user_action: Some("Check local storage and try again.".to_owned()),
            diagnostic: None,
        },
        SchedulerError::Analysis(_) => AppError {
            code: ErrorCode::NetworkError,
            message: "The scheduled public source could not be analyzed.".to_owned(),
            retryable: true,
            user_action: Some("Check the connection and try again later.".to_owned()),
            diagnostic: None,
        },
    }
}

fn repository_error(error: RepositoryError) -> AppError {
    match error {
        RepositoryError::NotFound { entity, .. } => not_found(entity),
        RepositoryError::Storage { .. } => AppError {
            code: ErrorCode::DatabaseUnavailable,
            message: "The scheduler data could not be read or saved locally.".to_owned(),
            retryable: true,
            user_action: Some("Check available disk space and try again.".to_owned()),
            diagnostic: None,
        },
        RepositoryError::Conflict { .. } | RepositoryError::InvalidData { .. } => AppError {
            code: ErrorCode::DatabaseCorrupt,
            message: "The scheduler request conflicts with inconsistent local data.".to_owned(),
            retryable: false,
            user_action: Some("Restart the application and try again.".to_owned()),
            diagnostic: None,
        },
    }
}

fn not_found(entity: &'static str) -> AppError {
    AppError {
        code: ErrorCode::MediaUnavailable,
        message: format!("The scheduled {entity} could not be found locally."),
        retryable: false,
        user_action: Some("Analyze the public source again and retry.".to_owned()),
        diagnostic: None,
    }
}

fn invalid_request(message: &str) -> AppError {
    AppError {
        code: ErrorCode::UnknownError,
        message: message.to_owned(),
        retryable: false,
        user_action: Some("Review the schedule fields and try again.".to_owned()),
        diagnostic: None,
    }
}

fn now_utc() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}
