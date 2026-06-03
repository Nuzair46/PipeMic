use std::{
    collections::HashSet,
    env, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::audio::mixer::DEFAULT_BUFFER_FRAMES;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MicSourceConfig {
    pub id: String,
    pub device_id: String,
    pub gain: f32,
    pub muted: bool,
}

impl MicSourceConfig {
    pub fn new(device_id: String, gain: f32, muted: bool) -> Self {
        Self {
            id: mic_source_id(&device_id),
            device_id,
            gain,
            muted,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSourceConfig {
    pub id: String,
    pub executable: String,
    pub display_name: Option<String>,
    pub gain: f32,
    pub muted: bool,
}

impl AppSourceConfig {
    pub fn new(executable: String, display_name: Option<String>, gain: f32, muted: bool) -> Self {
        let executable = normalize_executable(&executable).unwrap_or(executable);
        Self {
            id: app_source_id(&executable),
            executable,
            display_name,
            gain,
            muted,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub mic_sources: Vec<MicSourceConfig>,
    pub app_sources: Vec<AppSourceConfig>,
    pub output_device_id: Option<String>,
    pub master_gain: f32,
    pub buffer_frames: usize,
    pub downmix_to_mono: bool,
    pub shortcuts: ShortcutConfig,
    pub start_with_windows: bool,
    pub minimize_to_tray: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mic_sources: Vec::new(),
            app_sources: Vec::new(),
            output_device_id: None,
            master_gain: 1.0,
            buffer_frames: DEFAULT_BUFFER_FRAMES,
            downmix_to_mono: true,
            shortcuts: ShortcutConfig::default(),
            start_with_windows: true,
            minimize_to_tray: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutConfig {
    pub mic_mute: String,
    pub app_mute: String,
    pub routing: String,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            mic_mute: "Ctrl+Alt+M".to_string(),
            app_mute: "Ctrl+Alt+A".to_string(),
            routing: "Ctrl+Alt+S".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceControlUpdate {
    pub id: String,
    pub gain: f32,
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlUpdate {
    pub mic_sources: Vec<SourceControlUpdate>,
    pub app_sources: Vec<SourceControlUpdate>,
    pub master_gain: f32,
    pub downmix_to_mono: bool,
}

impl From<&AppConfig> for ControlUpdate {
    fn from(config: &AppConfig) -> Self {
        Self {
            mic_sources: config
                .mic_sources
                .iter()
                .map(|source| SourceControlUpdate {
                    id: source.id.clone(),
                    gain: source.gain,
                    muted: source.muted,
                })
                .collect(),
            app_sources: config
                .app_sources
                .iter()
                .map(|source| SourceControlUpdate {
                    id: source.id.clone(),
                    gain: source.gain,
                    muted: source.muted,
                })
                .collect(),
            master_gain: config.master_gain,
            downmix_to_mono: config.downmix_to_mono,
        }
    }
}

impl AppConfig {
    pub fn apply_controls(&mut self, controls: &ControlUpdate) {
        apply_source_controls(&mut self.mic_sources, &controls.mic_sources);
        apply_source_controls(&mut self.app_sources, &controls.app_sources);
        self.master_gain = controls.master_gain;
        self.downmix_to_mono = controls.downmix_to_mono;
    }
}

pub fn mic_source_id(device_id: &str) -> String {
    format!("mic:{device_id}")
}

pub fn app_source_id(executable: &str) -> String {
    format!("app:{}", executable.to_ascii_lowercase())
}

pub fn normalize_executable(executable: &str) -> Option<String> {
    let name = executable.trim();
    if name.is_empty() || !name.to_ascii_lowercase().ends_with(".exe") {
        return None;
    }

    Some(name.to_string())
}

trait SourceConfig {
    fn id(&self) -> &str;
    fn set_gain_muted(&mut self, gain: f32, muted: bool);
}

impl SourceConfig for MicSourceConfig {
    fn id(&self) -> &str {
        &self.id
    }

    fn set_gain_muted(&mut self, gain: f32, muted: bool) {
        self.gain = gain;
        self.muted = muted;
    }
}

impl SourceConfig for AppSourceConfig {
    fn id(&self) -> &str {
        &self.id
    }

    fn set_gain_muted(&mut self, gain: f32, muted: bool) {
        self.gain = gain;
        self.muted = muted;
    }
}

fn apply_source_controls<T: SourceConfig>(sources: &mut [T], controls: &[SourceControlUpdate]) {
    for source in sources {
        if let Some(control) = controls.iter().find(|control| control.id == source.id()) {
            source.set_gain_muted(control.gain, control.muted);
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAppConfig {
    #[serde(default)]
    mic_sources: Vec<MicSourceConfig>,
    #[serde(default)]
    app_sources: Vec<AppSourceConfig>,
    output_device_id: Option<String>,
    master_gain: Option<f32>,
    buffer_frames: Option<usize>,
    downmix_to_mono: Option<bool>,
    shortcuts: Option<ShortcutConfig>,
    start_with_windows: Option<bool>,
    minimize_to_tray: Option<bool>,
    #[allow(dead_code)]
    excluded_processes: Option<Vec<String>>,

    mic_device_id: Option<String>,
    app_session_id: Option<String>,
    app_process_name: Option<String>,
    mic_gain: Option<f32>,
    app_gain: Option<f32>,
    mic_muted: Option<bool>,
    app_muted: Option<bool>,
}

impl From<RawAppConfig> for AppConfig {
    fn from(raw: RawAppConfig) -> Self {
        let mut config = AppConfig {
            output_device_id: raw.output_device_id,
            master_gain: raw.master_gain.unwrap_or(1.0),
            buffer_frames: raw.buffer_frames.unwrap_or(DEFAULT_BUFFER_FRAMES),
            downmix_to_mono: raw.downmix_to_mono.unwrap_or(true),
            shortcuts: sanitize_shortcuts(raw.shortcuts.unwrap_or_default()),
            start_with_windows: raw.start_with_windows.unwrap_or(true),
            minimize_to_tray: raw.minimize_to_tray.unwrap_or(true),
            ..AppConfig::default()
        };

        config.mic_sources = sanitize_mic_sources(raw.mic_sources);
        if config.mic_sources.is_empty() {
            if let Some(device_id) = raw.mic_device_id {
                config.mic_sources.push(MicSourceConfig::new(
                    device_id,
                    raw.mic_gain.unwrap_or(1.0),
                    raw.mic_muted.unwrap_or(false),
                ));
            }
        }

        config.app_sources = sanitize_app_sources(raw.app_sources);
        if config.app_sources.is_empty() {
            let executable = raw
                .app_process_name
                .and_then(|name| normalize_executable(&name))
                .or_else(|| {
                    raw.app_session_id
                        .as_deref()
                        .and_then(executable_from_legacy_session_id)
                });

            if let Some(executable) = executable {
                config.app_sources.push(AppSourceConfig::new(
                    executable,
                    None,
                    raw.app_gain.unwrap_or(1.0),
                    raw.app_muted.unwrap_or(false),
                ));
            }
        }

        config
    }
}

fn sanitize_shortcuts(shortcuts: ShortcutConfig) -> ShortcutConfig {
    let defaults = ShortcutConfig::default();
    ShortcutConfig {
        mic_mute: sanitize_shortcut(shortcuts.mic_mute, &defaults.mic_mute),
        app_mute: sanitize_shortcut(shortcuts.app_mute, &defaults.app_mute),
        routing: sanitize_shortcut(shortcuts.routing, &defaults.routing),
    }
}

fn sanitize_shortcut(value: String, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

fn sanitize_mic_sources(sources: Vec<MicSourceConfig>) -> Vec<MicSourceConfig> {
    let mut seen = HashSet::new();
    sources
        .into_iter()
        .filter_map(|source| {
            let device_id = source.device_id.trim().to_string();
            if device_id.is_empty() || !seen.insert(device_id.clone()) {
                return None;
            }

            Some(MicSourceConfig {
                id: if source.id.trim().is_empty() {
                    mic_source_id(&device_id)
                } else {
                    source.id
                },
                device_id,
                gain: source.gain,
                muted: source.muted,
            })
        })
        .collect()
}

fn sanitize_app_sources(sources: Vec<AppSourceConfig>) -> Vec<AppSourceConfig> {
    let mut seen = HashSet::new();
    sources
        .into_iter()
        .filter_map(|source| {
            let executable = normalize_executable(&source.executable)?;
            let key = executable.to_ascii_lowercase();
            if !seen.insert(key) {
                return None;
            }

            Some(AppSourceConfig {
                id: if source.id.trim().is_empty() {
                    app_source_id(&executable)
                } else {
                    source.id
                },
                executable,
                display_name: source.display_name.filter(|name| !name.trim().is_empty()),
                gain: source.gain,
                muted: source.muted,
            })
        })
        .collect()
}

fn executable_from_legacy_session_id(session_id: &str) -> Option<String> {
    let mut parts = session_id.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("session"), Some(executable), Some(_pid), None) => normalize_executable(executable),
        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config: {0}")]
    Read(#[from] io::Error),
    #[error("failed to parse config: {0}")]
    Parse(#[from] serde_json::Error),
}

pub fn config_file_path() -> PathBuf {
    app_config_dir().join("config.json")
}

fn app_config_dir() -> PathBuf {
    if let Some(appdata) = env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("PipeMic");
    }

    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config_home).join("PipeMic");
    }

    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join(".config")
        .join("PipeMic")
}

pub fn load_config() -> Result<AppConfig, ConfigError> {
    load_config_from_path(&config_file_path())
}

pub fn save_config(config: &AppConfig) -> Result<(), ConfigError> {
    save_config_to_path(config, &config_file_path())
}

pub fn load_config_from_path(path: &Path) -> Result<AppConfig, ConfigError> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }

    let raw = fs::read_to_string(path)?;
    let raw_config: RawAppConfig = serde_json::from_str(&raw)?;
    Ok(raw_config.into())
}

pub fn save_config_to_path(config: &AppConfig, path: &Path) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let raw = serde_json::to_string_pretty(config)?;
    fs::write(path, raw)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config_path(name: &str) -> PathBuf {
        let id = format!(
            "pipemic-{name}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        env::temp_dir().join(id)
    }

    #[test]
    fn missing_config_loads_defaults() {
        let path = temp_config_path("missing");
        let config = load_config_from_path(&path).unwrap();

        assert!(config.mic_sources.is_empty());
        assert!(config.app_sources.is_empty());
        assert_eq!(config.buffer_frames, DEFAULT_BUFFER_FRAMES);
        assert!(config.downmix_to_mono);
        assert_eq!(config.shortcuts, ShortcutConfig::default());
        assert!(config.start_with_windows);
        assert!(config.minimize_to_tray);
    }

    #[test]
    fn config_round_trips_as_camel_case_json() {
        let path = temp_config_path("roundtrip");
        let config = AppConfig {
            mic_sources: vec![MicSourceConfig::new("mic-a".to_string(), 0.8, true)],
            app_sources: vec![AppSourceConfig::new(
                "Game.exe".to_string(),
                Some("Game".to_string()),
                1.2,
                false,
            )],
            output_device_id: Some("cable".to_string()),
            master_gain: 0.7,
            shortcuts: ShortcutConfig {
                mic_mute: "Ctrl+Shift+M".to_string(),
                app_mute: "Ctrl+Shift+A".to_string(),
                routing: "Ctrl+Shift+S".to_string(),
            },
            start_with_windows: false,
            minimize_to_tray: false,
            ..AppConfig::default()
        };

        save_config_to_path(&config, &path).unwrap();
        let raw = fs::read_to_string(&path).unwrap();

        assert!(raw.contains("micSources"));
        assert!(raw.contains("appSources"));
        assert!(raw.contains("downmixToMono"));
        assert!(raw.contains("startWithWindows"));
        assert!(raw.contains("minimizeToTray"));
        assert_eq!(load_config_from_path(&path).unwrap(), config);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn legacy_single_source_config_migrates_to_source_arrays() {
        let path = temp_config_path("legacy");
        fs::write(
            &path,
            r#"{
  "micDeviceId": "mic-a",
  "appSessionId": "session:spotify.exe:4242",
  "appProcessName": "Spotify.exe",
  "outputDeviceId": "cable",
  "micGain": 0.8,
  "appGain": 1.2,
  "masterGain": 0.7,
  "micMuted": true,
  "appMuted": false,
  "bufferFrames": 960,
  "downmixToMono": true,
  "excludedProcesses": ["Discord.exe", "VRChat.exe"]
}"#,
        )
        .unwrap();

        let config = load_config_from_path(&path).unwrap();

        assert_eq!(
            config.mic_sources,
            vec![MicSourceConfig::new("mic-a".to_string(), 0.8, true)]
        );
        assert_eq!(
            config.app_sources,
            vec![AppSourceConfig::new(
                "Spotify.exe".to_string(),
                None,
                1.2,
                false
            )]
        );
        assert_eq!(config.output_device_id, Some("cable".to_string()));
        assert_eq!(config.master_gain, 0.7);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn app_source_sanitization_deduplicates_by_executable() {
        let sources = sanitize_app_sources(vec![
            AppSourceConfig::new(
                "Spotify.exe".to_string(),
                Some("Spotify".to_string()),
                1.0,
                false,
            ),
            AppSourceConfig::new(
                "spotify.exe".to_string(),
                Some("Spotify 2".to_string()),
                0.5,
                true,
            ),
            AppSourceConfig::new("System Audio".to_string(), None, 1.0, false),
        ]);

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].executable, "Spotify.exe");
    }
}
