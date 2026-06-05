#![allow(non_snake_case, unsafe_op_in_unsafe_fn)]

use std::{
    mem::{self, ManuallyDrop},
    ptr,
    sync::{Mutex as StdMutex, mpsc},
    time::Duration,
};

use windows::{
    Win32::{
        Media::Audio::{
            AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_LOOPBACK,
            AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, AUDIOCLIENT_ACTIVATION_PARAMS,
            AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, ActivateAudioInterfaceAsync,
            IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
            IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient,
            IAudioRenderClient, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
        },
        System::Com::CoTaskMemFree,
        System::Variant::VT_BLOB,
    },
    core::{HRESULT, IUnknown, Interface, PROPVARIANT},
};

use super::{
    AudioError, AudioResult,
    capture::{AudioCapture, CaptureDiagnostics, CaptureSpec, ProcessLoopbackSpec},
    mixer::{FrameSpillBuffer, SAMPLE_RATE, StereoFrame},
    render::{AudioRender, RenderSpec},
    windows_wasapi,
};

const HNS_PER_SECOND: i64 = 10_000_000;
const WAVE_FORMAT_PCM_TAG: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT_TAG: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE_TAG: u16 = 0xfffe;
const AUDCLNT_E_DEVICE_IN_USE: &str = "HRESULT 0x8889000A";
const IEEE_FLOAT_SUBFORMAT: windows::core::GUID =
    windows::core::GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);
const PCM_SUBFORMAT: windows::core::GUID =
    windows::core::GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);

#[derive(Debug, Clone, Copy)]
enum SampleKind {
    Float32,
    Int16,
    Int24,
    Int32,
}

#[derive(Debug, Clone, Copy)]
struct StreamFormat {
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
    block_align: u16,
    kind: SampleKind,
}

pub(crate) struct WasapiCapture {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    format: StreamFormat,
    pending: FrameSpillBuffer,
    diagnostics: CaptureDiagnostics,
}

pub(crate) struct WasapiRender {
    client: IAudioClient,
    render: IAudioRenderClient,
    format: StreamFormat,
    buffer_frames: u32,
    downmix_to_mono: bool,
}

pub(crate) fn open_capture(spec: &CaptureSpec) -> AudioResult<Box<dyn AudioCapture>> {
    unsafe {
        windows_wasapi::init_com();
        let (client, format) =
            initialize_capture_client(&spec.device_id, spec.frames_per_buffer)?;
        let capture = client.GetService::<IAudioCaptureClient>().map_err(|error| {
            AudioError::CaptureFailed(format!(
                "microphone GetService(IAudioCaptureClient) failed: {}",
                windows_error_detail(&error)
            ))
        })?;
        client.Start().map_err(|error| {
            AudioError::CaptureFailed(format!(
                "microphone Start failed: {}",
                windows_error_detail(&error)
            ))
        })?;

        Ok(Box::new(WasapiCapture::new(
            client,
            capture,
            format,
            spec.frames_per_buffer,
        )))
    }
}

pub(crate) fn open_process_loopback_capture(
    spec: &ProcessLoopbackSpec,
) -> AudioResult<Box<dyn AudioCapture>> {
    unsafe {
        windows_wasapi::init_com();
        let client = activate_process_loopback_client(spec.process_id)?;
        let format = initialize_process_loopback_client(&client, spec.frames_per_buffer)?;
        let capture = client
            .GetService::<IAudioCaptureClient>()
            .map_err(|error| {
                AudioError::CaptureFailed(format!(
                    "process loopback GetService(IAudioCaptureClient) failed: {}",
                    error.message()
                ))
            })?;
        client.Start().map_err(|error| {
            AudioError::CaptureFailed(format!(
                "process loopback Start failed: {}",
                error.message()
            ))
        })?;

        Ok(Box::new(WasapiCapture::new(
            client,
            capture,
            format,
            spec.frames_per_buffer,
        )))
    }
}

pub(crate) fn open_render(spec: &RenderSpec) -> AudioResult<Box<dyn AudioRender>> {
    unsafe {
        windows_wasapi::init_com();
        let (client, format) = initialize_render_client(&spec.device_id, spec.frames_per_buffer)?;
        let render = client.GetService::<IAudioRenderClient>().map_err(|error| {
            AudioError::Backend(format!(
                "render GetService(IAudioRenderClient) failed: {}",
                windows_error_detail(&error)
            ))
        })?;
        let buffer_frames = client.GetBufferSize()?;
        client.Start().map_err(|error| {
            AudioError::Backend(format!(
                "render Start failed: {}",
                windows_error_detail(&error)
            ))
        })?;

        Ok(Box::new(WasapiRender {
            client,
            render,
            format,
            buffer_frames,
            downmix_to_mono: spec.downmix_to_mono,
        }))
    }
}

#[windows::core::implement(IActivateAudioInterfaceCompletionHandler)]
struct ActivationHandler {
    sender: StdMutex<Option<mpsc::Sender<Result<usize, String>>>>,
}

#[allow(non_snake_case)]
impl IActivateAudioInterfaceCompletionHandler_Impl for ActivationHandler_Impl {
    fn ActivateCompleted(
        &self,
        activateoperation: Option<&IActivateAudioInterfaceAsyncOperation>,
    ) -> windows_core::Result<()> {
        let result = unsafe { activated_audio_client_raw(activateoperation) };
        if let Some(sender) = self.sender.lock().ok().and_then(|mut guard| guard.take()) {
            let _ = sender.send(result);
        }
        Ok(())
    }
}

unsafe fn activated_audio_client_raw(
    activateoperation: Option<&IActivateAudioInterfaceAsyncOperation>,
) -> Result<usize, String> {
    let operation = activateoperation.ok_or_else(|| {
        "process loopback activation callback did not include an operation".to_string()
    })?;
    let mut activate_result = HRESULT(0);
    let mut activated: Option<IUnknown> = None;
    operation
        .GetActivateResult(&mut activate_result, &mut activated)
        .map_err(|error| error.message().to_string())?;
    activate_result
        .ok()
        .map_err(|error| error.message().to_string())?;

    let unknown =
        activated.ok_or_else(|| "process loopback activation returned no interface".to_string())?;
    let client: IAudioClient = unknown
        .cast()
        .map_err(|error| error.message().to_string())?;
    Ok(client.into_raw() as usize)
}

unsafe fn activate_process_loopback_client(process_id: u32) -> AudioResult<IAudioClient> {
    let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: process_id,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };

    let raw_prop = windows::core::imp::PROPVARIANT {
        Anonymous: windows::core::imp::PROPVARIANT_0 {
            Anonymous: windows::core::imp::PROPVARIANT_0_0 {
                vt: VT_BLOB.0,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: windows::core::imp::PROPVARIANT_0_0_0 {
                    blob: windows::core::imp::BLOB {
                        cbSize: mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                        pBlobData: &mut params as *mut AUDIOCLIENT_ACTIVATION_PARAMS as *mut u8,
                    },
                },
            },
        },
    };
    let prop = ManuallyDrop::new(PROPVARIANT::from_raw(raw_prop));
    let (sender, receiver) = mpsc::channel();
    let handler: IActivateAudioInterfaceCompletionHandler = ActivationHandler {
        sender: StdMutex::new(Some(sender)),
    }
    .into();

    let _operation = ActivateAudioInterfaceAsync(
        VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
        &IAudioClient::IID,
        Some(&*prop as *const PROPVARIANT),
        &handler,
    )?;

    let raw_client = receiver
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| {
            AudioError::CaptureFailed(format!(
                "timed out activating process loopback for PID {process_id}"
            ))
        })?
        .map_err(|message| {
            AudioError::CaptureFailed(format!(
                "failed to activate process loopback for PID {process_id}: {message}"
            ))
        })?;

    Ok(IAudioClient::from_raw(raw_client as _))
}

impl WasapiCapture {
    fn new(
        client: IAudioClient,
        capture: IAudioCaptureClient,
        format: StreamFormat,
        frames_per_buffer: usize,
    ) -> Self {
        Self {
            client,
            capture,
            format,
            pending: FrameSpillBuffer::new(frames_per_buffer.saturating_mul(4)),
            diagnostics: CaptureDiagnostics::default(),
        }
    }

    fn record_read_shape(&mut self, written: usize, requested: usize) {
        self.diagnostics.pending_overflows = self
            .diagnostics
            .pending_overflows
            .saturating_add(self.pending.take_overflowed());

        if requested == 0 {
            return;
        }

        if written == 0 {
            self.diagnostics.zero_reads = self.diagnostics.zero_reads.saturating_add(1);
        } else if written < requested {
            self.diagnostics.short_reads = self.diagnostics.short_reads.saturating_add(1);
        }
    }

    fn record_capture_error(&mut self) {
        self.diagnostics.errors = self.diagnostics.errors.saturating_add(1);
    }
}

impl AudioCapture for WasapiCapture {
    fn read_stereo(&mut self, output: &mut [StereoFrame]) -> AudioResult<usize> {
        unsafe {
            let requested = output.len();
            if requested == 0 {
                return Ok(0);
            }

            let mut written = self.pending.drain_into(output);
            let mut packet_frames = if written < output.len() {
                match self.capture.GetNextPacketSize() {
                    Ok(frames) => frames,
                    Err(error) => {
                        self.record_capture_error();
                        return Err(error.into());
                    }
                }
            } else {
                0
            };

            while packet_frames > 0 && written < output.len() {
                let mut data = ptr::null_mut();
                let mut frames_to_read = 0;
                let mut flags = 0;
                if let Err(error) =
                    self.capture
                        .GetBuffer(&mut data, &mut frames_to_read, &mut flags, None, None)
                {
                    self.record_capture_error();
                    return Err(error.into());
                }

                let available = frames_to_read as usize;
                let copy_frames = available.min(output.len() - written);
                let spill_frames = available.saturating_sub(copy_frames);
                let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;

                if silent || data.is_null() {
                    for frame in &mut output[written..written + copy_frames] {
                        *frame = [0.0, 0.0];
                    }
                    if spill_frames > 0 {
                        self.pending
                            .push_frames(std::iter::repeat([0.0, 0.0]).take(spill_frames));
                    }
                } else {
                    let format = self.format;
                    for frame_index in 0..copy_frames {
                        output[written + frame_index] = read_stereo_frame(data, frame_index, format);
                    }
                    if spill_frames > 0 {
                        self.pending.push_frames(
                            (copy_frames..available)
                                .map(|frame_index| read_stereo_frame(data, frame_index, format)),
                        );
                    }
                }

                if let Err(error) = self.capture.ReleaseBuffer(frames_to_read) {
                    self.record_capture_error();
                    return Err(error.into());
                }
                written += copy_frames;
                if written >= output.len() {
                    break;
                }

                packet_frames = match self.capture.GetNextPacketSize() {
                    Ok(frames) => frames,
                    Err(error) => {
                        self.record_capture_error();
                        return Err(error.into());
                    }
                };
            }

            self.record_read_shape(written, requested);
            Ok(written)
        }
    }

    fn take_diagnostics(&mut self) -> CaptureDiagnostics {
        self.diagnostics.pending_overflows = self
            .diagnostics
            .pending_overflows
            .saturating_add(self.pending.take_overflowed());
        let diagnostics = self.diagnostics;
        self.diagnostics = CaptureDiagnostics::default();
        diagnostics
    }
}

impl AudioRender for WasapiRender {
    fn write_stereo(&mut self, frames: &[StereoFrame]) -> AudioResult<usize> {
        unsafe {
            let padding = self.client.GetCurrentPadding()?;
            let writable = self.buffer_frames.saturating_sub(padding) as usize;
            let frames_to_write = writable.min(frames.len());
            if frames_to_write == 0 {
                return Ok(0);
            }

            let data = self.render.GetBuffer(frames_to_write as u32)?;
            for (frame_index, frame) in frames.iter().take(frames_to_write).enumerate() {
                write_stereo_frame(data, frame_index, *frame, self.format, self.downmix_to_mono);
            }
            self.render.ReleaseBuffer(frames_to_write as u32, 0)?;

            Ok(frames_to_write)
        }
    }
}

impl Drop for WasapiCapture {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

impl Drop for WasapiRender {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

unsafe fn initialize_capture_client(
    device_id: &str,
    frames_per_buffer: usize,
) -> AudioResult<(IAudioClient, StreamFormat)> {
    let mut failures = Vec::new();
    let desired = desired_float_stereo();

    for (flags, label) in [(stream_flags(), "with conversion"), (0, "without conversion")] {
        if let Some(result) = try_initialize_capture_format(
            device_id,
            &desired,
            frames_per_buffer,
            flags,
            &format!("48 kHz float stereo {label}"),
            &mut failures,
        ) {
            return Ok(result);
        }
    }

    for (flags, label) in [(stream_flags(), "with conversion"), (0, "without conversion")] {
        if let Some(result) = try_initialize_capture_mix_format(
            device_id,
            frames_per_buffer,
            flags,
            &format!("device mix format {label}"),
            &mut failures,
        ) {
            return Ok(result);
        }
    }

    Err(AudioError::CaptureFailed(capture_initialize_failure_message(
        &failures,
    )))
}

unsafe fn try_initialize_capture_format(
    device_id: &str,
    wave_format: &WAVEFORMATEX,
    frames_per_buffer: usize,
    flags: u32,
    label: &str,
    failures: &mut Vec<String>,
) -> Option<(IAudioClient, StreamFormat)> {
    let stream_format = match stream_format_from_wave(wave_format) {
        Ok(format) => format,
        Err(error) => {
            failures.push(format!("{label}: {error}"));
            return None;
        }
    };

    for (duration, duration_label) in [
        (
            buffer_duration_hns(frames_per_buffer, stream_format.sample_rate),
            "requested buffer",
        ),
        (0, "default buffer"),
    ] {
        let client = match windows_wasapi::audio_client_for_endpoint_id(device_id) {
            Ok(client) => client,
            Err(error) => {
                failures.push(format!("{label}, {duration_label}: {error}"));
                continue;
            }
        };

        match client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            flags,
            duration,
            0,
            wave_format,
            None,
        ) {
            Ok(()) => return Some((client, stream_format)),
            Err(error) => failures.push(format!(
                "{label}, {duration_label}: {}",
                windows_error_detail(&error)
            )),
        }
    }

    None
}

unsafe fn try_initialize_capture_mix_format(
    device_id: &str,
    frames_per_buffer: usize,
    flags: u32,
    label: &str,
    failures: &mut Vec<String>,
) -> Option<(IAudioClient, StreamFormat)> {
    for duration_label in ["requested buffer", "default buffer"] {
        let client = match windows_wasapi::audio_client_for_endpoint_id(device_id) {
            Ok(client) => client,
            Err(error) => {
                failures.push(format!("{label}, {duration_label}: {error}"));
                continue;
            }
        };
        let mix_format = match client.GetMixFormat() {
            Ok(format) if !format.is_null() => format,
            Ok(_) => {
                failures.push(format!("{label}, {duration_label}: null mix format"));
                continue;
            }
            Err(error) => {
                failures.push(format!(
                    "{label}, {duration_label}: GetMixFormat {}",
                    windows_error_detail(&error)
                ));
                continue;
            }
        };

        let stream_format = match stream_format_from_wave(&*mix_format) {
            Ok(format) => format,
            Err(error) => {
                CoTaskMemFree(Some(mix_format as _));
                failures.push(format!("{label}, {duration_label}: {error}"));
                continue;
            }
        };
        let duration = if duration_label == "requested buffer" {
            buffer_duration_hns(frames_per_buffer, stream_format.sample_rate)
        } else {
            0
        };

        let initialized = client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            flags,
            duration,
            0,
            mix_format,
            None,
        );
        CoTaskMemFree(Some(mix_format as _));

        match initialized {
            Ok(()) => return Some((client, stream_format)),
            Err(error) => failures.push(format!(
                "{label}, {duration_label}: {}",
                windows_error_detail(&error)
            )),
        }
    }

    None
}

unsafe fn initialize_render_client(
    device_id: &str,
    frames_per_buffer: usize,
) -> AudioResult<(IAudioClient, StreamFormat)> {
    let mut failures = Vec::new();
    let desired = desired_float_stereo();

    for (flags, label) in [(stream_flags(), "with conversion"), (0, "without conversion")] {
        if let Some(result) = try_initialize_render_format(
            device_id,
            &desired,
            frames_per_buffer,
            flags,
            &format!("48 kHz float stereo {label}"),
            &mut failures,
        ) {
            return Ok(result);
        }
    }

    for (flags, label) in [(stream_flags(), "with conversion"), (0, "without conversion")] {
        if let Some(result) = try_initialize_render_mix_format(
            device_id,
            frames_per_buffer,
            flags,
            &format!("device mix format {label}"),
            &mut failures,
        ) {
            return Ok(result);
        }
    }

    Err(AudioError::Backend(render_initialize_failure_message(&failures)))
}

unsafe fn try_initialize_render_format(
    device_id: &str,
    wave_format: &WAVEFORMATEX,
    frames_per_buffer: usize,
    flags: u32,
    label: &str,
    failures: &mut Vec<String>,
) -> Option<(IAudioClient, StreamFormat)> {
    let stream_format = match stream_format_from_wave(wave_format) {
        Ok(format) => format,
        Err(error) => {
            failures.push(format!("{label}: {error}"));
            return None;
        }
    };

    for (duration, duration_label) in [
        (
            buffer_duration_hns(frames_per_buffer, stream_format.sample_rate),
            "requested buffer",
        ),
        (0, "default buffer"),
    ] {
        let client = match windows_wasapi::audio_client_for_endpoint_id(device_id) {
            Ok(client) => client,
            Err(error) => {
                failures.push(format!("{label}, {duration_label}: {error}"));
                continue;
            }
        };

        match client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            flags,
            duration,
            0,
            wave_format,
            None,
        ) {
            Ok(()) => return Some((client, stream_format)),
            Err(error) => failures.push(format!(
                "{label}, {duration_label}: {}",
                windows_error_detail(&error)
            )),
        }
    }

    None
}

unsafe fn try_initialize_render_mix_format(
    device_id: &str,
    frames_per_buffer: usize,
    flags: u32,
    label: &str,
    failures: &mut Vec<String>,
) -> Option<(IAudioClient, StreamFormat)> {
    for duration_label in ["requested buffer", "default buffer"] {
        let client = match windows_wasapi::audio_client_for_endpoint_id(device_id) {
            Ok(client) => client,
            Err(error) => {
                failures.push(format!("{label}, {duration_label}: {error}"));
                continue;
            }
        };
        let mix_format = match client.GetMixFormat() {
            Ok(format) if !format.is_null() => format,
            Ok(_) => {
                failures.push(format!("{label}, {duration_label}: null mix format"));
                continue;
            }
            Err(error) => {
                failures.push(format!(
                    "{label}, {duration_label}: GetMixFormat {}",
                    windows_error_detail(&error)
                ));
                continue;
            }
        };

        let stream_format = match stream_format_from_wave(&*mix_format) {
            Ok(format) => format,
            Err(error) => {
                CoTaskMemFree(Some(mix_format as _));
                failures.push(format!("{label}, {duration_label}: {error}"));
                continue;
            }
        };
        let duration = if duration_label == "requested buffer" {
            buffer_duration_hns(frames_per_buffer, stream_format.sample_rate)
        } else {
            0
        };

        let initialized = client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            flags,
            duration,
            0,
            mix_format,
            None,
        );
        CoTaskMemFree(Some(mix_format as _));

        match initialized {
            Ok(()) => return Some((client, stream_format)),
            Err(error) => failures.push(format!(
                "{label}, {duration_label}: {}",
                windows_error_detail(&error)
            )),
        }
    }

    None
}

fn windows_error_detail(error: &windows::core::Error) -> String {
    let code = error.code().0 as u32;
    let message = error.message().to_string();
    if message.trim().is_empty() {
        format!("HRESULT 0x{code:08X}")
    } else {
        format!("HRESULT 0x{code:08X} ({message})")
    }
}

fn render_initialize_failure_message(failures: &[String]) -> String {
    if failures.iter().any(|failure| failure.contains(AUDCLNT_E_DEVICE_IN_USE)) {
        return "Virtual mic is busy (HRESULT 0x8889000A). Close apps using CABLE Input or CABLE Output, or disable exclusive mode for both VB-CABLE endpoints in Windows Sound settings, then start PipeMic again.".to_string();
    }

    format!(
        "render Initialize failed for all formats: {}",
        failures.join("; ")
    )
}

fn capture_initialize_failure_message(failures: &[String]) -> String {
    format!(
        "microphone Initialize failed for all formats: {}",
        failures.join("; ")
    )
}

unsafe fn initialize_process_loopback_client(
    client: &IAudioClient,
    frames_per_buffer: usize,
) -> AudioResult<StreamFormat> {
    let flags = stream_flags() | AUDCLNT_STREAMFLAGS_LOOPBACK;
    let candidates = [
        desired_float_stereo(),
        pcm_stereo(SAMPLE_RATE),
        pcm_stereo(44_100),
    ];
    let mut failures = Vec::new();

    for format in candidates {
        let stream_format = stream_format_from_wave(&format)?;
        match client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            flags,
            buffer_duration_hns(frames_per_buffer, stream_format.sample_rate),
            0,
            &format,
            None,
        ) {
            Ok(()) => return Ok(stream_format),
            Err(error) => {
                let sample_rate = stream_format.sample_rate;
                let bits_per_sample = stream_format.bits_per_sample;
                let format_tag = format.wFormatTag;
                failures.push(format!(
                    "{sample_rate} Hz / {bits_per_sample} bit / tag {format_tag}: {}",
                    error.message()
                ));
            }
        }
    }

    Err(AudioError::CaptureFailed(format!(
        "process loopback Initialize failed for all explicit formats: {}",
        failures.join("; ")
    )))
}

fn desired_float_stereo() -> WAVEFORMATEX {
    let channels = 2;
    let bits_per_sample = 32;
    let block_align = channels * (bits_per_sample / 8);

    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_IEEE_FLOAT_TAG,
        nChannels: channels,
        nSamplesPerSec: SAMPLE_RATE,
        nAvgBytesPerSec: SAMPLE_RATE * block_align as u32,
        nBlockAlign: block_align,
        wBitsPerSample: bits_per_sample,
        cbSize: 0,
    }
}

fn pcm_stereo(sample_rate: u32) -> WAVEFORMATEX {
    let channels = 2;
    let bits_per_sample = 16;
    let block_align = channels * (bits_per_sample / 8);

    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM_TAG,
        nChannels: channels,
        nSamplesPerSec: sample_rate,
        nAvgBytesPerSec: sample_rate * block_align as u32,
        nBlockAlign: block_align,
        wBitsPerSample: bits_per_sample,
        cbSize: 0,
    }
}

fn stream_flags() -> u32 {
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY
}

fn buffer_duration_hns(frames_per_buffer: usize, sample_rate: u32) -> i64 {
    ((frames_per_buffer as f64 / sample_rate as f64) * HNS_PER_SECOND as f64).round() as i64
}

unsafe fn stream_format_from_wave(format: &WAVEFORMATEX) -> AudioResult<StreamFormat> {
    let channels = format.nChannels;
    let sample_rate = format.nSamplesPerSec;
    let bits_per_sample = format.wBitsPerSample;
    let block_align = format.nBlockAlign;
    let tag = format.wFormatTag;

    let kind = if tag == WAVE_FORMAT_IEEE_FLOAT_TAG && bits_per_sample == 32 {
        SampleKind::Float32
    } else if tag == WAVE_FORMAT_PCM_TAG && bits_per_sample == 16 {
        SampleKind::Int16
    } else if tag == WAVE_FORMAT_PCM_TAG && bits_per_sample == 24 {
        SampleKind::Int24
    } else if tag == WAVE_FORMAT_PCM_TAG && bits_per_sample == 32 {
        SampleKind::Int32
    } else if tag == WAVE_FORMAT_EXTENSIBLE_TAG {
        let extensible = &*(format as *const WAVEFORMATEX as *const WAVEFORMATEXTENSIBLE);
        let sub_format = extensible.SubFormat;
        if sub_format == IEEE_FLOAT_SUBFORMAT && bits_per_sample == 32 {
            SampleKind::Float32
        } else if sub_format == PCM_SUBFORMAT && bits_per_sample == 16 {
            SampleKind::Int16
        } else if sub_format == PCM_SUBFORMAT && bits_per_sample == 24 {
            SampleKind::Int24
        } else if sub_format == PCM_SUBFORMAT && bits_per_sample == 32 {
            SampleKind::Int32
        } else {
            return Err(AudioError::CaptureFailed(format!(
                "unsupported WASAPI extensible format: {} bits",
                bits_per_sample
            )));
        }
    } else {
        return Err(AudioError::CaptureFailed(format!(
            "unsupported WASAPI format tag {tag} with {bits_per_sample} bits"
        )));
    };

    Ok(StreamFormat {
        channels,
        sample_rate,
        bits_per_sample,
        block_align,
        kind,
    })
}

unsafe fn read_stereo_frame(
    data: *const u8,
    frame_index: usize,
    format: StreamFormat,
) -> StereoFrame {
    let left = read_sample(data, frame_index, 0, format);
    let right_channel = if format.channels > 1 { 1 } else { 0 };
    let right = read_sample(data, frame_index, right_channel as usize, format);
    [left, right]
}

unsafe fn read_sample(
    data: *const u8,
    frame_index: usize,
    channel: usize,
    format: StreamFormat,
) -> f32 {
    let channel = channel.min(format.channels.saturating_sub(1) as usize);
    let offset =
        frame_index * format.block_align as usize + channel * (format.bits_per_sample as usize / 8);
    let ptr = data.add(offset);

    match format.kind {
        SampleKind::Float32 => ptr::read_unaligned(ptr as *const f32).clamp(-1.0, 1.0),
        SampleKind::Int16 => ptr::read_unaligned(ptr as *const i16) as f32 / i16::MAX as f32,
        SampleKind::Int24 => read_i24(ptr),
        SampleKind::Int32 => ptr::read_unaligned(ptr as *const i32) as f32 / i32::MAX as f32,
    }
}

unsafe fn read_i24(ptr: *const u8) -> f32 {
    let raw = ptr::read_unaligned(ptr) as u32
        | ((ptr::read_unaligned(ptr.add(1)) as u32) << 8)
        | ((ptr::read_unaligned(ptr.add(2)) as u32) << 16);
    let signed = if raw & 0x0080_0000 != 0 {
        (raw | 0xff00_0000) as i32
    } else {
        raw as i32
    };
    (signed as f32 / 8_388_607.0).clamp(-1.0, 1.0)
}

unsafe fn write_stereo_frame(
    data: *mut u8,
    frame_index: usize,
    frame: StereoFrame,
    format: StreamFormat,
    downmix_to_mono: bool,
) {
    let channels = format.channels.max(1) as usize;
    for channel in 0..channels {
        let sample = if downmix_to_mono || channels == 1 {
            (frame[0] + frame[1]) * 0.5
        } else if channel == 0 {
            frame[0]
        } else if channel == 1 {
            frame[1]
        } else {
            (frame[0] + frame[1]) * 0.5
        };
        write_sample(data, frame_index, channel, sample, format);
    }
}

unsafe fn write_sample(
    data: *mut u8,
    frame_index: usize,
    channel: usize,
    sample: f32,
    format: StreamFormat,
) {
    let offset =
        frame_index * format.block_align as usize + channel * (format.bits_per_sample as usize / 8);
    let ptr = data.add(offset);
    let sample = sample.clamp(-1.0, 1.0);

    match format.kind {
        SampleKind::Float32 => ptr::write_unaligned(ptr as *mut f32, sample),
        SampleKind::Int16 => {
            ptr::write_unaligned(ptr as *mut i16, (sample * i16::MAX as f32) as i16)
        }
        SampleKind::Int24 => write_i24(ptr, sample),
        SampleKind::Int32 => {
            ptr::write_unaligned(ptr as *mut i32, (sample * i32::MAX as f32) as i32)
        }
    }
}

unsafe fn write_i24(ptr: *mut u8, sample: f32) {
    let value = (sample * 8_388_607.0)
        .round()
        .clamp(-8_388_608.0, 8_388_607.0) as i32;
    ptr::write_unaligned(ptr, (value & 0xff) as u8);
    ptr::write_unaligned(ptr.add(1), ((value >> 8) & 0xff) as u8);
    ptr::write_unaligned(ptr.add(2), ((value >> 16) & 0xff) as u8);
}
