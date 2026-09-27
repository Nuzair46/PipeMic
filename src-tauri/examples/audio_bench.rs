//! `cargo run --release --example audio_bench --no-default-features --locked`
//! CPU/allocation probe only; this does not measure WASAPI or driver latency.
#![allow(dead_code)]

#[path = "../src/audio/buffer.rs"]
mod buffer;
#[path = "../src/audio/mixer.rs"]
mod mixer;
#[path = "../src/audio/resample.rs"]
mod resample;

use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

struct CountAllocations;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
#[global_allocator]
static ALLOCATOR: CountAllocations = CountAllocations;

unsafe impl GlobalAlloc for CountAllocations {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(pointer, layout, size) }
    }
}

fn measure(name: &str, mut block: impl FnMut()) {
    const BLOCKS: usize = 10_000;
    for _ in 0..100 {
        block();
    }
    let mut times = Vec::with_capacity(BLOCKS);
    ALLOCATIONS.store(0, Ordering::Relaxed);
    for _ in 0..BLOCKS {
        let start = Instant::now();
        block();
        times.push(start.elapsed().as_nanos());
    }
    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    times.sort_unstable();
    println!(
        "{name}: {:.2} allocations/block; median {:.1} us; p99 {:.1} us (5000 us audio block)",
        allocations as f64 / BLOCKS as f64,
        times[BLOCKS / 2] as f64 / 1000.0,
        times[BLOCKS * 99 / 100] as f64 / 1000.0
    );
}

fn main() {
    const SOURCES: usize = 8;
    const QUANTUM: usize = 240;
    let input = vec![[0.05; 2]; QUANTUM];
    let controls: Vec<_> = (0..SOURCES)
        .map(|i| mixer::SourceControl {
            id: format!("source:{i}"),
            gain: 1.0,
            muted: false,
        })
        .collect();

    // Allocation pattern from the pre-refactor processing loop at 1356d47:
    // clone all controls, clone a matching control per source, allocate inputs/output.
    measure("previous mixer allocation pattern", || {
        let current = black_box(&controls).clone();
        let mut sources = Vec::with_capacity(SOURCES);
        for source in &controls {
            let control = black_box(current.iter().find(|c| c.id == source.id).unwrap().clone());
            sources.push(mixer::SourceMix {
                frames: &input,
                gain: control.gain,
                muted: control.muted,
            });
        }
        let mut mixed = vec![[0.0; 2]; QUANTUM];
        mixer::mix_into(sources, &mut mixed, 1.0);
        black_box(mixed);
    });

    let mut queues: Vec<_> = (0..SOURCES)
        .map(|_| {
            let mut queue = buffer::SourceBuffer::new(960);
            queue.push(&[[0.05; 2]; 960]);
            queue
        })
        .collect();
    let mut frames = vec![vec![[0.0; 2]; QUANTUM]; SOURCES];
    let mut mixed = vec![[0.0; 2]; QUANTUM];
    let mut converted = Vec::with_capacity(QUANTUM + 32);
    let mut converter = resample::Resampler::new(48_000, 48_000, QUANTUM);
    measure("current queues + mixer + 48 kHz output", || {
        for (queue, frames) in queues.iter_mut().zip(&mut frames) {
            queue.push(black_box(&input));
            queue.read(frames);
        }
        mixer::mix_into(
            frames
                .iter()
                .zip(&controls)
                .map(|(frames, control)| mixer::SourceMix {
                    frames,
                    gain: control.gain,
                    muted: control.muted,
                }),
            &mut mixed,
            1.0,
        );
        converter.process(&mixed, &mut converted);
        black_box(&converted);
    });

    let mut converter = resample::Resampler::new(48_000, 44_100, QUANTUM);
    measure("44.1 kHz output conversion alone", || {
        converter.process(black_box(&mixed), &mut converted);
        black_box(&converted);
    });
}
