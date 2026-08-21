use crate::domain::errors::AppError;
use crate::domain::foundation::FoundationStatus;

#[tauri::command]
pub fn get_foundation_status() -> Result<FoundationStatus, AppError> {
    Ok(FoundationStatus::ready())
}
