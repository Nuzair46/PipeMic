use super::mixer::{SAMPLE_RATE, StereoFrame};
use serde::{Deserialize, Deserializer, Serialize};

const RAMP_FRAMES: usize = SAMPLE_RATE as usize / 50; // 20 ms

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToneConfig {
    #[serde(deserialize_with = "deserialize_axis")]
    pub x: f32,
    #[serde(deserialize_with = "deserialize_axis")]
    pub y: f32,
    pub bypassed: bool,
}

fn axis(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

fn deserialize_axis<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    f32::deserialize(deserializer).map(axis)
}

impl ToneConfig {
    pub fn normalized(self) -> Self {
        Self {
            x: axis(self.x),
            y: axis(self.y),
            ..self
        }
    }

    fn active(self) -> bool {
        !self.bypassed && (self.x != 0.0 || self.y != 0.0)
    }
}

// Normalized [b0, b1, b2, a1, a2]. Formulae: W3C Audio EQ Cookbook
// https://www.w3.org/TR/audio-eq-cookbook/ (RBJ peaking EQ and shelves).
type Coefficients = [f64; 5];

fn normalized(b: [f64; 3], a: [f64; 3]) -> Coefficients {
    [
        b[0] / a[0],
        b[1] / a[0],
        b[2] / a[0],
        a[1] / a[0],
        a[2] / a[0],
    ]
}

fn shelf(frequency: f64, gain_db: f64, high: bool) -> Coefficients {
    let a = 10.0_f64.powf(gain_db / 40.0);
    let omega = std::f64::consts::TAU * frequency / SAMPLE_RATE as f64;
    let c = omega.cos();
    // Shelf slope S = 1, so alpha = sin(omega) / sqrt(2).
    let beta = (2.0 * a).sqrt() * omega.sin();
    let p = a + 1.0;
    let m = a - 1.0;
    if high {
        normalized(
            [
                a * (p + m * c + beta),
                -2.0 * a * (m + p * c),
                a * (p + m * c - beta),
            ],
            [p - m * c + beta, 2.0 * (m - p * c), p - m * c - beta],
        )
    } else {
        normalized(
            [
                a * (p - m * c + beta),
                2.0 * a * (m - p * c),
                a * (p - m * c - beta),
            ],
            [p + m * c + beta, -2.0 * (m + p * c), p + m * c - beta],
        )
    }
}

fn coefficients(tone: ToneConfig) -> [Coefficients; 3] {
    let a = 10.0_f64.powf(tone.y as f64 * 6.0 / 40.0);
    let omega = std::f64::consts::TAU * 1500.0 / SAMPLE_RATE as f64;
    let alpha = omega.sin() / (2.0 * 0.8);
    let c = -2.0 * omega.cos();
    [
        shelf(200.0, -6.0 * tone.x as f64, false),
        shelf(4000.0, 6.0 * tone.x as f64, true),
        normalized(
            [1.0 + alpha * a, c, 1.0 - alpha * a],
            [1.0 + alpha / a, c, 1.0 - alpha / a],
        ),
    ]
}

#[derive(Clone, Copy, Default)]
struct History {
    x: [f64; 2],
    y: [f64; 2],
}

impl History {
    fn process(&mut self, input: f64, c: Coefficients) -> f64 {
        let output = c[0] * input + c[1] * self.x[0] + c[2] * self.x[1]
            - c[3] * self.y[0]
            - c[4] * self.y[1];
        self.x = [input, self.x[0]];
        self.y = [output, self.y[0]];
        output
    }

    fn clear_tiny_values(&mut self) {
        for sample in self.x.iter_mut().chain(self.y.iter_mut()) {
            if sample.abs() < 1.0e-30 {
                *sample = 0.0;
            }
        }
    }
}

/// One stereo source's EQ. All storage is fixed-size; processing allocates nothing.
pub struct ToneProcessor {
    tone: ToneConfig,
    current: [Coefficients; 3],
    target: [Coefficients; 3],
    step: [Coefficients; 3],
    history: [[History; 3]; 2],
    wet: f64,
    wet_target: f64,
    wet_step: f64,
    remaining: usize,
}

impl Default for ToneProcessor {
    fn default() -> Self {
        let tone = ToneConfig::default();
        let current = coefficients(tone);
        Self {
            tone,
            current,
            target: current,
            step: [[0.0; 5]; 3],
            history: [[History::default(); 3]; 2],
            wet: 0.0,
            wet_target: 0.0,
            wet_step: 0.0,
            remaining: 0,
        }
    }
}

impl ToneProcessor {
    pub fn set_tone(&mut self, tone: ToneConfig) {
        let tone = tone.normalized();
        if tone == self.tone {
            return;
        }
        self.tone = tone;
        self.target = coefficients(tone);
        self.wet_target = if tone.active() { 1.0 } else { 0.0 };
        for ((step, target), current) in self.step.iter_mut().zip(self.target).zip(self.current) {
            for i in 0..5 {
                step[i] = (target[i] - current[i]) / RAMP_FRAMES as f64;
            }
        }
        self.wet_step = (self.wet_target - self.wet) / RAMP_FRAMES as f64;
        self.remaining = RAMP_FRAMES;
    }

    /// Reconnects/discontinuities discard old samples without losing the setting.
    pub fn reset_history(&mut self) {
        self.history = [[History::default(); 3]; 2];
    }

    pub fn process(&mut self, frames: &mut [StereoFrame]) {
        if self.wet == 0.0 && self.wet_target == 0.0 {
            self.current = self.target;
            self.remaining = 0;
            return;
        }
        for frame in frames {
            if self.remaining > 0 {
                for (current, step) in self.current.iter_mut().zip(self.step) {
                    for i in 0..5 {
                        current[i] += step[i];
                    }
                }
                self.wet += self.wet_step;
                self.remaining -= 1;
                if self.remaining == 0 {
                    self.current = self.target;
                    self.wet = self.wet_target;
                    if self.wet == 0.0 {
                        self.reset_history();
                    }
                }
            }
            if self.wet == 0.0 {
                continue;
            }
            for (sample, history) in frame.iter_mut().zip(&mut self.history) {
                let dry = *sample as f64;
                let mut wet = dry;
                for (filter, c) in history.iter_mut().zip(self.current) {
                    wet = filter.process(wet, c);
                }
                *sample = (dry + self.wet * (wet - dry)) as f32;
            }
        }
        for history in self.history.iter_mut().flatten() {
            history.clear_tiny_values();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(x: f32, y: f32) -> ToneConfig {
        ToneConfig {
            x,
            y,
            bypassed: false,
        }
    }

    fn settle(processor: &mut ToneProcessor) {
        processor.process(&mut [[0.0; 2]; RAMP_FRAMES]);
    }

    fn response(tone: ToneConfig, frequency: f64) -> f64 {
        let mut processor = ToneProcessor::default();
        processor.set_tone(tone);
        settle(&mut processor);
        let mut input_energy = 0.0;
        let mut output_energy = 0.0;
        for n in 0..48_000 {
            let sample = (std::f64::consts::TAU * frequency * n as f64 / SAMPLE_RATE as f64).sin()
                as f32
                * 0.1;
            let mut frame = [[sample; 2]];
            processor.process(&mut frame);
            if n > 2400 {
                input_energy += (sample as f64).powi(2);
                output_energy += (frame[0][0] as f64).powi(2);
            }
        }
        10.0 * (output_energy / input_energy).log10()
    }

    #[test]
    fn neutral_and_settled_bypass_preserve_every_sample_bit() {
        let input = [[0.12345679, -0.0], [f32::MIN_POSITIVE, -0.97], [0.0, 0.83]];
        let mut processor = ToneProcessor::default();
        for next in [
            ToneConfig::default(),
            tone(1.0, -1.0),
            ToneConfig {
                bypassed: true,
                ..tone(1.0, -1.0)
            },
            ToneConfig::default(),
        ] {
            processor.set_tone(next);
            settle(&mut processor);
            if next.active() {
                continue;
            }
            let mut output = input;
            processor.process(&mut output);
            for (a, b) in input.iter().flatten().zip(output.iter().flatten()) {
                assert_eq!(a.to_bits(), b.to_bits());
            }
        }
    }

    #[test]
    fn shelves_tilt_in_opposite_directions_and_presence_peaks_at_1500_hz() {
        for sign in [-1.0, 1.0] {
            for (frequency, expected) in
                [(30.0, -6.0), (200.0, -3.0), (4000.0, 3.0), (18_000.0, 6.0)]
            {
                let db = response(tone(sign, 0.0), frequency);
                assert!(
                    (db - expected * sign as f64).abs() < 0.08,
                    "{frequency} Hz: {db} dB"
                );
            }
            let db = response(tone(0.0, sign), 1500.0);
            assert!((db - 6.0 * sign as f64).abs() < 0.01, "presence: {db} dB");
            assert!(response(tone(0.0, sign), 30.0).abs() < 0.02);
            assert!(response(tone(0.0, sign), 18_000.0).abs() < 0.02);
        }
    }

    #[test]
    fn stereo_channels_sources_and_resets_have_independent_histories() {
        let mut left = ToneProcessor::default();
        let mut right = ToneProcessor::default();
        left.set_tone(tone(0.8, 0.6));
        right.set_tone(tone(-0.5, -0.9));
        settle(&mut left);
        settle(&mut right);
        let mut impulse = [[0.0; 2]; 128];
        impulse[0][0] = 0.5;
        left.process(&mut impulse);
        assert!(impulse.iter().any(|frame| frame[0] != 0.0));
        assert!(impulse.iter().all(|frame| frame[1] == 0.0));
        let mut silence = [[0.0; 2]; 128];
        right.process(&mut silence);
        assert_eq!(silence, [[0.0; 2]; 128]);
        left.reset_history();
        left.process(&mut silence);
        assert_eq!(silence, [[0.0; 2]; 128]);
        // Swapping the input channels swaps output, with no shared delay state.
        right.set_tone(tone(0.8, 0.6));
        settle(&mut right);
        let mut swapped = [[0.0; 2]; 128];
        swapped[0][1] = 0.5;
        right.process(&mut swapped);
        for (a, b) in impulse.iter().zip(swapped) {
            assert_eq!(a[0], b[1]);
        }
    }

    #[test]
    fn transitions_take_20_ms_and_can_be_retargeted_mid_ramp() {
        let mut processor = ToneProcessor::default();
        processor.set_tone(tone(1.0, 1.0));
        processor.process(&mut [[0.1; 2]; RAMP_FRAMES / 2]);
        assert!((processor.wet - 0.5).abs() < 1e-12);
        let before = processor.current;
        processor.set_tone(tone(-1.0, -1.0));
        assert_eq!(processor.current, before);
        settle(&mut processor);
        assert_eq!(processor.current, coefficients(tone(-1.0, -1.0)));
        assert_eq!(processor.wet, 1.0);
        processor.set_tone(ToneConfig {
            bypassed: true,
            ..tone(-1.0, -1.0)
        });
        processor.process(&mut [[0.1; 2]; RAMP_FRAMES - 1]);
        assert!(processor.wet > 0.0);
        let mut last = [[0.123, -0.0]];
        processor.process(&mut last);
        assert_eq!(last[0][0].to_bits(), 0.123_f32.to_bits());
        assert_eq!(last[0][1].to_bits(), (-0.0_f32).to_bits());
        assert_eq!(processor.wet, 0.0);
    }

    #[test]
    fn rapid_extreme_changes_stay_finite_and_do_not_step_on_dc() {
        let mut processor = ToneProcessor::default();
        let mut previous = 0.1;
        let mut max_step = 0.0_f32;
        for i in 0..400 {
            let next = ToneConfig {
                bypassed: i % 7 == 0,
                ..tone(
                    if i % 2 == 0 { 1.0 } else { -1.0 },
                    if i % 3 == 0 { 1.0 } else { -1.0 },
                )
            };
            processor.set_tone(next);
            let mut frames = [[0.1, -0.1]; 240];
            processor.process(&mut frames);
            for frame in frames {
                assert!(frame.iter().all(|v| v.is_finite() && v.abs() < 0.5));
                max_step = max_step.max((frame[0] - previous).abs());
                previous = frame[0];
            }
        }
        assert!(max_step < 0.003, "largest transition step: {max_step}");
    }

    #[test]
    fn every_pad_corner_handles_full_scale_noise_and_silence() {
        let mut noise = 1_u32;
        for x in [-1.0, 0.0, 1.0] {
            for y in [-1.0, 0.0, 1.0] {
                let mut processor = ToneProcessor::default();
                processor.set_tone(tone(x, y));
                for _ in 0..100 {
                    let mut frames = [[0.0; 2]; 240];
                    for frame in &mut frames {
                        for sample in frame {
                            noise = noise.wrapping_mul(1664525).wrapping_add(1013904223);
                            *sample = noise as i32 as f32 / i32::MAX as f32;
                        }
                    }
                    processor.process(&mut frames);
                    assert!(
                        frames
                            .iter()
                            .flatten()
                            .all(|v| v.is_finite() && v.abs() < 8.0)
                    );
                }
                for _ in 0..100 {
                    let mut frames = [[0.0; 2]; 240];
                    processor.process(&mut frames);
                    assert!(frames.iter().flatten().all(|v| v.is_finite()));
                }
            }
        }
    }

    #[test]
    fn invalid_coordinates_are_bounded_before_filter_design() {
        assert_eq!(
            tone(f32::NAN, f32::INFINITY).normalized(),
            ToneConfig::default()
        );
        assert_eq!(tone(-9.0, 3.0).normalized(), tone(-1.0, 1.0));
    }
}
