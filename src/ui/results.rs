//! Results section: consolidated score, per-run table, report location, and
//! comparison across past results (lazy scan of <output>/*/summary.json).

use bevy::prelude::*;
use bevy_egui::egui;

use crate::bench::{BenchSession, metrics};
use crate::config::SCORE_ALGORITHM;

#[derive(Resource, Default)]
pub struct CompareCache {
    pub rows: Vec<CompareRow>,
    pub scanned: bool,
    pub error: Option<String>,
}

pub struct CompareRow {
    pub dir: String,
    pub preset: String,
    pub score: Option<f64>,
    pub avg_fps: Option<f64>,
    pub low1: Option<f64>,
    pub backend: String,
    pub gpu: String,
    pub official: bool,
    pub algorithm: String,
}

fn scan_results(output_dir: &str) -> Result<Vec<CompareRow>, String> {
    let mut rows = Vec::new();
    let entries = std::fs::read_dir(output_dir).map_err(|e| format!("{output_dir}: {e}"))?;
    for entry in entries.flatten() {
        let summary = entry.path().join("summary.json");
        let Ok(text) = std::fs::read_to_string(&summary) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        let m = &v["metrics"];
        rows.push(CompareRow {
            dir: entry.file_name().to_string_lossy().to_string(),
            preset: v["preset"].as_str().unwrap_or("?").to_string(),
            score: m["score_median"].as_f64(),
            avg_fps: m["avg_fps_mean"].as_f64(),
            low1: m["low_1pct_fps_mean"].as_f64(),
            backend: v["adapter"]["backend"].as_str().unwrap_or("?").to_string(),
            gpu: v["adapter"]["name"].as_str().unwrap_or("?").to_string(),
            official: v["execution"]["official"].as_bool().unwrap_or(false),
            algorithm: v["score_algorithm"].as_str().unwrap_or("?").to_string(),
        });
    }
    rows.sort_by(|a, b| b.score.unwrap_or(0.0).total_cmp(&a.score.unwrap_or(0.0)));
    Ok(rows)
}

pub fn draw_compare(ui: &mut egui::Ui, cache: &mut CompareCache, output_dir: &str) {
    ui.horizontal(|ui| {
        if ui.button(if cache.scanned { "Rescan" } else { "Scan past results" }).clicked() {
            match scan_results(output_dir) {
                Ok(rows) => {
                    cache.rows = rows;
                    cache.scanned = true;
                    cache.error = None;
                }
                Err(e) => cache.error = Some(e),
            }
        }
        if cache.scanned {
            ui.label(format!("{} result(s)", cache.rows.len()));
        }
    });
    if let Some(err) = &cache.error {
        ui.colored_label(egui::Color32::YELLOW, err);
    }
    if cache.rows.is_empty() {
        return;
    }
    egui::Grid::new("compare-table").striped(true).num_columns(7).show(ui, |ui| {
        for h in ["when", "preset", "score", "avg", "1% low", "API", "flags"] {
            ui.strong(h);
        }
        ui.end_row();
        for row in &cache.rows {
            ui.label(&row.dir).on_hover_text(&row.gpu);
            ui.label(&row.preset);
            ui.label(row.score.map(|s| format!("{s:.1}")).unwrap_or_else(|| "—".into()));
            ui.label(row.avg_fps.map(|s| format!("{s:.1}")).unwrap_or_else(|| "—".into()));
            ui.label(row.low1.map(|s| format!("{s:.1}")).unwrap_or_else(|| "—".into()));
            ui.label(&row.backend);
            let mut flags = Vec::new();
            if !row.official {
                flags.push("custom");
            }
            if row.algorithm != SCORE_ALGORITHM {
                flags.push("old algo"); // different score algorithm — not comparable
            }
            ui.label(flags.join(", "));
            ui.end_row();
        }
    });
    ui.label(
        egui::RichText::new("Scores are only comparable between runs with the same preset version, algorithm and official profile.")
            .weak(),
    );
}

pub fn draw(ui: &mut egui::Ui, session: &mut BenchSession) {
    if session.records.is_empty() {
        ui.label(egui::RichText::new("No benchmark completed yet.").weak());
        return;
    }

    let completed = session.completed_metrics();
    let threshold = session
        .plan
        .as_ref()
        .map(|p| p.settings.benchmark.unstable_threshold_pct)
        .unwrap_or(5.0);
    if let Some(cons) = metrics::consolidate(&completed, threshold) {
        ui.heading(format!("Score: {:.1}", cons.score_median));
        ui.label(format!(
            "runs {} · mean {:.1} · best {:.1} · worst {:.1} · variation {:.1}%",
            cons.runs, cons.score_mean, cons.score_best, cons.score_worst, cons.variation_pct
        ));
        ui.label(format!(
            "avg FPS {:.1} · 1% low {:.1}",
            cons.avg_fps_mean, cons.low_1pct_fps_mean
        ));
        if cons.unstable {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!("⚠ unstable: variation above {:.1}%", cons.unstable_threshold_pct),
            );
        }
        ui.separator();
    }

    egui::Grid::new("runs-table").striped(true).num_columns(6).show(ui, |ui| {
        for h in ["run", "avg fps", "1% low", "p99 ms", "stutters", "score"] {
            ui.strong(h);
        }
        ui.end_row();
        for record in &session.records {
            ui.label(format!("{}", record.index + 1));
            match (&record.metrics, record.canceled) {
                (Some(m), _) => {
                    ui.label(format!("{:.1}", m.avg_fps));
                    ui.label(format!("{:.1}", m.low_1pct_fps));
                    ui.label(format!("{:.2}", m.p99_ms));
                    ui.label(format!("{}", m.stutters));
                    ui.label(format!("{:.1}{}", m.score, if record.truncated { " (truncated)" } else { "" }));
                }
                (None, true) => {
                    ui.label("canceled");
                    for _ in 0..4 {
                        ui.label("—");
                    }
                }
                (None, false) => {
                    ui.label("no data");
                    for _ in 0..4 {
                        ui.label("—");
                    }
                }
            }
            ui.end_row();
        }
    });

    ui.separator();
    match (&session.report_error, &session.result_dir, session.report_written) {
        (Some(err), _, _) => {
            ui.colored_label(egui::Color32::from_rgb(255, 90, 90), err);
            if ui.button("Retry save").clicked() {
                session.report_written = false; // report system re-runs next frame
            }
        }
        (None, Some(dir), true) => {
            ui.label(format!("Saved to {}", dir.display()));
            if ui.button("Open report dir").clicked() {
                crate::platform::open_dir(dir);
            }
        }
        _ => {}
    }
}
