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
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("couldn't reload {path}: {message}")]
    Refresh { path: PathBuf, message: String },
    #[error("{path}: {description}")]
    Other { path: PathBuf, description: String },
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
    pub fn from_report(e: color_eyre::Report, path: &Path) -> Self {
        match e.downcast::<std::io::Error>() {
            Ok(io) => Self::from_io(io, path),
            Err(report) => Self::Other {
                path: path.to_owned(),
                description: format!("{report:#}"),
            },
        }
    }
}
