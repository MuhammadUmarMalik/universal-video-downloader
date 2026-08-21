//! Typed media-processing plans and filesystem safety policy.
//!
//! Slice 6.1 deliberately stops before subprocess execution. The types in this
//! module describe the only supported operations and produce validated paths
//! for the future Rust-owned FFmpeg runner.

use crate::downloader::harden_file_permissions;
use crate::downloader::path_safety::{
    validate_destination, validate_path_within_root, DestinationPathError,
};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

const MAX_PROCESSING_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
use thiserror::Error;

pub(crate) mod process;

pub use process::{FfmpegExecutable, FfmpegResolveError, SUPPORTED_FFMPEG_MAJOR};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum MediaProcessingConfiguration {
    MergeAudioVideo {
        video_input: String,
        audio_input: String,
        output_filename: String,
    },
    ExtractAudio {
        input: String,
        output_filename: String,
    },
}

impl MediaProcessingConfiguration {
    pub fn into_request(self) -> MediaProcessingRequest {
        match self {
            Self::MergeAudioVideo {
                video_input,
                audio_input,
                output_filename,
            } => MediaProcessingRequest::MergeAudioVideo {
                video_input: PathBuf::from(video_input),
                audio_input: PathBuf::from(audio_input),
                output_filename,
            },
            Self::ExtractAudio {
                input,
                output_filename,
            } => MediaProcessingRequest::ExtractAudio {
                input: PathBuf::from(input),
                output_filename,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaProcessingRequest {
    MergeAudioVideo {
        video_input: PathBuf,
        audio_input: PathBuf,
        output_filename: String,
    },
    ExtractAudio {
        input: PathBuf,
        output_filename: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaProcessingOperation {
    MergeAudioVideo {
        video_input: PathBuf,
        audio_input: PathBuf,
    },
    ExtractAudio {
        input: PathBuf,
    },
}

impl MediaProcessingOperation {
    fn input_paths(&self) -> Vec<&Path> {
        match self {
            Self::MergeAudioVideo {
                video_input,
                audio_input,
            } => vec![video_input.as_path(), audio_input.as_path()],
            Self::ExtractAudio { input } => vec![input.as_path()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessingOutput {
    pub root: PathBuf,
    pub final_path: PathBuf,
    pub temporary_path: PathBuf,
    pub filename: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaProcessingPlan {
    pub operation: MediaProcessingOperation,
    pub output: ProcessingOutput,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MediaProcessingError {
    #[error("media processing input path is unsafe: {0}")]
    InputPath(DestinationPathError),
    #[error("media processing output path is unsafe: {0}")]
    OutputPath(DestinationPathError),
    #[error("media processing input does not exist")]
    InputMissing,
    #[error("media processing input is not a regular file")]
    InputNotRegularFile,
    #[error("media processing input exceeds the supported size limit")]
    InputTooLarge,
    #[error("media processing inputs must be distinct")]
    DuplicateInput,
    #[error("media processing output already exists")]
    OutputAlreadyExists,
    #[error("media processing temporary output already exists")]
    TemporaryOutputAlreadyExists,
}

impl MediaProcessingPlan {
    pub fn resolve(
        root: &Path,
        request: MediaProcessingRequest,
    ) -> Result<Self, MediaProcessingError> {
        let (operation, output_filename) = match request {
            MediaProcessingRequest::MergeAudioVideo {
                video_input,
                audio_input,
                output_filename,
            } => (
                MediaProcessingOperation::MergeAudioVideo {
                    video_input,
                    audio_input,
                },
                output_filename,
            ),
            MediaProcessingRequest::ExtractAudio {
                input,
                output_filename,
            } => (
                MediaProcessingOperation::ExtractAudio { input },
                output_filename,
            ),
        };

        let destination = validate_destination(root, &output_filename)
            .map_err(MediaProcessingError::OutputPath)?;
        let temporary_path = destination
            .root
            .join(format!("{}.processing.part", destination.filename));
        validate_path_within_root(&destination.root, &temporary_path)
            .map_err(MediaProcessingError::OutputPath)?;
        reject_existing_path(&destination.destination, true)?;
        reject_existing_path(&temporary_path, false)?;

        let input_paths = operation.input_paths();
        for input in &input_paths {
            validate_path_within_root(&destination.root, input)
                .map_err(MediaProcessingError::InputPath)?;
            validate_input_file(input)?;
        }
        if input_paths.windows(2).any(|paths| paths[0] == paths[1]) {
            return Err(MediaProcessingError::DuplicateInput);
        }

        Ok(Self {
            operation,
            output: ProcessingOutput {
                root: destination.root,
                final_path: destination.destination,
                temporary_path,
                filename: destination.filename,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaProcessingArguments {
    inner: process::FfmpegArguments,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessedFinalizationResult {
    pub final_path: PathBuf,
    pub bytes_finalized: u64,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProcessedOutputError {
    #[error("FFmpeg processing output is missing")]
    OutputMissing,
    #[error("FFmpeg processing output is a symbolic link")]
    OutputSymlink,
    #[error("FFmpeg processing output is not a regular file")]
    OutputNotRegular,
    #[error("FFmpeg processing output is empty")]
    OutputEmpty,
    #[error("FFmpeg processing output exceeds the supported size limit")]
    OutputTooLarge,
    #[error("FFmpeg final destination already exists")]
    DestinationExists,
    #[error("FFmpeg processing output could not be synchronized")]
    OutputSyncFailed,
    #[error("FFmpeg processing output permissions could not be hardened")]
    OutputPermissionFailed,
    #[error("FFmpeg processing output could not be atomically finalized")]
    RenameFailed,
    #[error("FFmpeg destination directory could not be synchronized")]
    DirectorySyncFailed,
    #[error("FFmpeg finalized output could not be inspected")]
    FinalOutputInvalid,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MediaProcessingArgumentError {
    #[error("merge output must use the .mp4 extension")]
    MergeOutputExtension,
    #[error("audio extraction output must use the .m4a extension")]
    AudioOutputExtension,
    #[error("typed FFmpeg argument list could not be constructed")]
    EmptyArguments,
}

impl MediaProcessingArguments {
    pub fn from_plan(plan: &MediaProcessingPlan) -> Result<Self, MediaProcessingArgumentError> {
        let mut arguments = vec![
            OsString::from("-hide_banner"),
            OsString::from("-loglevel"),
            OsString::from("error"),
            OsString::from("-nostdin"),
            OsString::from("-n"),
        ];
        match &plan.operation {
            MediaProcessingOperation::MergeAudioVideo {
                video_input,
                audio_input,
            } => {
                if !has_extension(&plan.output.filename, "mp4") {
                    return Err(MediaProcessingArgumentError::MergeOutputExtension);
                }
                arguments.extend([
                    OsString::from("-i"),
                    video_input.as_os_str().to_owned(),
                    OsString::from("-i"),
                    audio_input.as_os_str().to_owned(),
                    OsString::from("-map"),
                    OsString::from("0:v:0"),
                    OsString::from("-map"),
                    OsString::from("1:a:0"),
                    OsString::from("-c:v"),
                    OsString::from("copy"),
                    OsString::from("-c:a"),
                    OsString::from("aac"),
                    OsString::from("-shortest"),
                    OsString::from("-f"),
                    OsString::from("mp4"),
                ]);
            }
            MediaProcessingOperation::ExtractAudio { input } => {
                if !has_extension(&plan.output.filename, "m4a") {
                    return Err(MediaProcessingArgumentError::AudioOutputExtension);
                }
                arguments.extend([
                    OsString::from("-i"),
                    input.as_os_str().to_owned(),
                    OsString::from("-map"),
                    OsString::from("0:a:0"),
                    OsString::from("-vn"),
                    OsString::from("-c:a"),
                    OsString::from("aac"),
                    OsString::from("-b:a"),
                    OsString::from("192k"),
                    OsString::from("-f"),
                    OsString::from("ipod"),
                ]);
            }
        }
        arguments.push(plan.output.temporary_path.as_os_str().to_owned());
        let inner = process::FfmpegArguments::new(arguments)
            .map_err(|_| MediaProcessingArgumentError::EmptyArguments)?;
        Ok(Self { inner })
    }

    #[allow(dead_code)]
    pub(crate) fn into_runner_arguments(self) -> process::FfmpegArguments {
        self.inner
    }

    #[cfg(test)]
    fn as_strings(&self) -> Vec<String> {
        self.inner
            .as_slice()
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }
}

pub fn finalize_processed_output(
    plan: &MediaProcessingPlan,
) -> Result<ProcessedFinalizationResult, ProcessedOutputError> {
    let temporary_metadata = fs::symlink_metadata(&plan.output.temporary_path)
        .map_err(|_| ProcessedOutputError::OutputMissing)?;
    if temporary_metadata.file_type().is_symlink() {
        return Err(ProcessedOutputError::OutputSymlink);
    }
    if !temporary_metadata.is_file() {
        return Err(ProcessedOutputError::OutputNotRegular);
    }
    if temporary_metadata.len() == 0 {
        return Err(ProcessedOutputError::OutputEmpty);
    }
    if temporary_metadata.len() > MAX_PROCESSING_FILE_BYTES {
        return Err(ProcessedOutputError::OutputTooLarge);
    }
    if fs::symlink_metadata(&plan.output.final_path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(ProcessedOutputError::OutputSymlink);
    }
    if fs::symlink_metadata(&plan.output.final_path).is_ok() {
        return Err(ProcessedOutputError::DestinationExists);
    }
    harden_file_permissions(&plan.output.temporary_path)
        .map_err(|_| ProcessedOutputError::OutputPermissionFailed)?;
    let output = fs::File::open(&plan.output.temporary_path)
        .map_err(|_| ProcessedOutputError::OutputMissing)?;
    output
        .sync_all()
        .map_err(|_| ProcessedOutputError::OutputSyncFailed)?;
    drop(output);
    fs::rename(&plan.output.temporary_path, &plan.output.final_path)
        .map_err(|_| ProcessedOutputError::RenameFailed)?;
    let directory = fs::OpenOptions::new()
        .read(true)
        .open(&plan.output.root)
        .map_err(|_| ProcessedOutputError::DirectorySyncFailed)?;
    directory
        .sync_all()
        .map_err(|_| ProcessedOutputError::DirectorySyncFailed)?;
    let final_metadata = fs::metadata(&plan.output.final_path)
        .map_err(|_| ProcessedOutputError::FinalOutputInvalid)?;
    if !final_metadata.is_file() || final_metadata.len() == 0 {
        return Err(ProcessedOutputError::FinalOutputInvalid);
    }
    if final_metadata.len() > MAX_PROCESSING_FILE_BYTES {
        return Err(ProcessedOutputError::OutputTooLarge);
    }
    Ok(ProcessedFinalizationResult {
        final_path: plan.output.final_path.clone(),
        bytes_finalized: final_metadata.len(),
    })
}

#[derive(Debug, Error)]
pub(crate) enum MediaProcessingExecutionError {
    #[error("typed FFmpeg argument construction failed: {0}")]
    Arguments(#[from] MediaProcessingArgumentError),
    #[error("FFmpeg process failed: {0}")]
    Process(#[from] process::FfmpegRunError),
    #[error("processed output validation failed: {0}")]
    Output(#[from] ProcessedOutputError),
}

#[derive(Debug, Clone)]
pub(crate) struct MediaProcessor {
    runner: process::FfmpegProcessRunner,
}

impl MediaProcessingExecutionError {
    pub(crate) fn is_cancelled(&self) -> bool {
        matches!(self, Self::Process(process::FfmpegRunError::Cancelled))
    }
}

impl MediaProcessor {
    pub(crate) async fn from_system() -> Result<Self, FfmpegResolveError> {
        let executable = FfmpegExecutable::resolve_system().await?;
        let runner = process::FfmpegProcessRunner::new(
            executable,
            process::DEFAULT_FFMPEG_TIMEOUT,
            process::DEFAULT_MAX_FFMPEG_DIAGNOSTIC_BYTES,
        )
        .map_err(|_| FfmpegResolveError::VersionCommandFailed)?;
        Ok(Self { runner })
    }

    pub(crate) async fn execute(
        &self,
        plan: &MediaProcessingPlan,
        cancelled: &AtomicBool,
    ) -> Result<ProcessedFinalizationResult, MediaProcessingExecutionError> {
        let arguments = MediaProcessingArguments::from_plan(plan)?;
        self.runner
            .run(arguments.into_runner_arguments(), cancelled)
            .await?;
        Ok(finalize_processed_output(plan)?)
    }
}

fn has_extension(filename: &str, expected: &str) -> bool {
    Path::new(filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn validate_input_file(path: &Path) -> Result<(), MediaProcessingError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            MediaProcessingError::InputMissing
        } else {
            MediaProcessingError::InputNotRegularFile
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(MediaProcessingError::InputNotRegularFile);
    }
    if metadata.len() > MAX_PROCESSING_FILE_BYTES {
        return Err(MediaProcessingError::InputTooLarge);
    }
    Ok(())
}

fn reject_existing_path(path: &Path, final_output: bool) -> Result<(), MediaProcessingError> {
    match fs::symlink_metadata(path) {
        Ok(_) if final_output => Err(MediaProcessingError::OutputAlreadyExists),
        Ok(_) => Err(MediaProcessingError::TemporaryOutputAlreadyExists),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) if final_output => Err(MediaProcessingError::OutputAlreadyExists),
        Err(_) => Err(MediaProcessingError::TemporaryOutputAlreadyExists),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FfmpegExecutable, MediaProcessingArgumentError, MediaProcessingArguments,
        MediaProcessingError, MediaProcessingOperation, MediaProcessingPlan,
        MediaProcessingRequest, MediaProcessor,
    };
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn merge_request(root: &std::path::Path) -> MediaProcessingRequest {
        MediaProcessingRequest::MergeAudioVideo {
            video_input: root.join("video.mp4"),
            audio_input: root.join("audio.m4a"),
            output_filename: "merged.mp4".to_owned(),
        }
    }

    #[test]
    fn resolves_typed_merge_operation_and_processing_output_paths() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("video.mp4"), b"video").unwrap();
        fs::write(directory.path().join("audio.m4a"), b"audio").unwrap();
        let plan = MediaProcessingPlan::resolve(directory.path(), merge_request(directory.path()))
            .unwrap();
        assert!(matches!(
            plan.operation,
            MediaProcessingOperation::MergeAudioVideo { .. }
        ));
        assert_eq!(plan.output.final_path, directory.path().join("merged.mp4"));
        assert_eq!(
            plan.output.temporary_path,
            directory.path().join("merged.mp4.processing.part")
        );
    }

    #[test]
    fn builds_fixed_merge_arguments_with_validated_paths_and_flags() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("video.mp4"), b"video").unwrap();
        fs::write(directory.path().join("audio.m4a"), b"audio").unwrap();
        let plan = MediaProcessingPlan::resolve(directory.path(), merge_request(directory.path()))
            .unwrap();
        let arguments = MediaProcessingArguments::from_plan(&plan)
            .unwrap()
            .as_strings();
        assert_eq!(
            &arguments[0..5],
            &["-hide_banner", "-loglevel", "error", "-nostdin", "-n"]
        );
        assert!(arguments.contains(&"0:v:0".to_owned()));
        assert!(arguments.contains(&"1:a:0".to_owned()));
        assert!(arguments.contains(&"copy".to_owned()));
        assert!(arguments.contains(&"aac".to_owned()));
        let expected_output = plan.output.temporary_path.to_string_lossy().into_owned();
        assert_eq!(arguments.last(), Some(&expected_output));
    }

    #[test]
    fn resolves_typed_audio_extraction_operation() {
        let directory = tempdir().unwrap();
        let input = directory.path().join("video.mp4");
        fs::write(&input, b"video").unwrap();
        let plan = MediaProcessingPlan::resolve(
            directory.path(),
            MediaProcessingRequest::ExtractAudio {
                input,
                output_filename: "audio.m4a".to_owned(),
            },
        )
        .unwrap();
        assert!(matches!(
            plan.operation,
            MediaProcessingOperation::ExtractAudio { .. }
        ));
        let arguments = MediaProcessingArguments::from_plan(&plan)
            .unwrap()
            .as_strings();
        assert!(arguments.contains(&"0:a:0".to_owned()));
        assert!(arguments.contains(&"-vn".to_owned()));
        assert!(arguments.contains(&"192k".to_owned()));
        let expected_output = plan.output.temporary_path.to_string_lossy().into_owned();
        assert_eq!(arguments.last(), Some(&expected_output));
    }

    #[test]
    fn rejects_operation_output_extension_mismatches() {
        let directory = tempdir().unwrap();
        let input = directory.path().join("video.mp4");
        let audio = directory.path().join("audio.m4a");
        fs::write(&input, b"video").unwrap();
        fs::write(&audio, b"audio").unwrap();
        let merge_plan = MediaProcessingPlan::resolve(
            directory.path(),
            MediaProcessingRequest::MergeAudioVideo {
                video_input: input.clone(),
                audio_input: audio,
                output_filename: "merged.m4a".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(
            MediaProcessingArguments::from_plan(&merge_plan),
            Err(MediaProcessingArgumentError::MergeOutputExtension)
        );
        let audio_plan = MediaProcessingPlan::resolve(
            directory.path(),
            MediaProcessingRequest::ExtractAudio {
                input,
                output_filename: "audio.mp3".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(
            MediaProcessingArguments::from_plan(&audio_plan),
            Err(MediaProcessingArgumentError::AudioOutputExtension)
        );
    }

    #[test]
    fn rejects_missing_non_regular_and_symlink_inputs() {
        let directory = tempdir().unwrap();
        let missing = MediaProcessingPlan::resolve(
            directory.path(),
            MediaProcessingRequest::ExtractAudio {
                input: directory.path().join("missing.mp4"),
                output_filename: "audio.m4a".to_owned(),
            },
        );
        assert_eq!(missing, Err(MediaProcessingError::InputMissing));

        let subdirectory = directory.path().join("folder");
        fs::create_dir(&subdirectory).unwrap();
        let non_regular = MediaProcessingPlan::resolve(
            directory.path(),
            MediaProcessingRequest::ExtractAudio {
                input: subdirectory,
                output_filename: "audio.m4a".to_owned(),
            },
        );
        assert_eq!(non_regular, Err(MediaProcessingError::InputNotRegularFile));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_input_and_output_collisions() {
        use std::os::unix::fs::symlink;
        let directory = tempdir().unwrap();
        let real_input = directory.path().join("real.mp4");
        let symlink_input = directory.path().join("link.mp4");
        fs::write(&real_input, b"video").unwrap();
        symlink(&real_input, &symlink_input).unwrap();
        let error = MediaProcessingPlan::resolve(
            directory.path(),
            MediaProcessingRequest::ExtractAudio {
                input: symlink_input,
                output_filename: "audio.m4a".to_owned(),
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            MediaProcessingError::InputPath(
                crate::downloader::path_safety::DestinationPathError::DestinationIsSymlink
            )
        );

        let output = directory.path().join("audio.m4a");
        fs::write(&output, b"existing").unwrap();
        let collision = MediaProcessingPlan::resolve(
            directory.path(),
            MediaProcessingRequest::ExtractAudio {
                input: real_input,
                output_filename: "audio.m4a".to_owned(),
            },
        );
        assert_eq!(collision, Err(MediaProcessingError::OutputAlreadyExists));
    }

    #[test]
    fn rejects_inputs_outside_the_selected_processing_root() {
        let directory = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let input = outside.path().join("video.mp4");
        fs::write(&input, b"video").unwrap();
        let error = MediaProcessingPlan::resolve(
            directory.path(),
            MediaProcessingRequest::ExtractAudio {
                input,
                output_filename: "audio.m4a".to_owned(),
            },
        )
        .unwrap_err();
        assert!(matches!(error, MediaProcessingError::InputPath(_)));
    }

    #[tokio::test]
    async fn executes_merge_and_audio_extraction_with_system_ffmpeg() {
        let Ok(executable) = FfmpegExecutable::resolve_system().await else {
            return;
        };
        let directory = tempdir().unwrap();
        let video = directory.path().join("video.mp4");
        let audio = directory.path().join("audio.m4a");
        let generate_video = std::process::Command::new(executable.path())
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=64x64:rate=10:duration=0.3",
                "-an",
                "-c:v",
                "mpeg4",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&video)
            .status()
            .unwrap();
        assert!(generate_video.success());
        let generate_audio = std::process::Command::new(executable.path())
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:duration=0.3",
                "-vn",
                "-c:a",
                "aac",
                "-b:a",
                "128k",
                "-f",
                "ipod",
            ])
            .arg(&audio)
            .status()
            .unwrap();
        assert!(generate_audio.success());

        let processor = MediaProcessor::from_system().await.unwrap();
        let merge_plan =
            MediaProcessingPlan::resolve(directory.path(), merge_request(directory.path()))
                .unwrap();
        let merged = processor
            .execute(&merge_plan, &std::sync::atomic::AtomicBool::new(false))
            .await
            .unwrap();
        assert!(merged.bytes_finalized > 0);
        assert!(merged.final_path.is_file());

        let extract_plan = MediaProcessingPlan::resolve(
            directory.path(),
            MediaProcessingRequest::ExtractAudio {
                input: merged.final_path,
                output_filename: "extracted.m4a".to_owned(),
            },
        )
        .unwrap();
        let extracted = processor
            .execute(&extract_plan, &std::sync::atomic::AtomicBool::new(false))
            .await
            .unwrap();
        assert!(extracted.bytes_finalized > 0);
        assert!(extracted.final_path.is_file());
    }

    #[test]
    fn finalizes_only_non_empty_processed_outputs_without_overwrite() {
        let directory = tempdir().unwrap();
        let input = directory.path().join("input.mp4");
        fs::write(&input, b"input").unwrap();
        let plan = MediaProcessingPlan::resolve(
            directory.path(),
            MediaProcessingRequest::ExtractAudio {
                input,
                output_filename: "output.m4a".to_owned(),
            },
        )
        .unwrap();
        fs::write(&plan.output.temporary_path, b"processed").unwrap();
        let result = super::finalize_processed_output(&plan).unwrap();
        assert_eq!(result.bytes_finalized, 9);
        assert!(!plan.output.temporary_path.exists());
        assert!(plan.output.final_path.is_file());

        let empty_plan = MediaProcessingPlan::resolve(
            directory.path(),
            MediaProcessingRequest::ExtractAudio {
                input: plan.output.final_path,
                output_filename: "empty.m4a".to_owned(),
            },
        )
        .unwrap();
        fs::write(&empty_plan.output.temporary_path, []).unwrap();
        assert_eq!(
            super::finalize_processed_output(&empty_plan),
            Err(super::ProcessedOutputError::OutputEmpty)
        );
    }

    #[test]
    fn rejects_oversized_processing_input() {
        let directory = tempdir().unwrap();
        let input = directory.path().join("video.mp4");
        let file = fs::File::create(&input).unwrap();
        file.set_len(super::MAX_PROCESSING_FILE_BYTES + 1).unwrap();
        let error = MediaProcessingPlan::resolve(
            directory.path(),
            MediaProcessingRequest::ExtractAudio {
                input,
                output_filename: "audio.m4a".to_owned(),
            },
        )
        .unwrap_err();
        assert_eq!(error, MediaProcessingError::InputTooLarge);
    }

    #[test]
    fn rejects_oversized_processing_output() {
        let directory = tempdir().unwrap();
        let video = directory.path().join("video.mp4");
        let audio = directory.path().join("audio.m4a");
        fs::write(&video, b"video").unwrap();
        fs::write(&audio, b"audio").unwrap();
        let plan = MediaProcessingPlan::resolve(
            directory.path(),
            MediaProcessingRequest::MergeAudioVideo {
                video_input: video,
                audio_input: audio,
                output_filename: "merged.mp4".to_owned(),
            },
        )
        .unwrap();
        let output = fs::File::create(&plan.output.temporary_path).unwrap();
        output
            .set_len(super::MAX_PROCESSING_FILE_BYTES + 1)
            .unwrap();
        assert_eq!(
            super::finalize_processed_output(&plan),
            Err(super::ProcessedOutputError::OutputTooLarge)
        );
    }

    #[test]
    fn rejects_duplicate_merge_inputs() {
        let directory = tempdir().unwrap();
        let input = directory.path().join("video.mp4");
        fs::write(&input, b"video").unwrap();
        let error = MediaProcessingPlan::resolve(
            directory.path(),
            MediaProcessingRequest::MergeAudioVideo {
                video_input: PathBuf::from(&input),
                audio_input: PathBuf::from(&input),
                output_filename: "merged.mp4".to_owned(),
            },
        )
        .unwrap_err();
        assert_eq!(error, MediaProcessingError::DuplicateInput);
    }
}
