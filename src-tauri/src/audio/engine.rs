use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

#[cfg(windows)]
use std::{thread, time::Instant};

use parking_lot::Mutex;

#[cfg(windows)]
use crate::config::AppSourceConfig;
use crate::config::{AppConfig, ControlUpdate, MicSourceConfig};

use super::{
    AudioResult, devices,
    mixer::{MixerControls, SourceControl},
    types::{
        AudioDevice, LevelMeters, RouteState, RouteStatus, is_canonical_vb_cable_input_name,
        is_sixteen_channel_cable_name,
    },
};

#[cfg(windows)]
use super::capture::CaptureDiagnostics;
#[cfg(windows)]
use super::mixer::{self, SourceMix, StereoFrame};
#[cfg(windows)]
use super::{sessions, types::SessionState};

const METER_DECAY_PER_SECOND: f32 = 2.8;
#[cfg(windows)]
const CAPTURE_PRESSURE_WARNING: &str =
    "Microphone capture is falling behind; increase buffer size or close audio-heavy apps.";
#[cfg(windows)]
const OUTPUT_PRESSURE_WARNING: &str =
    "Output device is not accepting audio quickly enough; close apps using the virtual mic or increase buffer size.";
#[cfg(windows)]
const PRESSURE_WARNING_COOLDOWN: Duration = Duration::from_secs(15);
#[cfg(windows)]
const CAPTURE_PRESSURE_WINDOW: Duration = Duration::from_secs(5);
#[cfg(windows)]
const CAPTURE_PRESSURE_WARNING_THRESHOLD: usize = 1;
#[cfg(windows)]
const OUTPUT_ZERO_WRITE_WARNING_THRESHOLD: usize = 30;

pub struct AudioEngine {
    controls: MixerControls,
    status: RouteStatus,
    worker: Option<RouteWorker>,
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self {
            controls: MixerControls::default(),
            status: RouteStatus::default(),
            worker: None,
        }
    }
}

impl AudioEngine {
    pub fn start(&mut self, config: &AppConfig) -> AudioResult<RouteStatus> {
        let capture_devices = devices::list_capture_devices()?;
        let render_devices = devices::list_render_devices()?;

        let output_device_id = match resolve_output_device_id(&render_devices, &config.output_device_id)
        {
            Some(id) => id,
            None => {
                self.stop_worker();
                self.status = RouteStatus {
                    state: RouteState::DeviceMissing,
                    message: "Selected output is unavailable".to_string(),
                    meters: LevelMeters::default(),
                    warnings: vec![
                        "Pick an active render endpoint, ideally VB-CABLE input.".to_string(),
                    ],
                };
                return Ok(self.status.clone());
            }
        };

        if !devices::contains_device(&render_devices, &Some(output_device_id.clone())) {
            self.stop_worker();
            self.status = RouteStatus {
                state: RouteState::DeviceMissing,
                message: "Selected output is unavailable".to_string(),
                meters: LevelMeters::default(),
                warnings: vec![
                    "Pick an active render endpoint, ideally VB-CABLE input.".to_string(),
                ],
            };
            return Ok(self.status.clone());
        }

        if config.mic_sources.is_empty() && config.app_sources.is_empty() {
            self.stop_worker();
            self.status = RouteStatus {
                state: RouteState::DeviceMissing,
                message: "No input sources configured".to_string(),
                meters: LevelMeters::default(),
                warnings: vec!["Add at least one microphone or application source.".to_string()],
            };
            return Ok(self.status.clone());
        }

        let warnings = self.source_warnings(config, &capture_devices);
        self.controls = controls_from_config(config);
        self.status = RouteStatus {
            state: RouteState::Running,
            message: "Routing to selected output".to_string(),
            meters: meters_for_config(config),
            warnings,
        };
        self.stop_worker();
        let mut worker_config = config.clone();
        worker_config.output_device_id = Some(output_device_id);
        self.worker = RouteWorker::start(
            worker_config,
            self.controls.clone(),
            self.status.clone(),
        );
        Ok(self.current_status())
    }

    pub fn stop(&mut self) -> RouteStatus {
        self.stop_worker();
        self.status = RouteStatus::default();
        self.status.clone()
    }

    pub fn update_controls(&mut self, controls: &ControlUpdate) -> RouteStatus {
        self.controls = MixerControls {
            mic_sources: controls
                .mic_sources
                .iter()
                .map(|control| SourceControl {
                    id: control.id.clone(),
                    gain: control.gain,
                    muted: control.muted,
                })
                .collect(),
            app_sources: controls
                .app_sources
                .iter()
                .map(|control| SourceControl {
                    id: control.id.clone(),
                    gain: control.gain,
                    muted: control.muted,
                })
                .collect(),
            master_gain: controls.master_gain,
            downmix_to_mono: controls.downmix_to_mono,
        };
        if let Some(worker) = &self.worker {
            *worker.controls.lock() = self.controls.clone();
        }
        self.current_status()
    }

    pub fn current_status(&mut self) -> RouteStatus {
        if let Some(worker) = &self.worker {
            self.status = worker.status.lock().clone();
        }
        self.status.clone()
    }

    fn source_warnings(&self, config: &AppConfig, capture_devices: &[AudioDevice]) -> Vec<String> {
        let mut warnings = Vec::new();
        for source in &config.mic_sources {
            if !capture_devices
                .iter()
                .any(|device| device.id == source.device_id)
            {
                warnings.push(format!(
                    "{} is unavailable.",
                    source_name_for_mic(source, capture_devices)
                ));
            }
        }

        warnings
    }

    fn stop_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.stop();
        }
    }
}

fn resolve_output_device_id(
    render_devices: &[AudioDevice],
    selected_id: &Option<String>,
) -> Option<String> {
    let selected_id = selected_id.as_ref()?;
    let selected = render_devices
        .iter()
        .find(|device| &device.id == selected_id)?;

    if is_sixteen_channel_cable_name(&selected.name) {
        if let Some(canonical) = render_devices
            .iter()
            .find(|device| is_canonical_vb_cable_input_name(&device.name))
        {
            return Some(canonical.id.clone());
        }
    }

    Some(selected.id.clone())
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop_worker();
    }
}

struct RouteWorker {
    stop_flag: Arc<AtomicBool>,
    controls: Arc<Mutex<MixerControls>>,
    status: Arc<Mutex<RouteStatus>>,
    handle: Option<JoinHandle<()>>,
}

#[cfg(windows)]
struct MicRuntime {
    source: MicSourceConfig,
    capture: Option<Box<dyn super::capture::AudioCapture>>,
    buffer: Vec<StereoFrame>,
    last_read: usize,
    last_peak: f32,
    last_peak_at: Instant,
    pressure: CapturePressureTracker,
}

#[cfg(windows)]
struct AppRuntime {
    source: AppSourceConfig,
    capture: Option<Box<dyn super::capture::AudioCapture>>,
    buffer: Vec<StereoFrame>,
    last_read: usize,
    last_peak: f32,
    last_peak_at: Instant,
    active_pid: Option<u32>,
    last_retry: Instant,
}

#[cfg(windows)]
#[derive(Debug)]
struct PressureEventWindow {
    started_at: Instant,
    events: usize,
    last_warning_at: Option<Instant>,
}

#[cfg(windows)]
impl Default for PressureEventWindow {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            events: 0,
            last_warning_at: None,
        }
    }
}

#[cfg(windows)]
impl PressureEventWindow {
    fn record(&mut self, events: usize, threshold: usize, now: Instant) -> bool {
        if events == 0 {
            return false;
        }

        if now.duration_since(self.started_at) >= CAPTURE_PRESSURE_WINDOW {
            self.started_at = now;
            self.events = 0;
        }

        self.events = self.events.saturating_add(events);
        if self.events < threshold {
            return false;
        }

        let can_warn = match self.last_warning_at {
            Some(last_warning_at) => {
                now.duration_since(last_warning_at) >= PRESSURE_WARNING_COOLDOWN
            }
            None => true,
        };

        if can_warn {
            self.events = 0;
            self.last_warning_at = Some(now);
            return true;
        }

        false
    }
}

#[cfg(windows)]
#[derive(Debug, Default)]
struct CapturePressureTracker {
    window: PressureEventWindow,
    zero_reads: usize,
    short_reads: usize,
    pending_overflows: usize,
    errors: usize,
}

#[cfg(windows)]
impl CapturePressureTracker {
    fn record_diagnostics(&mut self, diagnostics: CaptureDiagnostics, now: Instant) -> bool {
        self.zero_reads = self.zero_reads.saturating_add(diagnostics.zero_reads);
        self.short_reads = self.short_reads.saturating_add(diagnostics.short_reads);
        self.pending_overflows = self
            .pending_overflows
            .saturating_add(diagnostics.pending_overflows);
        self.errors = self.errors.saturating_add(diagnostics.errors);

        let pressure_events = diagnostics
            .pending_overflows
            .saturating_add(diagnostics.errors);
        self.window
            .record(pressure_events, CAPTURE_PRESSURE_WARNING_THRESHOLD, now)
    }

    fn record_error(&mut self, now: Instant) -> bool {
        self.errors = self.errors.saturating_add(1);
        self.window
            .record(1, CAPTURE_PRESSURE_WARNING_THRESHOLD, now)
    }
}

#[cfg(windows)]
#[derive(Debug, Default)]
struct RenderPressureTracker {
    zero_writes: usize,
    consecutive_zero_writes: usize,
    last_warning_at: Option<Instant>,
}

#[cfg(windows)]
impl RenderPressureTracker {
    fn record_zero_write(&mut self, now: Instant) -> bool {
        self.zero_writes = self.zero_writes.saturating_add(1);
        self.consecutive_zero_writes = self.consecutive_zero_writes.saturating_add(1);
        if self.consecutive_zero_writes < OUTPUT_ZERO_WRITE_WARNING_THRESHOLD {
            return false;
        }

        let can_warn = match self.last_warning_at {
            Some(last_warning_at) => {
                now.duration_since(last_warning_at) >= PRESSURE_WARNING_COOLDOWN
            }
            None => true,
        };

        if can_warn {
            self.last_warning_at = Some(now);
            return true;
        }

        false
    }

    fn record_progress(&mut self) {
        self.consecutive_zero_writes = 0;
    }
}

impl RouteWorker {
    #[cfg(windows)]
    fn start(config: AppConfig, controls: MixerControls, status: RouteStatus) -> Option<Self> {
        use super::{
            capture::{CaptureSpec, open_microphone_capture},
            render::{RenderSpec, open_render_output},
        };

        let stop_flag = Arc::new(AtomicBool::new(false));
        let controls = Arc::new(Mutex::new(controls));
        let status = Arc::new(Mutex::new(status));

        let thread_stop = Arc::clone(&stop_flag);
        let thread_controls = Arc::clone(&controls);
        let thread_status = Arc::clone(&status);
        let handle = thread::spawn(move || {
            let output_id = match config.output_device_id.clone() {
                Some(id) => id,
                None => {
                    set_worker_failure(&thread_status, "Selected output is unavailable");
                    return;
                }
            };

            let frames = config.buffer_frames.max(mixer::DEFAULT_BUFFER_FRAMES);
            let mut render = match open_render_output(&RenderSpec::cable(
                output_id,
                frames,
                config.downmix_to_mono,
            )) {
                Ok(render) => render,
                Err(error) => {
                    set_worker_failure(&thread_status, &format!("Output render failed: {error}"));
                    return;
                }
            };

            let mut mic_sources: Vec<MicRuntime> = config
                .mic_sources
                .iter()
                .map(|source| {
                    let capture = match open_microphone_capture(&CaptureSpec::mic(
                        source.device_id.clone(),
                        frames,
                    )) {
                        Ok(capture) => Some(capture),
                        Err(error) => {
                            push_worker_warning(
                                &thread_status,
                                format!(
                                    "{} capture failed: {error}",
                                    source_name_for_mic(source, &[])
                                ),
                            );
                            None
                        }
                    };
                    MicRuntime {
                        source: source.clone(),
                        capture,
                        buffer: vec![[0.0, 0.0]; frames],
                        last_read: 0,
                        last_peak: 0.0,
                        last_peak_at: Instant::now(),
                        pressure: CapturePressureTracker::default(),
                    }
                })
                .collect();

            let mut app_sources: Vec<AppRuntime> = config
                .app_sources
                .iter()
                .map(|source| {
                    let (active_pid, capture) = open_app_runtime_capture(source, frames);
                    AppRuntime {
                        source: source.clone(),
                        capture,
                        buffer: vec![[0.0, 0.0]; frames],
                        last_read: 0,
                        last_peak: 0.0,
                        last_peak_at: Instant::now(),
                        active_pid,
                        last_retry: Instant::now(),
                    }
                })
                .collect();
            let mut output_peak = 0.0;
            let mut output_peak_at = Instant::now();
            let mut render_pressure = RenderPressureTracker::default();

            while !thread_stop.load(Ordering::Relaxed) {
                let mut read_frames = 0;

                for source in &mut mic_sources {
                    source.buffer.fill([0.0, 0.0]);
                    source.last_read = 0;
                    update_meter_peak(&mut source.last_peak, &mut source.last_peak_at, 0.0);

                    let Some(capture) = source.capture.as_mut() else {
                        continue;
                    };

                    match capture.read_stereo(&mut source.buffer) {
                        Ok(read) => {
                            if source
                                .pressure
                                .record_diagnostics(capture.take_diagnostics(), Instant::now())
                            {
                                push_worker_warning(
                                    &thread_status,
                                    CAPTURE_PRESSURE_WARNING.to_string(),
                                );
                            }
                            source.last_read = read;
                            update_meter_peak(
                                &mut source.last_peak,
                                &mut source.last_peak_at,
                                mixer::peak(&source.buffer[..read]),
                            );
                            read_frames = read_frames.max(read);
                        }
                        Err(error) => {
                            let _ = source.pressure.record_error(Instant::now());
                            push_worker_warning(
                                &thread_status,
                                format!(
                                    "{} capture stopped: {error}",
                                    source_name_for_mic(&source.source, &[])
                                ),
                            );
                            source.capture = None;
                        }
                    }
                }

                for source in &mut app_sources {
                    source.buffer.fill([0.0, 0.0]);
                    source.last_read = 0;
                    update_meter_peak(&mut source.last_peak, &mut source.last_peak_at, 0.0);

                    if source.capture.is_none()
                        && source.last_retry.elapsed() >= Duration::from_millis(1000)
                    {
                        let (active_pid, capture) =
                            open_app_runtime_capture(&source.source, frames);
                        source.active_pid = active_pid;
                        source.capture = capture;
                        source.last_retry = Instant::now();
                    }

                    let Some(capture) = source.capture.as_mut() else {
                        continue;
                    };

                    match capture.read_stereo(&mut source.buffer) {
                        Ok(read) => {
                            let _ = capture.take_diagnostics();
                            source.last_read = read;
                            update_meter_peak(
                                &mut source.last_peak,
                                &mut source.last_peak_at,
                                mixer::peak(&source.buffer[..read]),
                            );
                            read_frames = read_frames.max(read);
                        }
                        Err(error) => {
                            let _ = error;
                            source.capture = None;
                            source.active_pid = None;
                            source.last_retry = Instant::now();
                        }
                    }
                }

                if read_frames == 0 {
                    update_meter_peak(&mut output_peak, &mut output_peak_at, 0.0);
                    set_running_status(&thread_status, &mic_sources, &app_sources, output_peak);
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }

                let controls = thread_controls.lock().clone();
                let mut source_inputs = Vec::with_capacity(mic_sources.len() + app_sources.len());
                for source in &mic_sources {
                    let control = mixer::source_control(&controls.mic_sources, &source.source.id);
                    source_inputs.push(SourceMix {
                        frames: &source.buffer[..read_frames],
                        gain: control.gain,
                        muted: control.muted,
                    });
                }
                for source in &app_sources {
                    let control = mixer::source_control(&controls.app_sources, &source.source.id);
                    source_inputs.push(SourceMix {
                        frames: &source.buffer[..read_frames],
                        gain: control.gain,
                        muted: control.muted,
                    });
                }

                let mixed =
                    mixer::mix_source_frames(&source_inputs, read_frames, controls.master_gain);
                update_meter_peak(&mut output_peak, &mut output_peak_at, mixer::peak(&mixed));
                set_running_status(&thread_status, &mic_sources, &app_sources, output_peak);

                let mut offset = 0;
                while offset < mixed.len() && !thread_stop.load(Ordering::Relaxed) {
                    match render.write_stereo(&mixed[offset..]) {
                        Ok(0) => {
                            if render_pressure.record_zero_write(Instant::now()) {
                                push_worker_warning(
                                    &thread_status,
                                    OUTPUT_PRESSURE_WARNING.to_string(),
                                );
                            }
                            thread::sleep(Duration::from_millis(1));
                        }
                        Ok(written) => {
                            render_pressure.record_progress();
                            offset += written;
                        }
                        Err(error) => {
                            set_worker_failure(
                                &thread_status,
                                &format!("Output render failed: {error}"),
                            );
                            return;
                        }
                    }
                }
            }
        });

        Some(Self {
            stop_flag,
            controls,
            status,
            handle: Some(handle),
        })
    }

    #[cfg(not(windows))]
    fn start(_config: AppConfig, _controls: MixerControls, _status: RouteStatus) -> Option<Self> {
        None
    }

    fn stop(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn set_worker_failure(status: &Arc<Mutex<RouteStatus>>, message: &str) {
    let mut status = status.lock();
    status.state = RouteState::CaptureFailed;
    status.message = message.to_string();
    status.meters = LevelMeters::default();
    if status.warnings.is_empty() {
        status
            .warnings
            .push("Adjust selections, then start again.".to_string());
    }
}

#[cfg(windows)]
fn set_running_status(
    status: &Arc<Mutex<RouteStatus>>,
    mic_sources: &[MicRuntime],
    app_sources: &[AppRuntime],
    output_peak: f32,
) {
    let mut mic_peaks = std::collections::BTreeMap::new();
    for source in mic_sources {
        mic_peaks.insert(source.source.id.clone(), source.last_peak);
    }

    let mut app_peaks = std::collections::BTreeMap::new();
    for source in app_sources {
        app_peaks.insert(source.source.id.clone(), source.last_peak);
    }

    let active_sources = mic_sources
        .iter()
        .filter(|source| source.capture.is_some())
        .count()
        + app_sources
            .iter()
            .filter(|source| source.capture.is_some())
            .count();
    let mut status = status.lock();
    status.state = RouteState::Running;
    status.message = if active_sources == 0 {
        "Waiting for configured sources".to_string()
    } else if active_sources == 1 {
        "Routing 1 source to selected output".to_string()
    } else {
        format!("Routing {active_sources} sources to selected output")
    };
    status.meters = LevelMeters {
        mic_peaks,
        app_peaks,
        output_peak,
    };
}

fn meters_for_config(config: &AppConfig) -> LevelMeters {
    LevelMeters {
        mic_peaks: config
            .mic_sources
            .iter()
            .map(|source| (source.id.clone(), 0.0))
            .collect(),
        app_peaks: config
            .app_sources
            .iter()
            .map(|source| (source.id.clone(), 0.0))
            .collect(),
        output_peak: 0.0,
    }
}

fn visible_meter_peak(current: f32, instant_peak: f32, elapsed: Duration) -> f32 {
    let current = current.clamp(0.0, 1.0);
    let instant_peak = instant_peak.clamp(0.0, 1.0);
    if instant_peak >= current {
        return instant_peak;
    }

    (current - METER_DECAY_PER_SECOND * elapsed.as_secs_f32())
        .max(instant_peak)
        .max(0.0)
}

#[cfg(windows)]
fn update_meter_peak(current: &mut f32, last_update: &mut Instant, instant_peak: f32) {
    let now = Instant::now();
    *current = visible_meter_peak(*current, instant_peak, now.duration_since(*last_update));
    *last_update = now;
}

fn push_worker_warning(status: &Arc<Mutex<RouteStatus>>, warning: String) {
    let mut status = status.lock();
    if !status.warnings.iter().any(|existing| existing == &warning) {
        status.warnings.push(warning);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_meter_peak_rises_immediately() {
        let peak = visible_meter_peak(0.12, 0.7, Duration::from_millis(50));

        assert_eq!(peak, 0.7);
    }

    #[test]
    fn visible_meter_peak_decays_instead_of_dropping_to_zero() {
        let peak = visible_meter_peak(0.8, 0.0, Duration::from_millis(100));

        assert!(peak > 0.0);
        assert!(peak < 0.8);
    }

    #[test]
    fn visible_meter_peak_clamps_to_valid_range() {
        assert_eq!(visible_meter_peak(0.0, 2.0, Duration::ZERO), 1.0);
        assert_eq!(visible_meter_peak(-1.0, -0.5, Duration::ZERO), 0.0);
    }
}

#[cfg(windows)]
fn open_app_runtime_capture(
    source: &AppSourceConfig,
    frames: usize,
) -> (Option<u32>, Option<Box<dyn super::capture::AudioCapture>>) {
    use super::capture::{ProcessLoopbackSpec, open_process_loopback_capture};

    let Some(process_id) = selected_app_process_id(&source.executable) else {
        return (None, None);
    };

    match open_process_loopback_capture(&ProcessLoopbackSpec::include_process_tree(
        process_id, frames,
    )) {
        Ok(capture) => (Some(process_id), Some(capture)),
        Err(_error) => (Some(process_id), None),
    }
}

#[cfg(windows)]
fn selected_app_process_id(executable: &str) -> Option<u32> {
    sessions::list_sessions()
        .ok()?
        .into_iter()
        .find(|session| {
            same_executable(&session.executable, executable)
                && session.state == SessionState::Active
        })
        .map(|session| session.process_id)
}

fn controls_from_config(config: &AppConfig) -> MixerControls {
    MixerControls {
        mic_sources: config
            .mic_sources
            .iter()
            .map(|source| SourceControl {
                id: source.id.clone(),
                gain: source.gain,
                muted: source.muted,
            })
            .collect(),
        app_sources: config
            .app_sources
            .iter()
            .map(|source| SourceControl {
                id: source.id.clone(),
                gain: source.gain,
                muted: source.muted,
            })
            .collect(),
        master_gain: config.master_gain,
        downmix_to_mono: config.downmix_to_mono,
    }
}

fn source_name_for_mic(source: &MicSourceConfig, devices: &[AudioDevice]) -> String {
    devices
        .iter()
        .find(|device| device.id == source.device_id)
        .map(|device| device.name.clone())
        .unwrap_or_else(|| source.device_id.clone())
}

fn same_executable(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}
