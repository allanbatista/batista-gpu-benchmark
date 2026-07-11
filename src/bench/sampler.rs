//! Per-frame sampling during the Running state. Preallocated, no per-frame
//! allocation or disk IO (spec §8/§17).

use bevy::prelude::*;

use crate::scene::orbits::BenchClock;

#[derive(Debug, Clone, Copy)]
pub struct FrameSample {
    /// Benchmark-clock time at capture (seconds since run start).
    pub elapsed: f64,
    pub ft_ms: f64,
    /// GPU frame time when TIMESTAMP_QUERY-based timing is active (advanced milestone).
    pub gpu_ms: Option<f64>,
}

/// Samples per second of headroom: even a 10 000 fps run fits.
pub const CAPACITY_PER_SECOND: usize = 10_000;

#[derive(Resource, Default)]
pub struct Sampler {
    pub samples: Vec<FrameSample>,
    pub active: bool,
    pub truncated: bool,
    skip_first: bool,
}

impl Sampler {
    /// Starts a capture with a fixed capacity; never reallocates afterwards.
    pub fn begin(&mut self, duration_s: f64) {
        let capacity = ((duration_s.ceil() as usize) + 1) * CAPACITY_PER_SECOND;
        self.samples.clear();
        if self.samples.capacity() < capacity {
            self.samples.reserve_exact(capacity - self.samples.capacity());
        }
        self.active = true;
        self.truncated = false;
        self.skip_first = true;
    }

    pub fn stop(&mut self) {
        self.active = false;
    }

    /// Moves the captured samples out (keeps the allocation for the next run).
    pub fn take(&mut self) -> Vec<FrameSample> {
        std::mem::take(&mut self.samples)
    }
}

/// Captures one sample per frame from the RAW real-time delta. Runs before the
/// benchmark runner so the frame that triggers the state transition is counted.
pub fn sample_frame(mut sampler: ResMut<Sampler>, time: Res<Time<Real>>, clock: Res<BenchClock>) {
    if !sampler.active {
        return;
    }
    let dt = time.delta_secs_f64();
    // First frame after entering Running carries warmup-boundary time; skip it.
    if sampler.skip_first {
        sampler.skip_first = false;
        return;
    }
    if dt <= 0.0 {
        return;
    }
    if sampler.samples.len() == sampler.samples.capacity() {
        // ponytail: hard stop instead of realloc — keeps the no-alloc guarantee;
        // the run is marked truncated in the report.
        sampler.active = false;
        sampler.truncated = true;
        return;
    }
    sampler.samples.push(FrameSample { elapsed: clock.t, ft_ms: dt * 1000.0, gpu_ms: None });
}
