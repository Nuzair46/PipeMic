use serde::{Deserialize, Serialize};

pub const SAMPLE_RATE: u32 = 48_000;
pub const DEFAULT_BUFFER_FRAMES: usize = 960;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceControl {
    pub id: String,
    pub gain: f32,
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MixerControls {
    pub mic_sources: Vec<SourceControl>,
    pub app_sources: Vec<SourceControl>,
    pub master_gain: f32,
    pub downmix_to_mono: bool,
}

impl Default for MixerControls {
    fn default() -> Self {
        Self {
            mic_sources: Vec::new(),
            app_sources: Vec::new(),
            master_gain: 1.0,
            downmix_to_mono: true,
        }
    }
}

pub type StereoFrame = [f32; 2];

#[derive(Debug, Clone, Copy)]
pub struct SourceMix<'a> {
    pub frames: &'a [StereoFrame],
    pub gain: f32,
    pub muted: bool,
}

pub fn source_control<'a>(controls: &'a [SourceControl], id: &str) -> SourceControl {
    controls
        .iter()
        .find(|control| control.id == id)
        .cloned()
        .unwrap_or_else(|| SourceControl {
            id: id.to_string(),
            gain: 1.0,
            muted: false,
        })
}

pub fn mix_source_frames(
    sources: &[SourceMix<'_>],
    frame_count: usize,
    master_gain: f32,
) -> Vec<StereoFrame> {
    let master_gain = master_gain.max(0.0);
    let mut mixed = vec![[0.0, 0.0]; frame_count];

    for source in sources {
        if source.muted {
            continue;
        }

        let gain = source.gain.max(0.0);
        for (index, output) in mixed.iter_mut().enumerate() {
            let frame = source.frames.get(index).copied().unwrap_or([0.0, 0.0]);
            output[0] += frame[0] * gain;
            output[1] += frame[1] * gain;
        }
    }

    for frame in &mut mixed {
        frame[0] = (frame[0] * master_gain).clamp(-1.0, 1.0);
        frame[1] = (frame[1] * master_gain).clamp(-1.0, 1.0);
    }

    mixed
}

pub fn peak(frames: &[StereoFrame]) -> f32 {
    frames
        .iter()
        .flat_map(|frame| frame.iter())
        .fold(0.0_f32, |acc, sample| acc.max(sample.abs()))
        .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_gain_and_mute_controls_for_multiple_sources() {
        let mic = [[0.5, -0.5], [0.25, 0.25]];
        let app = [[0.5, 0.5], [0.25, -0.25]];
        let muted = [[1.0, 1.0], [1.0, 1.0]];

        let mixed = mix_source_frames(
            &[
                SourceMix {
                    frames: &mic,
                    gain: 1.0,
                    muted: false,
                },
                SourceMix {
                    frames: &app,
                    gain: 0.5,
                    muted: false,
                },
                SourceMix {
                    frames: &muted,
                    gain: 1.0,
                    muted: true,
                },
            ],
            2,
            1.0,
        );

        assert_eq!(mixed, [[0.75, -0.25], [0.375, 0.125]]);
    }

    #[test]
    fn clips_mixed_samples() {
        let mixed = mix_source_frames(
            &[
                SourceMix {
                    frames: &[[0.9, -0.9]],
                    gain: 1.0,
                    muted: false,
                },
                SourceMix {
                    frames: &[[0.9, -0.9]],
                    gain: 1.0,
                    muted: false,
                },
            ],
            1,
            1.0,
        );

        assert_eq!(mixed, [[1.0, -1.0]]);
    }

    #[test]
    fn applies_master_gain_after_sum() {
        let mixed = mix_source_frames(
            &[
                SourceMix {
                    frames: &[[0.5, 0.5]],
                    gain: 1.0,
                    muted: false,
                },
                SourceMix {
                    frames: &[[0.5, 0.5]],
                    gain: 1.0,
                    muted: false,
                },
            ],
            1,
            0.5,
        );

        assert_eq!(mixed, [[0.5, 0.5]]);
    }
}
