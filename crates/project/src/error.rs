use std::path::PathBuf;

#[derive(Debug)]
pub enum ProjectError {
    Io { path: PathBuf, detail: String },
    Json { path: PathBuf, detail: String },
    NotFound { id: String },
    AlreadyExists { id: String },
    UnsupportedMedia { detail: String },
    Probe { detail: String },
    NoAppDataDirectory,
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, detail } => {
                write!(formatter, "io error at {}: {detail}", path.display())
            }
            Self::Json { path, detail } => {
                write!(formatter, "invalid json at {}: {detail}", path.display())
            }
            Self::NotFound { id } => write!(formatter, "project {id} not found"),
            Self::AlreadyExists { id } => write!(formatter, "project {id} already exists"),
            Self::UnsupportedMedia { detail } => {
                write!(formatter, "unsupported media file: {detail}")
            }
            Self::Probe { detail } => write!(formatter, "cannot probe media: {detail}"),
            Self::NoAppDataDirectory => write!(formatter, "no application data directory"),
        }
    }
}

impl std::error::Error for ProjectError {}

pub type Result<T> = std::result::Result<T, ProjectError>;

pub(crate) fn io_error(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> ProjectError {
    let path = path.into();
    move |error| ProjectError::Io {
        path,
        detail: error.to_string(),
    }
}
