use super::mixer::{SAMPLE_RATE, StereoFrame};
use std::collections::VecDeque;

/// The render endpoint is the clock. Packet arrival never determines the size
/// of a mix block. A small reservoir absorbs packet jitter; fractional reads
/// track slow clock drift without periodically dropping an entire packet.
pub struct SourceBuffer {
    frames: VecDeque<StereoFrame>,
    target: usize,
    capacity: usize,
    primed: bool,
    position: f64,
    step: f64,
    pub overflows: usize,
}

impl SourceBuffer {
    pub fn new(target: usize) -> Self {
        let target = target.max(2);
        Self {
            frames: VecDeque::with_capacity(target * 4),
            target,
            capacity: target * 4,
            primed: false,
            position: 0.0,
            step: 1.0,
            overflows: 0,
        }
    }

    pub fn clear(&mut self) {
        self.frames.clear();
        self.primed = false;
        self.position = 0.0;
        self.step = 1.0;
    }

    pub fn push(&mut self, frames: &[StereoFrame]) {
        for frame in frames {
            if self.frames.len() == self.capacity {
                self.frames.pop_front();
                self.overflows = self.overflows.saturating_add(1);
            }
            self.frames.push_back(*frame);
        }
    }

    pub fn read(&mut self, output: &mut [StereoFrame]) {
        output.fill([0.0; 2]);
        if !self.primed {
            if self.frames.len() < self.target {
                return;
            }
            self.primed = true;
        }
        let error = self.frames.len() as f64 - self.target as f64;
        let desired_step = (1.0 + error / (SAMPLE_RATE as f64 * 10.0)).clamp(0.998, 1.002);
        self.step += (desired_step - self.step) * 0.01;
        for out in output {
            let Some(left) = self.frames.front().copied() else {
                self.primed = false;
                self.position = 0.0;
                break;
            };
            let right = self.frames.get(1).copied().unwrap_or(left);
            let fraction = self.position as f32;
            *out = [
                left[0] + (right[0] - left[0]) * fraction,
                left[1] + (right[1] - left[1]) * fraction,
            ];
            self.position += self.step;
            while self.position >= 1.0 {
                self.frames.pop_front();
                self.position -= 1.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staggered_short_packets_fill_one_common_output_block() {
        let mut a = SourceBuffer::new(8);
        let mut b = SourceBuffer::new(8);
        a.push(&[[0.25, 0.25]; 8]);
        b.push(&[[0.5, 0.5]; 3]);
        b.push(&[[0.5, 0.5]; 5]);
        let mut left = [[0.0; 2]; 4];
        let mut right = left;
        a.read(&mut left);
        b.read(&mut right);
        assert_eq!(left, [[0.25, 0.25]; 4]);
        assert_eq!(right, [[0.5, 0.5]; 4]);
        b.push(&[[0.5, 0.5]; 2]);
        b.read(&mut right);
        assert_eq!(right, [[0.5, 0.5]; 4]);
    }

    #[test]
    fn backpressure_is_bounded_and_recovery_discards_old_audio() {
        let mut queue = SourceBuffer::new(8);
        queue.push(&[[0.2; 2]; 100]);
        assert_eq!(queue.frames.len(), 32);
        assert_eq!(queue.overflows, 68);
        queue.clear();
        queue.push(&[[0.8; 2]; 8]);
        let mut out = [[0.0; 2]; 4];
        queue.read(&mut out);
        assert_eq!(out, [[0.8; 2]; 4]);
    }

    #[test]
    fn a_faster_capture_clock_does_not_accumulate_unbounded_latency() {
        let mut queue = SourceBuffer::new(960);
        queue.push(&[[0.1; 2]; 960]);
        let mut out = [[0.0; 2]; 480];
        for tick in 0..60_000 {
            queue.read(&mut out);
            queue.push(&[[0.1; 2]; 480]);
            if tick % 10 == 0 {
                queue.push(&[[0.1; 2]]);
            }
        }
        assert_eq!(queue.overflows, 0);
        assert!(queue.frames.len() < 1920);
        assert!(queue.frames.len() > 480);
    }
}
