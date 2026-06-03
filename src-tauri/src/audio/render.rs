use super::{
    AudioResult,
    mixer::{SAMPLE_RATE, StereoFrame},
};

#[cfg(not(windows))]
use super::AudioError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSpec {
    pub device_id: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames_per_buffer: usize,
    pub downmix_to_mono: bool,
}

impl RenderSpec {
    pub fn cable(
        device_id: impl Into<String>,
        frames_per_buffer: usize,
        downmix_to_mono: bool,
    ) -> Self {
        Self {
            device_id: device_id.into(),
            sample_rate: SAMPLE_RATE,
            channels: if downmix_to_mono { 1 } else { 2 },
            frames_per_buffer,
            downmix_to_mono,
        }
    }
}

pub trait AudioRender {
    fn write_stereo(&mut self, frames: &[StereoFrame]) -> AudioResult<usize>;
}

#[cfg(windows)]
pub fn open_render_output(spec: &RenderSpec) -> AudioResult<Box<dyn AudioRender>> {
    super::wasapi_io::open_render(spec)
}

#[cfg(not(windows))]
pub fn open_render_output(_spec: &RenderSpec) -> AudioResult<Box<dyn AudioRender>> {
    Err(AudioError::CaptureFailed(
        "WASAPI render worker is not connected yet".to_string(),
    ))
}
