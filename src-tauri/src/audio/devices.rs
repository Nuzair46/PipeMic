use super::{
    AudioResult,
    types::{AudioDevice, DeviceFlow},
};

#[cfg(windows)]
use super::windows_wasapi;

pub fn list_capture_devices() -> AudioResult<Vec<AudioDevice>> {
    list_devices(DeviceFlow::Capture)
}

pub fn list_render_devices() -> AudioResult<Vec<AudioDevice>> {
    list_devices(DeviceFlow::Render)
}

#[cfg(windows)]
fn list_devices(flow: DeviceFlow) -> AudioResult<Vec<AudioDevice>> {
    windows_wasapi::list_devices(flow)
}

#[cfg(not(windows))]
fn list_devices(_flow: DeviceFlow) -> AudioResult<Vec<AudioDevice>> {
    Ok(Vec::new())
}

pub fn contains_device(devices: &[AudioDevice], id: &Option<String>) -> bool {
    match id {
        Some(id) => devices.iter().any(|device| &device.id == id),
        None => false,
    }
}
