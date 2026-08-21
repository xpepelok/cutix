use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("no encoder backend named `{0}` is registered")]
    UnknownBackend(String),
    #[error("the project has nothing to render")]
    Empty,
    #[error("invalid output size {width}x{height}")]
    InvalidSize { width: u32, height: u32 },
    #[error("invalid frame rate")]
    InvalidFrameRate,
    #[error("compose failed: {0}")]
    Compose(String),
    #[error("encoder failed: {0}")]
    Encoder(String),
    #[error("muxer failed: {0}")]
    Muxer(String),
    #[error("project package: {0}")]
    Package(String),
    #[error("cannot write {path}: {detail}")]
    Io { path: String, detail: String },
    #[error("export was cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, ExportError>;
