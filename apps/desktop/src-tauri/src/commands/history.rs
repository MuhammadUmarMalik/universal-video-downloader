use crate::application::ports::RepositoryError;
use crate::application::services::AppServices;
use crate::domain::entities::HistoryEntry;
use crate::domain::errors::{AppError, ErrorCode};
use tauri::State;

fn repository_error(error: RepositoryError) -> AppError {
    let (code, message) = match error {
        RepositoryError::Storage { .. } => (
            ErrorCode::DatabaseUnavailable,
            "Local history storage is unavailable.",
        ),
        RepositoryError::Conflict { .. }
        | RepositoryError::InvalidData { .. }
        | RepositoryError::NotFound { .. } => (
            ErrorCode::UnknownError,
            "The local history operation was rejected.",
        ),
    };
    AppError {
        code,
        message: message.to_owned(),
        retryable: matches!(code, ErrorCode::DatabaseUnavailable),
        user_action: Some("Retry the history operation.".to_owned()),
        diagnostic: None,
    }
}

#[tauri::command]
pub async fn get_history(
    services: State<'_, AppServices>,
    query: Option<String>,
) -> Result<Vec<HistoryEntry>, AppError> {
    services
        .list_history(query.as_deref())
        .await
        .map_err(repository_error)
}

#[tauri::command]
pub async fn delete_history_entry(
    services: State<'_, AppServices>,
    id: String,
) -> Result<bool, AppError> {
    if id.trim().is_empty() {
        return Err(AppError {
            code: ErrorCode::UnknownError,
            message: "A history entry ID is required.".to_owned(),
            retryable: false,
            user_action: Some("Select a history entry and try again.".to_owned()),
            diagnostic: None,
        });
    }
    services
        .delete_history_entry(&id)
        .await
        .map_err(repository_error)
}

#[tauri::command]
pub async fn clear_history(services: State<'_, AppServices>) -> Result<u64, AppError> {
    services.clear_history().await.map_err(repository_error)
}
