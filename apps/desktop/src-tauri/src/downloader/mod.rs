//! Pure downloader domain policies for Phase 5.
//!
//! Network streaming, filesystem finalization, and bounded worker orchestration are
//! implemented behind this Rust boundary. IPC, resume validators, and scheduler integration
//! remain deferred to their dedicated phases.

mod cancellation;
mod finalization;
pub(crate) mod path_safety;
mod plan;
mod progress;
mod recovery;
mod retry;
mod state_machine;
mod storage;
mod streaming;
mod worker;

pub use cancellation::{CancellationRegistry, CancellationToken};
pub use finalization::{finalize_part, FinalizationError, FinalizationResult};
pub use path_safety::{validate_destination, DestinationPathError, DestinationPaths};
pub use plan::{DownloadPlan, DownloadPlanError};
pub use progress::{
    DownloadProgress, LiveProgressEvent, ProgressBroadcaster, ProgressSampler, StreamProgress,
};
pub use recovery::{RecoveryError, RecoveryReport, StartupRecoveryCoordinator};
pub use retry::{RetryDecision, RetryPolicy, RetryPolicyError, RetryRejection};
pub use state_machine::{DownloadStateError, DownloadStateMachine};
pub use storage::{
    ensure_available_space, harden_file_permissions, StorageError, MIN_FREE_HEADROOM_BYTES,
};
pub use streaming::{
    ResumableStreamResult, StreamResult, StreamingEngine, StreamingError,
    DEFAULT_MAX_RESPONSE_BYTES,
};
pub use worker::{
    execute_jobs_bounded, ApplicationJobExecutor, ClaimedJobExecutor, DownloadWorkerPool,
    EventIdSource, JobExecutionOutcome, WorkerPoolError, WorkerPoolReport,
};
