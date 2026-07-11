//! App assembly: plugins, window/backend from settings, settings persistence.

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{Backends, PowerPreference, WgpuSettings};
use bevy::window::{MonitorSelection, PresentMode, VideoModeSelection, WindowMode, WindowResolution};
use bevy::winit::{UpdateMode, WinitSettings};

use crate::config::{BackendSetting, PowerPrefSetting, RunOptions, Settings, WindowModeSetting};

/// The live settings (mirrors ./config/settings.toml).
#[derive(Resource, Clone)]
pub struct AppSettings(pub Settings);

#[derive(Resource, Clone, Default)]
pub struct RunOpts(pub RunOptions);

/// Renderer config captured at startup; differing live settings require a restart.
#[derive(Resource, Clone)]
pub struct StartupRenderCfg {
    pub backend: BackendSetting,
    pub adapter: Option<String>,
    pub power: PowerPrefSetting,
    pub render_mode: crate::config::RenderModeSetting,
}

/// Debounced settings persistence state.
#[derive(Resource, Default)]
pub struct SettingsIo {
    pub dirty_at: Option<f64>,
    pub last_save_error: Option<String>,
}

pub fn run(settings: Settings, run: RunOptions) -> AppExit {
    let mut app = App::new();
    let asset_root = run.asset_root.clone();

    // Reduced logging during automated benchmark runs (spec §14/§17).
    // Benchmark mode: silence engine noise, keep this app's few own lines.
    let log_filter = if run.autostart {
        "error,batista_gpu_benchmark=info"
    } else {
        "info,wgpu=error,naga=warn,bevy_render=info"
    };

    // NVIDIA requires a per-app DLSS project id, inserted before DefaultPlugins.
    #[cfg(feature = "dlss")]
    app.insert_resource(bevy::anti_alias::dlss::DlssProjectId(uuid::Uuid::from_u128(
        0xb471_57ba_0b54_4d10_a3a2_6a2c_2b4e_9d01,
    )));

    app.insert_resource(StartupRenderCfg {
        backend: settings.renderer.backend,
        adapter: settings.renderer.adapter.clone(),
        power: settings.renderer.power_preference,
        render_mode: settings.renderer.render_mode,
    });
    app.insert_resource(AppSettings(settings.clone()));
    app.insert_resource(RunOpts(run));
    app.init_resource::<SettingsIo>();

    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(main_window(&settings)),
                ..default()
            })
            .set(RenderPlugin {
                render_creation: wgpu_settings(&settings).into(),
                ..default()
            })
            .set(LogPlugin {
                filter: log_filter.into(),
                ..default()
            })
            // Allows --model to load a GLB from any filesystem path; file_path
            // points at the resolved assets dir (portable/system/dev layouts).
            .set(bevy::asset::AssetPlugin {
                unapproved_path_mode: bevy::asset::UnapprovedPathMode::Allow,
                file_path: asset_root.unwrap_or_else(|| "assets".into()),
                ..default()
            }),
    );
    app.add_plugins(bevy::pbr::wireframe::WireframePlugin::default());

    // Keep rendering continuously, focused or not (benchmark must not throttle).
    app.insert_resource(WinitSettings {
        focused_mode: UpdateMode::Continuous,
        unfocused_mode: UpdateMode::Continuous,
    });

    #[cfg(feature = "rt-experimental")]
    app.add_plugins(crate::rt::RtPlugin);

    app.add_plugins(FrameTimeDiagnosticsPlugin::default());
    // CPU+GPU per-render-pass timing; GPU spans appear only when the adapter
    // has TIMESTAMP_QUERY (bevy already requests all supported features).
    app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
    // Optional telemetry (spec §9.3): cpu/mem % + amdgpu sysfs, best-effort.
    app.add_plugins(bevy::diagnostic::SystemInformationDiagnosticsPlugin);
    app.init_resource::<crate::platform::telemetry::Telemetry>();
    app.add_systems(Update, crate::platform::telemetry::poll);
    app.add_plugins((
        crate::bench::BenchPlugin,
        crate::scene::ScenePlugin,
        crate::render_cfg::RenderCfgPlugin,
        crate::ui::UiPlugin,
    ));
    app.add_systems(Update, (save_dirty_settings, debug_screenshot));

    app.run()
}

/// Hidden dev aid: captures the scene to a PNG a few seconds in, then exits.
/// Also exercises the Screenshot plumbing the report generator reuses.
fn debug_screenshot(
    run: Res<RunOpts>,
    clock: Res<crate::scene::orbits::BenchClock>,
    metrics: Res<crate::scene::SceneMetrics>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
    mut ready_at: Local<Option<f64>>,
    mut taken: Local<bool>,
) {
    let Some(path) = run.0.debug_screenshot.clone() else { return };
    if metrics.ready && ready_at.is_none() {
        *ready_at = Some(clock.t);
    }
    match *ready_at {
        Some(t0) => {
            if clock.t > t0 + 2.0 && !*taken {
                *taken = true;
                use bevy::render::view::screenshot::{Screenshot, save_to_disk};
                commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
            }
            if clock.t > t0 + 4.0 {
                exit.write(AppExit::Success);
            }
        }
        // Scene never became ready (e.g. model failed to load): bail out.
        None if clock.t > 25.0 => {
            exit.write(AppExit::from_code(1));
        }
        None => {}
    }
}

fn main_window(s: &Settings) -> Window {
    let d = &s.display;
    let monitor = MonitorSelection::Index(d.monitor);
    Window {
        title: "Batista GPU Benchmark".into(),
        resolution: WindowResolution::new(d.width, d.height),
        present_mode: if d.vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync },
        mode: match d.window_mode {
            WindowModeSetting::Windowed => WindowMode::Windowed,
            WindowModeSetting::Borderless => WindowMode::BorderlessFullscreen(monitor),
            WindowModeSetting::Fullscreen => WindowMode::Fullscreen(monitor, VideoModeSelection::Current),
        },
        ..default()
    }
}

fn wgpu_settings(s: &Settings) -> WgpuSettings {
    let mut wgpu = WgpuSettings::default();
    wgpu.backends = Some(match s.renderer.backend {
        BackendSetting::Auto => wgpu.backends.unwrap_or(Backends::PRIMARY),
        BackendSetting::Vulkan => Backends::VULKAN,
        BackendSetting::Dx12 => Backends::DX12,
        BackendSetting::Metal => Backends::METAL,
        BackendSetting::Gl => Backends::GL,
    });
    wgpu.power_preference = match s.renderer.power_preference {
        PowerPrefSetting::Auto => wgpu.power_preference,
        PowerPrefSetting::LowPower => PowerPreference::LowPower,
        PowerPrefSetting::HighPerformance => PowerPreference::HighPerformance,
    };
    if s.renderer.adapter.is_some() {
        wgpu.adapter_name = s.renderer.adapter.clone();
    }
    wgpu
}

fn save_dirty_settings(mut io: ResMut<SettingsIo>, settings: Res<AppSettings>, time: Res<Time<Real>>) {
    let Some(at) = io.dirty_at else { return };
    if time.elapsed_secs_f64() - at < 0.5 {
        return;
    }
    io.dirty_at = None;
    match settings.0.save() {
        Ok(()) => io.last_save_error = None,
        Err(e) => io.last_save_error = Some(e.to_string()),
    }
}
