//! Settings side panel (spec §7.2–§7.6), hidden while benchmarking.

use bevy::prelude::*;
use bevy::window::Monitor;
use bevy_egui::{EguiContexts, egui};

use crate::app::{AppSettings, SettingsIo, StartupRenderCfg};
use crate::bench::{BenchState, LastError};
use crate::config::{
    self, AaMode, BackendSetting, PowerPrefSetting, PresetId, RESOLUTION_PRESETS, Settings,
    TonemappingSetting, WindowModeSetting,
};
use crate::render_cfg::Capabilities;
use crate::ui::system_info::{self, SystemInfo};

#[allow(clippy::too_many_arguments)]
pub fn panels_ui(
    mut contexts: EguiContexts,
    mut settings: ResMut<AppSettings>,
    mut io: ResMut<SettingsIo>,
    startup: Res<StartupRenderCfg>,
    caps: Res<Capabilities>,
    state: Res<State<BenchState>>,
    last_error: Res<LastError>,
    sysinfo: Res<SystemInfo>,
    monitors: Query<&Monitor>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if state.get().is_benchmarking() {
        return Ok(());
    }
    let mut root = crate::ui::root_ui(ctx, "panels-root");

    if let Some(err) = &last_error.0 {
        egui::Panel::top("error-banner").show(&mut root, |ui| {
            ui.colored_label(egui::Color32::from_rgb(255, 90, 90), format!("⚠ {err}"));
        });
    }

    let mut edited = settings.0.clone();

    egui::Panel::right("settings-panel")
        .default_size(380.0)
        .show(&mut root, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Batista GPU Benchmark");
                ui.add_space(4.0);

                section(ui, "Display", |ui| display_section(ui, &mut edited, &monitors));
                section(ui, "Renderer", |ui| renderer_section(ui, &mut edited, &caps));
                section(ui, "Scene", |ui| scene_section(ui, &mut edited));
                section(ui, "Benchmark", |ui| benchmark_section(ui, &mut edited));
                section(ui, "System info", |ui| system_info::draw(ui, &sysinfo, &caps));

                if let Some(e) = &io.last_save_error {
                    ui.colored_label(egui::Color32::YELLOW, format!("settings save failed: {e}"));
                }
            });
        });

    // Anything managed by a preset was hand-edited → the run becomes Custom.
    if edited.benchmark.preset != PresetId::Custom && !preset_matches(&edited) {
        edited.benchmark.preset = PresetId::Custom;
    }

    let restart_needed = edited.renderer.backend != startup.backend
        || edited.renderer.adapter != startup.adapter
        || edited.renderer.power_preference != startup.power;
    if restart_needed {
        egui::Panel::bottom("restart-bar").show(&mut root, |ui| {
            ui.horizontal(|ui| {
                ui.label("Graphics API / adapter changes require a restart.");
                if ui.button("Apply & Restart").clicked() {
                    let _ = edited.save();
                    match crate::platform::respawn() {
                        Ok(()) => {
                            exit.write(AppExit::Success);
                        }
                        Err(e) => {
                            io.last_save_error =
                                Some(format!("auto-restart failed ({e}); please restart manually"));
                        }
                    }
                }
            });
        });
    }

    if edited != settings.0 {
        settings.0 = edited;
        io.dirty_at = Some(time.elapsed_secs_f64());
    }
    Ok(())
}

fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(title).default_open(true).show(ui, add);
    ui.add_space(2.0);
}

fn display_section(ui: &mut egui::Ui, s: &mut Settings, monitors: &Query<&Monitor>) {
    let d = &mut s.display;

    let current = format!("{}x{}", d.width, d.height);
    let mut selected = (d.width, d.height);
    egui::ComboBox::from_label("Resolution")
        .selected_text(&current)
        .show_ui(ui, |ui| {
            for (w, h) in RESOLUTION_PRESETS {
                ui.selectable_value(&mut selected, (w, h), format!("{w}x{h}"));
            }
            if let Some(m) = monitors.iter().next() {
                let native = (m.physical_width, m.physical_height);
                ui.selectable_value(&mut selected, native, format!("{}x{} (native)", native.0, native.1));
            }
        });
    (d.width, d.height) = selected;
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut d.width).range(320..=7680).prefix("w "));
        ui.add(egui::DragValue::new(&mut d.height).range(240..=4320).prefix("h "));
    });

    egui::ComboBox::from_label("Window mode")
        .selected_text(format!("{:?}", d.window_mode))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut d.window_mode, WindowModeSetting::Windowed, "Windowed");
            ui.selectable_value(&mut d.window_mode, WindowModeSetting::Borderless, "Borderless fullscreen");
            ui.selectable_value(&mut d.window_mode, WindowModeSetting::Fullscreen, "Exclusive fullscreen");
        });

    let monitor_count = monitors.iter().count();
    if monitor_count > 1 {
        egui::ComboBox::from_label("Monitor")
            .selected_text(format!("#{}", d.monitor))
            .show_ui(ui, |ui| {
                for (i, m) in monitors.iter().enumerate() {
                    let name = m.name.clone().unwrap_or_else(|| format!("Monitor {i}"));
                    ui.selectable_value(&mut d.monitor, i, name);
                }
            });
    }
    if let Some(m) = monitors.iter().nth(d.monitor.min(monitor_count.saturating_sub(1))) {
        if let Some(mhz) = m.refresh_rate_millihertz {
            ui.label(format!("Detected refresh rate: {:.0} Hz", mhz as f64 / 1000.0));
        }
    }

    ui.checkbox(&mut d.vsync, "VSync");
    ui.horizontal(|ui| {
        ui.label("FPS limit (0 = off)");
        ui.add(egui::DragValue::new(&mut d.fps_limit).range(0..=1000));
    });
    ui.label(egui::RichText::new("HDR display output: unavailable (wgpu has no HDR swapchain)").weak());
}

fn renderer_section(ui: &mut egui::Ui, s: &mut Settings, caps: &Capabilities) {
    let r = &mut s.renderer;

    egui::ComboBox::from_label("Graphics API")
        .selected_text(r.backend.label())
        .show_ui(ui, |ui| {
            for b in BackendSetting::all() {
                let enabled = b.available_on_this_os();
                ui.add_enabled_ui(enabled, |ui| {
                    ui.selectable_value(&mut r.backend, b, b.label())
                        .on_disabled_hover_text("Not available on this OS");
                });
            }
        });
    if caps.ready {
        ui.label(format!("Active API: {} — {}", caps.backend, caps.adapter_name));
    }

    egui::ComboBox::from_label("Power preference")
        .selected_text(format!("{:?}", r.power_preference))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut r.power_preference, PowerPrefSetting::Auto, "Auto");
            ui.selectable_value(&mut r.power_preference, PowerPrefSetting::LowPower, "Low power");
            ui.selectable_value(&mut r.power_preference, PowerPrefSetting::HighPerformance, "High performance");
        });

    let mut adapter = r.adapter.clone().unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label("Adapter filter");
        if ui.text_edit_singleline(&mut adapter).changed() {
            r.adapter = if adapter.trim().is_empty() { None } else { Some(adapter.trim().to_string()) };
        }
    });

    egui::ComboBox::from_label("Anti-aliasing")
        .selected_text(r.aa.label())
        .show_ui(ui, |ui| {
            for mode in AaMode::all() {
                let (enabled, reason) = aa_available(mode, caps);
                ui.add_enabled_ui(enabled, |ui| {
                    ui.selectable_value(&mut r.aa, mode, mode.label())
                        .on_disabled_hover_text(reason);
                });
            }
        });

    ui.add(egui::Slider::new(&mut r.render_scale, 0.25..=1.0).text("Render scale"));
    ui.add(egui::Slider::new(&mut r.bloom, 0.0..=0.5).text("Bloom intensity (0 = off)"));
    if r.bloom > 0.0 {
        r.hdr = true; // bloom requires an HDR render target
    }
    ui.checkbox(&mut r.hdr, "HDR rendering (internal)");

    egui::ComboBox::from_label("Tonemapping")
        .selected_text(r.tonemapping.label())
        .show_ui(ui, |ui| {
            for t in TonemappingSetting::all() {
                ui.selectable_value(&mut r.tonemapping, t, t.label());
            }
        });

    ui.checkbox(&mut r.shadows, "Shadows");
    egui::ComboBox::from_label("Shadow map size")
        .selected_text(r.shadow_map_size.to_string())
        .show_ui(ui, |ui| {
            for size in [1024u32, 2048, 4096] {
                ui.selectable_value(&mut r.shadow_map_size, size, size.to_string());
            }
        });

    ui.separator();
    ui.add_enabled_ui(caps.wireframe || !caps.ready, |ui| {
        ui.checkbox(&mut s.renderer.wireframe, "Wireframe (debug)")
            .on_disabled_hover_text("POLYGON_MODE_LINE not supported by this adapter");
    });
    ui.checkbox(&mut s.renderer.show_aabb, "Show bounding boxes (debug)");
}

fn aa_available(mode: AaMode, caps: &Capabilities) -> (bool, &'static str) {
    match mode {
        AaMode::Msaa8 if caps.ready && caps.max_msaa < 8 => (false, "MSAA 8x not supported by this adapter"),
        AaMode::Dlss => {
            if cfg!(feature = "dlss") {
                (caps.dlss_supported, "Requires an NVIDIA RTX GPU on Vulkan")
            } else {
                (false, "This build was compiled without DLSS support")
            }
        }
        AaMode::Fsr1 => (false, "FSR 1.0 upscaler lands with the advanced feature set"),
        _ => (true, ""),
    }
}

fn scene_section(ui: &mut egui::Ui, s: &mut Settings) {
    let sc = &mut s.scene;
    ui.add(egui::Slider::new(&mut sc.light_count, 1..=64).text("Lights"));
    ui.add(egui::Slider::new(&mut sc.shadow_caster_count, 0..=10).text("Shadow-casting lights"));
    ui.checkbox(&mut sc.directional_light, "Directional light");
    ui.checkbox(&mut sc.point_shadows, "Point light shadows");
    ui.checkbox(&mut sc.spot_shadows, "Spot light shadows");
    ui.add(egui::Slider::new(&mut sc.light_intensity, 0.1..=3.0).text("Light intensity ×"));
    ui.add(egui::Slider::new(&mut sc.light_speed, 0.1..=3.0).text("Light speed ×"));
    ui.add(egui::Slider::new(&mut sc.camera_speed, 0.1..=3.0).text("Camera speed ×"));
    ui.add(egui::Slider::new(&mut sc.camera_distance, 0.5..=3.0).text("Camera distance ×"));
    ui.checkbox(&mut sc.animate_model, "Model animation (if present)");
    ui.horizontal(|ui| {
        ui.label("Seed");
        ui.add(egui::DragValue::new(&mut sc.seed));
    });
    match &sc.model {
        Some(path) => {
            ui.label(format!("Model: {path}"));
            if ui.button("Use bundled model").clicked() {
                sc.model = None;
            }
        }
        None => {
            ui.label("Model: bundled benchmark.glb (pass --model <path> to override)");
        }
    }
}

fn benchmark_section(ui: &mut egui::Ui, s: &mut Settings) {
    let before = s.benchmark.preset;
    egui::ComboBox::from_label("Preset")
        .selected_text(format!("{:?}", s.benchmark.preset))
        .show_ui(ui, |ui| {
            for p in [PresetId::Low, PresetId::Medium, PresetId::High, PresetId::Extreme, PresetId::Custom] {
                ui.selectable_value(&mut s.benchmark.preset, p, format!("{p:?}"));
            }
        });
    if s.benchmark.preset != before && s.benchmark.preset != PresetId::Custom {
        config::apply_preset(s, s.benchmark.preset);
    }
    ui.label(format!("Workload id: {}", config::preset_label(s)));

    let b = &mut s.benchmark;
    ui.horizontal(|ui| {
        ui.label("Warmup (s)");
        ui.add(egui::DragValue::new(&mut b.warmup_s).range(0.0..=120.0));
        ui.label("Duration (s)");
        ui.add(egui::DragValue::new(&mut b.duration_s).range(1.0..=600.0));
    });
    ui.horizontal(|ui| {
        ui.label("Runs");
        ui.add(egui::DragValue::new(&mut b.runs).range(1..=20));
        ui.label("Interval (s)");
        ui.add(egui::DragValue::new(&mut b.interval_s).range(0.0..=60.0));
    });
    ui.checkbox(&mut b.screenshot, "Save final-frame screenshot");

    ui.horizontal(|ui| {
        ui.add_enabled_ui(false, |ui| {
            let _ = ui.button("▶ Start benchmark").on_disabled_hover_text("Benchmark runner lands in F3");
        });
        if ui.button("Open results dir").clicked() {
            let _ = std::fs::create_dir_all(&b.output_dir);
            crate::platform::open_dir(std::path::Path::new(&b.output_dir));
        }
    });

    let deviations = config::official_deviations(s);
    if deviations.is_empty() {
        ui.label(egui::RichText::new("✔ Official comparable profile").color(egui::Color32::LIGHT_GREEN));
    } else {
        ui.label(egui::RichText::new(format!("Custom profile ({} deviations)", deviations.len())).weak())
            .on_hover_text(deviations.join("\n"));
    }
}

/// True when the preset-managed fields still match the selected preset's definition.
fn preset_matches(s: &Settings) -> bool {
    let Some(def) = config::preset_def(s.benchmark.preset) else {
        return true;
    };
    s.scene.light_count == def.lights
        && s.scene.shadow_caster_count == def.shadow_casters
        && s.scene.directional_light == def.directional_light
        && s.renderer.aa == def.aa
        && s.renderer.bloom == def.bloom
        && s.renderer.shadow_map_size == def.shadow_map_size
}
