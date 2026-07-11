//! Frame-time metrics (spec §9/§10/§11). Pure functions over the sampled
//! frame-time vector; every criterion is versioned so reports from different
//! algorithm versions are never compared silently.
//!
//! Definitions (documented in every report):
//! - `avg_fps = 1000·n / Σft` (harmonic — never the arithmetic mean of fps)
//! - percentile: nearest-rank on ascending frame times,
//!   `P(q) = ft[min(n−1, ceil(q·n/100)−1)]`
//! - lows (`low-agg-v1`): `k = max(1, floor(n·q))`, `low_q = 1000·k / Σ(worst k ft)`
//! - stutter (`stutter-v1`): `ft > max(50 ms, 3·P50)` — the absolute 50 ms floor is
//!   deliberate: a 20 ms hitch in a 240 fps run is not counted
//! - `score_v1 = avg_fps × clamp(low1/avg_fps, 0, 1)` ≡ `min(low1_fps, avg_fps)`
//! - consolidation (`consolidation-v1`): range-based,
//!   `variation_pct = 100·(best − worst)/mean` over per-run scores

use serde::Serialize;

pub const METRICS_VERSION: &str = "metrics-v1";
pub const PERCENTILE_METHOD: &str = "percentile-nearest-rank-v1";
pub const LOW_AGG_METHOD: &str = "low-agg-v1";
pub const STUTTER_CRITERION: &str = "stutter-v1";
pub const CONSOLIDATION_METHOD: &str = "consolidation-v1";

/// Minimum samples for metrics to be meaningful.
pub const MIN_SAMPLES: usize = 16;

#[derive(Debug, Clone, Serialize)]
pub struct RunMetrics {
    pub frames: usize,
    pub sampled_seconds: f64,
    pub wall_seconds: f64,
    pub avg_fps: f64,
    pub min_fps: f64,
    pub max_fps: f64,
    pub median_fps: f64,
    pub low_1pct_fps: f64,
    pub low_01pct_fps: f64,
    pub avg_frame_ms: f64,
    pub min_frame_ms: f64,
    pub max_frame_ms: f64,
    pub p50_ms: f64,
    pub p90_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub stddev_ms: f64,
    pub frames_over_16_7ms: usize,
    pub frames_over_33_3ms: usize,
    pub frames_over_50ms: usize,
    pub stutters: usize,
    pub score: f64,
}

/// Nearest-rank percentile over an ascending-sorted slice.
fn percentile(sorted: &[f64], q: f64) -> f64 {
    let n = sorted.len();
    let rank = ((q / 100.0) * n as f64).ceil() as usize;
    sorted[rank.clamp(1, n) - 1]
}

/// Aggregate low: average of the worst `q` fraction of frames, as FPS.
fn low_agg(sorted: &[f64], q: f64) -> f64 {
    let n = sorted.len();
    let k = ((n as f64 * q).floor() as usize).max(1);
    let worst_sum: f64 = sorted[n - k..].iter().sum();
    1000.0 * k as f64 / worst_sum
}

pub fn compute(frame_times_ms: &[f64], wall_seconds: f64) -> Option<RunMetrics> {
    let n = frame_times_ms.len();
    if n < MIN_SAMPLES {
        return None;
    }
    let mut sorted = frame_times_ms.to_vec();
    sorted.sort_by(f64::total_cmp);

    let sum: f64 = sorted.iter().sum();
    let avg_ms = sum / n as f64;
    let avg_fps = 1000.0 * n as f64 / sum;
    let min_ms = sorted[0];
    let max_ms = sorted[n - 1];
    let p50 = percentile(&sorted, 50.0);
    let p90 = percentile(&sorted, 90.0);
    let p95 = percentile(&sorted, 95.0);
    let p99 = percentile(&sorted, 99.0);
    let low1 = low_agg(&sorted, 0.01);
    let low01 = low_agg(&sorted, 0.001);
    let variance = sorted.iter().map(|ft| (ft - avg_ms).powi(2)).sum::<f64>() / n as f64;
    let stutter_threshold = 50f64.max(3.0 * p50);

    let score = avg_fps * (low1 / avg_fps).clamp(0.0, 1.0); // ≡ min(low1, avg_fps)

    Some(RunMetrics {
        frames: n,
        sampled_seconds: sum / 1000.0,
        wall_seconds,
        avg_fps,
        min_fps: 1000.0 / max_ms,
        max_fps: 1000.0 / min_ms,
        median_fps: 1000.0 / p50,
        low_1pct_fps: low1,
        low_01pct_fps: low01,
        avg_frame_ms: avg_ms,
        min_frame_ms: min_ms,
        max_frame_ms: max_ms,
        p50_ms: p50,
        p90_ms: p90,
        p95_ms: p95,
        p99_ms: p99,
        stddev_ms: variance.sqrt(),
        frames_over_16_7ms: sorted.iter().filter(|ft| **ft > 1000.0 / 60.0).count(),
        frames_over_33_3ms: sorted.iter().filter(|ft| **ft > 1000.0 / 30.0).count(),
        frames_over_50ms: sorted.iter().filter(|ft| **ft > 50.0).count(),
        stutters: sorted.iter().filter(|ft| **ft > stutter_threshold).count(),
        score,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Consolidated {
    pub runs: usize,
    pub score_mean: f64,
    pub score_median: f64,
    pub score_best: f64,
    pub score_worst: f64,
    pub avg_fps_mean: f64,
    pub low_1pct_fps_mean: f64,
    pub variation_pct: f64,
    pub unstable_threshold_pct: f64,
    pub unstable: bool,
}

pub fn consolidate(runs: &[&RunMetrics], threshold_pct: f64) -> Option<Consolidated> {
    if runs.is_empty() {
        return None;
    }
    let mut scores: Vec<f64> = runs.iter().map(|r| r.score).collect();
    scores.sort_by(f64::total_cmp);
    let n = scores.len();
    let mean = scores.iter().sum::<f64>() / n as f64;
    let median = if n % 2 == 1 { scores[n / 2] } else { (scores[n / 2 - 1] + scores[n / 2]) / 2.0 };
    let best = scores[n - 1];
    let worst = scores[0];
    let variation_pct = if mean > 0.0 { 100.0 * (best - worst) / mean } else { 0.0 };
    Some(Consolidated {
        runs: n,
        score_mean: mean,
        score_median: median,
        score_best: best,
        score_worst: worst,
        avg_fps_mean: runs.iter().map(|r| r.avg_fps).sum::<f64>() / n as f64,
        low_1pct_fps_mean: runs.iter().map(|r| r.low_1pct_fps).sum::<f64>() / n as f64,
        variation_pct,
        unstable_threshold_pct: threshold_pct,
        unstable: variation_pct > threshold_pct,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uniform(ms: f64, n: usize) -> Vec<f64> {
        vec![ms; n]
    }

    #[test]
    fn uniform_frames_all_equal() {
        let m = compute(&uniform(10.0, 1000), 10.0).unwrap();
        assert!((m.avg_fps - 100.0).abs() < 1e-9);
        assert!((m.min_fps - 100.0).abs() < 1e-9);
        assert!((m.median_fps - 100.0).abs() < 1e-9);
        assert!((m.low_1pct_fps - 100.0).abs() < 1e-9);
        assert!((m.low_01pct_fps - 100.0).abs() < 1e-9);
        assert!((m.score - 100.0).abs() < 1e-9);
        assert_eq!(m.stutters, 0);
        assert_eq!(m.frames_over_16_7ms, 0);
        assert!((m.stddev_ms).abs() < 1e-9);
    }

    #[test]
    fn invariant_chain_on_spiky_distribution() {
        // 990 fast frames + 10 slow spikes
        let mut ft = uniform(5.0, 990);
        ft.extend(uniform(60.0, 10));
        let m = compute(&ft, 5.5).unwrap();
        assert!(m.min_fps <= m.low_01pct_fps + 1e-9, "min {} low01 {}", m.min_fps, m.low_01pct_fps);
        assert!(m.low_01pct_fps <= m.low_1pct_fps + 1e-9);
        assert!(m.low_1pct_fps <= m.avg_fps + 1e-9);
        assert!(m.avg_fps <= m.max_fps + 1e-9);
        assert_eq!(m.frames_over_50ms, 10);
        assert_eq!(m.frames_over_33_3ms, 10);
        assert_eq!(m.frames_over_16_7ms, 10);
        // stutter threshold = max(50, 3×5) = 50 → the 60 ms frames count
        assert_eq!(m.stutters, 10);
        // score is exactly min(low1, avg)
        assert!((m.score - m.low_1pct_fps.min(m.avg_fps)).abs() < 1e-9);
    }

    #[test]
    fn percentile_nearest_rank_handmade() {
        let sorted = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(percentile(&sorted, 50.0), 2.0); // ceil(0.5·4)=2 → 2nd
        assert_eq!(percentile(&sorted, 51.0), 3.0); // ceil(2.04)=3 → 3rd
        assert_eq!(percentile(&sorted, 100.0), 4.0);
        assert_eq!(percentile(&sorted, 1.0), 1.0);
    }

    #[test]
    fn low_agg_handmade() {
        // 100 frames: 99×10ms + 1×100ms → low1: k=1, worst=100ms → 10 fps
        let mut ft = uniform(10.0, 99);
        ft.push(100.0);
        let m = compute(&ft, 1.09).unwrap();
        assert!((m.low_1pct_fps - 10.0).abs() < 1e-9);
        // low0.1: k = max(1, floor(0.1)) = 1 → same single worst frame
        assert!((m.low_01pct_fps - 10.0).abs() < 1e-9);
        // p99: rank ceil(0.99·100)=99 → 99th value (10.0)
        assert_eq!(m.p99_ms, 10.0);
        assert_eq!(m.max_frame_ms, 100.0);
    }

    #[test]
    fn stutter_uses_median_multiple_at_low_fps() {
        // median 40ms → threshold max(50, 120) = 120
        let mut ft = uniform(40.0, 100);
        ft.extend(uniform(100.0, 5)); // above 50 but below 120 → NOT stutters
        ft.extend(uniform(130.0, 2)); // above 120 → stutters
        let m = compute(&ft, 5.0).unwrap();
        assert_eq!(m.stutters, 2);
        assert_eq!(m.frames_over_50ms, 7);
    }

    #[test]
    fn too_few_samples_yields_none() {
        assert!(compute(&uniform(10.0, MIN_SAMPLES - 1), 0.1).is_none());
        assert!(compute(&uniform(10.0, MIN_SAMPLES), 0.2).is_some());
    }

    #[test]
    fn consolidation_variation_and_instability() {
        let a = compute(&uniform(10.0, 100), 1.0).unwrap(); // score 100
        let b = compute(&uniform(12.5, 100), 1.25).unwrap(); // score 80
        let c = compute(&uniform(10.5, 100), 1.05).unwrap(); // ~95.2
        let cons = consolidate(&[&a, &b, &c], 5.0).unwrap();
        assert_eq!(cons.score_best.round() as i64, 100);
        assert_eq!(cons.score_worst.round() as i64, 80);
        assert!(cons.variation_pct > 20.0);
        assert!(cons.unstable);

        let cons_stable = consolidate(&[&a, &a, &a], 5.0).unwrap();
        assert!((cons_stable.variation_pct).abs() < 1e-9);
        assert!(!cons_stable.unstable);
        assert_eq!(cons_stable.score_median.round() as i64, 100);
    }
}
