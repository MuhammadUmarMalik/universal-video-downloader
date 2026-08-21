use serde::Serialize;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(dead_code)] // Stable error catalog reserved for later domain/application phases.
pub enum ErrorCode {
    InvalidUrl,
    UnsupportedPlatform,
    AccessRestricted,
    AuthRequired,
    MediaUnavailable,
    FormatUnavailable,
    NetworkError,
    RateLimited,
    DiskFull,
    PermissionDenied,
    ChecksumFailed,
    FfmpegFailed,
    DatabaseUnavailable,
    DatabaseMigrationFailed,
    DatabaseCorrupt,
    UnknownError,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

impl AppError {
    #[allow(dead_code)] // Exercised by foundation tests; production constructors arrive with later use cases.
    pub fn foundation_failure(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::UnknownError,
            message: message.into(),
            retryable: false,
            user_action: Some("Restart the application and try again.".to_owned()),
            diagnostic: None,
        }
    }
}

impl Display for AppError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl Display for ErrorCode {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let code = match self {
            Self::InvalidUrl => "INVALID_URL",
            Self::UnsupportedPlatform => "UNSUPPORTED_PLATFORM",
            Self::AccessRestricted => "ACCESS_RESTRICTED",
            Self::AuthRequired => "AUTH_REQUIRED",
            Self::MediaUnavailable => "MEDIA_UNAVAILABLE",
            Self::FormatUnavailable => "FORMAT_UNAVAILABLE",
            Self::NetworkError => "NETWORK_ERROR",
            Self::RateLimited => "RATE_LIMITED",
            Self::DiskFull => "DISK_FULL",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::ChecksumFailed => "CHECKSUM_FAILED",
            Self::FfmpegFailed => "FFMPEG_FAILED",
            Self::DatabaseUnavailable => "DATABASE_UNAVAILABLE",
            Self::DatabaseMigrationFailed => "DATABASE_MIGRATION_FAILED",
            Self::DatabaseCorrupt => "DATABASE_CORRUPT",
            Self::UnknownError => "UNKNOWN_ERROR",
        };
        formatter.write_str(code)
    }
}

#[cfg(test)]
mod tests {
    use super::{AppError, ErrorCode};

    #[test]
    fn foundation_error_is_safe_for_ui_serialization() {
        let error = AppError::foundation_failure("Foundation is unavailable.");
        assert_eq!(error.code, ErrorCode::UnknownError);
        assert!(!error.retryable);
        assert_eq!(
            error.user_action.as_deref(),
            Some("Restart the application and try again.")
        );
    }
}
