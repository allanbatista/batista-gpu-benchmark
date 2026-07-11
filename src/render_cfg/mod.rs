//! Runtime graphics capabilities + applying display settings + manual FPS limiter.

use bevy::anti_alias::contrast_adaptive_sharpening::ContrastAdaptiveSharpening;
use bevy::anti_alias::fxaa::Fxaa;
use bevy::anti_alias::smaa::Smaa;
use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::camera::primitives::Aabb;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{DirectionalLightShadowMap, PointLightShadowMap};
use bevy::pbr::wireframe::WireframeConfig;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::renderer::{RenderAdapterInfo, RenderDevice};
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, VideoModeSelection, WindowMode};

use crate::app::AppSettings;
use crate::config::{AaMode, TonemappingSetting, WindowModeSetting};
use crate::scene::BenchCamera;

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
    /// Adapter exposes the wgpu features bevy_solari needs (ray queries etc).
    pub rt_supported: bool,
    pub max_msaa: u32,
    pub features: Vec<String>,
    pub limits: Vec<(String, String)>,
}

pub struct RenderCfgPlugin;

impl Plugin for RenderCfgPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Capabilities>()
            .init_resource::<PointLightShadowMap>()
            .init_resource::<DirectionalLightShadowMap>()
            .add_systems(
                Update,
                (capture_capabilities, apply_display_settings, apply_camera_settings, draw_aabb_gizmos),
            )
            .add_systems(Last, fps_limiter);
        #[cfg(feature = "dlss")]
        app.add_systems(Update, detect_dlss);
    }
}

#[cfg(feature = "dlss")]
fn detect_dlss(
    mut caps: ResMut<Capabilities>,
    supported: Option<Res<bevy::anti_alias::dlss::DlssSuperResolutionSupported>>,
) {
    let available = supported.is_some();
    if caps.dlss_supported != available {
        caps.dlss_supported = available;
    }
}

/// Everything that maps settings → camera components / global render resources.
#[derive(PartialEq, Clone)]
struct CameraCfgKey {
    aa: AaMode,
    fsr1_quality: crate::config::Fsr1Quality,
    bloom: f32,
    hdr: bool,
    tonemapping: TonemappingSetting,
    render_scale: f32,
    shadow_map_size: u32,
    wireframe: bool,
    window: UVec2,
    caps_ready: bool,
}

#[allow(clippy::too_many_arguments)]
fn apply_camera_settings(
    settings: Res<AppSettings>,
    caps: Res<Capabilities>,
    cameras: Query<Entity, With<BenchCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut commands: Commands,
    mut wireframe: ResMut<WireframeConfig>,
    mut point_map: ResMut<PointLightShadowMap>,
    mut dir_map: ResMut<DirectionalLightShadowMap>,
    mut key: Local<Option<CameraCfgKey>>,
) {
    let r = &settings.0.renderer;
    let window_size = windows
        .single()
        .map(|w| {
            let s = w.resolution.physical_size();
            UVec2::new(s.x, s.y)
        })
        .unwrap_or(UVec2::new(1920, 1080));
    let new_key = CameraCfgKey {
        aa: r.aa,
        fsr1_quality: r.fsr1_quality,
        bloom: r.bloom,
        hdr: r.hdr,
        tonemapping: r.tonemapping,
        render_scale: r.render_scale,
        shadow_map_size: r.shadow_map_size,
        wireframe: r.wireframe,
        window: window_size,
        caps_ready: caps.ready,
    };
    if key.as_ref() == Some(&new_key) {
        return;
    }
    *key = Some(new_key);

    let Ok(camera) = cameras.single() else { return };
    let mut entity = commands.entity(camera);
    entity.remove::<(
        Fxaa,
        Smaa,
        TemporalAntiAliasing,
        ContrastAdaptiveSharpening,
        Bloom,
        bevy::camera::MainPassResolutionOverride,
    )>();

    let msaa = match r.aa {
        AaMode::Msaa2 => Msaa::Sample2,
        AaMode::Msaa4 => Msaa::Sample4,
        AaMode::Msaa8 if !caps.ready || caps.max_msaa >= 8 => Msaa::Sample8,
        AaMode::Msaa8 => Msaa::Sample4, // clamped; reported as achieved value
        _ => Msaa::Off,
    };
    entity.insert(msaa);
    match r.aa {
        AaMode::Fxaa => {
            entity.insert(Fxaa::default());
        }
        AaMode::Smaa => {
            entity.insert(Smaa::default());
        }
        AaMode::Taa => {
            entity.insert(TemporalAntiAliasing::default());
        }
        AaMode::Fsr1 => {
            // ponytail: FSR1-style = low-res main pass + bevy's bilinear upscale +
            // AMD FidelityFX CAS sharpening. Full EASU edge reconstruction is the
            // upgrade path if image quality ever warrants a custom pass.
            entity.insert(ContrastAdaptiveSharpening {
                enabled: true,
                sharpening_strength: 0.6,
                ..default()
            });
        }
        #[cfg(feature = "dlss")]
        AaMode::Dlss => {
            if caps.dlss_supported {
                entity.insert(bevy::anti_alias::dlss::Dlss::<
                    bevy::anti_alias::dlss::DlssSuperResolutionFeature,
                > {
                    perf_quality_mode: bevy::anti_alias::dlss::DlssPerfQualityMode::Auto,
                    ..default()
                });
            }
        }
        _ => {}
    }

    if r.hdr {
        entity.insert(bevy::camera::Hdr);
    } else {
        entity.remove::<bevy::camera::Hdr>();
    }
    if r.bloom > 0.0 {
        entity.insert(Bloom { intensity: r.bloom, ..Bloom::NATURAL });
    }
    entity.insert(match r.tonemapping {
        TonemappingSetting::None => Tonemapping::None,
        TonemappingSetting::Reinhard => Tonemapping::Reinhard,
        TonemappingSetting::AcesFitted => Tonemapping::AcesFitted,
        TonemappingSetting::AgX => Tonemapping::AgX,
        TonemappingSetting::TonyMcMapface => Tonemapping::TonyMcMapface,
        TonemappingSetting::BlenderFilmic => Tonemapping::BlenderFilmic,
        TonemappingSetting::KhronosPbrNeutral => Tonemapping::KhronosPbrNeutral,
    });

    // FSR1 drives its own internal resolution; otherwise the manual render scale applies.
    let effective_scale = if r.aa == AaMode::Fsr1 { r.fsr1_quality.render_scale() } else { r.render_scale };
    if effective_scale < 1.0 {
        let scaled = (window_size.as_vec2() * effective_scale).as_uvec2().max(UVec2::ONE);
        entity.insert(bevy::camera::MainPassResolutionOverride(scaled));
    }

    wireframe.global = r.wireframe && caps.wireframe;
    point_map.size = r.shadow_map_size as usize;
    dir_map.size = r.shadow_map_size as usize;
}

fn draw_aabb_gizmos(
    settings: Res<AppSettings>,
    mut gizmos: Gizmos,
    query: Query<(&Aabb, &GlobalTransform), With<Mesh3d>>,
) {
    if !settings.0.renderer.show_aabb {
        return;
    }
    for (aabb, gt) in &query {
        let transform = gt.compute_transform()
            * Transform::from_translation(Vec3::from(aabb.center))
                .with_scale(Vec3::from(aabb.half_extents) * 2.0);
        gizmos.cube(transform, Color::srgb(0.2, 1.0, 0.4));
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
    #[cfg(feature = "rt-experimental")]
    {
        caps.rt_supported = features.contains(bevy::solari::prelude::SolariPlugins::required_wgpu_features());
    }
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
