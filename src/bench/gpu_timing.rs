//! GPU pass timing via RenderDiagnosticsPlugin + TIMESTAMP_QUERY (spec §9.2).
//!
//! Bevy's default `WgpuSettingsPriority::Functionality` already requests every
//! feature the adapter supports, so TIMESTAMP_QUERY is active whenever the GPU
//! offers it — no explicit feature request needed. When unavailable (e.g. GL),
//! everything here stays inert and reports "unavailable"; values are NEVER
//! estimated.
//!
//! Caveat (documented in reports): GPU timestamps read back 1–3 frames late,
//! so per-frame `gpu_time_ms` rows are best-effort; per-run averages are the
//! authoritative numbers.

use bevy::diagnostic::DiagnosticsStore;
use bevy::prelude::*;
use serde::Serialize;

use super::BenchState;
use super::sampler::Sampler;

const CATEGORIES: usize = 4;
const MAIN: usize = 0;
const SHADOW: usize = 1;
const POST: usize = 2;
const OTHER: usize = 3;

fn category(path: &str) -> usize {
    if path.contains("shadow") {
        SHADOW
    } else if path.contains("prepass")
        || path.contains("main_opaque")
        || path.contains("main_transparent")
        || path.contains("oit")
    {
        MAIN
    } else if path.contains("tonemapping")
        || path.contains("bloom")
        || path.contains("upscaling")
        || path.contains("fxaa")
        || path.contains("smaa")
        || path.contains("taa")
        || path.contains("post")
        || path.contains("mip")
    {
        POST
    } else {
        OTHER
    }
}

#[derive(Resource, Default)]
pub struct GpuTiming {
    pub available: bool,
    sums_ms: [f64; CATEGORIES],
    total_ms: f64,
    frames: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuAverages {
    pub frames: u64,
    pub total_ms_avg: f64,
    pub main_ms_avg: f64,
    pub shadow_ms_avg: f64,
    pub post_ms_avg: f64,
    pub other_ms_avg: f64,
}

impl GpuTiming {
    pub fn reset(&mut self) {
        self.sums_ms = [0.0; CATEGORIES];
        self.total_ms = 0.0;
        self.frames = 0;
    }

    /// Per-run averages; None when no GPU timestamps were observed.
    pub fn averages(&self) -> Option<GpuAverages> {
        if !self.available || self.frames == 0 {
            return None;
        }
        let n = self.frames as f64;
        Some(GpuAverages {
            frames: self.frames,
            total_ms_avg: self.total_ms / n,
            main_ms_avg: self.sums_ms[MAIN] / n,
            shadow_ms_avg: self.sums_ms[SHADOW] / n,
            post_ms_avg: self.sums_ms[POST] / n,
            other_ms_avg: self.sums_ms[OTHER] / n,
        })
    }
}

/// Collects the latest GPU span measurements each Running frame and attaches
/// the per-frame total to the most recent sample.
pub fn collect(
    state: Res<State<BenchState>>,
    store: Res<DiagnosticsStore>,
    mut gpu: ResMut<GpuTiming>,
    mut sampler: ResMut<Sampler>,
) {
    if *state.get() != BenchState::Running {
        return;
    }
    let mut frame_cats = [0.0f64; CATEGORIES];
    let mut total = 0.0;
    let mut any = false;
    for diagnostic in store.iter() {
        let path = diagnostic.path().as_str();
        if !path.ends_with("elapsed_gpu") {
            continue;
        }
        let Some(value) = diagnostic.value() else { continue };
        any = true;
        total += value;
        frame_cats[category(path)] += value;
    }
    if !any {
        return;
    }
    gpu.available = true;
    for (sum, frame) in gpu.sums_ms.iter_mut().zip(frame_cats) {
        *sum += frame;
    }
    gpu.total_ms += total;
    gpu.frames += 1;
    if let Some(last) = sampler.samples.last_mut()
        && last.gpu_ms.is_none()
    {
        last.gpu_ms = Some(total);
    }
}
