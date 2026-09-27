use super::{
    AudioResult,
    buffer::SourceBuffer,
    capture::AudioCapture,
    mixer::{self, MixerControls, SAMPLE_RATE, SourceMix, StereoFrame},
    render::{AudioRender, RenderSpec},
    resample::Resampler,
    tone::{ToneConfig, ToneProcessor},
    types::{RouteState, RouteStatus},
};
use crate::config::AppConfig;
use parking_lot::Mutex;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureTarget {
    Microphone(String),
    Process(u32),
}

#[derive(Debug, Clone)]
pub enum SourceKind {
    Microphone(String),
    Application(String),
}

#[derive(Debug, Clone)]
pub struct SourceSpec {
    pub id: String,
    pub name: String,
    pub kind: SourceKind,
}

pub trait AudioBackend: Send + Sync + 'static {
    fn resolve(
        &self,
        source: &SourceSpec,
        current: Option<&CaptureTarget>,
    ) -> AudioResult<Option<CaptureTarget>>;
    // Clients are created, used and dropped on the calling worker thread.
    fn capture(
        &self,
        target: &CaptureTarget,
        frames: usize,
        stop: &AtomicBool,
    ) -> AudioResult<Box<dyn AudioCapture>>;
    fn render(&self, spec: &RenderSpec) -> AudioResult<Box<dyn AudioRender>>;
}

struct SourceState {
    buffer: SourceBuffer,
    reset_tone: bool,
    active: bool,
    warning: Option<String>,
}
struct SourcePort {
    spec: SourceSpec,
    shared: Arc<Mutex<SourceState>>,
    frames: Vec<StereoFrame>,
    peak: f32,
    tone: ToneProcessor,
}

impl SourceState {
    fn clear_audio(&mut self) {
        self.buffer.clear();
        self.reset_tone = true;
    }
}

impl SourcePort {
    fn read_processed(&mut self, tone: ToneConfig, elapsed: Duration) {
        let mut state = self.shared.lock();
        if std::mem::take(&mut state.reset_tone) {
            self.tone.reset_history();
        }
        state.buffer.read(&mut self.frames);
        drop(state);
        self.tone.set_tone(tone);
        self.tone.process(&mut self.frames);
        self.peak = visible_meter_peak(self.peak, mixer::peak(&self.frames), elapsed);
    }
}

pub struct RouteWorker {
    stop: Arc<AtomicBool>,
    pub controls: Arc<Mutex<Arc<MixerControls>>>,
    pub status: Arc<Mutex<RouteStatus>>,
    handle: Option<JoinHandle<()>>,
}

impl RouteWorker {
    pub fn start(
        config: AppConfig,
        controls: MixerControls,
        status: RouteStatus,
        backend: Arc<dyn AudioBackend>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let controls = Arc::new(Mutex::new(Arc::new(controls)));
        let status = Arc::new(Mutex::new(status));
        let thread_stop = Arc::clone(&stop);
        let thread_controls = Arc::clone(&controls);
        let thread_status = Arc::clone(&status);
        let handle = thread::spawn(move || {
            if let Err(error) = render_loop(
                config,
                backend,
                &thread_stop,
                &thread_controls,
                &thread_status,
            ) {
                let mut status = thread_status.lock();
                status.state = RouteState::CaptureFailed;
                status.message = format!("Output render failed: {error}");
                status.meters = Default::default();
            }
            thread_stop.store(true, Ordering::Release);
        });
        Self {
            stop,
            controls,
            status,
            handle: Some(handle),
        }
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        // An OS driver may still be inside activation. It owns its resources and
        // observes cancellation before processing; stopping never joins that call.
        if let Some(handle) = self.handle.take() {
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for RouteWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

fn spawn_source(
    spec: SourceSpec,
    frames: usize,
    quantum: usize,
    backend: Arc<dyn AudioBackend>,
    stop: Arc<AtomicBool>,
) -> SourcePort {
    let shared = Arc::new(Mutex::new(SourceState {
        buffer: SourceBuffer::new(frames),
        reset_tone: true,
        active: false,
        warning: None,
    }));
    let state = Arc::clone(&shared);
    let source = spec.clone();
    thread::spawn(move || capture_loop(source, frames, backend, stop, state));
    SourcePort {
        spec,
        shared,
        frames: vec![[0.0; 2]; quantum],
        peak: 0.0,
        tone: ToneProcessor::default(),
    }
}

fn capture_loop(
    spec: SourceSpec,
    frames: usize,
    backend: Arc<dyn AudioBackend>,
    stop: Arc<AtomicBool>,
    state: Arc<Mutex<SourceState>>,
) {
    let mut target = None;
    let mut capture: Option<Box<dyn AudioCapture>> = None;
    let mut converter = Resampler::new(SAMPLE_RATE, SAMPLE_RATE, frames);
    let mut raw = vec![[0.0; 2]; frames];
    let mut converted = Vec::with_capacity(frames * 4);
    let mut next_resolve = Instant::now();
    let mut next_retry = Instant::now();
    let mut backoff = Duration::from_millis(250);

    while !stop.load(Ordering::Acquire) {
        let now = Instant::now();
        if now >= next_resolve {
            match backend.resolve(&spec, target.as_ref()) {
                Ok(next) if next != target => {
                    capture = None;
                    target = next;
                    next_retry = now;
                    let mut state = state.lock();
                    state.active = false;
                    state.clear_audio();
                    state.warning = None;
                }
                Err(error) => state.lock().warning = Some(format!("{}: {error}", spec.name)),
                _ => {}
            }
            next_resolve = now + Duration::from_secs(1);
        }
        if capture.is_none() && now >= next_retry {
            if let Some(target) = &target {
                match backend.capture(target, frames, &stop) {
                    Ok(client) => {
                        converter = Resampler::new(client.sample_rate(), SAMPLE_RATE, frames);
                        converted.reserve(
                            (frames as f64 * SAMPLE_RATE as f64 / client.sample_rate() as f64)
                                .ceil() as usize
                                + 32,
                        );
                        capture = Some(client);
                        backoff = Duration::from_millis(250);
                        let mut state = state.lock();
                        state.clear_audio();
                        state.active = true;
                        state.warning = None;
                    }
                    Err(error) => {
                        state.lock().warning =
                            Some(format!("{} capture failed: {error}", spec.name));
                        next_retry = Instant::now() + backoff;
                        backoff = (backoff * 2).min(Duration::from_secs(4));
                    }
                }
            }
        }
        if stop.load(Ordering::Acquire) {
            break;
        }
        let Some(client) = capture.as_mut() else {
            thread::sleep(Duration::from_millis(10));
            continue;
        };
        match client.read_stereo(&mut raw) {
            Ok(read) => {
                let diagnostics = client.take_diagnostics();
                if diagnostics.discontinuities > 0 || diagnostics.pending_overflows > 0 {
                    converter = Resampler::new(client.sample_rate(), SAMPLE_RATE, frames);
                    state.lock().clear_audio();
                }
                converter.process(&raw[..read], &mut converted);
                let mut state = state.lock();
                state.buffer.push(&converted);
                if state.buffer.overflows > 0 {
                    state.reset_tone = true;
                }
                if diagnostics.pending_overflows > 0
                    || diagnostics.errors > 0
                    || state.buffer.overflows > 0
                {
                    state.warning = Some(format!(
                        "{} capture is falling behind; increase buffer size or close audio-heavy apps.",
                        spec.name
                    ));
                    state.buffer.overflows = 0;
                }
                drop(state);
                if read == 0 {
                    thread::sleep(Duration::from_millis(2));
                }
            }
            Err(error) => {
                capture = None;
                next_retry = Instant::now() + backoff;
                let mut state = state.lock();
                state.active = false;
                state.clear_audio();
                state.warning = Some(format!("{} capture stopped: {error}", spec.name));
            }
        }
    }
}

fn render_loop(
    config: AppConfig,
    backend: Arc<dyn AudioBackend>,
    stop: &Arc<AtomicBool>,
    controls: &Arc<Mutex<Arc<MixerControls>>>,
    status: &Arc<Mutex<RouteStatus>>,
) -> AudioResult<()> {
    let frames = config
        .buffer_frames
        .clamp(mixer::DEFAULT_BUFFER_FRAMES, SAMPLE_RATE as usize);
    let quantum = (frames / 4).clamp(48, 480);
    let mut render = backend.render(&RenderSpec::cable(
        config.output_device_id.clone().unwrap_or_default(),
        frames,
        config.downmix_to_mono,
    ))?;
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    let mut sources = Vec::new();
    for source in &config.mic_sources {
        let spec = SourceSpec {
            id: source.id.clone(),
            name: source.device_id.clone(),
            kind: SourceKind::Microphone(source.device_id.clone()),
        };
        sources.push(spawn_source(
            spec,
            frames,
            quantum,
            Arc::clone(&backend),
            Arc::clone(stop),
        ));
    }
    for source in &config.app_sources {
        let spec = SourceSpec {
            id: source.id.clone(),
            name: source
                .display_name
                .clone()
                .unwrap_or_else(|| source.executable.clone()),
            kind: SourceKind::Application(source.executable.clone()),
        };
        sources.push(spawn_source(
            spec,
            frames,
            quantum,
            Arc::clone(&backend),
            Arc::clone(stop),
        ));
    }
    let mut mixed = vec![[0.0; 2]; quantum];
    let mut converted = Vec::with_capacity(
        (quantum as f64 * render.sample_rate() as f64 / SAMPLE_RATE as f64).ceil() as usize + 32,
    );
    let mut converter = Resampler::new(SAMPLE_RATE, render.sample_rate(), quantum);
    let mut output_peak = 0.0_f32;
    let mut last_meter = Instant::now();
    let mut last_publish = Instant::now() - Duration::from_millis(80);
    let mut stalled_at = None;
    while !stop.load(Ordering::Acquire) {
        let now = Instant::now();
        let elapsed = now.duration_since(last_meter);
        last_meter = now;
        let current_controls = Arc::clone(&controls.lock());
        for source in &mut sources {
            let list = match source.spec.kind {
                SourceKind::Microphone(_) => &current_controls.mic_sources,
                SourceKind::Application(_) => &current_controls.app_sources,
            };
            let tone = list
                .iter()
                .find(|control| control.id == source.spec.id)
                .map(|control| control.tone)
                .unwrap_or_default();
            source.read_processed(tone, elapsed);
        }
        let inputs = sources.iter().map(|source| {
            let list = match source.spec.kind {
                SourceKind::Microphone(_) => &current_controls.mic_sources,
                SourceKind::Application(_) => &current_controls.app_sources,
            };
            let (gain, muted) = list
                .iter()
                .find(|control| control.id == source.spec.id)
                .map(|control| (control.gain, control.muted))
                .unwrap_or((1.0, false));
            SourceMix {
                frames: &source.frames,
                gain,
                muted,
            }
        });
        mixer::mix_into(inputs, &mut mixed, current_controls.master_gain);
        render.set_downmix(current_controls.downmix_to_mono);
        output_peak = visible_meter_peak(output_peak, mixer::peak(&mixed), elapsed);
        converter.process(&mixed, &mut converted);
        let mut offset = 0;
        while offset < converted.len() && !stop.load(Ordering::Acquire) {
            match render.write_stereo(&converted[offset..])? {
                0 => {
                    stalled_at.get_or_insert_with(Instant::now);
                    thread::sleep(Duration::from_millis(1));
                }
                written => {
                    offset += written;
                    stalled_at = None;
                }
            }
            if last_publish.elapsed() >= Duration::from_millis(80) {
                publish_status(status, &sources, output_peak, stalled_at);
                last_publish = Instant::now();
            }
        }
    }
    Ok(())
}

fn publish_status(
    status: &Mutex<RouteStatus>,
    sources: &[SourcePort],
    output_peak: f32,
    stalled_at: Option<Instant>,
) {
    let mut status = status.lock();
    status.warnings.clear();
    let mut active = 0;
    for source in sources {
        let state = source.shared.lock();
        active += usize::from(state.active);
        if let Some(warning) = &state.warning {
            status.warnings.push(warning.clone());
        }
        let meters = match source.spec.kind {
            SourceKind::Microphone(_) => &mut status.meters.mic_peaks,
            SourceKind::Application(_) => &mut status.meters.app_peaks,
        };
        if let Some(peak) = meters.get_mut(&source.spec.id) {
            *peak = source.peak;
        }
    }
    if stalled_at.is_some_and(|time| time.elapsed() >= Duration::from_millis(500)) {
        status.warnings.push("Output device is not accepting audio quickly enough; close apps using the virtual mic or increase buffer size.".into());
    }
    status.state = RouteState::Running;
    status.message = match active {
        0 => "Waiting for configured sources".into(),
        1 => "Routing 1 source to selected output".into(),
        n => format!("Routing {n} sources to selected output"),
    };
    status.meters.output_peak = output_peak;
}

pub fn visible_meter_peak(current: f32, instant_peak: f32, elapsed: Duration) -> f32 {
    let current = current.clamp(0.0, 1.0);
    let instant_peak = instant_peak.clamp(0.0, 1.0);
    (current - 2.8 * elapsed.as_secs_f32())
        .max(instant_peak)
        .max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppSourceConfig, MicSourceConfig};
    use std::sync::atomic::AtomicUsize;

    struct Capture;
    impl AudioCapture for Capture {
        fn read_stereo(&mut self, output: &mut [StereoFrame]) -> AudioResult<usize> {
            thread::sleep(Duration::from_millis(5));
            output[..240].fill([0.25; 2]);
            Ok(240)
        }
    }
    struct Render(Arc<AtomicBool>);
    impl AudioRender for Render {
        fn write_stereo(&mut self, frames: &[StereoFrame]) -> AudioResult<usize> {
            if frames.iter().any(|frame| frame[0] > 0.1) {
                self.0.store(true, Ordering::Release);
            }
            thread::sleep(Duration::from_millis(5));
            Ok(frames.len())
        }
    }
    struct Backend {
        heard: Arc<AtomicBool>,
        activating: AtomicBool,
        cancelled: AtomicBool,
        attempts: AtomicUsize,
        fail_mic_once: bool,
    }
    impl AudioBackend for Backend {
        fn resolve(
            &self,
            source: &SourceSpec,
            _: Option<&CaptureTarget>,
        ) -> AudioResult<Option<CaptureTarget>> {
            Ok(Some(match source.kind {
                SourceKind::Microphone(_) => CaptureTarget::Microphone("mic".into()),
                SourceKind::Application(_) => CaptureTarget::Process(1),
            }))
        }
        fn capture(
            &self,
            target: &CaptureTarget,
            _: usize,
            stop: &AtomicBool,
        ) -> AudioResult<Box<dyn AudioCapture>> {
            if matches!(target, CaptureTarget::Process(_)) {
                self.activating.store(true, Ordering::Release);
                while !stop.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(2));
                }
                self.cancelled.store(true, Ordering::Release);
                return Err(super::super::AudioError::CaptureFailed("cancelled".into()));
            }
            let attempt = self.attempts.fetch_add(1, Ordering::AcqRel);
            if self.fail_mic_once && attempt == 0 {
                return Err(super::super::AudioError::CaptureFailed(
                    "temporarily unplugged".into(),
                ));
            }
            Ok(Box::new(Capture))
        }
        fn render(&self, _: &RenderSpec) -> AudioResult<Box<dyn AudioRender>> {
            Ok(Box::new(Render(Arc::clone(&self.heard))))
        }
    }

    fn wait_until(condition: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !condition() {
            assert!(Instant::now() < deadline, "worker failed to make progress");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn a_blocked_app_activation_does_not_stall_microphones_or_stop() {
        let backend = Arc::new(Backend {
            heard: Arc::new(AtomicBool::new(false)),
            activating: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            attempts: AtomicUsize::new(0),
            fail_mic_once: false,
        });
        let config = AppConfig {
            mic_sources: vec![MicSourceConfig::new("mic".into(), 1.0, false)],
            app_sources: vec![AppSourceConfig::new("slow.exe".into(), None, 1.0, false)],
            output_device_id: Some("out".into()),
            ..AppConfig::default()
        };
        let worker = RouteWorker::start(
            config,
            MixerControls::default(),
            RouteStatus::default(),
            backend.clone(),
        );
        wait_until(|| {
            backend.activating.load(Ordering::Acquire) && backend.heard.load(Ordering::Acquire)
        });
        let start = Instant::now();
        worker.stop();
        assert!(start.elapsed() < Duration::from_millis(100));
        wait_until(|| backend.cancelled.load(Ordering::Acquire));
    }

    #[test]
    fn failed_microphones_reconnect_without_restarting_the_mix() {
        let backend = Arc::new(Backend {
            heard: Arc::new(AtomicBool::new(false)),
            activating: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            attempts: AtomicUsize::new(0),
            fail_mic_once: true,
        });
        let config = AppConfig {
            mic_sources: vec![MicSourceConfig::new("mic".into(), 1.0, false)],
            output_device_id: Some("out".into()),
            ..AppConfig::default()
        };
        let worker = RouteWorker::start(
            config,
            MixerControls::default(),
            RouteStatus::default(),
            backend.clone(),
        );
        wait_until(|| backend.heard.load(Ordering::Acquire));
        assert!(backend.attempts.load(Ordering::Acquire) >= 2);
        worker.stop();
    }

    #[test]
    fn live_tone_updates_do_not_reopen_capture_and_meters_precede_gain_and_mute() {
        let backend = Arc::new(Backend {
            heard: Arc::new(AtomicBool::new(false)),
            activating: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            attempts: AtomicUsize::new(0),
            fail_mic_once: false,
        });
        let config = AppConfig {
            mic_sources: vec![MicSourceConfig::new("mic".into(), 1.0, false)],
            output_device_id: Some("out".into()),
            ..AppConfig::default()
        };
        let mut status = RouteStatus::default();
        status.meters.mic_peaks.insert("mic:mic".into(), 0.0);
        let worker = RouteWorker::start(config, MixerControls::default(), status, backend.clone());
        wait_until(|| worker.status.lock().meters.mic_peaks["mic:mic"] >= 0.24);
        *worker.controls.lock() = Arc::new(MixerControls {
            mic_sources: vec![mixer::SourceControl {
                id: "mic:mic".into(),
                gain: 0.0,
                muted: true,
                tone: ToneConfig {
                    x: -1.0,
                    y: 0.0,
                    bypassed: false,
                },
            }],
            ..MixerControls::default()
        });
        wait_until(|| {
            let status = worker.status.lock();
            status.meters.mic_peaks["mic:mic"] > 0.45 && status.meters.output_peak == 0.0
        });
        assert_eq!(backend.attempts.load(Ordering::Acquire), 1);
        worker.stop();
    }

    #[test]
    fn cleared_capture_audio_discards_eq_history_before_the_next_block() {
        let shared = Arc::new(Mutex::new(SourceState {
            buffer: SourceBuffer::new(960),
            reset_tone: true,
            active: true,
            warning: None,
        }));
        let mut port = SourcePort {
            spec: SourceSpec {
                id: "mic".into(),
                name: "Mic".into(),
                kind: SourceKind::Microphone("mic".into()),
            },
            shared: shared.clone(),
            frames: vec![[0.0; 2]; 240],
            peak: 0.0,
            tone: ToneProcessor::default(),
        };
        let tone = ToneConfig {
            x: -1.0,
            y: 1.0,
            bypassed: false,
        };
        shared.lock().buffer.push(&[[0.25; 2]; 960]);
        for _ in 0..4 {
            port.read_processed(tone, Duration::from_millis(5));
        }
        shared.lock().clear_audio();
        shared.lock().buffer.push(&[[0.0; 2]; 960]);
        port.read_processed(tone, Duration::from_millis(5));
        assert!(port.frames.iter().flatten().all(|sample| *sample == 0.0));
    }

    struct ChangingBackend {
        target: Mutex<Option<CaptureTarget>>,
        opened: Mutex<Vec<CaptureTarget>>,
        resolved: AtomicUsize,
    }
    impl AudioBackend for ChangingBackend {
        fn resolve(
            &self,
            _: &SourceSpec,
            _: Option<&CaptureTarget>,
        ) -> AudioResult<Option<CaptureTarget>> {
            let target = self.target.lock().clone();
            self.resolved.fetch_add(1, Ordering::AcqRel);
            Ok(target)
        }
        fn capture(
            &self,
            target: &CaptureTarget,
            _: usize,
            _: &AtomicBool,
        ) -> AudioResult<Box<dyn AudioCapture>> {
            self.opened.lock().push(target.clone());
            Ok(Box::new(Capture))
        }
        fn render(&self, _: &RenderSpec) -> AudioResult<Box<dyn AudioRender>> {
            Ok(Box::new(Render(Arc::new(AtomicBool::new(false)))))
        }
    }

    #[test]
    fn discovery_reconnects_an_unplugged_mic_and_a_replaced_process() {
        for (kind, initial, replacement) in [
            (
                SourceKind::Microphone("mic".into()),
                None,
                CaptureTarget::Microphone("mic".into()),
            ),
            (
                SourceKind::Application("app.exe".into()),
                Some(CaptureTarget::Process(1)),
                CaptureTarget::Process(2),
            ),
        ] {
            let backend = Arc::new(ChangingBackend {
                target: Mutex::new(initial.clone()),
                opened: Mutex::new(Vec::new()),
                resolved: AtomicUsize::new(0),
            });
            let stop = Arc::new(AtomicBool::new(false));
            let _port = spawn_source(
                SourceSpec {
                    id: "source".into(),
                    name: "Source".into(),
                    kind,
                },
                960,
                240,
                backend.clone(),
                stop.clone(),
            );
            if initial.is_some() {
                wait_until(|| backend.opened.lock().len() == 1);
            } else {
                wait_until(|| backend.resolved.load(Ordering::Acquire) > 0);
            }
            *backend.target.lock() = Some(replacement.clone());
            wait_until(|| backend.opened.lock().last() == Some(&replacement));
            // A healthy capture does not reopen on every reconciliation.
            assert_eq!(
                backend.opened.lock().len(),
                if initial.is_some() { 2 } else { 1 }
            );
            stop.store(true, Ordering::Release);
        }
    }

    struct StalledRender(Arc<AtomicBool>);
    impl AudioRender for StalledRender {
        fn write_stereo(&mut self, _: &[StereoFrame]) -> AudioResult<usize> {
            Ok(0)
        }
    }
    impl Drop for StalledRender {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    struct StalledBackend(Arc<AtomicBool>);
    impl AudioBackend for StalledBackend {
        fn resolve(
            &self,
            _: &SourceSpec,
            _: Option<&CaptureTarget>,
        ) -> AudioResult<Option<CaptureTarget>> {
            Ok(Some(CaptureTarget::Microphone("mic".into())))
        }
        fn capture(
            &self,
            _: &CaptureTarget,
            _: usize,
            _: &AtomicBool,
        ) -> AudioResult<Box<dyn AudioCapture>> {
            Ok(Box::new(Capture))
        }
        fn render(&self, _: &RenderSpec) -> AudioResult<Box<dyn AudioRender>> {
            Ok(Box::new(StalledRender(self.0.clone())))
        }
    }

    #[test]
    fn prolonged_render_backpressure_reports_status_and_releases_output_on_stop() {
        let released = Arc::new(AtomicBool::new(false));
        let worker = RouteWorker::start(
            AppConfig {
                mic_sources: vec![MicSourceConfig::new("mic".into(), 1.0, false)],
                output_device_id: Some("out".into()),
                ..AppConfig::default()
            },
            MixerControls::default(),
            RouteStatus::default(),
            Arc::new(StalledBackend(released.clone())),
        );
        wait_until(|| {
            worker
                .status
                .lock()
                .warnings
                .iter()
                .any(|warning| warning.starts_with("Output device is not accepting"))
        });
        let start = Instant::now();
        worker.stop();
        assert!(start.elapsed() < Duration::from_millis(100));
        wait_until(|| released.load(Ordering::Acquire));
    }
}
