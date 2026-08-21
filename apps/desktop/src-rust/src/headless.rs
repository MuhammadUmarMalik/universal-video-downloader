use crate::application::analyzer::{AnalyzeRequest, AnalyzerService};
use crate::application::services::AppServices;
use crate::commands::analyzer::analyze_url_core;
use crate::commands::downloader::{
    cancel_download_core, create_download_core, get_bandwidth_status_core, get_download_jobs_core,
    set_bandwidth_limit_core, CreateDownloadRequest,
};
use crate::commands::foundation::get_foundation_status_core;
use crate::commands::history::{clear_history_core, delete_history_entry_core, get_history_core};
use crate::commands::scheduler::{
    create_schedule_core, delete_schedule_core, get_scheduler_enabled_core, get_schedules_core,
    run_scheduler_now_core, set_scheduler_enabled_core, update_schedule_core,
    CreateScheduleRequest, UpdateScheduleRequest,
};
use crate::domain::errors::{AppError, ErrorCode};
use crate::downloader::{
    DownloadWorkerPool, LiveProgressEvent, StartupRecoveryCoordinator, StreamingEngine,
};
use crate::infrastructure::logging::init_logging;
use crate::persistence::Database;
use crate::scheduler::SchedulerLoop;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::Mutex;

#[derive(Debug, Deserialize)]
struct RpcRequest {
    id: Value,
    command: String,
    #[serde(default)]
    args: Value,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    id: Value,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<AppError>,
}

#[derive(Debug, Serialize)]
struct RpcEvent<'a> {
    event: &'a str,
    payload: LiveProgressEvent,
}

struct RuntimeState {
    _database: Database,
    services: Arc<AppServices>,
    analyzer: AnalyzerService,
    pool: DownloadWorkerPool<crate::downloader::ApplicationJobExecutor>,
    scheduler: SchedulerLoop,
}

type Output = Arc<Mutex<BufWriter<tokio::io::Stdout>>>;

pub fn run_headless() {
    init_logging();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("headless runtime should initialize");
    runtime.block_on(async {
        if let Err(error) = run_server().await {
            tracing::error!(event = "headless_runtime_failed", error = %error);
        }
    });
}

async fn run_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = initialize_state().await?;
    let output: Output = Arc::new(Mutex::new(BufWriter::new(tokio::io::stdout())));
    let mut lines = BufReader::new(tokio::io::stdin()).lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let request = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(request) => request,
            Err(error) => {
                write_line(
                    &output,
                    &RpcResponse {
                        id: Value::Null,
                        ok: false,
                        result: None,
                        error: Some(protocol_error(&format!("Invalid IPC request: {error}"))),
                    },
                )
                .await?;
                continue;
            }
        };
        let response = dispatch(&state, &output, request).await;
        write_line(&output, &response).await?;
    }
    Ok(())
}

async fn initialize_state() -> Result<RuntimeState, Box<dyn std::error::Error + Send + Sync>> {
    let app_data_dir = std::env::var_os("UMD_APP_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".umd-data"));
    let database = Database::from_app_data_dir(app_data_dir)
        .await
        .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)?;
    let services = AppServices::from_database(&database);
    let analyzer = AnalyzerService::with_defaults(services.clone())
        .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)?;
    let services_arc = Arc::new(services);
    let pool = DownloadWorkerPool::from_services(
        Arc::clone(&services_arc),
        Arc::new(
            StreamingEngine::new()
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)?,
        ),
    )
    .await
    .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)?;
    let recovery = StartupRecoveryCoordinator::new(Arc::clone(&services_arc));
    let report = recovery
        .recover()
        .await
        .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)?;
    tracing::info!(
        event = "headless_startup_recovery_completed",
        inspected = report.inspected,
        requeued = report.requeued,
        completed = report.completed,
        failed = report.failed
    );
    let scheduler = SchedulerLoop::new(Arc::clone(&services_arc), analyzer.clone(), pool.clone());
    scheduler.start();
    Ok(RuntimeState {
        _database: database,
        services: services_arc,
        analyzer,
        pool,
        scheduler,
    })
}

async fn dispatch(state: &RuntimeState, output: &Output, request: RpcRequest) -> RpcResponse {
    let id = request.id.clone();
    let result = match request.command.as_str() {
        "get_foundation_status" => value(get_foundation_status_core()),
        "analyze_url" => match parse_arg::<AnalyzeRequest>(&request.args, "request") {
            Ok(input) => value(analyze_url_core(&state.analyzer, input).await),
            Err(error) => Err(error),
        },
        "create_download" => match parse_arg::<CreateDownloadRequest>(&request.args, "request") {
            Ok(input) => value(create_download_core(&state.services, &state.pool, input).await),
            Err(error) => Err(error),
        },
        "get_bandwidth_status" => value(get_bandwidth_status_core(&state.pool)),
        "set_bandwidth_limit" => match parse_limit(&request.args) {
            Ok(limit) => value(set_bandwidth_limit_core(&state.services, &state.pool, limit).await),
            Err(error) => Err(error),
        },
        "cancel_download" => value(
            parse_string_arg(&request.args, "jobId")
                .and_then(|job_id| cancel_download_core(&state.pool, job_id)),
        ),
        "get_download_jobs" => value(get_download_jobs_core(&state.services).await),
        "subscribe_download_progress" => {
            subscribe_progress(state, output.clone()).await;
            Ok(json!(true))
        }
        "get_history" => value(
            get_history_core(&state.services, optional_string_arg(&request.args, "query")).await,
        ),
        "delete_history_entry" => match parse_string_arg(&request.args, "id") {
            Ok(id) => value(delete_history_entry_core(&state.services, id).await),
            Err(error) => Err(error),
        },
        "clear_history" => value(clear_history_core(&state.services).await),
        "get_schedules" => value(get_schedules_core(&state.services).await),
        "create_schedule" => match parse_arg::<CreateScheduleRequest>(&request.args, "request") {
            Ok(input) => value(create_schedule_core(&state.services, &state.analyzer, input).await),
            Err(error) => Err(error),
        },
        "update_schedule" => match parse_arg::<UpdateScheduleRequest>(&request.args, "request") {
            Ok(input) => value(update_schedule_core(&state.services, &state.analyzer, input).await),
            Err(error) => Err(error),
        },
        "delete_schedule" => match parse_string_arg(&request.args, "id") {
            Ok(id) => value(delete_schedule_core(&state.services, id).await),
            Err(error) => Err(error),
        },
        "get_scheduler_enabled" => value(get_scheduler_enabled_core(&state.services).await),
        "set_scheduler_enabled" => match parse_bool_arg(&request.args, "enabled") {
            Ok(enabled) => value(set_scheduler_enabled_core(&state.services, enabled).await),
            Err(error) => Err(error),
        },
        "run_scheduler_now" => value(run_scheduler_now_core(&state.scheduler).await),
        _ => Err(protocol_error(
            "The requested desktop command is not supported.",
        )),
    };

    match result {
        Ok(result) => RpcResponse {
            id,
            ok: true,
            result: Some(result),
            error: None,
        },
        Err(error) => RpcResponse {
            id,
            ok: false,
            result: None,
            error: Some(error),
        },
    }
}

async fn subscribe_progress(state: &RuntimeState, output: Output) {
    let mut receiver = state.pool.subscribe_progress();
    tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(payload) => {
                    if write_line(
                        &output,
                        &RpcEvent {
                            event: "download-progress",
                            payload,
                        },
                    )
                    .await
                    .is_err()
                    {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

async fn write_line<T: Serialize>(output: &Output, value: &T) -> Result<(), std::io::Error> {
    let line = serde_json::to_vec(value).map_err(std::io::Error::other)?;
    let mut writer = output.lock().await;
    writer.write_all(&line).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await
}

fn value<T>(result: Result<T, AppError>) -> Result<Value, AppError>
where
    T: Serialize,
{
    result.map(|value| serde_json::to_value(value).unwrap_or(Value::Null))
}

fn parse_arg<T>(args: &Value, key: &str) -> Result<T, AppError>
where
    T: for<'de> Deserialize<'de>,
{
    let value = args
        .get(key)
        .cloned()
        .ok_or_else(|| protocol_error("Missing IPC argument."))?;
    serde_json::from_value(value)
        .map_err(|_| protocol_error("The IPC argument has an invalid shape."))
}

fn parse_string_arg(args: &Value, key: &str) -> Result<String, AppError> {
    let value = args
        .get(key)
        .or_else(|| args.get(key.trim_end_matches("Id")))
        .and_then(Value::as_str)
        .ok_or_else(|| protocol_error("The IPC string argument is missing."))?;
    Ok(value.to_owned())
}

fn optional_string_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn parse_bool_arg(args: &Value, key: &str) -> Result<bool, AppError> {
    args.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| protocol_error("The IPC boolean argument is missing."))
}

fn parse_limit(args: &Value) -> Result<u32, AppError> {
    args.get("limitKbps")
        .or_else(|| args.get("limit_kbps"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| protocol_error("The bandwidth limit must be an unsigned integer."))
}

fn protocol_error(message: &str) -> AppError {
    AppError {
        code: ErrorCode::UnknownError,
        message: message.to_owned(),
        retryable: false,
        user_action: Some("Restart the desktop application and try again.".to_owned()),
        diagnostic: None,
    }
}
