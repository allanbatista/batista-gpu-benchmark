//! Results section: consolidated score, per-run table, report location.

use bevy_egui::egui;

use crate::bench::{BenchSession, metrics};

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
