//! Optional telemetry (spec §9.3): CPU/RAM % from bevy's sysinfo diagnostics,
//! plus Linux amdgpu sysfs (GPU busy %, VRAM, temperature, power). Everything
//! is best-effort: any failure leaves fields absent; nothing here can abort a
//! benchmark. macOS GPU telemetry is unavailable by design.

use bevy::diagnostic::{DiagnosticsStore, SystemInformationDiagnosticsPlugin};
use bevy::prelude::*;
use serde::Serialize;

use crate::bench::BenchState;

#[derive(Debug, Clone, Copy, Default)]
pub struct TelemetrySample {
    pub cpu_pct: Option<f64>,
    pub mem_pct: Option<f64>,
    pub gpu_busy_pct: Option<f64>,
    pub vram_used_mb: Option<f64>,
    pub gpu_temp_c: Option<f64>,
    pub gpu_power_w: Option<f64>,
}

#[derive(Resource, Default)]
pub struct Telemetry {
    pub samples: Vec<TelemetrySample>,
    last_poll: f64,
    gpu_paths: Option<AmdgpuPaths>,
    probed: bool,
}

impl Telemetry {
    pub fn reset(&mut self) {
        self.samples.clear();
    }

    pub fn aggregate(&self) -> Option<TelemetryAggregate> {
        if self.samples.is_empty() {
            return None;
        }
        fn agg(values: impl Iterator<Item = Option<f64>>) -> Option<(f64, f64)> {
            let values: Vec<f64> = values.flatten().collect();
            if values.is_empty() {
                return None;
            }
            let avg = values.iter().sum::<f64>() / values.len() as f64;
            let max = values.iter().cloned().fold(f64::MIN, f64::max);
            Some(((avg * 10.0).round() / 10.0, (max * 10.0).round() / 10.0))
        }
        let s = &self.samples;
        Some(TelemetryAggregate {
            samples: s.len(),
            cpu_pct: agg(s.iter().map(|x| x.cpu_pct)),
            mem_pct: agg(s.iter().map(|x| x.mem_pct)),
            gpu_busy_pct: agg(s.iter().map(|x| x.gpu_busy_pct)),
            vram_used_mb: agg(s.iter().map(|x| x.vram_used_mb)),
            gpu_temp_c: agg(s.iter().map(|x| x.gpu_temp_c)),
            gpu_power_w: agg(s.iter().map(|x| x.gpu_power_w)),
        })
    }
}

/// (average, max) pairs; None = metric unavailable on this system.
#[derive(Debug, Clone, Serialize)]
pub struct TelemetryAggregate {
    pub samples: usize,
    pub cpu_pct: Option<(f64, f64)>,
    pub mem_pct: Option<(f64, f64)>,
    pub gpu_busy_pct: Option<(f64, f64)>,
    pub vram_used_mb: Option<(f64, f64)>,
    pub gpu_temp_c: Option<(f64, f64)>,
    pub gpu_power_w: Option<(f64, f64)>,
}

#[derive(Debug, Clone)]
struct AmdgpuPaths {
    busy: std::path::PathBuf,
    vram_used: std::path::PathBuf,
    temp: Option<std::path::PathBuf>,
    power: Option<std::path::PathBuf>,
}

/// Locates the first amdgpu card exposing busy% in sysfs (Linux only).
fn probe_amdgpu() -> Option<AmdgpuPaths> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let drm = std::path::Path::new("/sys/class/drm");
    for entry in std::fs::read_dir(drm).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let device = entry.path().join("device");
        let busy = device.join("gpu_busy_percent");
        if !busy.exists() {
            continue;
        }
        let mut temp = None;
        let mut power = None;
        if let Ok(hwmons) = std::fs::read_dir(device.join("hwmon")) {
            for hwmon in hwmons.flatten() {
                let t = hwmon.path().join("temp1_input");
                if t.exists() {
                    temp = Some(t);
                }
                for candidate in ["power1_average", "power1_input"] {
                    let p = hwmon.path().join(candidate);
                    if p.exists() {
                        power = Some(p);
                        break;
                    }
                }
            }
        }
        return Some(AmdgpuPaths { busy, vram_used: device.join("mem_info_vram_used"), temp, power });
    }
    None
}

fn read_f64(path: &std::path::Path) -> Option<f64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// 1 Hz poll during capture states. Plain file reads — cheap enough that a
/// dedicated thread isn't warranted.
pub fn poll(
    state: Res<State<BenchState>>,
    time: Res<Time<Real>>,
    diagnostics: Res<DiagnosticsStore>,
    mut telemetry: ResMut<Telemetry>,
) {
    if !matches!(*state.get(), BenchState::Warmup | BenchState::Running) {
        return;
    }
    let now = time.elapsed_secs_f64();
    if now - telemetry.last_poll < 1.0 {
        return;
    }
    telemetry.last_poll = now;
    if !telemetry.probed {
        telemetry.probed = true;
        telemetry.gpu_paths = probe_amdgpu();
        if telemetry.gpu_paths.is_some() {
            info!("telemetry: amdgpu sysfs metrics available");
        }
    }

    let mut sample = TelemetrySample {
        cpu_pct: diagnostics
            .get(&SystemInformationDiagnosticsPlugin::SYSTEM_CPU_USAGE)
            .and_then(|d| d.value()),
        mem_pct: diagnostics
            .get(&SystemInformationDiagnosticsPlugin::SYSTEM_MEM_USAGE)
            .and_then(|d| d.value()),
        ..default()
    };
    if let Some(gpu) = telemetry.gpu_paths.clone() {
        sample.gpu_busy_pct = read_f64(&gpu.busy);
        sample.vram_used_mb = read_f64(&gpu.vram_used).map(|b| b / (1024.0 * 1024.0));
        sample.gpu_temp_c = gpu.temp.as_deref().and_then(read_f64).map(|v| v / 1000.0);
        sample.gpu_power_w = gpu.power.as_deref().and_then(read_f64).map(|v| v / 1_000_000.0);
    }
    telemetry.samples.push(sample);
}
