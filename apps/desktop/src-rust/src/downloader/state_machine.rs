use crate::domain::entities::{DownloadJob, DownloadStatus};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DownloadStateError {
    #[error("download job transition from {from} to {to} is not allowed")]
    InvalidTransition {
        from: DownloadStatus,
        to: DownloadStatus,
    },
    #[error("download job has an invalid negative field: {field}")]
    NegativeField { field: &'static str },
    #[error("downloaded bytes cannot exceed the known total")]
    DownloadedExceedsTotal,
    #[error("a completed download must have all known bytes")]
    CompletedBeforeTotal,
    #[error("new download jobs must start in the queued state")]
    InitialStatusMustBeQueued,
    #[error("new download jobs must not contain progress or retry state")]
    InitialStateNotEmpty,
    #[error("download job field must not be empty: {field}")]
    EmptyField { field: &'static str },
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DownloadStateMachine;

impl DownloadStateMachine {
    pub const fn can_transition(from: DownloadStatus, to: DownloadStatus) -> bool {
        matches!(
            (from, to),
            (DownloadStatus::Queued, DownloadStatus::Resolving)
                | (DownloadStatus::Queued, DownloadStatus::Cancelled)
                | (DownloadStatus::Resolving, DownloadStatus::Downloading)
                | (DownloadStatus::Resolving, DownloadStatus::Failed)
                | (DownloadStatus::Resolving, DownloadStatus::Cancelled)
                | (DownloadStatus::Downloading, DownloadStatus::Paused)
                | (DownloadStatus::Downloading, DownloadStatus::Processing)
                | (DownloadStatus::Downloading, DownloadStatus::Completed)
                | (DownloadStatus::Downloading, DownloadStatus::Failed)
                | (DownloadStatus::Downloading, DownloadStatus::Cancelled)
                | (DownloadStatus::Paused, DownloadStatus::Downloading)
                | (DownloadStatus::Paused, DownloadStatus::Cancelled)
                | (DownloadStatus::Processing, DownloadStatus::Completed)
                | (DownloadStatus::Processing, DownloadStatus::Failed)
                | (DownloadStatus::Failed, DownloadStatus::Queued)
        )
    }

    pub fn validate_transition(
        from: DownloadStatus,
        to: DownloadStatus,
    ) -> Result<(), DownloadStateError> {
        Self::can_transition(from.clone(), to.clone())
            .then_some(())
            .ok_or(DownloadStateError::InvalidTransition { from, to })
    }

    pub fn validate_job(job: &DownloadJob) -> Result<(), DownloadStateError> {
        if job.priority < 0 {
            return Err(DownloadStateError::NegativeField { field: "priority" });
        }
        if job.downloaded_bytes < 0 {
            return Err(DownloadStateError::NegativeField {
                field: "downloaded_bytes",
            });
        }
        if job.total_bytes.is_some_and(|value| value < 0) {
            return Err(DownloadStateError::NegativeField {
                field: "total_bytes",
            });
        }
        if job
            .total_bytes
            .is_some_and(|total| job.downloaded_bytes > total)
        {
            return Err(DownloadStateError::DownloadedExceedsTotal);
        }
        if job.speed_bytes_per_sec.is_some_and(|value| value < 0) {
            return Err(DownloadStateError::NegativeField {
                field: "speed_bytes_per_sec",
            });
        }
        if job.eta_seconds.is_some_and(|value| value < 0) {
            return Err(DownloadStateError::NegativeField {
                field: "eta_seconds",
            });
        }
        if job.retry_count < 0 {
            return Err(DownloadStateError::NegativeField {
                field: "retry_count",
            });
        }
        if job.max_retries < 0 {
            return Err(DownloadStateError::NegativeField {
                field: "max_retries",
            });
        }
        if job.status == DownloadStatus::Completed
            && job
                .total_bytes
                .is_some_and(|total| job.downloaded_bytes < total)
        {
            return Err(DownloadStateError::CompletedBeforeTotal);
        }
        Ok(())
    }

    pub fn validate_new_job(job: &DownloadJob) -> Result<(), DownloadStateError> {
        Self::validate_job(job)?;
        if job.id.is_empty() {
            return Err(DownloadStateError::EmptyField { field: "id" });
        }
        if job.media_item_id.is_empty() {
            return Err(DownloadStateError::EmptyField {
                field: "media_item_id",
            });
        }
        if job.destination_path.is_empty() {
            return Err(DownloadStateError::EmptyField {
                field: "destination_path",
            });
        }
        if job.filename.is_empty() {
            return Err(DownloadStateError::EmptyField { field: "filename" });
        }
        if job.status != DownloadStatus::Queued {
            return Err(DownloadStateError::InitialStatusMustBeQueued);
        }
        if job.downloaded_bytes != 0
            || job.speed_bytes_per_sec.is_some()
            || job.eta_seconds.is_some()
            || job.retry_count != 0
            || job.started_at.is_some()
            || job.completed_at.is_some()
            || job.error_code.is_some()
            || job.error_message.is_some()
        {
            return Err(DownloadStateError::InitialStateNotEmpty);
        }
        Ok(())
    }

    pub fn transition(
        job: &DownloadJob,
        to: DownloadStatus,
    ) -> Result<DownloadJob, DownloadStateError> {
        Self::validate_job(job)?;
        Self::validate_transition(job.status.clone(), to.clone())?;
        let mut next = job.clone();
        next.status = to;
        Self::validate_job(&next)?;
        Ok(next)
    }

    pub fn validate_progress(
        downloaded_bytes: i64,
        total_bytes: Option<i64>,
    ) -> Result<(), DownloadStateError> {
        if downloaded_bytes < 0 {
            return Err(DownloadStateError::NegativeField {
                field: "downloaded_bytes",
            });
        }
        if total_bytes.is_some_and(|value| value < 0) {
            return Err(DownloadStateError::NegativeField {
                field: "total_bytes",
            });
        }
        if total_bytes.is_some_and(|total| downloaded_bytes > total) {
            return Err(DownloadStateError::DownloadedExceedsTotal);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{DownloadStateError, DownloadStateMachine};
    use crate::domain::entities::{DownloadJob, DownloadStatus};

    fn job(status: DownloadStatus) -> DownloadJob {
        DownloadJob {
            id: "job-1".to_owned(),
            media_item_id: "item-1".to_owned(),
            format_id: Some("format-1".to_owned()),
            status,
            priority: 0,
            destination_path: "/downloads/video.mp4".to_owned(),
            temp_path: Some("/downloads/video.mp4.part".to_owned()),
            filename: "video.mp4".to_owned(),
            total_bytes: Some(100),
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
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn allows_only_documented_transitions() {
        let allowed = [
            (DownloadStatus::Queued, DownloadStatus::Resolving),
            (DownloadStatus::Queued, DownloadStatus::Cancelled),
            (DownloadStatus::Resolving, DownloadStatus::Downloading),
            (DownloadStatus::Resolving, DownloadStatus::Failed),
            (DownloadStatus::Resolving, DownloadStatus::Cancelled),
            (DownloadStatus::Downloading, DownloadStatus::Paused),
            (DownloadStatus::Downloading, DownloadStatus::Processing),
            (DownloadStatus::Downloading, DownloadStatus::Completed),
            (DownloadStatus::Downloading, DownloadStatus::Failed),
            (DownloadStatus::Downloading, DownloadStatus::Cancelled),
            (DownloadStatus::Paused, DownloadStatus::Downloading),
            (DownloadStatus::Paused, DownloadStatus::Cancelled),
            (DownloadStatus::Processing, DownloadStatus::Completed),
            (DownloadStatus::Processing, DownloadStatus::Failed),
            (DownloadStatus::Failed, DownloadStatus::Queued),
        ];
        for (from, to) in allowed {
            assert!(DownloadStateMachine::can_transition(from, to));
        }

        for status in [
            DownloadStatus::Completed,
            DownloadStatus::Cancelled,
            DownloadStatus::Failed,
        ] {
            assert!(
                !DownloadStateMachine::can_transition(status.clone(), DownloadStatus::Queued)
                    || status == DownloadStatus::Failed
            );
        }
    }

    #[test]
    fn rejects_skipped_and_terminal_transitions() {
        for (from, to) in [
            (DownloadStatus::Queued, DownloadStatus::Downloading),
            (DownloadStatus::Resolving, DownloadStatus::Completed),
            (DownloadStatus::Paused, DownloadStatus::Completed),
            (DownloadStatus::Failed, DownloadStatus::Downloading),
            (DownloadStatus::Completed, DownloadStatus::Failed),
            (DownloadStatus::Cancelled, DownloadStatus::Queued),
        ] {
            assert!(matches!(
                DownloadStateMachine::validate_transition(from, to),
                Err(DownloadStateError::InvalidTransition { .. })
            ));
        }
    }

    #[test]
    fn validates_new_jobs_as_empty_queued_records() {
        let valid = job(DownloadStatus::Queued);
        assert!(DownloadStateMachine::validate_new_job(&valid).is_ok());

        let mut non_queued = valid.clone();
        non_queued.status = DownloadStatus::Resolving;
        assert_eq!(
            DownloadStateMachine::validate_new_job(&non_queued),
            Err(DownloadStateError::InitialStatusMustBeQueued)
        );

        let mut progressed = valid.clone();
        progressed.retry_count = 1;
        assert_eq!(
            DownloadStateMachine::validate_new_job(&progressed),
            Err(DownloadStateError::InitialStateNotEmpty)
        );

        let mut missing_filename = valid;
        missing_filename.filename.clear();
        assert_eq!(
            DownloadStateMachine::validate_new_job(&missing_filename),
            Err(DownloadStateError::EmptyField { field: "filename" })
        );
    }

    #[test]
    fn transitions_return_a_new_job_without_mutating_the_input() {
        let original = job(DownloadStatus::Queued);
        let next = DownloadStateMachine::transition(&original, DownloadStatus::Resolving).unwrap();
        assert_eq!(original.status, DownloadStatus::Queued);
        assert_eq!(next.status, DownloadStatus::Resolving);
        assert_eq!(next.id, original.id);
    }

    #[test]
    fn validates_progress_and_job_invariants() {
        assert!(DownloadStateMachine::validate_progress(50, Some(100)).is_ok());
        assert!(matches!(
            DownloadStateMachine::validate_progress(101, Some(100)),
            Err(DownloadStateError::DownloadedExceedsTotal)
        ));
        assert!(matches!(
            DownloadStateMachine::validate_progress(-1, Some(100)),
            Err(DownloadStateError::NegativeField {
                field: "downloaded_bytes"
            })
        ));

        let mut invalid = job(DownloadStatus::Downloading);
        invalid.downloaded_bytes = 101;
        assert_eq!(
            DownloadStateMachine::validate_job(&invalid),
            Err(DownloadStateError::DownloadedExceedsTotal)
        );
    }

    #[test]
    fn completed_jobs_must_reach_known_total() {
        let incomplete = job(DownloadStatus::Completed);
        assert_eq!(
            DownloadStateMachine::validate_job(&incomplete),
            Err(DownloadStateError::CompletedBeforeTotal)
        );

        let mut complete = incomplete;
        complete.downloaded_bytes = 100;
        assert!(DownloadStateMachine::validate_job(&complete).is_ok());
    }
}
