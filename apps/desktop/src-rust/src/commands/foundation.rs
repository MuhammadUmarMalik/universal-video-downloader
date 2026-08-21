use crate::domain::errors::AppError;
use crate::domain::foundation::FoundationStatus;

pub fn get_foundation_status_core() -> Result<FoundationStatus, AppError> {
    Ok(FoundationStatus::ready())
}
