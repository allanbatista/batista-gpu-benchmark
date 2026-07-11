//! Report generation (spec §12): results/<timestamp>/{summary.json, frames.csv,
//! settings.toml, screenshot.png}. Writing happens once on Completed — never
//! during capture. Failures keep samples in memory and allow a retry (spec §16).

use bevy::prelude::*;
use bevy::render::view::screenshot::Capturing;
use bevy::window::{PrimaryWindow, WindowMode};
use serde_json::json;
use std::fmt::Write as _;
use std::path::PathBuf;

use super::{BenchSession, BenchState, metrics};
use crate::app::RunOpts;
use crate::config::{self, APP_VERSION, BEVY_VERSION, SCORE_ALGORITHM};
use crate::render_cfg::Capabilities;
use crate::ui::system_info::SystemInfo;

#[allow(clippy::too_many_arguments)]
pub fn write_when_completed(
    state: Res<State<BenchState>>,
    mut session: ResMut<BenchSession>,
    run_opts: Res<RunOpts>,
    caps: Res<Capabilities>,
    sysinfo: Res<SystemInfo>,
    windows: Query<&Window, With<PrimaryWindow>>,
    monitors: Query<&bevy::window::Monitor>,
    telemetry: Res<crate::platform::telemetry::Telemetry>,
) {
    if *state.get() != BenchState::Completed || session.report_written {
        return;
    }
    session.report_written = true;
    session.report_error = None;

    let Some(plan) = session.plan.clone() else { return };
    let dir = match &session.result_dir {
        Some(dir) => dir.clone(),
        None => {
            session.report_error = Some("no result directory (creation failed at start)".into());
            return;
        }
    };

    let window = windows.single().ok();
    let achieved = window.map(|w| {
        let size = w.resolution.physical_size();
        let mode = match w.mode {
            WindowMode::Windowed => "windowed",
            WindowMode::BorderlessFullscreen(_) => "borderless",
            WindowMode::Fullscreen(..) => "fullscreen",
        };
        let refresh = monitors
            .iter()
            .next()
            .and_then(|m| m.refresh_rate_millihertz)
            .map(|mhz| mhz as f64 / 1000.0);
        json!({ "width": size.x, "height": size.y, "mode": mode, "refresh_rate_hz": refresh })
    });

    let mut deviations = config::official_deviations(&plan.settings);
    if session.focus_lost {
        deviations.push("focus_lost".into());
    }
    for record in &session.records {
        if record.canceled {
            deviations.push(format!("run_{}_canceled", record.index));
        }
        if record.truncated {
            deviations.push(format!("run_{}_truncated", record.index));
        }
    }

    let completed = session.completed_metrics();
    let consolidated = metrics::consolidate(&completed, plan.settings.benchmark.unstable_threshold_pct);

    let summary = json!({
        "benchmark_version": APP_VERSION,
        "bevy_version": BEVY_VERSION,
        "preset": config::preset_label(&plan.settings),
        "score_algorithm": SCORE_ALGORITHM,
        "criteria_versions": {
            "metrics": metrics::METRICS_VERSION,
            "percentile": metrics::PERCENTILE_METHOD,
            "lows": metrics::LOW_AGG_METHOD,
            "stutter": metrics::STUTTER_CRITERION,
            "consolidation": metrics::CONSOLIDATION_METHOD,
            "official_profile": config::OFFICIAL_PROFILE,
        },
        "started_at": session.started_at_local,
        "system": {
            "os": sysinfo.os,
            "os_version": sysinfo.os_version,
            "kernel": sysinfo.kernel,
            "arch": sysinfo.arch,
            "cpu": sysinfo.cpu,
            "physical_cores": sysinfo.physical_cores,
            "logical_cores": sysinfo.logical_cores,
            "ram_gib": (sysinfo.ram_gb * 10.0).round() / 10.0,
        },
        "adapter": {
            "name": caps.adapter_name,
            "backend": caps.backend,
            "device_type": caps.device_type,
            "driver": caps.driver,
            "driver_info": caps.driver_info,
            "vendor_id": caps.vendor_id,
            "device_id": caps.device_id,
            "features": caps.features,
            "limits": caps.limits.iter().cloned().collect::<std::collections::BTreeMap<_, _>>(),
        },
        "display": {
            "requested": {
                "width": plan.settings.display.width,
                "height": plan.settings.display.height,
                "mode": format!("{:?}", plan.settings.display.window_mode),
                "vsync": plan.settings.display.vsync,
                "fps_limit": plan.settings.display.fps_limit,
                "monitor": plan.settings.display.monitor,
            },
            "achieved": achieved,
        },
        "renderer": {
            "backend_setting": format!("{:?}", plan.settings.renderer.backend),
            "mode": match plan.settings.renderer.render_mode {
                crate::config::RenderModeSetting::Pbr => "pbr-v1",
                crate::config::RenderModeSetting::RtExperimental => "rt-experimental-v1",
            },
            "experimental": plan.settings.renderer.render_mode != crate::config::RenderModeSetting::Pbr,
            "aa": format!("{:?}", plan.settings.renderer.aa),
            "render_scale": plan.settings.renderer.render_scale,
            "bloom": plan.settings.renderer.bloom,
            "tonemapping": format!("{:?}", plan.settings.renderer.tonemapping),
            "hdr": plan.settings.renderer.hdr,
            "shadows": plan.settings.renderer.shadows,
            "shadow_map_size": plan.settings.renderer.shadow_map_size,
            "wireframe": plan.settings.renderer.wireframe,
        },
        "scene": {
            "model": plan.settings.scene.model.clone().unwrap_or_else(|| "bundled:benchmark.glb".into()),
            "seed": plan.settings.scene.seed,
            "light_count": plan.settings.scene.light_count,
            "shadow_casters": plan.settings.scene.shadow_caster_count,
            "directional_light": plan.settings.scene.directional_light,
            "light_intensity": plan.settings.scene.light_intensity,
            "light_speed": plan.settings.scene.light_speed,
            "camera_speed": plan.settings.scene.camera_speed,
            "camera_distance": plan.settings.scene.camera_distance,
            "animate_model": plan.settings.scene.animate_model,
        },
        "execution": {
            "warmup_s": plan.warmup_s,
            "duration_s": plan.duration_s,
            "runs": plan.runs,
            "interval_s": plan.interval_s,
            "prewarm_frames": session.prewarm_frames,
            "prewarm_seconds": (session.prewarm_seconds * 1000.0).round() / 1000.0,
            "focus_lost": session.focus_lost,
            "cli_overrides": run_opts.0.cli_overrides,
            "official": deviations.is_empty(),
            "deviations": deviations,
            "build": if cfg!(debug_assertions) { "debug" } else { "release" },
        },
        "gpu_timing": if session.records.iter().any(|r| r.gpu.is_some()) {
            json!({
                "supported": true,
                "source": "wgpu TIMESTAMP_QUERY via RenderDiagnosticsPlugin",
                "note": "per-run averages are authoritative; per-frame gpu_time_ms rows lag by 1-3 frames",
            })
        } else {
            json!("unavailable")
        },
        "telemetry": match telemetry.aggregate() {
            Some(t) => serde_json::to_value(t).unwrap_or_else(|_| json!("unavailable")),
            None => json!("unavailable"),
        },
        "metrics": consolidated,
        "runs": session.records.iter().map(|r| json!({
            "index": r.index,
            "canceled": r.canceled,
            "truncated": r.truncated,
            "metrics": r.metrics,
            "gpu": r.gpu,
        })).collect::<Vec<_>>(),
    });

    let result = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("summary.json"), serde_json::to_string_pretty(&summary).unwrap())?;
        std::fs::write(dir.join("frames.csv"), frames_csv(&session))?;
        let settings_snapshot =
            toml::to_string_pretty(&plan.settings).unwrap_or_else(|e| format!("# serialize error: {e}"));
        std::fs::write(dir.join("settings.toml"), settings_snapshot)?;
        Ok(())
    })();

    match result {
        Ok(()) => info!("report written to {}", dir.display()),
        Err(e) => {
            session.report_error = Some(format!("failed to save report: {e}"));
            error!("failed to save report to {}: {e}", dir.display());
        }
    }
}

fn frames_csv(session: &BenchSession) -> String {
    let total: usize = session.records.iter().map(|r| r.samples.len()).sum();
    let mut csv = String::with_capacity(total * 40 + 64);
    csv.push_str("run_index,frame_index,elapsed_seconds,frame_time_ms,fps,gpu_time_ms\n");
    for record in &session.records {
        for (i, s) in record.samples.iter().enumerate() {
            let fps = 1000.0 / s.ft_ms;
            match s.gpu_ms {
                Some(gpu) => {
                    let _ = writeln!(csv, "{},{},{:.6},{:.4},{:.2},{:.4}", record.index, i, s.elapsed, s.ft_ms, fps, gpu);
                }
                None => {
                    let _ = writeln!(csv, "{},{},{:.6},{:.4},{:.2},", record.index, i, s.elapsed, s.ft_ms, fps);
                }
            }
        }
    }
    csv
}

/// With --exit-after-benchmark: leave once the report (and any in-flight
/// screenshot) is done. Errors exit with code 1.
pub fn exit_after_benchmark(
    state: Res<State<BenchState>>,
    run_opts: Res<RunOpts>,
    session: Res<BenchSession>,
    capturing: Query<(), With<Capturing>>,
    mut exit: MessageWriter<AppExit>,
) {
    if !run_opts.0.exit_after {
        return;
    }
    match state.get() {
        BenchState::Completed if session.report_written && capturing.is_empty() => {
            exit.write(if session.report_error.is_some() {
                AppExit::from_code(1)
            } else {
                AppExit::Success
            });
        }
        BenchState::Error => {
            exit.write(AppExit::from_code(1));
        }
        _ => {}
    }
}

/// Creates the session's result directory as the session starts (never during
/// capture). Called from the runner on Loading entry.
pub fn create_result_dir(output_dir: &str, timestamp: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(output_dir).join(timestamp);
    match std::fs::create_dir_all(&dir) {
        Ok(()) => Some(dir),
        Err(e) => {
            error!("could not create result dir {}: {e}", dir.display());
            None
        }
    }
}
