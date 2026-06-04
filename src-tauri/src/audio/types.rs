use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeviceFlow {
    Capture,
    Render,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SessionState {
    Active,
    Inactive,
    Expired,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RouteState {
    Stopped,
    Running,
    DeviceMissing,
    AppInactive,
    CaptureFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub flow: DeviceFlow,
    pub is_default: bool,
    pub is_virtual_cable_like: bool,
    pub channels: Option<u16>,
    pub sample_rate: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSession {
    pub id: String,
    pub display_name: String,
    pub executable: String,
    pub process_id: u32,
    pub state: SessionState,
    pub is_excluded_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LevelMeters {
    pub mic_peaks: BTreeMap<String, f32>,
    pub app_peaks: BTreeMap<String, f32>,
    pub output_peak: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteStatus {
    pub state: RouteState,
    pub message: String,
    pub meters: LevelMeters,
    pub warnings: Vec<String>,
}

impl Default for RouteStatus {
    fn default() -> Self {
        Self {
            state: RouteState::Stopped,
            message: "Routing stopped".to_string(),
            meters: LevelMeters::default(),
            warnings: Vec::new(),
        }
    }
}

pub fn is_virtual_cable_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    normalized.contains("vb-audio")
        || normalized.contains("vb cable")
        || normalized.contains("vb-cable")
        || normalized.contains("cable input")
        || normalized.contains("virtual cable")
        || normalized.contains("voicemeeter")
}

pub fn is_canonical_vb_cable_input_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    is_virtual_cable_name(name)
        && normalized.contains("cable input")
        && !is_sixteen_channel_cable_name(name)
}

pub fn is_sixteen_channel_cable_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    normalized.contains("cable") && normalized.contains("16ch")
}
