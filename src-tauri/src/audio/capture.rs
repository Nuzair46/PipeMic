use super::{
    AudioResult,
    mixer::{SAMPLE_RATE, StereoFrame},
};

#[cfg(not(windows))]
use super::AudioError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureSpec {
    pub device_id: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames_per_buffer: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessLoopbackSpec {
    pub process_id: u32,
    pub frames_per_buffer: usize,
}

impl CaptureSpec {
    pub fn mic(device_id: impl Into<String>, frames_per_buffer: usize) -> Self {
        Self {
            device_id: device_id.into(),
            sample_rate: SAMPLE_RATE,
            channels: 2,
            frames_per_buffer,
        }
    }
}

impl ProcessLoopbackSpec {
    pub fn include_process_tree(process_id: u32, frames_per_buffer: usize) -> Self {
        Self {
            process_id,
            frames_per_buffer,
        }
    }
}

pub trait AudioCapture {
    fn read_stereo(&mut self, output: &mut [StereoFrame]) -> AudioResult<usize>;
}

#[cfg(windows)]
pub fn open_microphone_capture(spec: &CaptureSpec) -> AudioResult<Box<dyn AudioCapture>> {
    super::wasapi_io::open_capture(spec)
}

#[cfg(windows)]
pub fn open_process_loopback_capture(
    spec: &ProcessLoopbackSpec,
) -> AudioResult<Box<dyn AudioCapture>> {
    super::wasapi_io::open_process_loopback_capture(spec)
}

#[cfg(not(windows))]
pub fn open_microphone_capture(_spec: &CaptureSpec) -> AudioResult<Box<dyn AudioCapture>> {
    Err(AudioError::CaptureFailed(
        "WASAPI microphone capture worker is not connected yet".to_string(),
    ))
}

#[cfg(not(windows))]
pub fn open_process_loopback_capture(
    _spec: &ProcessLoopbackSpec,
) -> AudioResult<Box<dyn AudioCapture>> {
    Err(AudioError::CaptureFailed(
        "WASAPI process loopback capture is not connected on this platform".to_string(),
    ))
}
