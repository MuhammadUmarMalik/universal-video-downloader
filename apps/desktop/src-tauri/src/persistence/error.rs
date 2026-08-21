use crate::domain::errors::{AppError, ErrorCode};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("the application data directory could not be resolved: {0}")]
    AppDataDirectory(String),
    #[error("the database directory could not be prepared")]
    PrepareDirectory(#[source] std::io::Error),
    #[error("the database connection could not be established")]
    Connection(#[source] sqlx::Error),
    #[error("database migrations could not be applied")]
    Migration(#[source] sqlx::migrate::MigrateError),
    #[error("database health check failed")]
    HealthCheck(#[source] sqlx::Error),
    #[error("SQLite foreign-key enforcement is disabled")]
    ForeignKeysDisabled,
}

#[allow(dead_code)] // Application and IPC layers consume these mappings in later Phase 2 subtasks.
impl PersistenceError {
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::AppDataDirectory(_) | Self::PrepareDirectory(_) | Self::Connection(_) => {
                ErrorCode::DatabaseUnavailable
            }
            Self::Migration(_) => ErrorCode::DatabaseMigrationFailed,
            Self::HealthCheck(_) | Self::ForeignKeysDisabled => ErrorCode::DatabaseCorrupt,
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(self, Self::Connection(_) | Self::HealthCheck(_))
    }

    pub fn into_app_error(self) -> AppError {
        let retryable = self.retryable();
        let code = self.error_code();
        let user_action = match code {
            ErrorCode::DatabaseMigrationFailed => {
                Some("Restart the application or restore the database backup.".to_owned())
            }
            ErrorCode::DatabaseCorrupt => {
                Some("Open diagnostics or restore the database backup.".to_owned())
            }
            _ if retryable => Some("Retry the operation.".to_owned()),
            _ => Some("Restart the application and try again.".to_owned()),
        };

        AppError {
            code,
            message: match code {
                ErrorCode::DatabaseMigrationFailed => "Database migration failed.".to_owned(),
                ErrorCode::DatabaseCorrupt => "The local database is not healthy.".to_owned(),
                ErrorCode::DatabaseUnavailable => "The local database is unavailable.".to_owned(),
                _ => "The local database operation failed.".to_owned(),
            },
            retryable,
            user_action,
            diagnostic: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PersistenceError;
    use crate::domain::errors::ErrorCode;

    #[test]
    fn migration_errors_are_safe_for_user_facing_mapping() {
        let error = PersistenceError::ForeignKeysDisabled.into_app_error();
        assert_eq!(error.code, ErrorCode::DatabaseCorrupt);
        assert!(!error.retryable);
        assert_eq!(error.diagnostic, None);
    }
}
