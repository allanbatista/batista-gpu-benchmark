//! Settings model, quality presets, official profile, TOML persistence and CLI merging.

use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SETTINGS_PATH: &str = "config/settings.toml";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Kept in sync with Cargo.toml manually (bevy provides no version constant).
pub const BEVY_VERSION: &str = "0.19";
pub const SCORE_ALGORITHM: &str = "gpu-benchmark-score-v1";
pub const OFFICIAL_PROFILE: &str = "official-profile-v1";
pub const DEFAULT_SEED: u32 = 42;
pub const DEFAULT_MODEL: &str = "models/benchmark.glb";

// ---------------------------------------------------------------------------
// Enums shared by settings + CLI
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "lowercase")]
pub enum BackendSetting {
    #[default]
    Auto,
    Vulkan,
    Dx12,
    Metal,
    Gl,
}

impl BackendSetting {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Vulkan => "Vulkan",
            Self::Dx12 => "DirectX 12",
            Self::Metal => "Metal",
            Self::Gl => "OpenGL/GLES",
        }
    }

    /// Backends selectable on the current OS (spec §3).
    pub fn available_on_this_os(self) -> bool {
        match self {
            Self::Auto => true,
            Self::Vulkan => cfg!(any(target_os = "linux", target_os = "windows")),
            Self::Dx12 => cfg!(target_os = "windows"),
            Self::Metal => cfg!(target_os = "macos"),
            Self::Gl => cfg!(any(target_os = "linux", target_os = "windows")),
        }
    }

    pub fn all() -> [Self; 5] {
        [Self::Auto, Self::Vulkan, Self::Dx12, Self::Metal, Self::Gl]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "lowercase")]
pub enum WindowModeSetting {
    #[default]
    Windowed,
    Borderless,
    Fullscreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PowerPrefSetting {
    #[default]
    Auto,
    LowPower,
    HighPerformance,
}

/// Anti-aliasing selector (native-resolution AA only; upscalers are a
/// separate axis — see [`UpscalerSetting`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AaMode {
    Off,
    Msaa2,
    #[default]
    Msaa4,
    Msaa8,
    Fxaa,
    Smaa,
    Taa,
}

impl AaMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Msaa2 => "MSAA 2x",
            Self::Msaa4 => "MSAA 4x",
            Self::Msaa8 => "MSAA 8x",
            Self::Fxaa => "FXAA",
            Self::Smaa => "SMAA",
            Self::Taa => "TAA",
        }
    }

    pub fn all() -> [Self; 7] {
        [Self::Off, Self::Msaa2, Self::Msaa4, Self::Msaa8, Self::Fxaa, Self::Smaa, Self::Taa]
    }
}

/// Upscaler selector, independent from anti-aliasing. FSR 1.0 is spatial and
/// composes with any AA mode; DLSS is temporal and replaces AA entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UpscalerSetting {
    #[default]
    Off,
    Fsr1,
    Dlss,
}

impl UpscalerSetting {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off (native)",
            Self::Fsr1 => "FSR 1.0",
            Self::Dlss => "DLSS",
        }
    }

    pub fn all() -> [Self; 3] {
        [Self::Off, Self::Fsr1, Self::Dlss]
    }
}

/// Rendering mode (spec §4): rasterized PBR is the official mode; ray tracing
/// is experimental (bevy_solari), never comparable, and requires an RT build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RenderModeSetting {
    #[default]
    Pbr,
    RtExperimental,
}

/// FSR 1.0 official per-dimension scale factors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Fsr1Quality {
    UltraQuality,
    #[default]
    Quality,
    Balanced,
    Performance,
}

impl Fsr1Quality {
    pub fn render_scale(self) -> f32 {
        match self {
            Self::UltraQuality => 1.0 / 1.3,
            Self::Quality => 1.0 / 1.5,
            Self::Balanced => 1.0 / 1.7,
            Self::Performance => 1.0 / 2.0,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::UltraQuality => "Ultra Quality (1.3x)",
            Self::Quality => "Quality (1.5x)",
            Self::Balanced => "Balanced (1.7x)",
            Self::Performance => "Performance (2.0x)",
        }
    }
    pub fn all() -> [Self; 4] {
        [Self::UltraQuality, Self::Quality, Self::Balanced, Self::Performance]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TonemappingSetting {
    None,
    Reinhard,
    AcesFitted,
    AgX,
    #[default]
    TonyMcMapface,
    BlenderFilmic,
    KhronosPbrNeutral,
}

impl TonemappingSetting {
    pub fn all() -> [Self; 7] {
        [
            Self::None,
            Self::Reinhard,
            Self::AcesFitted,
            Self::AgX,
            Self::TonyMcMapface,
            Self::BlenderFilmic,
            Self::KhronosPbrNeutral,
        ]
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Reinhard => "Reinhard",
            Self::AcesFitted => "ACES Fitted",
            Self::AgX => "AgX",
            Self::TonyMcMapface => "Tony McMapface",
            Self::BlenderFilmic => "Blender Filmic",
            Self::KhronosPbrNeutral => "Khronos PBR Neutral",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "lowercase")]
pub enum PresetId {
    Low,
    Medium,
    #[default]
    High,
    Extreme,
    Custom,
}

// ---------------------------------------------------------------------------
// Settings sections
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplaySettings {
    pub monitor: usize,
    pub width: u32,
    pub height: u32,
    pub window_mode: WindowModeSetting,
    pub vsync: bool,
    /// 0 = uncapped.
    pub fps_limit: u32,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            monitor: 0,
            width: 1920,
            height: 1080,
            window_mode: WindowModeSetting::Windowed,
            vsync: false,
            fps_limit: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RendererSettings {
    pub backend: BackendSetting,
    /// Adapter name substring (wgpu has no index-based selection).
    pub adapter: Option<String>,
    pub power_preference: PowerPrefSetting,
    pub render_mode: RenderModeSetting,
    pub aa: AaMode,
    pub upscaler: UpscalerSetting,
    pub fsr1_quality: Fsr1Quality,
    /// Internal 3D render scale, (0, 1]. 1.0 = native.
    pub render_scale: f32,
    pub shadows: bool,
    pub shadow_map_size: u32,
    /// 0.0 = bloom off.
    pub bloom: f32,
    pub tonemapping: TonemappingSetting,
    /// Internal HDR render target (required by bloom). Not display HDR.
    pub hdr: bool,
    pub wireframe: bool,
    pub show_aabb: bool,
}

impl Default for RendererSettings {
    fn default() -> Self {
        Self {
            backend: BackendSetting::Auto,
            adapter: None,
            power_preference: PowerPrefSetting::Auto,
            render_mode: RenderModeSetting::Pbr,
            aa: AaMode::Msaa4,
            upscaler: UpscalerSetting::Off,
            fsr1_quality: Fsr1Quality::Quality,
            render_scale: 1.0,
            shadows: true,
            shadow_map_size: 2048,
            bloom: 0.15,
            tonemapping: TonemappingSetting::TonyMcMapface,
            hdr: true,
            wireframe: false,
            show_aabb: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneSettings {
    /// None = bundled assets/models/benchmark.glb; Some = external file path.
    pub model: Option<String>,
    pub light_count: u32,
    pub shadow_caster_count: u32,
    pub directional_light: bool,
    /// Global multipliers applied on top of preset base values.
    pub light_intensity: f32,
    pub light_speed: f32,
    pub camera_speed: f32,
    pub camera_distance: f32,
    pub animate_model: bool,
    pub point_shadows: bool,
    pub spot_shadows: bool,
    pub seed: u32,
}

impl Default for SceneSettings {
    fn default() -> Self {
        Self {
            model: None,
            light_count: 16,
            shadow_caster_count: 4,
            directional_light: false,
            light_intensity: 1.0,
            light_speed: 1.0,
            camera_speed: 1.0,
            camera_distance: 1.0,
            animate_model: false,
            point_shadows: true,
            spot_shadows: true,
            seed: DEFAULT_SEED,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BenchmarkSettings {
    pub preset: PresetId,
    pub warmup_s: f64,
    pub duration_s: f64,
    pub runs: u32,
    pub interval_s: f64,
    pub screenshot: bool,
    pub output_dir: String,
    /// Variation threshold (%) above which the consolidated result is flagged unstable.
    pub unstable_threshold_pct: f64,
}

impl Default for BenchmarkSettings {
    fn default() -> Self {
        Self {
            preset: PresetId::High,
            warmup_s: 10.0,
            duration_s: 60.0,
            runs: 3,
            interval_s: 5.0,
            screenshot: true,
            output_dir: "results".into(),
            unstable_threshold_pct: 5.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub display: DisplaySettings,
    pub renderer: RendererSettings,
    pub scene: SceneSettings,
    pub benchmark: BenchmarkSettings,
}

/// Settings location: the spec's `./config/settings.toml` when running from a
/// dev/portable layout; the user config dir (XDG / APPDATA) for system installs
/// (deb/rpm/AppImage/snap/flatpak), where the cwd is not ours.
pub fn settings_path() -> PathBuf {
    if Path::new("Cargo.toml").is_file() || Path::new("config").is_dir() {
        return PathBuf::from(SETTINGS_PATH);
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("batista-gpu-benchmark").join("settings.toml")
}

impl Settings {
    pub fn load() -> Self {
        let path = settings_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("warning: invalid {} ({e}); using defaults", path.display());
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = settings_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).expect("settings serialize");
        std::fs::write(path, text)
    }
}

// ---------------------------------------------------------------------------
// Presets (versioned — bump the version when values change; spec §6)
// ---------------------------------------------------------------------------

pub struct PresetDef {
    pub id: PresetId,
    pub version: &'static str,
    pub lights: u32,
    pub shadow_casters: u32,
    pub directional_light: bool,
    pub aa: AaMode,
    pub bloom: f32,
    pub shadow_map_size: u32,
}

pub const PRESETS: [PresetDef; 4] = [
    PresetDef {
        id: PresetId::Low,
        version: "low-v1",
        lights: 4,
        shadow_casters: 0,
        directional_light: false,
        aa: AaMode::Off,
        bloom: 0.0,
        shadow_map_size: 1024,
    },
    PresetDef {
        id: PresetId::Medium,
        version: "medium-v1",
        lights: 8,
        shadow_casters: 2,
        directional_light: false,
        aa: AaMode::Msaa2,
        bloom: 0.10,
        shadow_map_size: 2048,
    },
    PresetDef {
        id: PresetId::High,
        version: "high-v1",
        lights: 16,
        shadow_casters: 4,
        directional_light: false,
        aa: AaMode::Msaa4,
        bloom: 0.15,
        shadow_map_size: 2048,
    },
    PresetDef {
        id: PresetId::Extreme,
        version: "extreme-v1",
        lights: 32,
        shadow_casters: 8,
        directional_light: true,
        aa: AaMode::Msaa8,
        bloom: 0.20,
        shadow_map_size: 4096,
    },
];

pub fn preset_def(id: PresetId) -> Option<&'static PresetDef> {
    PRESETS.iter().find(|p| p.id == id)
}

/// Preset label for reports, e.g. "high-v1" or "custom".
pub fn preset_label(settings: &Settings) -> String {
    match preset_def(settings.benchmark.preset) {
        Some(def) => def.version.to_string(),
        None => "custom".to_string(),
    }
}

/// Applies a preset's values onto the settings (no-op for Custom).
pub fn apply_preset(settings: &mut Settings, id: PresetId) {
    settings.benchmark.preset = id;
    let Some(def) = preset_def(id) else { return };
    settings.scene.light_count = def.lights;
    settings.scene.shadow_caster_count = def.shadow_casters;
    settings.scene.directional_light = def.directional_light;
    settings.renderer.aa = def.aa;
    settings.renderer.bloom = def.bloom;
    settings.renderer.shadow_map_size = def.shadow_map_size;
    settings.renderer.shadows = def.shadow_casters > 0 || def.directional_light;
}

/// Deviations from the official comparable profile (spec §14). Empty = official.
pub fn official_deviations(s: &Settings) -> Vec<String> {
    let mut dev = Vec::new();
    let Some(def) = preset_def(s.benchmark.preset) else {
        return vec!["preset=custom".into()];
    };
    if s.display.vsync {
        dev.push("vsync=on".into());
    }
    if s.display.fps_limit != 0 {
        dev.push(format!("fps_limit={}", s.display.fps_limit));
    }
    if s.renderer.render_scale != 1.0 {
        dev.push(format!("render_scale={}", s.renderer.render_scale));
    }
    if s.renderer.aa != def.aa {
        dev.push(format!("aa_mode={:?}", s.renderer.aa));
    }
    if s.renderer.upscaler != UpscalerSetting::Off {
        dev.push(format!("upscaler={:?}", s.renderer.upscaler));
    }
    if s.renderer.bloom != def.bloom {
        dev.push(format!("bloom={}", s.renderer.bloom));
    }
    if s.renderer.shadow_map_size != def.shadow_map_size {
        dev.push(format!("shadow_map_size={}", s.renderer.shadow_map_size));
    }
    if s.renderer.wireframe {
        dev.push("wireframe=on".into());
    }
    if s.renderer.render_mode != RenderModeSetting::Pbr {
        dev.push("render_mode=rt-experimental".into());
    }
    if s.scene.seed != DEFAULT_SEED {
        dev.push(format!("seed={}", s.scene.seed));
    }
    if s.scene.model.is_some() {
        dev.push("model=custom".into());
    }
    if s.scene.light_count != def.lights
        || s.scene.shadow_caster_count != def.shadow_casters
        || s.scene.directional_light != def.directional_light
    {
        dev.push("lights=custom".into());
    }
    if (s.scene.light_intensity, s.scene.light_speed, s.scene.camera_speed, s.scene.camera_distance)
        != (1.0, 1.0, 1.0, 1.0)
    {
        dev.push("scene_multipliers=custom".into());
    }
    if s.scene.animate_model {
        dev.push("animation=on".into());
    }
    if s.benchmark.warmup_s != 10.0 {
        dev.push(format!("warmup={}", s.benchmark.warmup_s));
    }
    if s.benchmark.duration_s != 60.0 {
        dev.push(format!("duration={}", s.benchmark.duration_s));
    }
    if s.benchmark.runs != 3 {
        dev.push(format!("runs={}", s.benchmark.runs));
    }
    if cfg!(debug_assertions) {
        dev.push("debug_build".into());
    }
    dev
}

// ---------------------------------------------------------------------------
// CLI (spec §13)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OnOff {
    On,
    Off,
}

#[derive(Parser, Debug)]
#[command(name = "batista-gpu-benchmark", version, about = "Cross-platform GPU benchmark (Bevy + wgpu)")]
pub struct Cli {
    /// Start the benchmark automatically on launch.
    #[arg(long)]
    pub benchmark: bool,
    #[arg(long, value_enum)]
    pub preset: Option<PresetId>,
    #[arg(long, value_enum)]
    pub backend: Option<BackendSetting>,
    /// Adapter index (see --list-adapters) or name substring.
    #[arg(long)]
    pub adapter: Option<String>,
    /// Window resolution, e.g. 1920x1080.
    #[arg(long)]
    pub resolution: Option<String>,
    /// Internal 3D render scale, (0, 1].
    #[arg(long)]
    pub render_scale: Option<f32>,
    #[arg(long, value_enum)]
    pub window_mode: Option<WindowModeSetting>,
    #[arg(long, value_enum)]
    pub vsync: Option<OnOff>,
    /// Warmup seconds before each measured run.
    #[arg(long)]
    pub warmup: Option<f64>,
    /// Measured duration in seconds per run.
    #[arg(long)]
    pub duration: Option<f64>,
    /// Number of benchmark repetitions.
    #[arg(long)]
    pub runs: Option<u32>,
    #[arg(long)]
    pub seed: Option<u32>,
    /// Path to a GLB model replacing the bundled one.
    #[arg(long)]
    pub model: Option<PathBuf>,
    /// Output directory for results.
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[arg(long)]
    pub exit_after_benchmark: bool,
    /// List GPU adapters and exit.
    #[arg(long)]
    pub list_adapters: bool,
    /// Dev aid: save a scene screenshot to this path after a few seconds and exit.
    #[arg(long, hide = true)]
    pub debug_screenshot: Option<PathBuf>,
}

/// Runtime options that are not persisted in settings.toml.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub autostart: bool,
    pub exit_after: bool,
    /// True when any CLI flag overrode persisted settings (report marks source).
    pub cli_overrides: bool,
    pub debug_screenshot: Option<PathBuf>,
    /// Absolute assets dir resolved at startup (system installs put assets in
    /// <prefix>/share/batista-gpu-benchmark/assets). None = bevy's default.
    pub asset_root: Option<String>,
}

/// Merges CLI args over loaded settings. Errors are returned as user-facing strings.
pub fn merge_cli(settings: &mut Settings, cli: &Cli) -> Result<RunOptions, String> {
    let mut overrode = false;

    if let Some(backend) = cli.backend {
        if !backend.available_on_this_os() {
            return Err(format!(
                "backend '{}' is not available on {}",
                backend.label(),
                std::env::consts::OS
            ));
        }
        if backend == BackendSetting::Gl {
            // Bevy 0.19 cannot create a GL surface (its surface fallback finds
            // zero present modes) — fail clearly instead of panicking mid-init.
            return Err(
                "the OpenGL/GLES backend is not supported by Bevy 0.19 (the engine cannot \
                 create a GL surface); use --backend vulkan or --backend auto"
                    .into(),
            );
        }
        settings.renderer.backend = backend;
        overrode = true;
    }
    if let Some(preset) = cli.preset {
        apply_preset(settings, preset);
        overrode = true;
    }
    if let Some(adapter) = &cli.adapter {
        settings.renderer.adapter = Some(adapter.clone());
        overrode = true;
    }
    if let Some(res) = &cli.resolution {
        let (w, h) = parse_resolution(res)?;
        settings.display.width = w;
        settings.display.height = h;
        overrode = true;
    }
    if let Some(scale) = cli.render_scale {
        if !(scale > 0.0 && scale <= 1.0) {
            return Err(format!(
                "render scale {scale} out of range (0, 1] — supersampling (>1.0) is not supported"
            ));
        }
        settings.renderer.render_scale = scale;
        overrode = true;
    }
    if let Some(mode) = cli.window_mode {
        settings.display.window_mode = mode;
        overrode = true;
    }
    if let Some(vsync) = cli.vsync {
        settings.display.vsync = vsync == OnOff::On;
        overrode = true;
    }
    if let Some(warmup) = cli.warmup {
        if warmup < 0.0 {
            return Err("warmup must be >= 0".into());
        }
        settings.benchmark.warmup_s = warmup;
        overrode = true;
    }
    if let Some(duration) = cli.duration {
        if duration <= 0.0 {
            return Err("duration must be > 0".into());
        }
        settings.benchmark.duration_s = duration;
        overrode = true;
    }
    if let Some(runs) = cli.runs {
        if runs == 0 {
            return Err("runs must be >= 1".into());
        }
        settings.benchmark.runs = runs;
        overrode = true;
    }
    if let Some(seed) = cli.seed {
        settings.scene.seed = seed;
        overrode = true;
    }
    if let Some(model) = &cli.model {
        if !model.is_file() {
            return Err(format!("model file not found: {}", model.display()));
        }
        settings.scene.model = Some(model.display().to_string());
        overrode = true;
    }
    if let Some(output) = &cli.output {
        settings.benchmark.output_dir = output.display().to_string();
        overrode = true;
    }

    Ok(RunOptions {
        autostart: cli.benchmark,
        exit_after: cli.exit_after_benchmark,
        cli_overrides: overrode,
        debug_screenshot: cli.debug_screenshot.clone(),
        asset_root: None, // resolved later in main
    })
}

fn parse_resolution(text: &str) -> Result<(u32, u32), String> {
    let (w, h) = text
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("invalid resolution '{text}', expected WIDTHxHEIGHT"))?;
    let w: u32 = w.trim().parse().map_err(|_| format!("invalid width in '{text}'"))?;
    let h: u32 = h.trim().parse().map_err(|_| format!("invalid height in '{text}'"))?;
    if w == 0 || h == 0 {
        return Err("resolution must be positive".into());
    }
    Ok((w, h))
}

pub const RESOLUTION_PRESETS: [(u32, u32); 4] = [(1280, 720), (1920, 1080), (2560, 1440), (3840, 2160)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_toml_roundtrip() {
        let mut s = Settings::default();
        s.renderer.aa = AaMode::Taa;
        s.scene.seed = 7;
        s.benchmark.runs = 5;
        let text = toml::to_string_pretty(&s).unwrap();
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn resolution_parsing() {
        assert_eq!(parse_resolution("1920x1080").unwrap(), (1920, 1080));
        assert_eq!(parse_resolution("3840X2160").unwrap(), (3840, 2160));
        assert!(parse_resolution("1920").is_err());
        assert!(parse_resolution("0x100").is_err());
    }

    #[test]
    fn preset_apply_and_official() {
        let mut s = Settings::default();
        apply_preset(&mut s, PresetId::High);
        let dev = official_deviations(&s);
        // debug builds always deviate; ignore that entry for the assertion
        let dev: Vec<_> = dev.into_iter().filter(|d| d != "debug_build").collect();
        assert!(dev.is_empty(), "unexpected deviations: {dev:?}");

        s.display.vsync = true;
        s.renderer.upscaler = UpscalerSetting::Fsr1;
        let dev = official_deviations(&s);
        assert!(dev.iter().any(|d| d == "vsync=on"));
        assert!(dev.iter().any(|d| d.starts_with("upscaler")));
    }

    #[test]
    fn cli_merge_backend_validation() {
        let mut s = Settings::default();
        let cli = Cli::parse_from(["x", "--backend", "metal"]);
        let err = merge_cli(&mut s, &cli);
        if cfg!(target_os = "macos") {
            assert!(err.is_ok());
        } else {
            assert!(err.is_err());
        }
    }

    #[test]
    fn cli_render_scale_range() {
        let mut s = Settings::default();
        let cli = Cli::parse_from(["x", "--render-scale", "1.5"]);
        assert!(merge_cli(&mut s, &cli).is_err());
        let cli = Cli::parse_from(["x", "--render-scale", "0.5"]);
        assert!(merge_cli(&mut s, &cli).is_ok());
    }
}
