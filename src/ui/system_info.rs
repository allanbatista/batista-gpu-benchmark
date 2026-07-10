//! Static system information (spec §7.6) gathered once via sysinfo.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::config::{APP_VERSION, BEVY_VERSION};
use crate::render_cfg::Capabilities;

#[derive(Resource)]
pub struct SystemInfo {
    pub os: String,
    pub os_version: String,
    pub kernel: String,
    pub arch: String,
    pub cpu: String,
    pub physical_cores: String,
    pub logical_cores: usize,
    pub ram_gb: f64,
}

impl FromWorld for SystemInfo {
    fn from_world(_: &mut World) -> Self {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();
        Self {
            os: sysinfo::System::name().unwrap_or_else(|| std::env::consts::OS.into()),
            os_version: sysinfo::System::os_version().unwrap_or_default(),
            kernel: sysinfo::System::kernel_version().unwrap_or_default(),
            arch: std::env::consts::ARCH.into(),
            cpu: sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default(),
            physical_cores: sysinfo::System::physical_core_count()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".into()),
            logical_cores: sys.cpus().len(),
            ram_gb: sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0),
        }
    }
}

pub fn draw(ui: &mut egui::Ui, info: &SystemInfo, caps: &Capabilities) {
    egui::Grid::new("sysinfo").num_columns(2).striped(true).show(ui, |ui| {
        let mut row = |k: &str, v: &str| {
            ui.label(k);
            ui.label(v);
            ui.end_row();
        };
        row("OS", &format!("{} {}", info.os, info.os_version));
        row("Kernel", &info.kernel);
        row("Arch", &info.arch);
        row("CPU", &info.cpu);
        row("Cores", &format!("{} physical / {} logical", info.physical_cores, info.logical_cores));
        row("RAM", &format!("{:.1} GiB", info.ram_gb));
        row("App version", APP_VERSION);
        row("Bevy version", BEVY_VERSION);
    });

    ui.separator();
    if !caps.ready {
        ui.label("GPU: detecting…");
        return;
    }
    egui::Grid::new("gpuinfo").num_columns(2).striped(true).show(ui, |ui| {
        let mut row = |k: &str, v: &str| {
            ui.label(k);
            ui.label(v);
            ui.end_row();
        };
        row("GPU", &caps.adapter_name);
        row("Type", &caps.device_type);
        row("Graphics API", &caps.backend);
        row("Vendor / Device ID", &format!("0x{:04x} / 0x{:04x}", caps.vendor_id, caps.device_id));
        row("Driver", &format!("{} {}", caps.driver, caps.driver_info));
    });

    ui.collapsing(format!("Features ({})", caps.features.len()), |ui| {
        for f in &caps.features {
            ui.monospace(f);
        }
    });
    ui.collapsing("Limits", |ui| {
        egui::Grid::new("limits").num_columns(2).striped(true).show(ui, |ui| {
            for (k, v) in &caps.limits {
                ui.monospace(k);
                ui.monospace(v);
                ui.end_row();
            }
        });
    });
}
