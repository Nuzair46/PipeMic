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
    capture::{AudioCapture, CaptureSpec, ProcessLoopbackSpec},
    mixer::{SAMPLE_RATE, StereoFrame},
    render::{AudioRender, RenderSpec},
    windows_wasapi,
};

const HNS_PER_SECOND: i64 = 10_000_000;
const WAVE_FORMAT_PCM_TAG: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT_TAG: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE_TAG: u16 = 0xfffe;
const IEEE_FLOAT_SUBFORMAT: windows::core::GUID =
    windows::core::GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);
const PCM_SUBFORMAT: windows::core::GUID =
    windows::core::GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);

#[derive(Debug, Clone, Copy)]
enum SampleKind {
    Float32,
    Int16,
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
        let client = windows_wasapi::audio_client_for_endpoint_id(&spec.device_id)?;
        let format = initialize_client(
            &client,
            desired_float_stereo(),
            spec.frames_per_buffer,
            stream_flags(),
        )?;
        let capture = client.GetService::<IAudioCaptureClient>()?;
        client.Start()?;

        Ok(Box::new(WasapiCapture {
            client,
            capture,
            format,
        }))
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

        Ok(Box::new(WasapiCapture {
            client,
            capture,
            format,
        }))
    }
}

pub(crate) fn open_render(spec: &RenderSpec) -> AudioResult<Box<dyn AudioRender>> {
    unsafe {
        windows_wasapi::init_com();
        let client = windows_wasapi::audio_client_for_endpoint_id(&spec.device_id)?;
        let format = initialize_client(
            &client,
            desired_float_stereo(),
            spec.frames_per_buffer,
            stream_flags(),
        )?;
        let render = client.GetService::<IAudioRenderClient>()?;
        let buffer_frames = client.GetBufferSize()?;
        client.Start()?;

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

impl AudioCapture for WasapiCapture {
    fn read_stereo(&mut self, output: &mut [StereoFrame]) -> AudioResult<usize> {
        unsafe {
            let mut written = 0;
            let mut packet_frames = self.capture.GetNextPacketSize()?;

            while packet_frames > 0 && written < output.len() {
                let mut data = ptr::null_mut();
                let mut frames_to_read = 0;
                let mut flags = 0;
                self.capture
                    .GetBuffer(&mut data, &mut frames_to_read, &mut flags, None, None)?;

                let available = frames_to_read as usize;
                let copy_frames = available.min(output.len() - written);
                let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;

                if silent || data.is_null() {
                    for frame in &mut output[written..written + copy_frames] {
                        *frame = [0.0, 0.0];
                    }
                } else {
                    for frame_index in 0..copy_frames {
                        output[written + frame_index] =
                            read_stereo_frame(data, frame_index, self.format);
                    }
                }

                self.capture.ReleaseBuffer(frames_to_read)?;
                written += copy_frames;
                packet_frames = self.capture.GetNextPacketSize()?;
            }

            Ok(written)
        }
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

unsafe fn initialize_client(
    client: &IAudioClient,
    desired: WAVEFORMATEX,
    frames_per_buffer: usize,
    flags: u32,
) -> AudioResult<StreamFormat> {
    let desired_supported = client
        .IsFormatSupported(AUDCLNT_SHAREMODE_SHARED, &desired, None)
        .0
        == 0;

    if desired_supported {
        let format = stream_format_from_wave(&desired)?;
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            flags,
            buffer_duration_hns(frames_per_buffer, format.sample_rate),
            0,
            &desired,
            None,
        )?;
        return Ok(format);
    }

    let mix_format = client.GetMixFormat()?;
    if mix_format.is_null() {
        return Err(AudioError::CaptureFailed(
            "WASAPI returned a null mix format".to_string(),
        ));
    }

    let format = stream_format_from_wave(&*mix_format)?;
    client.Initialize(
        AUDCLNT_SHAREMODE_SHARED,
        flags,
        buffer_duration_hns(frames_per_buffer, format.sample_rate),
        0,
        mix_format,
        None,
    )?;
    CoTaskMemFree(Some(mix_format as _));
    Ok(format)
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
    } else if tag == WAVE_FORMAT_PCM_TAG && bits_per_sample == 32 {
        SampleKind::Int32
    } else if tag == WAVE_FORMAT_EXTENSIBLE_TAG {
        let extensible = &*(format as *const WAVEFORMATEX as *const WAVEFORMATEXTENSIBLE);
        let sub_format = extensible.SubFormat;
        if sub_format == IEEE_FLOAT_SUBFORMAT && bits_per_sample == 32 {
            SampleKind::Float32
        } else if sub_format == PCM_SUBFORMAT && bits_per_sample == 16 {
            SampleKind::Int16
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
        SampleKind::Int32 => ptr::read_unaligned(ptr as *const i32) as f32 / i32::MAX as f32,
    }
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
        SampleKind::Int32 => {
            ptr::write_unaligned(ptr as *mut i32, (sample * i32::MAX as f32) as i32)
        }
    }
}
