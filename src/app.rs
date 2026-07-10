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
}

/// Debounced settings persistence state.
#[derive(Resource, Default)]
pub struct SettingsIo {
    pub dirty_at: Option<f64>,
    pub last_save_error: Option<String>,
}

pub fn run(settings: Settings, run: RunOptions) -> AppExit {
    let mut app = App::new();

    // Reduced logging during automated benchmark runs (spec §14/§17).
    let log_filter = if run.autostart {
        "error,batista_gpu_benchmark=warn"
    } else {
        "info,wgpu=error,naga=warn,bevy_render=info"
    };

    app.insert_resource(StartupRenderCfg {
        backend: settings.renderer.backend,
        adapter: settings.renderer.adapter.clone(),
        power: settings.renderer.power_preference,
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
            }),
    );

    // Keep rendering continuously, focused or not (benchmark must not throttle).
    app.insert_resource(WinitSettings {
        focused_mode: UpdateMode::Continuous,
        unfocused_mode: UpdateMode::Continuous,
    });

    app.add_plugins(FrameTimeDiagnosticsPlugin::default());
    app.add_plugins((crate::bench::BenchPlugin, crate::render_cfg::RenderCfgPlugin, crate::ui::UiPlugin));
    app.add_systems(Update, save_dirty_settings);

    app.run()
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
