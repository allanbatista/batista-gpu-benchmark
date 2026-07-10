//! Permanent compact overlay (spec §7.1): FPS, frame time, API, resolution, state.
//! Strings refresh at 4 Hz to avoid per-frame formatting churn.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};

use crate::bench::BenchState;
use crate::render_cfg::Capabilities;

#[derive(Default)]
pub struct OverlayCache {
    last_refresh: f64,
    lines: [String; 4],
}

pub fn overlay_ui(
    mut contexts: EguiContexts,
    diagnostics: Res<DiagnosticsStore>,
    caps: Res<Capabilities>,
    state: Res<State<BenchState>>,
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
        let fps_avg = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|d| d.average())
            .unwrap_or(0.0);
        let ft = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
            .and_then(|d| d.smoothed())
            .unwrap_or(0.0);
        let ft_avg = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
            .and_then(|d| d.average())
            .unwrap_or(0.0);
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
        cache.lines[3] = format!("state: {}", state.get().label());
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
