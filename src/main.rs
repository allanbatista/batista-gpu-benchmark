mod app;
mod bench;
mod config;
mod platform;
mod render_cfg;
#[cfg(feature = "rt-experimental")]
mod rt;
mod scene;
mod ui;

use bevy::app::AppExit;
use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = config::Cli::parse();

    if cli.list_adapters {
        return list_adapters();
    }

    let mut settings = config::Settings::load();
    let mut run = match config::merge_cli(&mut settings, &cli) {
        Ok(run) => run,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    run.asset_root = resolve_asset_root();

    // `--adapter <index>` → resolve to the adapter's name via a wgpu probe
    // (wgpu itself only selects by name substring).
    if let Some(index) = settings.renderer.adapter.as_ref().and_then(|a| a.parse::<usize>().ok()) {
        let adapters = enumerate_adapters(probe_backends(settings.renderer.backend));
        match adapters.get(index) {
            Some(info) => {
                println!("adapter [{index}] resolved to: {}", info.name);
                settings.renderer.adapter = Some(info.name.clone());
            }
            None => {
                eprintln!("error: adapter index {index} out of range ({} found — see --list-adapters)", adapters.len());
                return ExitCode::from(2);
            }
        }
    }

    install_panic_hook();

    match app::run(settings, run) {
        AppExit::Success => ExitCode::SUCCESS,
        AppExit::Error(code) => ExitCode::from(code.get()),
    }
}

/// Finds the assets dir across layouts: portable (next to the exe), FHS
/// installs (<prefix>/share/batista-gpu-benchmark/assets — deb/rpm/AppImage/
/// snap/flatpak) and the dev tree (cwd). None keeps bevy's default resolution.
fn resolve_asset_root() -> Option<String> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("assets"));
        candidates.push(dir.join("../share/batista-gpu-benchmark/assets"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("assets"));
    }
    candidates
        .into_iter()
        .find(|c| c.join("models/benchmark.glb").is_file())
        .map(|p| p.to_string_lossy().into_owned())
}

fn probe_backends(setting: config::BackendSetting) -> wgpu::Backends {
    match setting {
        config::BackendSetting::Auto => wgpu::Backends::PRIMARY | wgpu::Backends::GL,
        config::BackendSetting::Vulkan => wgpu::Backends::VULKAN,
        config::BackendSetting::Dx12 => wgpu::Backends::DX12,
        config::BackendSetting::Metal => wgpu::Backends::METAL,
        config::BackendSetting::Gl => wgpu::Backends::GL,
    }
}

fn enumerate_adapters(backends: wgpu::Backends) -> Vec<wgpu::AdapterInfo> {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = backends;
    let instance = wgpu::Instance::new(desc);
    bevy::tasks::block_on(instance.enumerate_adapters(backends))
        .iter()
        .map(|a| a.get_info())
        .collect()
}

fn list_adapters() -> ExitCode {
    let adapters = enumerate_adapters(wgpu::Backends::all());
    if adapters.is_empty() {
        eprintln!("no GPU adapters found");
        return ExitCode::from(1);
    }
    println!("{} adapter(s):", adapters.len());
    for (i, info) in adapters.iter().enumerate() {
        println!(
            "[{i}] {} — {:?} ({:?}) driver: {} {}",
            info.name, info.backend, info.device_type, info.driver, info.driver_info
        );
    }
    ExitCode::SUCCESS
}

/// Friendly message for unrecoverable graphics failures (spec §16): bevy panics
/// inside the render plugin when no adapter/backend is usable.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        let lower = msg.to_lowercase();
        if ["adapter", "gpu", "backend", "surface", "device"].iter().any(|k| lower.contains(k)) {
            eprintln!();
            eprintln!("Graphics initialization failed.");
            eprintln!("  - Try `--backend auto` (or a different --backend)");
            eprintln!("  - Remove any --adapter filter");
            eprintln!("  - Update your GPU drivers");
        }
    }));
}
