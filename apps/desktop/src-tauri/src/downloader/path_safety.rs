use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DestinationPathError {
    #[error("download directory must be an absolute path")]
    RootMustBeAbsolute,
    #[error("download directory contains a parent traversal component")]
    RootContainsTraversal,
    #[error("download directory is an existing symbolic link")]
    RootIsSymlink,
    #[error("download filename must be a single non-empty component")]
    InvalidFilename,
    #[error("download filename contains a control character")]
    FilenameContainsControl,
    #[error("download filename contains a path separator")]
    FilenameContainsSeparator,
    #[error("download filename uses a reserved operating-system name")]
    ReservedFilename,
    #[error("download filename cannot end with the temporary-file suffix")]
    TemporaryFilename,
    #[error("download destination escapes the selected directory")]
    DestinationEscapesRoot,
    #[error("download destination or temporary path is an existing symbolic link")]
    DestinationIsSymlink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinationPaths {
    pub root: PathBuf,
    pub destination: PathBuf,
    pub temporary: PathBuf,
    pub filename: String,
}

pub fn validate_destination(
    root: &Path,
    filename: &str,
) -> Result<DestinationPaths, DestinationPathError> {
    let root = normalize_root(root)?;
    validate_filename(filename)?;

    let destination = root.join(filename);
    let temporary = root.join(format!("{filename}.part"));
    if !is_within(&root, &destination) || !is_within(&root, &temporary) {
        return Err(DestinationPathError::DestinationEscapesRoot);
    }
    reject_existing_symlink(&destination)?;
    reject_existing_symlink(&temporary)?;

    Ok(DestinationPaths {
        root,
        destination,
        temporary,
        filename: filename.to_owned(),
    })
}

pub(crate) fn validate_path_within_root(
    root: &Path,
    candidate: &Path,
) -> Result<(), DestinationPathError> {
    let root = normalize_root(root)?;
    if !candidate.is_absolute() {
        return Err(DestinationPathError::RootMustBeAbsolute);
    }
    if candidate
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(DestinationPathError::RootContainsTraversal);
    }
    if !is_within(&root, candidate) {
        return Err(DestinationPathError::DestinationEscapesRoot);
    }
    reject_existing_symlink(candidate)
}

fn normalize_root(root: &Path) -> Result<PathBuf, DestinationPathError> {
    if !root.is_absolute() {
        return Err(DestinationPathError::RootMustBeAbsolute);
    }
    if root
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(DestinationPathError::RootContainsTraversal);
    }
    let mut normalized = PathBuf::new();
    for component in root.components() {
        if !matches!(component, Component::CurDir) {
            normalized.push(component.as_os_str());
        }
    }
    reject_symlink_components(&normalized)?;
    Ok(normalized)
}

fn validate_filename(filename: &str) -> Result<(), DestinationPathError> {
    if filename.is_empty() || filename == "." || filename == ".." || filename.len() > 255 {
        return Err(DestinationPathError::InvalidFilename);
    }
    if filename.chars().any(char::is_control) {
        return Err(DestinationPathError::FilenameContainsControl);
    }
    if filename.contains('/') || filename.contains('\\') {
        return Err(DestinationPathError::FilenameContainsSeparator);
    }
    if filename.ends_with(".part") {
        return Err(DestinationPathError::TemporaryFilename);
    }
    if filename.ends_with('.') || filename.ends_with(' ') {
        return Err(DestinationPathError::InvalidFilename);
    }
    if filename
        .chars()
        .any(|character| matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return Err(DestinationPathError::InvalidFilename);
    }
    if is_reserved_name(filename) {
        return Err(DestinationPathError::ReservedFilename);
    }
    if Path::new(filename)
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(DestinationPathError::InvalidFilename);
    }
    Ok(())
}

fn is_reserved_name(filename: &str) -> bool {
    let stem = filename
        .split('.')
        .next()
        .unwrap_or(filename)
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn is_within(root: &Path, candidate: &Path) -> bool {
    candidate.starts_with(root)
}

fn reject_symlink_components(path: &Path) -> Result<(), DestinationPathError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if fs::symlink_metadata(&current)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(DestinationPathError::RootIsSymlink);
        }
    }
    Ok(())
}

fn reject_existing_symlink(path: &Path) -> Result<(), DestinationPathError> {
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(DestinationPathError::DestinationIsSymlink);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_destination, DestinationPathError};
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn accepts_absolute_root_and_builds_final_and_part_paths() {
        let root = Path::new("/downloads/media");
        let paths = validate_destination(root, "creator - title.mp4").unwrap();
        assert_eq!(paths.root, root);
        assert_eq!(paths.destination, root.join("creator - title.mp4"));
        assert_eq!(paths.temporary, root.join("creator - title.mp4.part"));
    }

    #[test]
    fn rejects_relative_and_traversal_roots() {
        assert_eq!(
            validate_destination(Path::new("downloads"), "video.mp4"),
            Err(DestinationPathError::RootMustBeAbsolute)
        );
        assert_eq!(
            validate_destination(Path::new("/downloads/../outside"), "video.mp4"),
            Err(DestinationPathError::RootContainsTraversal)
        );
    }

    #[test]
    fn rejects_filename_traversal_separators_controls_and_temp_names() {
        for filename in [
            "",
            ".",
            "..",
            "../video.mp4",
            "..\\video.mp4",
            "video\u{0000}.mp4",
        ] {
            assert!(validate_destination(Path::new("/downloads"), filename).is_err());
        }
        assert_eq!(
            validate_destination(Path::new("/downloads"), "video.mp4.part"),
            Err(DestinationPathError::TemporaryFilename)
        );
    }

    #[test]
    fn rejects_reserved_and_cross_platform_invalid_names() {
        for filename in ["CON", "con.txt", "NUL", "LPT1.log", "video?.mp4", "video."] {
            assert!(validate_destination(Path::new("/downloads"), filename).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_existing_root_and_destination_symlinks() {
        use std::os::unix::fs::symlink;
        let directory = tempdir().unwrap();
        let real_root = directory.path().join("real");
        std::fs::create_dir(&real_root).unwrap();
        let root_link = directory.path().join("root-link");
        symlink(&real_root, &root_link).unwrap();
        assert_eq!(
            validate_destination(&root_link, "video.mp4"),
            Err(DestinationPathError::RootIsSymlink)
        );

        let nested_link = directory.path().join("nested-link");
        symlink(&real_root, &nested_link).unwrap();
        assert_eq!(
            validate_destination(&nested_link.join("child"), "video.mp4"),
            Err(DestinationPathError::RootIsSymlink)
        );

        let destination_link = real_root.join("video.mp4");
        let target = directory.path().join("target");
        std::fs::write(&target, b"target").unwrap();
        symlink(&target, &destination_link).unwrap();
        assert_eq!(
            validate_destination(&real_root, "video.mp4"),
            Err(DestinationPathError::DestinationIsSymlink)
        );
    }
}
