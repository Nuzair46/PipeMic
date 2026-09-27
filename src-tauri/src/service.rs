use crate::{
    audio::{AudioResult, engine::AudioEngine, types::RouteStatus},
    config::{self, AppConfig, AppSettings, ControlUpdate},
    persistence::ConfigWriter,
};

pub trait RoutingEngine: Send {
    fn start(&mut self, config: &AppConfig) -> AudioResult<RouteStatus>;
    fn stop(&mut self) -> RouteStatus;
    fn update_controls(&mut self, controls: &ControlUpdate) -> RouteStatus;
    fn status(&mut self) -> RouteStatus;
}

impl RoutingEngine for AudioEngine {
    fn start(&mut self, config: &AppConfig) -> AudioResult<RouteStatus> {
        self.start(config)
    }
    fn stop(&mut self) -> RouteStatus {
        self.stop()
    }
    fn update_controls(&mut self, controls: &ControlUpdate) -> RouteStatus {
        self.update_controls(controls)
    }
    fn status(&mut self) -> RouteStatus {
        self.current_status()
    }
}

// Owned behind one mutex by the command adapter. All mutations are performed
// off the UI thread; persistence has a separate, coalescing writer.
pub struct RoutingService<E: RoutingEngine> {
    config: Result<AppConfig, String>,
    engine: E,
    writer: ConfigWriter,
    load_warning: Option<String>,
}

impl<E: RoutingEngine> RoutingService<E> {
    pub fn new(engine: E) -> Self {
        let path = config::config_file_path();
        let loaded = config::load_config_with_recovery(&path);
        let (config, load_warning) = match loaded {
            Ok((config, warning)) => (Ok(config), warning),
            Err(error) => (Err(error.to_string()), None),
        };
        Self {
            config,
            load_warning,
            engine,
            writer: ConfigWriter::new(path),
        }
    }

    pub fn config(&self) -> Result<AppConfig, String> {
        self.config.clone()
    }

    pub fn minimize_to_tray(&self) -> bool {
        self.config
            .as_ref()
            .is_ok_and(|config| config.minimize_to_tray)
    }

    pub fn save(&mut self, config: AppConfig) -> Result<AppConfig, String> {
        self.config.as_ref().map_err(Clone::clone)?;
        self.writer.schedule(&config);
        self.config = Ok(config.clone());
        Ok(config)
    }

    pub fn settings(&mut self, settings: AppSettings) -> Result<AppConfig, String> {
        let mut config = self.config()?;
        config.apply_settings(settings);
        self.save(config)
    }

    pub fn start(&mut self, config: AppConfig) -> Result<RouteStatus, String> {
        self.config.as_ref().map_err(Clone::clone)?;
        self.engine
            .start(&config)
            .map_err(|error| error.to_string())?;
        self.save(config)?;
        Ok(self.status())
    }

    pub fn stop(&mut self) -> RouteStatus {
        self.engine.stop();
        self.status()
    }

    pub fn controls(&mut self, controls: ControlUpdate) -> Result<RouteStatus, String> {
        let mut config = self.config()?;
        config.apply_controls(&controls);
        self.engine.update_controls(&controls);
        self.save(config)?;
        Ok(self.status())
    }

    pub fn status(&mut self) -> RouteStatus {
        let mut status = self.engine.status();
        if let Some(warning) = &self.load_warning {
            status.warnings.push(warning.clone());
        }
        if let Some(error) = self.writer.error() {
            status
                .warnings
                .push(format!("Could not save settings: {error}"));
        }
        status
    }

    pub fn shutdown(&mut self) -> Result<(), String> {
        self.engine.stop();
        self.writer.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct FakeEngine {
        muted: bool,
    }
    impl RoutingEngine for FakeEngine {
        fn start(&mut self, _: &AppConfig) -> AudioResult<RouteStatus> {
            Ok(self.status())
        }
        fn stop(&mut self) -> RouteStatus {
            self.status()
        }
        fn update_controls(&mut self, controls: &ControlUpdate) -> RouteStatus {
            self.muted = controls.mic_sources[0].muted;
            self.status()
        }
        fn status(&mut self) -> RouteStatus {
            RouteStatus::default()
        }
    }

    #[test]
    fn disk_failure_does_not_block_mute_and_settings_preserve_live_controls() {
        let mut config = AppConfig::default();
        config
            .mic_sources
            .push(config::MicSourceConfig::new("mic".into(), 0.7, false));
        let mut service = RoutingService {
            config: Ok(config.clone()),
            engine: FakeEngine::default(),
            writer: ConfigWriter::with_sink(|_| Err("disk full".into())),
            load_warning: None,
        };
        let mut controls = ControlUpdate::from(&config);
        controls.mic_sources[0].muted = true;
        service.controls(controls).unwrap();
        assert!(service.writer.flush().is_err());
        assert!(service.engine.muted);
        let saved = service
            .settings(AppSettings {
                shortcuts: config.shortcuts,
                start_with_windows: false,
                minimize_to_tray: true,
            })
            .unwrap();
        assert!(saved.mic_sources[0].muted);
        assert_eq!(saved.mic_sources[0].gain, 0.7);
        assert!(service.status().warnings[0].contains("disk full"));
    }
}
