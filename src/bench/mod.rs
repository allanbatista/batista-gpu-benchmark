//! Benchmark state machine. F1: states + error surface only; runner lands in F3.

use bevy::prelude::*;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BenchState {
    #[default]
    Idle,
    Loading,
    Prewarming,
    Warmup,
    Running,
    Finalizing,
    /// Interval between repetitions (internal state, spec §11).
    Cooldown,
    Completed,
    Error,
}

impl BenchState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Loading => "loading",
            Self::Prewarming => "prewarming",
            Self::Warmup => "warmup",
            Self::Running => "benchmark",
            Self::Finalizing => "finalizing",
            Self::Cooldown => "cooldown",
            Self::Completed => "completed",
            Self::Error => "error",
        }
    }

    /// Settings panel hidden, camera locked, config frozen.
    pub fn is_benchmarking(self) -> bool {
        matches!(
            self,
            Self::Loading | Self::Prewarming | Self::Warmup | Self::Running | Self::Finalizing | Self::Cooldown
        )
    }
}

/// Last user-facing error (spec §16). Shown as a banner in the UI.
#[derive(Resource, Default)]
pub struct LastError(pub Option<String>);

pub struct BenchPlugin;

impl Plugin for BenchPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<BenchState>().init_resource::<LastError>();
    }
}
