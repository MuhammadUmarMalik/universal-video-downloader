use super::{DownloadPlan, StreamResult};
use crate::downloader::harden_file_permissions;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FinalizationError {
    #[error("the streamed byte count does not match the download plan")]
    ResultSizeMismatch,
    #[error("the temporary download file is missing")]
    TemporaryFileMissing,
    #[error("the temporary or destination path is a symbolic link")]
    PathIsSymlink,
    #[error("the final destination already exists")]
    DestinationExists,
    #[error("the temporary download file is not a regular file")]
    TemporaryFileNotRegular,
    #[error("the temporary download file could not be synchronized")]
    TemporarySyncFailed,
    #[error("the temporary download file could not be atomically finalized")]
    RenameFailed,
    #[error("the destination directory could not be synchronized")]
    DirectorySyncFailed,
    #[error("the finalized file could not be inspected")]
    FinalFileInvalid,
    #[error("the finalized file permissions could not be hardened")]
    FinalFilePermissionFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizationResult {
    pub final_path: PathBuf,
    pub bytes_finalized: u64,
}

pub fn finalize_part(
    plan: &DownloadPlan,
    stream_result: &StreamResult,
) -> Result<FinalizationResult, FinalizationError> {
    if stream_result
        .content_length
        .is_some_and(|length| length != stream_result.bytes_written)
        || plan
            .total_bytes
            .is_some_and(|total| total < 0 || total as u64 != stream_result.bytes_written)
    {
        return Err(FinalizationError::ResultSizeMismatch);
    }

    reject_symlink(&plan.destination.temporary)?;
    reject_symlink(&plan.destination.destination)?;
    let temporary_metadata = fs::symlink_metadata(&plan.destination.temporary)
        .map_err(|_| FinalizationError::TemporaryFileMissing)?;
    if !temporary_metadata.is_file() {
        return Err(FinalizationError::TemporaryFileNotRegular);
    }
    if fs::symlink_metadata(&plan.destination.destination).is_ok() {
        return Err(FinalizationError::DestinationExists);
    }

    let temporary = File::open(&plan.destination.temporary)
        .map_err(|_| FinalizationError::TemporaryFileMissing)?;
    temporary
        .sync_all()
        .map_err(|_| FinalizationError::TemporarySyncFailed)?;
    drop(temporary);

    fs::rename(&plan.destination.temporary, &plan.destination.destination)
        .map_err(|_| FinalizationError::RenameFailed)?;
    sync_directory(&plan.destination.root)?;

    harden_file_permissions(&plan.destination.destination)
        .map_err(|_| FinalizationError::FinalFilePermissionFailed)?;
    let final_metadata = fs::metadata(&plan.destination.destination)
        .map_err(|_| FinalizationError::FinalFileInvalid)?;
    if !final_metadata.is_file() || final_metadata.len() != stream_result.bytes_written {
        return Err(FinalizationError::FinalFileInvalid);
    }
    Ok(FinalizationResult {
        final_path: plan.destination.destination.clone(),
        bytes_finalized: final_metadata.len(),
    })
}

fn reject_symlink(path: &Path) -> Result<(), FinalizationError> {
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(FinalizationError::PathIsSymlink);
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), FinalizationError> {
    let directory = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|_| FinalizationError::DirectorySyncFailed)?;
    directory
        .sync_all()
        .map_err(|_| FinalizationError::DirectorySyncFailed)
}

#[cfg(test)]
mod tests {
    use super::{finalize_part, FinalizationError};
    use crate::downloader::{DestinationPaths, DownloadPlan, StreamResult};
    use std::path::Path;
    use tempfile::tempdir;
    use url::Url;

    fn plan(root: &Path, total_bytes: Option<i64>) -> DownloadPlan {
        DownloadPlan {
            media_item_id: "item-1".to_owned(),
            format_id: "format-1".to_owned(),
            platform_id: "reddit".to_owned(),
            source_url: Url::parse("https://v.redd.it/item-1/video.mp4").unwrap(),
            destination: DestinationPaths {
                root: root.to_owned(),
                destination: root.join("video.mp4"),
                temporary: root.join("video.mp4.part"),
                filename: "video.mp4".to_owned(),
            },
            total_bytes,
        }
    }

    #[test]
    fn atomically_renames_synced_part_to_final_destination() {
        let directory = tempdir().unwrap();
        let plan = plan(directory.path(), Some(5));
        std::fs::write(&plan.destination.temporary, b"hello").unwrap();
        let result = finalize_part(
            &plan,
            &StreamResult {
                bytes_written: 5,
                content_length: Some(5),
            },
        )
        .unwrap();
        assert_eq!(result.bytes_finalized, 5);
        assert_eq!(std::fs::read(&result.final_path).unwrap(), b"hello");
        assert!(!plan.destination.temporary.exists());
    }

    #[test]
    fn rejects_missing_part_and_mismatched_stream_result() {
        let directory = tempdir().unwrap();
        let plan = plan(directory.path(), Some(5));
        assert_eq!(
            finalize_part(
                &plan,
                &StreamResult {
                    bytes_written: 5,
                    content_length: Some(4),
                },
            ),
            Err(FinalizationError::ResultSizeMismatch)
        );
        assert_eq!(
            finalize_part(
                &plan,
                &StreamResult {
                    bytes_written: 5,
                    content_length: Some(5),
                },
            ),
            Err(FinalizationError::TemporaryFileMissing)
        );
    }

    #[test]
    fn rejects_existing_destination_without_consuming_part() {
        let directory = tempdir().unwrap();
        let plan = plan(directory.path(), Some(5));
        std::fs::write(&plan.destination.temporary, b"hello").unwrap();
        std::fs::write(&plan.destination.destination, b"old").unwrap();
        assert_eq!(
            finalize_part(
                &plan,
                &StreamResult {
                    bytes_written: 5,
                    content_length: Some(5),
                },
            ),
            Err(FinalizationError::DestinationExists)
        );
        assert!(plan.destination.temporary.exists());
        assert_eq!(
            std::fs::read(&plan.destination.destination).unwrap(),
            b"old"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_part_or_destination() {
        use std::os::unix::fs::symlink;
        let directory = tempdir().unwrap();
        let plan = plan(directory.path(), Some(5));
        let target = directory.path().join("target");
        std::fs::write(&target, b"hello").unwrap();
        symlink(&target, &plan.destination.temporary).unwrap();
        assert_eq!(
            finalize_part(
                &plan,
                &StreamResult {
                    bytes_written: 5,
                    content_length: Some(5),
                },
            ),
            Err(FinalizationError::PathIsSymlink)
        );
    }
}
