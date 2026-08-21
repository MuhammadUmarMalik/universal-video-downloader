use crate::domain::errors::ErrorCode;
use thiserror::Error;

const MAX_JITTER_BASIS_POINTS: u16 = 1_000;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RetryPolicyError {
    #[error("retry backoff base must be at least one second")]
    ZeroBase,
    #[error("retry backoff base must not exceed its maximum")]
    BaseExceedsMaximum,
    #[error("retry backoff maximum must not exceed 86400 seconds")]
    MaximumExceedsLimit,
    #[error("retry jitter must be between 0 and 1000 basis points")]
    InvalidJitter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    base_seconds: u64,
    max_seconds: u64,
}

impl RetryPolicy {
    pub fn new(base_seconds: u64, max_seconds: u64) -> Result<Self, RetryPolicyError> {
        if base_seconds == 0 {
            return Err(RetryPolicyError::ZeroBase);
        }
        if base_seconds > max_seconds {
            return Err(RetryPolicyError::BaseExceedsMaximum);
        }
        if max_seconds > 86_400 {
            return Err(RetryPolicyError::MaximumExceedsLimit);
        }
        Ok(Self {
            base_seconds,
            max_seconds,
        })
    }

    pub const fn base_seconds(self) -> u64 {
        self.base_seconds
    }

    pub const fn max_seconds(self) -> u64 {
        self.max_seconds
    }

    pub const fn is_retryable(code: ErrorCode) -> bool {
        matches!(code, ErrorCode::NetworkError | ErrorCode::RateLimited)
    }

    pub fn decision(
        self,
        code: ErrorCode,
        retry_count: i64,
        max_retries: i64,
        jitter_basis_points: u16,
    ) -> Result<RetryDecision, RetryPolicyError> {
        if jitter_basis_points > MAX_JITTER_BASIS_POINTS {
            return Err(RetryPolicyError::InvalidJitter);
        }
        if retry_count < 0 || max_retries < 0 || retry_count >= max_retries {
            return Ok(RetryDecision::DoNotRetry {
                reason: RetryRejection::RetryBudgetExhausted,
            });
        }
        if !Self::is_retryable(code) {
            return Ok(RetryDecision::DoNotRetry {
                reason: RetryRejection::NonRetryableError,
            });
        }

        let delay_seconds = self.delay_seconds(retry_count as u32, jitter_basis_points);
        Ok(RetryDecision::Retry {
            next_retry_count: retry_count + 1,
            delay_seconds,
        })
    }

    pub fn delay_seconds(&self, retry_index: u32, jitter_basis_points: u16) -> u64 {
        let exponential = self
            .base_seconds
            .checked_mul(2_u64.saturating_pow(retry_index))
            .unwrap_or(self.max_seconds);
        let capped = exponential.min(self.max_seconds);
        let jitter = capped
            .saturating_mul(u64::from(jitter_basis_points))
            .checked_div(10_000)
            .unwrap_or(0);
        capped.saturating_add(jitter).min(self.max_seconds)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    Retry {
        next_retry_count: i64,
        delay_seconds: u64,
    },
    DoNotRetry {
        reason: RetryRejection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryRejection {
    NonRetryableError,
    RetryBudgetExhausted,
}

#[cfg(test)]
mod tests {
    use super::{RetryDecision, RetryPolicy, RetryPolicyError, RetryRejection};
    use crate::domain::errors::ErrorCode;

    #[test]
    fn classifies_only_network_and_rate_limit_failures_as_retryable() {
        assert!(RetryPolicy::is_retryable(ErrorCode::NetworkError));
        assert!(RetryPolicy::is_retryable(ErrorCode::RateLimited));
        for code in [
            ErrorCode::InvalidUrl,
            ErrorCode::UnsupportedPlatform,
            ErrorCode::AccessRestricted,
            ErrorCode::AuthRequired,
            ErrorCode::MediaUnavailable,
            ErrorCode::FormatUnavailable,
            ErrorCode::DiskFull,
            ErrorCode::PermissionDenied,
            ErrorCode::ChecksumFailed,
            ErrorCode::FfmpegFailed,
            ErrorCode::DatabaseUnavailable,
            ErrorCode::DatabaseMigrationFailed,
            ErrorCode::DatabaseCorrupt,
            ErrorCode::UnknownError,
        ] {
            assert!(!RetryPolicy::is_retryable(code));
        }
    }

    #[test]
    fn validates_backoff_bounds() {
        assert_eq!(RetryPolicy::new(0, 10), Err(RetryPolicyError::ZeroBase));
        assert_eq!(
            RetryPolicy::new(10, 2),
            Err(RetryPolicyError::BaseExceedsMaximum)
        );
        assert_eq!(
            RetryPolicy::new(1, 86_401),
            Err(RetryPolicyError::MaximumExceedsLimit)
        );
        assert_eq!(
            RetryPolicy::new(1, 10)
                .unwrap()
                .decision(ErrorCode::NetworkError, 0, 1, 1_001),
            Err(RetryPolicyError::InvalidJitter)
        );
    }

    #[test]
    fn computes_capped_exponential_backoff_with_bounded_jitter() {
        let policy = RetryPolicy::new(2, 15).unwrap();
        assert_eq!(policy.delay_seconds(0, 0), 2);
        assert_eq!(policy.delay_seconds(1, 0), 4);
        assert_eq!(policy.delay_seconds(2, 500), 8);
        assert_eq!(policy.delay_seconds(3, 1_000), 15);
        assert_eq!(policy.delay_seconds(20, 1_000), 15);
    }

    #[test]
    fn returns_retry_decisions_until_budget_is_exhausted() {
        let policy = RetryPolicy::new(2, 900).unwrap();
        assert_eq!(
            policy.decision(ErrorCode::NetworkError, 0, 3, 0),
            Ok(RetryDecision::Retry {
                next_retry_count: 1,
                delay_seconds: 2,
            })
        );
        assert_eq!(
            policy.decision(ErrorCode::RateLimited, 2, 3, 0),
            Ok(RetryDecision::Retry {
                next_retry_count: 3,
                delay_seconds: 8,
            })
        );
        assert_eq!(
            policy.decision(ErrorCode::NetworkError, 3, 3, 0),
            Ok(RetryDecision::DoNotRetry {
                reason: RetryRejection::RetryBudgetExhausted,
            })
        );
    }

    #[test]
    fn rejects_non_retryable_errors_and_invalid_counts() {
        let policy = RetryPolicy::new(2, 900).unwrap();
        for code in [ErrorCode::AuthRequired, ErrorCode::DiskFull] {
            assert_eq!(
                policy.decision(code, 0, 3, 0),
                Ok(RetryDecision::DoNotRetry {
                    reason: RetryRejection::NonRetryableError,
                })
            );
        }
        for retry_count in [-1, 3] {
            assert_eq!(
                policy.decision(ErrorCode::NetworkError, retry_count, 3, 0),
                Ok(RetryDecision::DoNotRetry {
                    reason: RetryRejection::RetryBudgetExhausted,
                })
            );
        }
    }
}
