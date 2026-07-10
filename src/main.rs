mod app;
mod bench;
mod config;
mod platform;
mod render_cfg;
mod ui;

use bevy::app::AppExit;
use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = config::Cli::parse();

    if cli.list_adapters {
        eprintln!("--list-adapters is not implemented yet (coming with the CLI milestone)");
        return ExitCode::from(1);
    }

    let mut settings = config::Settings::load();
    let run = match config::merge_cli(&mut settings, &cli) {
        Ok(run) => run,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    install_panic_hook();

    match app::run(settings, run) {
        AppExit::Success => ExitCode::SUCCESS,
        AppExit::Error(code) => ExitCode::from(code.get()),
    }
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
