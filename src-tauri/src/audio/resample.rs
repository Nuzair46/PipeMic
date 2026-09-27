use super::mixer::StereoFrame;
use std::{collections::VecDeque, f64::consts::PI};

const TAPS: usize = 32;
const PHASES: usize = 512;

/// Streaming, band-limited conversion. State is retained across packet boundaries.
/// Equal rates bypass the filter so normal 48 kHz routing remains sample-exact.
pub struct Resampler {
    input: VecDeque<StereoFrame>,
    position: f64,
    step: f64,
    kernel: Vec<[f32; TAPS]>,
}

impl Resampler {
    pub fn new(input_rate: u32, output_rate: u32, max_packet: usize) -> Self {
        assert!(input_rate > 0 && output_rate > 0);
        let step = input_rate as f64 / output_rate as f64;
        let mut kernel = Vec::new();
        if input_rate != output_rate {
            let cutoff = (output_rate as f64 / input_rate as f64).min(1.0) * 0.94;
            for phase in 0..PHASES {
                let mut weights = [0.0; TAPS];
                for (tap, weight) in weights.iter_mut().enumerate() {
                    let x = tap as f64 - (TAPS / 2 - 1) as f64 - phase as f64 / PHASES as f64;
                    let sinc = if x.abs() < 1e-9 {
                        cutoff
                    } else {
                        (PI * x * cutoff).sin() / (PI * x)
                    };
                    *weight = (sinc * (0.5 + 0.5 * (PI * x / (TAPS / 2) as f64).cos())) as f32;
                }
                let sum: f32 = weights.iter().sum();
                for weight in &mut weights {
                    *weight /= sum;
                }
                kernel.push(weights);
            }
        }
        let mut input = VecDeque::with_capacity(max_packet + TAPS * 2);
        input.extend(std::iter::repeat_n([0.0; 2], TAPS / 2 - 1));
        Self {
            input,
            position: (TAPS / 2 - 1) as f64,
            step,
            kernel,
        }
    }

    pub fn process(&mut self, frames: &[StereoFrame], output: &mut Vec<StereoFrame>) {
        output.clear();
        if self.kernel.is_empty() {
            output.extend_from_slice(frames);
            return;
        }
        self.input.extend(frames.iter().copied());
        while self.position.floor() as usize + TAPS / 2 < self.input.len() {
            let center = self.position.floor() as usize;
            let phase = ((self.position.fract() * PHASES as f64) as usize).min(PHASES - 1);
            let first = center - (TAPS / 2 - 1);
            let mut frame = [0.0; 2];
            for tap in 0..TAPS {
                let sample = self.input[first + tap];
                frame[0] += sample[0] * self.kernel[phase][tap];
                frame[1] += sample[1] * self.kernel[phase][tap];
            }
            output.push(frame);
            self.position += self.step;
        }
        let consumed = (self.position.floor() as usize)
            .saturating_sub(TAPS / 2 - 1)
            .min(self.input.len());
        self.input.drain(..consumed);
        self.position -= consumed as f64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fallback_rates_preserve_duration_across_unequal_packets() {
        for (input_rate, output_rate) in [(44_100, 48_000), (48_000, 44_100), (96_000, 48_000)] {
            let frames: Vec<_> = (0..input_rate)
                .map(|i| {
                    let sample = (2.0 * PI * 440.0 * i as f64 / input_rate as f64).sin() as f32;
                    [sample, -sample]
                })
                .collect();
            let mut converter = Resampler::new(input_rate, output_rate, 997);
            let mut converted = Vec::new();
            let mut all = Vec::new();
            for chunk in frames.chunks(997) {
                converter.process(chunk, &mut converted);
                all.extend_from_slice(&converted);
            }
            assert!((all.len() as i64 - output_rate as i64).abs() <= 32);
            for (i, frame) in all.iter().enumerate().skip(100).take(all.len() - 200) {
                let expected = (2.0 * PI * 440.0 * i as f64 / output_rate as f64).sin() as f32;
                assert!((frame[0] - expected).abs() < 0.015);
                assert!((frame[0] + frame[1]).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn downsampling_filters_frequencies_above_the_new_nyquist_limit() {
        let input: Vec<_> = (0..9600)
            .map(|i| [(2.0 * PI * 36_000.0 * i as f64 / 96_000.0).sin() as f32; 2])
            .collect();
        let mut out = Vec::new();
        Resampler::new(96_000, 48_000, input.len()).process(&input, &mut out);
        let rms = (out[100..]
            .iter()
            .map(|frame| frame[0] * frame[0])
            .sum::<f32>()
            / (out.len() - 100) as f32)
            .sqrt();
        assert!(rms < 0.01, "alias RMS: {rms}");
    }

    #[test]
    fn equal_rates_preserve_samples_exactly() {
        let frames = [[0.2, -0.6], [0.9, 0.1]];
        let mut out = Vec::new();
        Resampler::new(48_000, 48_000, 2).process(&frames, &mut out);
        assert_eq!(out, frames);
    }
}
