pub mod capture;
pub mod devices;
pub mod engine;
pub mod mixer;
pub mod render;
pub mod sessions;
pub mod types;

#[cfg(windows)]
mod wasapi_io;
#[cfg(windows)]
mod windows_wasapi;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("capture failed: {0}")]
    CaptureFailed(String),
    #[error("{0}")]
    Backend(String),
}

#[cfg(windows)]
impl From<windows::core::Error> for AudioError {
    fn from(error: windows::core::Error) -> Self {
        Self::Backend(error.message().to_string())
    }
}

pub type AudioResult<T> = Result<T, AudioError>;
