//! Permanent compact overlay (spec §7.1): FPS, frame time, API, resolution, state.
//! Strings refresh at 4 Hz to avoid per-frame formatting churn.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};

use crate::bench::sampler::Sampler;
use crate::bench::{BenchSession, BenchState};
use crate::render_cfg::Capabilities;

#[derive(Default)]
pub struct OverlayCache {
    last_refresh: f64,
    lines: [String; 4],
}

#[allow(clippy::too_many_arguments)]
pub fn overlay_ui(
    mut contexts: EguiContexts,
    diagnostics: Res<DiagnosticsStore>,
    caps: Res<Capabilities>,
    state: Res<State<BenchState>>,
    session: Res<BenchSession>,
    sampler: Res<Sampler>,
    windows: Query<&Window, With<PrimaryWindow>>,
    time: Res<Time<Real>>,
    mut cache: Local<OverlayCache>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    let now = time.elapsed_secs_f64();
    if now - cache.last_refresh >= 0.25 || cache.lines[0].is_empty() {
        cache.last_refresh = now;
        let fps = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|d| d.smoothed())
            .unwrap_or(0.0);
        // During capture the average comes from the actual benchmark samples.
        let fps_avg = if sampler.active && sampler.samples.len() > 8 {
            let sum: f64 = sampler.samples.iter().map(|s| s.ft_ms).sum();
            1000.0 * sampler.samples.len() as f64 / sum
        } else {
            diagnostics
                .get(&FrameTimeDiagnosticsPlugin::FPS)
                .and_then(|d| d.average())
                .unwrap_or(0.0)
        };
        let ft = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
            .and_then(|d| d.smoothed())
            .unwrap_or(0.0);
        let ft_avg = if fps_avg > 0.0 { 1000.0 / fps_avg } else { 0.0 };
        let resolution = windows
            .single()
            .map(|w| {
                let size = w.resolution.physical_size();
                format!("{}x{}", size.x, size.y)
            })
            .unwrap_or_default();

        cache.lines[0] = format!("FPS {fps:>7.1}   avg {fps_avg:>7.1}");
        cache.lines[1] = format!("ft  {ft:>6.2}ms  avg {ft_avg:>6.2}ms");
        cache.lines[2] = format!("{} | {}", caps.backend_label(), resolution);
        cache.lines[3] = if state.get().is_benchmarking() {
            let total = session.plan.as_ref().map(|p| p.runs).unwrap_or(1);
            format!("state: {} (run {}/{})", state.get().label(), session.current_run + 1, total)
        } else {
            format!("state: {}", state.get().label())
        };
    }

    egui::Window::new("perf-overlay")
        .title_bar(false)
        .resizable(false)
        .interactable(false)
        .movable(false)
        .anchor(egui::Align2::LEFT_TOP, [8.0, 8.0])
        .show(ctx, |ui| {
            for line in &cache.lines {
                ui.monospace(line);
            }
        });
    Ok(())
}

impl Capabilities {
    fn backend_label(&self) -> &str {
        if self.ready { &self.backend } else { "…" }
    }
}
