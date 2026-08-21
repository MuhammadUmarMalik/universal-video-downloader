use fs2::available_space;
use std::io;
use std::path::Path;
use thiserror::Error;

pub const MIN_FREE_HEADROOM_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StorageError {
    #[error("the destination directory is unavailable")]
    DestinationUnavailable,
    #[error("the destination directory is not writable")]
    PermissionDenied,
    #[error("the destination does not have enough free space")]
    DiskFull,
}

pub fn ensure_available_space(
    root: &Path,
    expected_bytes: Option<u64>,
    max_response_bytes: u64,
) -> Result<(), StorageError> {
    let free = available_space(root).map_err(map_io_error)?;
    let expected = expected_bytes.unwrap_or(MIN_FREE_HEADROOM_BYTES);
    let required = expected
        .min(max_response_bytes)
        .saturating_add(MIN_FREE_HEADROOM_BYTES);
    if free < required {
        return Err(StorageError::DiskFull);
    }
    Ok(())
}

pub fn harden_file_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn map_io_error(error: io::Error) -> StorageError {
    if error.kind() == io::ErrorKind::PermissionDenied {
        return StorageError::PermissionDenied;
    }
    if matches!(error.raw_os_error(), Some(28 | 39 | 112)) {
        return StorageError::DiskFull;
    }
    StorageError::DestinationUnavailable
}

#[cfg(test)]
mod tests {
    use super::{ensure_available_space, StorageError, MIN_FREE_HEADROOM_BYTES};
    use tempfile::tempdir;

    #[test]
    fn rejects_missing_destination_directory() {
        let error = ensure_available_space(
            std::path::Path::new("/definitely/missing/umd-destination"),
            Some(1),
            1024,
        )
        .unwrap_err();
        assert_eq!(error, StorageError::DestinationUnavailable);
    }

    #[test]
    fn accepts_a_normal_temporary_destination() {
        let directory = tempdir().unwrap();
        ensure_available_space(directory.path(), Some(1), MIN_FREE_HEADROOM_BYTES * 2).unwrap();
    }
}
