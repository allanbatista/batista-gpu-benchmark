//! Benchmark state machine (spec §8): Idle → Loading → Prewarming → Warmup →
//! Running → Finalizing → (Cooldown → Warmup)* → Completed, plus Error and
//! Esc-cancel. One runner system owns all transitions; per-state entry work is
//! detected via a cached previous-state field (no OnEnter plumbing).

pub mod gpu_timing;
pub mod metrics;
pub mod report;
pub mod sampler;

use bevy::prelude::*;
use bevy::render::render_resource::PipelineCache;
use bevy::render::{ExtractSchedule, MainWorld, RenderApp};
use bevy::window::{PrimaryWindow, WindowOccluded};

use crate::app::{AppSettings, RunOpts};
use crate::config::Settings;
use crate::scene::SceneMetrics;
use crate::scene::orbits::{BenchClock, tick_clock};
use metrics::RunMetrics;
use sampler::{FrameSample, Sampler};

pub const LOADING_TIMEOUT_S: f64 = 60.0;
pub const PREWARM_TIMEOUT_S: f64 = 30.0;
/// Consecutive frames with an empty pipeline queue required to leave Prewarming.
pub const PREWARM_CLEAN_FRAMES: u32 = 60;
pub const PREWARM_MIN_S: f64 = 1.0;

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

/// UI → runner: start a benchmark session.
#[derive(Message)]
pub struct StartBenchmark;

/// Mirrored from the render world: true when the pipeline queue is empty.
#[derive(Resource, Default)]
pub struct PipelinesReady(pub bool);

/// Immutable snapshot taken when the session starts (config frozen, spec §8).
#[derive(Clone)]
pub struct BenchPlan {
    pub settings: Settings,
    pub warmup_s: f64,
    pub duration_s: f64,
    pub runs: u32,
    pub interval_s: f64,
}

pub struct RunRecord {
    pub index: u32,
    pub metrics: Option<RunMetrics>,
    pub gpu: Option<gpu_timing::GpuAverages>,
    pub samples: Vec<FrameSample>,
    pub canceled: bool,
    pub truncated: bool,
}

#[derive(Resource, Default)]
pub struct BenchSession {
    pub plan: Option<BenchPlan>,
    pub current_run: u32,
    pub records: Vec<RunRecord>,
    pub focus_lost: bool,
    pub prewarm_frames: u32,
    pub prewarm_seconds: f64,
    pub started_at_local: Option<String>,
    pub result_dir: Option<std::path::PathBuf>,
    pub report_written: bool,
    pub report_error: Option<String>,
    // runner bookkeeping
    prev_state: BenchState,
    state_entered: f64,
    clean_frames: u32,
    run_wall_start: Option<std::time::Instant>,
}

impl BenchSession {
    pub fn completed_metrics(&self) -> Vec<&RunMetrics> {
        self.records.iter().filter_map(|r| r.metrics.as_ref()).collect()
    }
}

pub struct BenchPlugin;

impl Plugin for BenchPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<BenchState>()
            .init_resource::<LastError>()
            .init_resource::<PipelinesReady>()
            .init_resource::<BenchSession>()
            .init_resource::<Sampler>()
            .add_message::<StartBenchmark>()
            .init_resource::<gpu_timing::GpuTiming>()
            .add_systems(
                Update,
                (
                    sampler::sample_frame,
                    gpu_timing::collect,
                    runner,
                    watch_focus,
                    cancel_on_esc,
                    autostart,
                    report::write_when_completed,
                    report::exit_after_benchmark,
                )
                    .chain()
                    .after(tick_clock),
            );

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(ExtractSchedule, mirror_pipelines_ready);
        }
    }
}

fn mirror_pipelines_ready(mut main_world: ResMut<MainWorld>, cache: Option<Res<PipelineCache>>) {
    let Some(cache) = cache else { return };
    if let Some(mut ready) = main_world.get_resource_mut::<PipelinesReady>() {
        let value = cache.waiting_pipelines().count() == 0;
        if ready.0 != value {
            ready.0 = value;
        }
    }
}

/// Fires --benchmark autostart once, as soon as the app reaches Idle.
fn autostart(
    run_opts: Res<RunOpts>,
    state: Res<State<BenchState>>,
    mut start: MessageWriter<StartBenchmark>,
    mut fired: Local<bool>,
) {
    if *fired || !run_opts.0.autostart || *state.get() != BenchState::Idle {
        return;
    }
    *fired = true;
    start.write(StartBenchmark);
}

fn watch_focus(
    state: Res<State<BenchState>>,
    mut session: ResMut<BenchSession>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut occluded: MessageReader<WindowOccluded>,
) {
    let occluded_now = occluded.read().any(|o| o.occluded);
    if *state.get() != BenchState::Running {
        return;
    }
    let unfocused = windows.single().map(|w| !w.focused).unwrap_or(false);
    if (unfocused || occluded_now) && !session.focus_lost {
        session.focus_lost = true;
        warn!("window lost focus/was occluded during capture — run marked non-comparable");
    }
}

fn cancel_on_esc(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<BenchState>>,
    mut next: ResMut<NextState<BenchState>>,
    mut session: ResMut<BenchSession>,
    mut samp: ResMut<Sampler>,
    mut last_error: ResMut<LastError>,
) {
    if !state.get().is_benchmarking() || !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    samp.stop();
    let index = session.current_run;
    session.records.push(RunRecord {
        index,
        metrics: None,
        gpu: None,
        samples: samp.take(),
        canceled: true,
        truncated: false,
    });
    last_error.0 = Some("benchmark canceled".into());
    info!("benchmark canceled by user");
    next.set(BenchState::Idle);
}

#[allow(clippy::too_many_arguments)]
fn runner(
    state: Res<State<BenchState>>,
    mut next: ResMut<NextState<BenchState>>,
    mut session: ResMut<BenchSession>,
    mut samp: ResMut<Sampler>,
    mut clock: ResMut<BenchClock>,
    mut start: MessageReader<StartBenchmark>,
    settings: Res<AppSettings>,
    scene: Res<SceneMetrics>,
    pipelines: Res<PipelinesReady>,
    mut last_error: ResMut<LastError>,
    mut gpu: ResMut<gpu_timing::GpuTiming>,
    mut telemetry: ResMut<crate::platform::telemetry::Telemetry>,
    time: Res<Time<Real>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    let current = *state.get();

    // Per-state entry work, without OnEnter schedules.
    if session.prev_state != current {
        session.prev_state = current;
        session.state_entered = now;
        match current {
            BenchState::Loading => {
                session.records.clear();
                session.current_run = 0;
                session.focus_lost = false;
                session.prewarm_frames = 0;
                session.prewarm_seconds = 0.0;
                session.report_written = false;
                session.report_error = None;
                telemetry.reset();
                let ts = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
                // Created up-front so no directory IO happens during capture.
                session.result_dir = report::create_result_dir(&settings.0.benchmark.output_dir, &ts);
                session.started_at_local = Some(ts);
            }
            BenchState::Prewarming => {
                session.clean_frames = 0;
                clock.t = 0.0; // sweep the actual run trajectory
            }
            BenchState::Warmup => {
                clock.t = 0.0; // every run replays the identical trajectory
            }
            BenchState::Running => {
                let duration = session.plan.as_ref().map(|p| p.duration_s).unwrap_or(60.0);
                samp.begin(duration);
                gpu.reset();
                session.run_wall_start = Some(std::time::Instant::now());
            }
            _ => {}
        }
    }
    let elapsed_in_state = now - session.state_entered;

    match current {
        BenchState::Idle | BenchState::Completed | BenchState::Error => {
            if start.read().next().is_some() {
                let s = &settings.0;
                session.plan = Some(BenchPlan {
                    settings: s.clone(),
                    warmup_s: s.benchmark.warmup_s,
                    duration_s: s.benchmark.duration_s,
                    runs: s.benchmark.runs.max(1),
                    interval_s: s.benchmark.interval_s,
                });
                last_error.0 = None;
                next.set(BenchState::Loading);
            }
        }
        BenchState::Loading => {
            if last_error.0.is_some() {
                next.set(BenchState::Error);
            } else if scene.ready {
                next.set(BenchState::Prewarming);
            } else if elapsed_in_state > LOADING_TIMEOUT_S {
                last_error.0 = Some("loading timed out (model/assets never became ready)".into());
                next.set(BenchState::Error);
            }
        }
        BenchState::Prewarming => {
            session.prewarm_frames += 1;
            if pipelines.0 {
                session.clean_frames += 1;
            } else {
                session.clean_frames = 0;
            }
            let done = elapsed_in_state >= PREWARM_MIN_S && session.clean_frames >= PREWARM_CLEAN_FRAMES;
            let timed_out = elapsed_in_state > PREWARM_TIMEOUT_S;
            if done || timed_out {
                if timed_out {
                    warn!("prewarm timed out with pipelines still compiling; warmup will absorb the rest");
                }
                session.prewarm_seconds = elapsed_in_state;
                next.set(BenchState::Warmup);
            }
        }
        BenchState::Warmup => {
            let warmup = session.plan.as_ref().map(|p| p.warmup_s).unwrap_or(10.0);
            if elapsed_in_state >= warmup {
                next.set(BenchState::Running);
            }
        }
        BenchState::Running => {
            let duration = session.plan.as_ref().map(|p| p.duration_s).unwrap_or(60.0);
            if elapsed_in_state >= duration || (!samp.active && samp.truncated) {
                samp.stop();
                next.set(BenchState::Finalizing);
            }
        }
        BenchState::Finalizing => {
            let wall = session.run_wall_start.take().map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
            let samples = samp.take();
            let frame_times: Vec<f64> = samples.iter().map(|s| s.ft_ms).collect();
            let run_metrics = metrics::compute(&frame_times, wall);
            if let Some(m) = &run_metrics {
                info!(
                    "run {}/{}: avg {:.1} fps, 1% low {:.1}, score {:.1}",
                    session.current_run + 1,
                    session.plan.as_ref().map(|p| p.runs).unwrap_or(1),
                    m.avg_fps,
                    m.low_1pct_fps,
                    m.score
                );
            }
            let index = session.current_run;
            let truncated = samp.truncated;
            session.records.push(RunRecord {
                index,
                metrics: run_metrics,
                gpu: gpu.averages(),
                samples,
                canceled: false,
                truncated,
            });

            let total_runs = session.plan.as_ref().map(|p| p.runs).unwrap_or(1);
            if session.current_run + 1 < total_runs {
                session.current_run += 1;
                next.set(BenchState::Cooldown);
            } else {
                // Final frame screenshot (spec §8): after the last sample, so the
                // copy-pipeline hitch cannot pollute the metrics.
                let want_shot = session.plan.as_ref().map(|p| p.settings.benchmark.screenshot).unwrap_or(false);
                if want_shot && let Some(dir) = &session.result_dir {
                    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
                    commands
                        .spawn(Screenshot::primary_window())
                        .observe(save_to_disk(dir.join("screenshot.png")));
                }
                next.set(BenchState::Completed);
            }
        }
        BenchState::Cooldown => {
            let interval = session.plan.as_ref().map(|p| p.interval_s).unwrap_or(5.0);
            if elapsed_in_state >= interval {
                next.set(BenchState::Warmup);
            }
        }
    }
}
