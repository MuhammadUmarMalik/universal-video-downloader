pub mod adapters;
pub mod application;
mod commands;
pub mod domain;
pub mod downloader;
mod infrastructure;
pub mod media;
pub mod persistence;
mod scheduler;
mod security;

use commands::analyzer::analyze_url;
use commands::downloader::{
    cancel_download, create_download, get_download_jobs, subscribe_download_progress,
};
use commands::foundation::get_foundation_status;
use commands::history::{clear_history, delete_history_entry, get_history};
use commands::scheduler::{
    create_schedule, delete_schedule, get_scheduler_enabled, get_schedules, run_scheduler_now,
    set_scheduler_enabled, update_schedule,
};
use downloader::{DownloadWorkerPool, StartupRecoveryCoordinator, StreamingEngine};
use infrastructure::logging::init_logging;
use persistence::{Database, PersistenceError};
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_foundation_status,
            analyze_url,
            create_download,
            cancel_download,
            get_download_jobs,
            subscribe_download_progress,
            get_history,
            delete_history_entry,
            clear_history,
            get_schedules,
            create_schedule,
            update_schedule,
            delete_schedule,
            get_scheduler_enabled,
            set_scheduler_enabled,
            run_scheduler_now
        ])
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().map_err(|error| {
                Box::new(PersistenceError::AppDataDirectory(error.to_string()))
                    as Box<dyn std::error::Error>
            })?;
            let database =
                tauri::async_runtime::block_on(Database::from_app_data_dir(app_data_dir))
                    .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;

            let services = application::services::AppServices::from_database(&database);
            let analyzer = application::analyzer::AnalyzerService::with_defaults(services.clone())
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
            let worker_pool = tauri::async_runtime::block_on(DownloadWorkerPool::from_services(
                Arc::new(services.clone()),
                Arc::new(
                    StreamingEngine::new()
                        .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?,
                ),
            ))
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
            let services = Arc::new(services);
            let recovery = StartupRecoveryCoordinator::new(Arc::clone(&services));
            let recovery_report = tauri::async_runtime::block_on(recovery.recover())
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
            tracing::info!(
                event = "startup_recovery_completed",
                inspected = recovery_report.inspected,
                requeued = recovery_report.requeued,
                completed = recovery_report.completed,
                failed = recovery_report.failed
            );
            let scheduler = scheduler::SchedulerLoop::new(
                Arc::clone(&services),
                analyzer.clone(),
                worker_pool.clone(),
            );
            scheduler.start();
            app.manage(database);
            app.manage((*services).clone());
            app.manage(analyzer);
            app.manage(worker_pool);
            app.manage(scheduler);
            tracing::info!(event = "database_initialized");
            tracing::info!(event = "application_services_initialized");
            tracing::info!(event = "analyzer_service_initialized");
            tracing::info!(event = "download_worker_pool_initialized");
            tracing::info!(event = "app_started", app = ?app.package_info().name);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Universal Media Downloader");
}
