use crate::domain::errors::{AppError, ErrorCode};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};

pub const SUPPORTED_FFMPEG_MAJOR: u64 = 6;
pub const DEFAULT_FFMPEG_TIMEOUT: Duration = Duration::from_secs(300);
pub const DEFAULT_MAX_FFMPEG_DIAGNOSTIC_BYTES: usize = 16 * 1024;
const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegExecutable {
    path: PathBuf,
    major_version: u64,
}

impl FfmpegExecutable {
    pub async fn resolve_system() -> Result<Self, FfmpegResolveError> {
        let path = locate_on_path().ok_or(FfmpegResolveError::NotFound)?;
        Self::resolve_path(path).await
    }

    pub(crate) async fn resolve_path(path: PathBuf) -> Result<Self, FfmpegResolveError> {
        validate_executable_file(&path)?;
        let output = Command::new(&path)
            .arg("-version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .map_err(|_| FfmpegResolveError::VersionCommandFailed)?;
        if !output.status.success() {
            return Err(FfmpegResolveError::VersionCommandFailed);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let major_version =
            parse_major_version(&stdout).ok_or(FfmpegResolveError::VersionOutputInvalid)?;
        if major_version != SUPPORTED_FFMPEG_MAJOR {
            return Err(FfmpegResolveError::UnsupportedMajor {
                expected: SUPPORTED_FFMPEG_MAJOR,
                actual: major_version,
            });
        }
        Ok(Self {
            path,
            major_version,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn major_version(&self) -> u64 {
        self.major_version
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FfmpegResolveError {
    #[error("FFmpeg was not found on the system PATH")]
    NotFound,
    #[error("the configured FFmpeg path is not an executable file")]
    NotExecutable,
    #[error("FFmpeg version probing failed")]
    VersionCommandFailed,
    #[error("FFmpeg version output could not be parsed")]
    VersionOutputInvalid,
    #[error("unsupported FFmpeg major version: expected {expected}, found {actual}")]
    UnsupportedMajor { expected: u64, actual: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FfmpegArguments(Vec<OsString>);

impl FfmpegArguments {
    pub(crate) fn new(arguments: Vec<OsString>) -> Result<Self, FfmpegRunError> {
        if arguments.is_empty() {
            return Err(FfmpegRunError::EmptyArguments);
        }
        Ok(Self(arguments))
    }

    #[cfg(test)]
    pub(crate) fn from_strings(arguments: &[&str]) -> Result<Self, FfmpegRunError> {
        Self::new(arguments.iter().map(OsString::from).collect())
    }

    pub(crate) fn as_slice(&self) -> &[OsString] {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FfmpegProcessRunner {
    executable: FfmpegExecutable,
    timeout: Duration,
    max_diagnostic_bytes: usize,
}

impl FfmpegProcessRunner {
    pub(crate) fn new(
        executable: FfmpegExecutable,
        timeout: Duration,
        max_diagnostic_bytes: usize,
    ) -> Result<Self, FfmpegRunError> {
        if timeout.is_zero() || max_diagnostic_bytes == 0 {
            return Err(FfmpegRunError::InvalidRunnerConfiguration);
        }
        Ok(Self {
            executable,
            timeout,
            max_diagnostic_bytes,
        })
    }

    pub(crate) async fn run(
        &self,
        arguments: FfmpegArguments,
        cancelled: &AtomicBool,
    ) -> Result<FfmpegRunResult, FfmpegRunError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(FfmpegRunError::Cancelled);
        }
        let mut child = Command::new(&self.executable.path)
            .args(arguments.as_slice())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| FfmpegRunError::SpawnFailed)?;
        let stderr = child
            .stderr
            .take()
            .ok_or(FfmpegRunError::StderrUnavailable)?;
        let max_diagnostic_bytes = self.max_diagnostic_bytes;
        let diagnostic_task =
            tokio::spawn(
                async move { read_bounded_diagnostic(stderr, max_diagnostic_bytes).await },
            );
        let started_at = Instant::now();
        let wait_result = timeout(self.timeout, wait_for_exit(&mut child, cancelled)).await;
        let status = match wait_result {
            Ok(Ok(status)) => status,
            Ok(Err(FfmpegRunError::Cancelled)) => {
                terminate_child(&mut child).await;
                let _ = diagnostic_task.await;
                return Err(FfmpegRunError::Cancelled);
            }
            Ok(Err(error)) => {
                terminate_child(&mut child).await;
                let _ = diagnostic_task.await;
                return Err(error);
            }
            Err(_) => {
                terminate_child(&mut child).await;
                let _ = diagnostic_task.await;
                return Err(FfmpegRunError::TimedOut {
                    timeout_millis: self.timeout.as_millis().min(u64::MAX as u128) as u64,
                });
            }
        };
        let diagnostic = diagnostic_task
            .await
            .map_err(|_| FfmpegRunError::DiagnosticTaskFailed)??;
        let elapsed = started_at.elapsed();
        let Some(code) = status.code() else {
            return Err(FfmpegRunError::Terminated { diagnostic });
        };
        if !status.success() {
            return Err(FfmpegRunError::NonZeroExit { code, diagnostic });
        }
        Ok(FfmpegRunResult {
            code,
            diagnostic,
            elapsed,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FfmpegRunResult {
    pub code: i32,
    pub diagnostic: Option<String>,
    pub elapsed: Duration,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum FfmpegRunError {
    #[error("FFmpeg arguments must not be empty")]
    EmptyArguments,
    #[error("FFmpeg runner configuration is invalid")]
    InvalidRunnerConfiguration,
    #[error("FFmpeg process could not be spawned")]
    SpawnFailed,
    #[error("FFmpeg stderr could not be captured")]
    StderrUnavailable,
    #[error("FFmpeg diagnostic collection failed")]
    DiagnosticTaskFailed,
    #[error("FFmpeg execution was cancelled")]
    Cancelled,
    #[error("FFmpeg execution timed out after {timeout_millis} milliseconds")]
    TimedOut { timeout_millis: u64 },
    #[error("FFmpeg exited with a non-zero status: {code}")]
    NonZeroExit {
        code: i32,
        diagnostic: Option<String>,
    },
    #[error("FFmpeg process terminated without an exit code")]
    Terminated { diagnostic: Option<String> },
}

impl FfmpegResolveError {
    #[allow(dead_code)]
    pub fn to_app_error(&self) -> AppError {
        AppError {
            code: ErrorCode::FfmpegFailed,
            message: "FFmpeg is unavailable or does not meet the supported version policy."
                .to_owned(),
            retryable: false,
            user_action: Some(
                "Install the supported system FFmpeg version and try again.".to_owned(),
            ),
            diagnostic: None,
        }
    }
}

impl FfmpegRunError {
    #[allow(dead_code)]
    pub fn to_app_error(&self) -> AppError {
        let (message, user_action) = match self {
            Self::Cancelled => (
                "FFmpeg processing was cancelled.",
                "Retry the processing operation if needed.",
            ),
            Self::TimedOut { .. } => (
                "FFmpeg processing timed out.",
                "Try a smaller input or retry the operation.",
            ),
            Self::NonZeroExit { .. } | Self::Terminated { .. } => (
                "FFmpeg could not process the selected media.",
                "Verify the media files and retry the operation.",
            ),
            Self::EmptyArguments
            | Self::InvalidRunnerConfiguration
            | Self::SpawnFailed
            | Self::StderrUnavailable
            | Self::DiagnosticTaskFailed => (
                "FFmpeg processing could not be started.",
                "Check the FFmpeg installation and retry the operation.",
            ),
        };
        AppError {
            code: ErrorCode::FfmpegFailed,
            message: message.to_owned(),
            retryable: matches!(self, Self::TimedOut { .. }),
            user_action: Some(user_action.to_owned()),
            diagnostic: None,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn diagnostic(&self) -> Option<&str> {
        match self {
            Self::NonZeroExit { diagnostic, .. } | Self::Terminated { diagnostic } => {
                diagnostic.as_deref()
            }
            _ => None,
        }
    }
}

async fn wait_for_exit(
    child: &mut Child,
    cancelled: &AtomicBool,
) -> Result<std::process::ExitStatus, FfmpegRunError> {
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(FfmpegRunError::Cancelled);
        }
        if let Some(status) = child.try_wait().map_err(|_| FfmpegRunError::SpawnFailed)? {
            return Ok(status);
        }
        sleep(CANCELLATION_POLL_INTERVAL).await;
    }
}

async fn terminate_child(child: &mut Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

async fn read_bounded_diagnostic<R>(
    mut reader: R,
    limit: usize,
) -> Result<Option<String>, FfmpegRunError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(limit.min(4096));
    let mut buffer = [0_u8; 4096];
    let mut truncated = false;
    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(|_| FfmpegRunError::DiagnosticTaskFailed)?;
        if read == 0 {
            break;
        }
        if bytes.len() < limit {
            let remaining = limit - bytes.len();
            let take = remaining.min(read);
            bytes.extend_from_slice(&buffer[..take]);
            if take < read {
                truncated = true;
            }
        } else {
            truncated = true;
        }
    }
    if bytes.is_empty() {
        return Ok(None);
    }
    let mut diagnostic = String::from_utf8_lossy(&bytes).trim().to_owned();
    if truncated {
        diagnostic.push_str(" [truncated]");
    }
    Ok((!diagnostic.is_empty()).then_some(diagnostic))
}

fn locate_on_path() -> Option<PathBuf> {
    let executable_name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(executable_name))
        .find(|candidate| validate_executable_file(candidate).is_ok())
}

fn validate_executable_file(path: &Path) -> Result<(), FfmpegResolveError> {
    let metadata = std::fs::metadata(path).map_err(|_| FfmpegResolveError::NotExecutable)?;
    if !metadata.is_file() {
        return Err(FfmpegResolveError::NotExecutable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        if mode & 0o111 == 0 || mode & 0o022 != 0 {
            return Err(FfmpegResolveError::NotExecutable);
        }
    }
    Ok(())
}

fn parse_major_version(output: &str) -> Option<u64> {
    let first_line = output.lines().next()?;
    let version = first_line
        .strip_prefix("ffmpeg version ")?
        .split_whitespace()
        .next()?;
    version
        .trim_start_matches('n')
        .split('.')
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::{
        parse_major_version, validate_executable_file, FfmpegArguments, FfmpegExecutable,
        FfmpegProcessRunner, FfmpegResolveError, FfmpegRunError, SUPPORTED_FFMPEG_MAJOR,
    };
    use crate::domain::errors::ErrorCode;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    #[test]
    fn parses_supported_and_invalid_version_output() {
        assert_eq!(
            parse_major_version("ffmpeg version 6.1.1 Copyright"),
            Some(6)
        );
        assert_eq!(parse_major_version("ffmpeg version n6.1.1"), Some(6));
        assert_eq!(parse_major_version("not ffmpeg"), None);
    }

    #[tokio::test]
    async fn resolves_the_development_system_ffmpeg_when_available() {
        let result = FfmpegExecutable::resolve_system().await;
        if let Ok(executable) = result {
            assert_eq!(executable.major_version(), SUPPORTED_FFMPEG_MAJOR);
            assert!(executable.path().is_absolute());
        }
    }

    #[tokio::test]
    async fn rejects_missing_and_unsupported_executables() {
        assert_eq!(
            FfmpegExecutable::resolve_path(PathBuf::from("/definitely/missing/ffmpeg")).await,
            Err(FfmpegResolveError::NotExecutable)
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_world_writable_ffmpeg_binaries() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::set_permissions(file.path(), fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(
            validate_executable_file(file.path()),
            Err(FfmpegResolveError::NotExecutable)
        );
    }

    #[test]
    fn rejects_empty_arguments_and_invalid_runner_configuration() {
        assert_eq!(
            FfmpegArguments::new(Vec::new()),
            Err(FfmpegRunError::EmptyArguments)
        );
        let executable = FfmpegExecutable {
            path: PathBuf::from("ffmpeg"),
            major_version: SUPPORTED_FFMPEG_MAJOR,
        };
        assert_eq!(
            FfmpegProcessRunner::new(executable.clone(), Duration::ZERO, 16),
            Err(FfmpegRunError::InvalidRunnerConfiguration)
        );
        assert_eq!(
            FfmpegProcessRunner::new(executable, Duration::from_secs(1), 0),
            Err(FfmpegRunError::InvalidRunnerConfiguration)
        );
    }

    #[tokio::test]
    async fn maps_a_successful_direct_process_without_shell_interpretation() {
        let Ok(executable) = FfmpegExecutable::resolve_system().await else {
            return;
        };
        let runner = FfmpegProcessRunner::new(executable, Duration::from_secs(5), 1024).unwrap();
        let args = FfmpegArguments::from_strings(&["-version"]).unwrap();
        let result = runner.run(args, &AtomicBool::new(false)).await.unwrap();
        assert_eq!(result.code, 0);
    }

    #[tokio::test]
    async fn captures_a_bounded_diagnostic_for_non_zero_exit() {
        let Ok(executable) = FfmpegExecutable::resolve_system().await else {
            return;
        };
        let runner = FfmpegProcessRunner::new(executable, Duration::from_secs(5), 8).unwrap();
        let args = FfmpegArguments::from_strings(&["-this-argument-does-not-exist"]).unwrap();
        let error = runner.run(args, &AtomicBool::new(false)).await.unwrap_err();
        let FfmpegRunError::NonZeroExit { diagnostic, .. } = error else {
            panic!("invalid FFmpeg arguments should return a non-zero exit");
        };
        let diagnostic = diagnostic.expect("FFmpeg should report a bounded diagnostic");
        assert!(diagnostic.ends_with("[truncated]"));
        assert!(diagnostic.len() <= 8 + " [truncated]".len());
    }

    #[tokio::test]
    async fn maps_timeout_and_cancellation_without_exposing_diagnostics() {
        let Ok(executable) = FfmpegExecutable::resolve_system().await else {
            return;
        };
        let runner =
            FfmpegProcessRunner::new(executable.clone(), Duration::from_millis(100), 1024).unwrap();
        let long_args = FfmpegArguments::from_strings(&[
            "-hide_banner",
            "-loglevel",
            "error",
            "-re",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=16x16:r=1",
            "-t",
            "10",
            "-f",
            "null",
            "-",
        ])
        .unwrap();
        let timeout_error = runner
            .run(long_args, &AtomicBool::new(false))
            .await
            .unwrap_err();
        assert!(matches!(timeout_error, FfmpegRunError::TimedOut { .. }));
        let timeout_app_error = timeout_error.to_app_error();
        assert_eq!(timeout_app_error.code, ErrorCode::FfmpegFailed);
        assert_eq!(timeout_app_error.message, "FFmpeg processing timed out.");
        assert!(timeout_app_error.retryable);
        assert_eq!(timeout_app_error.diagnostic, None);
        let app_error = FfmpegRunError::NonZeroExit {
            code: 1,
            diagnostic: Some("private diagnostic".to_owned()),
        }
        .to_app_error();
        assert_eq!(app_error.code, ErrorCode::FfmpegFailed);
        assert_eq!(app_error.diagnostic, None);

        let runner = FfmpegProcessRunner::new(executable, Duration::from_secs(5), 1024).unwrap();
        let cancellation = std::sync::Arc::new(AtomicBool::new(false));
        let cancellation_for_task = std::sync::Arc::clone(&cancellation);
        let task = tokio::spawn(async move {
            runner
                .run(long_args_for_cancellation(), &cancellation_for_task)
                .await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancellation.store(true, Ordering::Release);
        assert!(matches!(
            task.await.unwrap(),
            Err(FfmpegRunError::Cancelled)
        ));
    }

    fn long_args_for_cancellation() -> FfmpegArguments {
        FfmpegArguments::from_strings(&[
            "-hide_banner",
            "-loglevel",
            "error",
            "-re",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=16x16:r=1",
            "-t",
            "10",
            "-f",
            "null",
            "-",
        ])
        .unwrap()
    }
}
