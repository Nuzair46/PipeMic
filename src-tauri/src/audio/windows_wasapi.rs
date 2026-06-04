#![allow(unsafe_op_in_unsafe_fn)]

use std::path::PathBuf;

use windows::{
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::{BOOL, CloseHandle, HANDLE, HWND, LPARAM},
        Media::Audio::{
            AudioSessionStateActive, AudioSessionStateExpired, AudioSessionStateInactive,
            DEVICE_STATE_ACTIVE, EDataFlow, IAudioClient, IAudioSessionControl2,
            IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture,
            eConsole, eRender,
        },
        System::{
            Com::{
                CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
                STGM_READ, StructuredStorage::PropVariantToStringAlloc,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
            IsWindowVisible,
        },
    },
    core::{Interface, PWSTR},
};

use super::{
    AudioResult,
    types::{
        AppDiscoverySource, AudioDevice, AudioSession, DeviceFlow, SessionState,
        is_virtual_cable_name,
    },
};

#[derive(Debug, Clone)]
struct WindowAppCandidate {
    title: String,
    executable: String,
    process_id: u32,
}

pub fn list_devices(flow: DeviceFlow) -> AudioResult<Vec<AudioDevice>> {
    init_com();
    let data_flow = data_flow(flow);

    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let default_id = default_endpoint_id(&enumerator, data_flow);
        let collection = enumerator.EnumAudioEndpoints(data_flow, DEVICE_STATE_ACTIVE)?;
        let count = collection.GetCount()?;
        let mut devices = Vec::with_capacity(count as usize);

        for index in 0..count {
            let device = collection.Item(index)?;
            let id = device_id(&device);
            let name = friendly_name(&device).unwrap_or_else(|| id.clone());
            let (channels, sample_rate) = mix_format(&device).unwrap_or((None, None));

            devices.push(AudioDevice {
                id: id.clone(),
                name: name.clone(),
                flow,
                is_default: default_id.as_deref() == Some(id.as_str()),
                is_virtual_cable_like: is_virtual_cable_name(&name),
                channels,
                sample_rate,
            });
        }

        devices.sort_by(|left, right| {
            right
                .is_virtual_cable_like
                .cmp(&left.is_virtual_cable_like)
                .then_with(|| right.is_default.cmp(&left.is_default))
                .then_with(|| left.name.cmp(&right.name))
        });

        Ok(devices)
    }
}

pub fn list_sessions() -> AudioResult<Vec<AudioSession>> {
    init_com();

    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let collection = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        let device_count = collection.GetCount()?;
        let mut sessions = Vec::new();

        for device_index in 0..device_count {
            let device = collection.Item(device_index)?;
            let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
            let session_enumerator = manager.GetSessionEnumerator()?;
            let session_count = session_enumerator.GetCount()?;

            for session_index in 0..session_count {
                let control = session_enumerator.GetSession(session_index)?;
                let control2: IAudioSessionControl2 = control.cast()?;
                let process_id = control2.GetProcessId()?;
                let executable =
                    process_executable(process_id).unwrap_or_else(|| "System Audio".to_string());
                let display_name = {
                    let raw = control.GetDisplayName()?;
                    let display = owned_pwstr_to_string(raw);
                    if display.is_empty() {
                        executable.trim_end_matches(".exe").to_string()
                    } else {
                        display
                    }
                };

                let raw_state = control.GetState()?;
                let state = if raw_state == AudioSessionStateActive {
                    SessionState::Active
                } else if raw_state == AudioSessionStateInactive {
                    SessionState::Inactive
                } else if raw_state == AudioSessionStateExpired {
                    SessionState::Expired
                } else {
                    SessionState::Inactive
                };

                sessions.push(AudioSession {
                    id: format!("session:{}:{process_id}", executable.to_ascii_lowercase()),
                    display_name,
                    executable: executable.clone(),
                    process_id,
                    state,
                    is_excluded_default: false,
                    window_title: None,
                    has_audio_session: true,
                    discovery_source: AppDiscoverySource::AudioSession,
                });
            }
        }

        for window in visible_app_windows() {
            sessions.push(AudioSession {
                id: format!(
                    "window:{}:{}",
                    window.executable.to_ascii_lowercase(),
                    window.process_id
                ),
                display_name: window.executable.trim_end_matches(".exe").to_string(),
                executable: window.executable,
                process_id: window.process_id,
                state: SessionState::Inactive,
                is_excluded_default: false,
                window_title: Some(window.title),
                has_audio_session: false,
                discovery_source: AppDiscoverySource::Window,
            });
        }

        sessions.sort_by(|left, right| {
            right
                .state
                .eq(&SessionState::Active)
                .cmp(&left.state.eq(&SessionState::Active))
                .then_with(|| left.display_name.cmp(&right.display_name))
        });

        Ok(sessions)
    }
}

unsafe fn visible_app_windows() -> Vec<WindowAppCandidate> {
    let mut windows = Vec::new();
    let lparam = LPARAM(&mut windows as *mut Vec<WindowAppCandidate> as isize);
    let _ = EnumWindows(Some(collect_visible_app_window), lparam);
    windows
}

unsafe extern "system" fn collect_visible_app_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if let Some(window) = visible_app_window(hwnd) {
        let windows = &mut *(lparam.0 as *mut Vec<WindowAppCandidate>);
        windows.push(window);
    }

    BOOL(1)
}

unsafe fn visible_app_window(hwnd: HWND) -> Option<WindowAppCandidate> {
    if !IsWindowVisible(hwnd).as_bool() {
        return None;
    }

    let title = window_title(hwnd)?;
    let mut process_id = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut process_id));
    if process_id == 0 {
        return None;
    }

    let executable = process_executable(process_id)?;
    Some(WindowAppCandidate {
        title,
        executable,
        process_id,
    })
}

unsafe fn window_title(hwnd: HWND) -> Option<String> {
    let length = GetWindowTextLengthW(hwnd);
    if length <= 0 {
        return None;
    }

    let mut buffer = vec![0u16; length as usize + 1];
    let written = GetWindowTextW(hwnd, &mut buffer);
    if written <= 0 {
        return None;
    }

    let title = String::from_utf16_lossy(&buffer[..written as usize])
        .trim()
        .to_string();
    (!title.is_empty()).then_some(title)
}

pub(crate) fn init_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

pub(crate) unsafe fn audio_client_for_endpoint_id(id: &str) -> AudioResult<IAudioClient> {
    init_com();
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let wide = wide_null(id);
    let device = enumerator.GetDevice(windows::core::PCWSTR(wide.as_ptr()))?;
    Ok(device.Activate(CLSCTX_ALL, None)?)
}

fn data_flow(flow: DeviceFlow) -> EDataFlow {
    match flow {
        DeviceFlow::Capture => eCapture,
        DeviceFlow::Render => eRender,
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn default_endpoint_id(enumerator: &IMMDeviceEnumerator, flow: EDataFlow) -> Option<String> {
    enumerator
        .GetDefaultAudioEndpoint(flow, eConsole)
        .ok()
        .map(|device| device_id(&device))
}

unsafe fn device_id(device: &IMMDevice) -> String {
    let raw = device.GetId().unwrap_or_else(|_| PWSTR::null());
    owned_pwstr_to_string(raw)
}

unsafe fn friendly_name(device: &IMMDevice) -> Option<String> {
    let store = device.OpenPropertyStore(STGM_READ).ok()?;
    let prop = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
    let raw = PropVariantToStringAlloc(&prop).ok()?;
    let name = owned_pwstr_to_string(raw);
    (!name.is_empty()).then_some(name)
}

unsafe fn mix_format(device: &IMMDevice) -> Option<(Option<u16>, Option<u32>)> {
    let audio_client: IAudioClient = device.Activate(CLSCTX_ALL, None).ok()?;
    let format = audio_client.GetMixFormat().ok()?;
    if format.is_null() {
        return None;
    }

    let channels = Some((*format).nChannels);
    let sample_rate = Some((*format).nSamplesPerSec);
    CoTaskMemFree(Some(format as _));
    Some((channels, sample_rate))
}

unsafe fn owned_pwstr_to_string(raw: PWSTR) -> String {
    if raw.is_null() {
        return String::new();
    }

    let value = wide_ptr_to_string(raw.0);
    CoTaskMemFree(Some(raw.0 as _));
    value
}

unsafe fn wide_ptr_to_string(raw: *const u16) -> String {
    if raw.is_null() {
        return String::new();
    }

    let mut len = 0;
    while *raw.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(raw, len))
}

unsafe fn process_executable(process_id: u32) -> Option<String> {
    if process_id == 0 {
        return Some("System Audio".to_string());
    }

    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;
    let path = process_image_path(handle);
    let _ = CloseHandle(handle);

    path.and_then(|path| {
        PathBuf::from(path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
    })
}

unsafe fn process_image_path(handle: HANDLE) -> Option<String> {
    let mut buffer = [0u16; 32_768];
    let mut size = buffer.len() as u32;
    if QueryFullProcessImageNameW(
        handle,
        PROCESS_NAME_WIN32,
        PWSTR(buffer.as_mut_ptr()),
        &mut size,
    )
    .is_err()
    {
        return None;
    }

    Some(String::from_utf16_lossy(&buffer[..size as usize]))
}
