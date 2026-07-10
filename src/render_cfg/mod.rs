//! Runtime graphics capabilities + applying display settings + manual FPS limiter.

use bevy::prelude::*;
use bevy::render::renderer::{RenderAdapterInfo, RenderDevice};
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, VideoModeSelection, WindowMode};

use crate::app::AppSettings;
use crate::config::WindowModeSetting;

/// What the current adapter/backend actually supports. UI disables (never hides)
/// unsupported options based on this (spec §16/§18).
#[derive(Resource, Default, Clone)]
pub struct Capabilities {
    pub ready: bool,
    pub adapter_name: String,
    pub backend: String,
    pub device_type: String,
    pub driver: String,
    pub driver_info: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub timestamp_query: bool,
    pub wireframe: bool,
    /// Set in the advanced milestone when the DLSS feature detects an RTX+Vulkan adapter.
    pub dlss_supported: bool,
    pub max_msaa: u32,
    pub features: Vec<String>,
    pub limits: Vec<(String, String)>,
}

pub struct RenderCfgPlugin;

impl Plugin for RenderCfgPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Capabilities>()
            .add_systems(Update, (capture_capabilities, apply_display_settings))
            .add_systems(Last, fps_limiter);
    }
}

fn capture_capabilities(
    mut caps: ResMut<Capabilities>,
    info: Option<Res<RenderAdapterInfo>>,
    device: Option<Res<RenderDevice>>,
) {
    if caps.ready {
        return;
    }
    let (Some(info), Some(device)) = (info, device) else {
        return;
    };
    let features = device.features();
    let limits = device.limits();

    caps.adapter_name = info.name.clone();
    caps.backend = info.backend.to_str().to_string();
    caps.device_type = format!("{:?}", info.device_type);
    caps.driver = info.driver.clone();
    caps.driver_info = info.driver_info.clone();
    caps.vendor_id = info.vendor;
    caps.device_id = info.device;
    caps.timestamp_query = features.contains(bevy::render::settings::WgpuFeatures::TIMESTAMP_QUERY);
    caps.wireframe = features.contains(bevy::render::settings::WgpuFeatures::POLYGON_MODE_LINE);
    // ponytail: wgpu has no direct "max MSAA" query; GL commonly caps at 4, others do 8.
    caps.max_msaa = if caps.backend.eq_ignore_ascii_case("gl") { 4 } else { 8 };
    caps.features = features.iter_names().map(|(name, _)| name.to_string()).collect();
    caps.limits = vec![
        ("max_texture_dimension_2d", limits.max_texture_dimension_2d.to_string()),
        ("max_texture_array_layers", limits.max_texture_array_layers.to_string()),
        ("max_bind_groups", limits.max_bind_groups.to_string()),
        ("max_uniform_buffer_binding_size", limits.max_uniform_buffer_binding_size.to_string()),
        ("max_storage_buffer_binding_size", limits.max_storage_buffer_binding_size.to_string()),
        ("max_vertex_buffers", limits.max_vertex_buffers.to_string()),
        ("max_samplers_per_shader_stage", limits.max_samplers_per_shader_stage.to_string()),
        ("max_compute_workgroup_size_x", limits.max_compute_workgroup_size_x.to_string()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    caps.ready = true;

    info!(
        "adapter: {} | backend: {} | driver: {} {}",
        caps.adapter_name, caps.backend, caps.driver, caps.driver_info
    );
}

fn apply_display_settings(
    settings: Res<AppSettings>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if !settings.is_changed() {
        return;
    }
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    let d = &settings.0.display;
    let present = if d.vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync };
    if window.present_mode != present {
        window.present_mode = present;
    }
    let monitor = MonitorSelection::Index(d.monitor);
    let mode = match d.window_mode {
        WindowModeSetting::Windowed => WindowMode::Windowed,
        WindowModeSetting::Borderless => WindowMode::BorderlessFullscreen(monitor),
        WindowModeSetting::Fullscreen => WindowMode::Fullscreen(monitor, VideoModeSelection::Current),
    };
    if window.mode != mode {
        window.mode = mode;
    }
    if d.window_mode == WindowModeSetting::Windowed {
        let current = window.resolution.physical_size();
        if current.x != d.width || current.y != d.height {
            window.resolution.set_physical_resolution(d.width, d.height);
        }
    }
}

/// Manual frame limiter (no frame-pacing crate supports bevy 0.19).
/// ponytail: coarse sleep + spin; the official profile runs uncapped anyway.
fn fps_limiter(settings: Res<AppSettings>, mut last: Local<Option<std::time::Instant>>) {
    let limit = settings.0.display.fps_limit;
    if limit == 0 {
        *last = None;
        return;
    }
    let target = std::time::Duration::from_secs_f64(1.0 / limit as f64);
    if let Some(prev) = *last {
        let elapsed = prev.elapsed();
        if elapsed < target {
            let remain = target - elapsed;
            if remain > std::time::Duration::from_millis(2) {
                std::thread::sleep(remain - std::time::Duration::from_millis(1));
            }
            while prev.elapsed() < target {
                std::hint::spin_loop();
            }
        }
    }
    *last = Some(std::time::Instant::now());
}
