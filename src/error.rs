use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error("{0} already exists")]
    AlreadyExists(PathBuf),
    #[error("permission denied: {0}")]
    PermissionDenied(PathBuf),
    #[error("{0} no longer exists")]
    NotFound(PathBuf),
    #[error("can't paste a directory into itself")]
    PasteIntoSelf,
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("couldn't reload {path}: {message}")]
    Refresh { path: PathBuf, message: String },
}

impl UiError {
    pub fn from_io(e: std::io::Error, path: &Path) -> Self {
        match e.kind() {
            ErrorKind::AlreadyExists => Self::AlreadyExists(path.to_owned()),
            ErrorKind::PermissionDenied => Self::PermissionDenied(path.to_owned()),
            ErrorKind::NotFound => Self::NotFound(path.to_owned()),
            _ => Self::Io {
                path: path.to_owned(),
                source: e,
            },
        }
    }
}
