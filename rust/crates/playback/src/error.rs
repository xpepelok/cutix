use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlaybackError {
    #[error("scene '{0}' was not found in the project")]
    SceneNotFound(String),
    #[error("media '{0}' could not be resolved to a file")]
    MediaNotFound(String),
    #[error("gpu is unavailable: {0}")]
    Gpu(String),
    #[error("composition failed: {0}")]
    Compositor(String),
    #[error("decode failed: {0}")]
    Decode(String),
    #[error("unsupported media: {0}")]
    UnsupportedMedia(String),
    #[error("audio device failed: {0}")]
    AudioDevice(String),
    #[error("io failed: {0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, PlaybackError>;

impl From<compositor::CompositorError> for PlaybackError {
    fn from(error: compositor::CompositorError) -> Self {
        Self::Compositor(error.to_string())
    }
}
