use super::{
    AudioError, AudioResult,
    capture::AudioCapture,
    discovery::Discovery,
    mixer::{MixerControls, SourceControl},
    render::{AudioRender, RenderSpec},
    runtime::{AudioBackend, CaptureTarget, RouteWorker, SourceKind, SourceSpec},
    types::{
        AudioDevice, AudioSession, LevelMeters, RouteState, RouteStatus, SessionState,
        is_canonical_vb_cable_input_name, is_sixteen_channel_cable_name,
    },
};
use crate::config::{AppConfig, ControlUpdate};
use std::sync::Arc;

pub struct AudioEngine {
    discovery: Arc<Discovery>,
    status: RouteStatus,
    worker: Option<RouteWorker>,
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new(Arc::new(Discovery::new()))
    }
}

impl AudioEngine {
    pub fn new(discovery: Arc<Discovery>) -> Self {
        Self {
            discovery,
            status: RouteStatus::default(),
            worker: None,
        }
    }

    pub fn start(&mut self, config: &AppConfig) -> AudioResult<RouteStatus> {
        let snapshot = self.discovery.initial_snapshot();
        let render_devices = snapshot.render.map_err(AudioError::Backend)?;
        let output = resolve_output_device_id(&render_devices, &config.output_device_id);
        self.stop();
        let Some(output) = output else {
            self.status.state = RouteState::DeviceMissing;
            self.status.message = "Selected output is unavailable".into();
            self.status
                .warnings
                .push("Pick an active render endpoint, ideally VB-CABLE input.".into());
            return Ok(self.status.clone());
        };
        if config.mic_sources.is_empty() && config.app_sources.is_empty() {
            self.status.state = RouteState::DeviceMissing;
            self.status.message = "No input sources configured".into();
            self.status
                .warnings
                .push("Add at least one microphone or application source.".into());
            return Ok(self.status.clone());
        }
        let mut config = config.clone();
        config.output_device_id = Some(output);
        self.status = RouteStatus {
            state: RouteState::Running,
            message: "Routing to selected output".into(),
            meters: meters_for_config(&config),
            warnings: Vec::new(),
        };
        let backend = Arc::new(NativeBackend {
            discovery: Arc::clone(&self.discovery),
        });
        self.worker = Some(RouteWorker::start(
            config.clone(),
            controls_from_config(&config),
            self.status.clone(),
            backend,
        ));
        Ok(self.current_status())
    }

    pub fn stop(&mut self) -> RouteStatus {
        if let Some(worker) = self.worker.take() {
            worker.stop();
        }
        self.status = RouteStatus::default();
        self.status.clone()
    }

    pub fn update_controls(&mut self, controls: &ControlUpdate) -> RouteStatus {
        if let Some(worker) = &self.worker {
            *worker.controls.lock() = Arc::new(MixerControls {
                mic_sources: controls
                    .mic_sources
                    .iter()
                    .map(|c| SourceControl {
                        id: c.id.clone(),
                        gain: c.gain,
                        muted: c.muted,
                        tone: c.tone.normalized(),
                    })
                    .collect(),
                app_sources: controls
                    .app_sources
                    .iter()
                    .map(|c| SourceControl {
                        id: c.id.clone(),
                        gain: c.gain,
                        muted: c.muted,
                        tone: c.tone.normalized(),
                    })
                    .collect(),
                master_gain: controls.master_gain,
                downmix_to_mono: controls.downmix_to_mono,
            });
        }
        self.current_status()
    }

    pub fn current_status(&mut self) -> RouteStatus {
        if let Some(worker) = &self.worker {
            self.status = worker.status.lock().clone();
        }
        self.status.clone()
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

struct NativeBackend {
    discovery: Arc<Discovery>,
}
impl AudioBackend for NativeBackend {
    fn resolve(
        &self,
        source: &SourceSpec,
        current: Option<&CaptureTarget>,
    ) -> AudioResult<Option<CaptureTarget>> {
        let snapshot = self.discovery.snapshot();
        match &source.kind {
            SourceKind::Microphone(id) => Ok(snapshot
                .capture
                .map_err(AudioError::Backend)?
                .iter()
                .any(|device| &device.id == id)
                .then(|| CaptureTarget::Microphone(id.clone()))),
            SourceKind::Application(executable) => {
                #[cfg(windows)]
                let alive = super::windows_wasapi::process_alive;
                #[cfg(not(windows))]
                let alive = |_: u32| false;
                Ok(application_target(
                    executable,
                    current,
                    &snapshot.sessions.map_err(AudioError::Backend)?,
                    alive,
                ))
            }
        }
    }

    fn capture(
        &self,
        target: &CaptureTarget,
        frames: usize,
        stop: &std::sync::atomic::AtomicBool,
    ) -> AudioResult<Box<dyn AudioCapture>> {
        use super::capture::{self, CaptureSpec, ProcessLoopbackSpec};
        match target {
            CaptureTarget::Microphone(id) => {
                capture::open_microphone_capture(&CaptureSpec::mic(id, frames))
            }
            CaptureTarget::Process(pid) => capture::open_process_loopback_capture(
                &ProcessLoopbackSpec::include_process_tree(*pid, frames),
                stop,
            ),
        }
    }
    fn render(&self, spec: &RenderSpec) -> AudioResult<Box<dyn AudioRender>> {
        super::render::open_render_output(spec)
    }
}

fn application_target(
    executable: &str,
    current: Option<&CaptureTarget>,
    sessions: &[AudioSession],
    alive: impl Fn(u32) -> bool,
) -> Option<CaptureTarget> {
    if let Some(CaptureTarget::Process(pid)) = current {
        if alive(*pid) {
            return Some(CaptureTarget::Process(*pid));
        }
    }
    sessions
        .iter()
        .find(|session| {
            session.executable.eq_ignore_ascii_case(executable)
                && session.state == SessionState::Active
        })
        .map(|session| CaptureTarget::Process(session.process_id))
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

fn controls_from_config(config: &AppConfig) -> MixerControls {
    MixerControls {
        mic_sources: config
            .mic_sources
            .iter()
            .map(|source| SourceControl {
                id: source.id.clone(),
                gain: source.gain,
                muted: source.muted,
                tone: source.tone.normalized(),
            })
            .collect(),
        app_sources: config
            .app_sources
            .iter()
            .map(|source| SourceControl {
                id: source.id.clone(),
                gain: source.gain,
                muted: source.muted,
                tone: source.tone.normalized(),
            })
            .collect(),
        master_gain: config.master_gain,
        downmix_to_mono: config.downmix_to_mono,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::runtime::visible_meter_peak;
    use std::time::Duration;

    #[test]
    fn a_quiet_live_application_keeps_its_capture_target() {
        let current = CaptureTarget::Process(10);
        assert_eq!(
            application_target("player.exe", Some(&current), &[], |pid| pid == 10),
            Some(current)
        );
    }

    #[test]
    fn an_exited_application_switches_to_its_new_active_process() {
        let session = AudioSession {
            id: "replacement".into(),
            display_name: "Player".into(),
            executable: "PLAYER.exe".into(),
            process_id: 20,
            state: SessionState::Active,
            is_excluded_default: false,
            window_title: None,
            has_audio_session: true,
            discovery_source: super::super::types::AppDiscoverySource::AudioSession,
        };
        let current = CaptureTarget::Process(10);
        assert_eq!(
            application_target("player.exe", Some(&current), &[session], |_| false),
            Some(CaptureTarget::Process(20))
        );
        assert_eq!(
            application_target("player.exe", Some(&current), &[], |_| false),
            None
        );
    }

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
