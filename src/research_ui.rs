//! Local research workspaces with one bounded background job at a time.
use crate::{
    config::AppConfig,
    core::Candle,
    genesis::{self, SearchOptions, SearchReport},
    oracle::{self, ReplayOptions, ReplayReport},
    research, telemetry,
};
use eframe::egui::{self, Color32, Pos2, RichText, Sense, Stroke, Vec2};
use std::{
    path::Path,
    sync::mpsc::{self, Receiver},
    time::Instant,
};

enum Completed {
    Oracle(ReplayReport),
    Genesis(SearchReport),
}
pub struct ResearchWorkspace {
    config_path: String,
    candles_path: String,
    oracle_options_path: String,
    genesis_options_path: String,
    receiver: Option<Receiver<Result<Completed, String>>>,
    running_head: Option<usize>,
    started: Option<Instant>,
    error: Option<String>,
    oracle: Option<ReplayReport>,
    genesis: Option<SearchReport>,
}
impl Default for ResearchWorkspace {
    fn default() -> Self {
        Self {
            config_path: "config/strategies.toml".into(),
            candles_path: String::new(),
            oracle_options_path: String::new(),
            genesis_options_path: String::new(),
            receiver: None,
            running_head: None,
            started: None,
            error: None,
            oracle: None,
            genesis: None,
        }
    }
}
impl ResearchWorkspace {
    pub fn oracle_ready(&self) -> bool {
        self.oracle.is_some()
    }
    pub fn genesis_ready(&self) -> bool {
        self.genesis.is_some()
    }
    /// Returns the subsystem that actually completed, for an event-backed pulse.
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<usize> {
        let result = self.receiver.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "Research worker ended without returning a report".into(),
            )),
        });
        let result = result?;
        let head = self.running_head.take();
        self.receiver = None;
        self.started = None;
        match result {
            Ok(Completed::Oracle(report)) => {
                self.oracle = Some(report);
                self.error = None;
            }
            Ok(Completed::Genesis(report)) => {
                self.genesis = Some(report);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        ctx.request_repaint();
        head
    }
    pub fn show(&mut self, ui: &mut egui::Ui, optimizer: bool) {
        ui.colored_label(
            Color32::from_rgb(246, 188, 83),
            "SIMULATED RESEARCH · local candle files",
        );
        ui.label("Select a candle JSON dataset and strategy TOML. Reports use explicit execution-cost assumptions.");
        ui.add_enabled_ui(self.receiver.is_none(), |ui| {
            path(ui, "Strategy TOML", &mut self.config_path);
            path(ui, "Candles JSON", &mut self.candles_path);
            if optimizer {
                path(
                    ui,
                    "Search options JSON (optional)",
                    &mut self.genesis_options_path,
                );
                ui.label("Blank options use seed 1 and bounded CPU evaluation.");
                if ui.button("Copy default search options JSON").clicked() {
                    match serde_json::to_string_pretty(&SearchOptions::default()) {
                        Ok(json) => ui.ctx().copy_text(json),
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
            } else {
                path(
                    ui,
                    "Replay options JSON (optional)",
                    &mut self.oracle_options_path,
                );
                ui.label("Blank options use default fees, slippage and 240 warmup bars.");
                if ui.button("Copy default replay options JSON").clicked() {
                    match serde_json::to_string_pretty(&ReplayOptions::default()) {
                        Ok(json) => ui.ctx().copy_text(json),
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
            }
            let label = if optimizer {
                "Run bounded optimization"
            } else {
                "Run backtest"
            };
            if ui.button(label).clicked() {
                self.start(optimizer, ui.ctx());
            }
        });
        if let Some(started) = self.started {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!(
                    "{} job running · {:.1}s · one job at a time",
                    if self.running_head == Some(6) {
                        "Genesis"
                    } else {
                        "Oracle"
                    },
                    started.elapsed().as_secs_f32()
                ));
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
        if let Some(error) = &self.error {
            ui.colored_label(Color32::from_rgb(246, 188, 83), error);
        }
        ui.separator();
        if (self.receiver.is_some() || self.error.is_some())
            && (if optimizer {
                self.genesis.is_some()
            } else {
                self.oracle.is_some()
            })
        {
            ui.label("Displayed results belong to the last completed job.");
        }
        if optimizer {
            if let Some(report) = &self.genesis {
                genesis_report(ui, report);
            }
        } else if let Some(report) = &self.oracle {
            oracle_report(ui, report);
        }
    }
    fn start(&mut self, optimizer: bool, ctx: &egui::Context) {
        if self.receiver.is_some() {
            return;
        }
        if self.candles_path.trim().is_empty() || self.config_path.trim().is_empty() {
            self.error = Some("Select both strategy configuration and candle dataset paths".into());
            return;
        }
        let config = self.config_path.clone();
        let candles = self.candles_path.clone();
        let options = if optimizer {
            self.genesis_options_path.clone()
        } else {
            self.oracle_options_path.clone()
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let context = ctx.clone();
        let result = std::thread::Builder::new()
            .name("hydra-offline-research".into())
            .spawn(move || {
                let result = (|| -> anyhow::Result<Completed> {
                    let cfg = AppConfig::load_research(&config, None)?;
                    let candles: Vec<Candle> = research::read_json(Path::new(&candles))?;
                    if optimizer {
                        let options: SearchOptions = if options.trim().is_empty() {
                            SearchOptions::default()
                        } else {
                            research::read_json(Path::new(&options))?
                        };
                        Ok(Completed::Genesis(genesis::optimize(
                            &cfg, &candles, &options,
                        )?))
                    } else {
                        let options: ReplayOptions = if options.trim().is_empty() {
                            ReplayOptions::default()
                        } else {
                            research::read_json(Path::new(&options))?
                        };
                        Ok(Completed::Oracle(oracle::replay(&cfg, &candles, &options)?))
                    }
                })()
                .map_err(|error| telemetry::redact_diagnostic(&format!("{error:#}")));
                let _ = tx.send(result);
                context.request_repaint();
            });
        match result {
            Ok(_) => {
                self.receiver = Some(rx);
                self.running_head = Some(if optimizer { 6 } else { 5 });
                self.started = Some(Instant::now());
                self.error = None;
            }
            Err(error) => self.error = Some(format!("Could not start research worker: {error}")),
        }
    }
}
fn path(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(value).desired_width(600.0));
    });
}
fn oracle_report(ui: &mut egui::Ui, report: &ReplayReport) {
    ui.heading("Oracle simulation result");
    ui.horizontal_wrapped(|ui| {
        ui.monospace(format!(
            "{} · {} bars · {} trades",
            report.symbol,
            report.bar_count,
            report.trades.len()
        ));
        ui.monospace(format!(
            "Equity {:.2} → {:.2}",
            report.initial_equity, report.final_equity
        ));
        ui.monospace(format!(
            "Drawdown {:.2}% · fees {:.4}",
            report.max_drawdown_fraction * 100.0,
            report.total_fees
        ));
    });
    ui.label(format!(
        "Dataset {} → {} ms · {} rejected entries",
        report.data_start_ms, report.data_end_ms, report.rejected_entries
    ));
    equity_plot(ui, &report.equity);
    ui.collapsing("Execution assumptions and model limitations", |ui| {
        ui.monospace(format!(
            "Risk {} · max exposure {} · fees {} bps · slippage {} bps · warmup {} bars",
            report.options.risk_fraction,
            report.options.max_exposure_fraction,
            report.options.fee_bps,
            report.options.slippage_bps,
            report.options.warmup_bars
        ));
        for limitation in &report.model_limitations {
            ui.label(limitation);
        }
    });
    copy_report(ui, "Copy simulation report JSON", report);
    ui.label(format!(
        "Showing up to 256 of {} simulated trades",
        report.trades.len()
    ));
    egui::ScrollArea::both()
        .max_height(380.0)
        .id_salt("oracle_trades")
        .show(ui, |ui| {
            egui::Grid::new("oracle_trade_grid")
                .striped(true)
                .show(ui, |ui| {
                    for title in [
                        "Strategy", "Side", "Entry ms", "Exit ms", "Qty", "Entry", "Exit", "Net",
                        "Reason",
                    ] {
                        ui.strong(title);
                    }
                    ui.end_row();
                    for trade in report.trades.iter().take(256) {
                        ui.label(&trade.strategy);
                        ui.label(trade.side.as_str());
                        ui.monospace(trade.entry_time_ms.to_string());
                        ui.monospace(trade.exit_time_ms.to_string());
                        ui.monospace(format!("{:.6}", trade.quantity));
                        ui.monospace(format!("{:.4}", trade.entry_price));
                        ui.monospace(format!("{:.4}", trade.exit_price));
                        ui.monospace(format!("{:.4}", trade.net_pnl));
                        ui.label(&trade.exit_reason);
                        ui.end_row();
                    }
                });
        });
}
fn genesis_report(ui: &mut egui::Ui, report: &SearchReport) {
    let frontier: std::collections::BTreeSet<&str> = report
        .pareto_candidate_ids
        .iter()
        .map(String::as_str)
        .collect();
    ui.heading("Genesis candidate search");
    ui.horizontal_wrapped(|ui| {
        ui.monospace(format!(
            "Seed {} · {} evaluations · {} generations · {} Pareto candidates",
            report.options.seed,
            report.candidates.len(),
            report.generations.len(),
            report.pareto_candidate_ids.len()
        ));
    });
    ui.label(format!("{} · {}", report.algorithm, report.termination));
    ui.label(&report.strategy_profile);
    pareto_plot(ui, report);
    ui.collapsing("Search assumptions and limitations", |ui| {
        for limitation in &report.limitations {
            ui.label(limitation);
        }
    });
    copy_report(ui, "Copy search report JSON", report);
    ui.label(format!(
        "Showing up to 256 of {} evaluated candidates",
        report.candidates.len()
    ));
    egui::ScrollArea::both()
        .max_height(400.0)
        .id_salt("genesis_candidates")
        .show(ui, |ui| {
            egui::Grid::new("genesis_grid")
                .striped(true)
                .show(ui, |ui| {
                    for title in [
                        "Candidate",
                        "EMA fast/slow",
                        "ATR",
                        "Min ATR %",
                        "Return %",
                        "DD %",
                        "Fees %",
                        "Trades",
                        "Pareto",
                    ] {
                        ui.strong(title);
                    }
                    ui.end_row();
                    for candidate in report.candidates.iter().take(256) {
                        ui.monospace(&candidate.id);
                        ui.monospace(format!(
                            "{}/{}",
                            candidate.parameters.fast_ema, candidate.parameters.slow_ema
                        ));
                        ui.monospace(candidate.parameters.atr_period.to_string());
                        ui.monospace(format!(
                            "{:.3}",
                            candidate.parameters.min_atr_milli_pct as f64 / 1000.0
                        ));
                        ui.monospace(format!("{:.3}", candidate.objectives.net_return * 100.0));
                        ui.monospace(format!(
                            "{:.3}",
                            candidate.objectives.drawdown_fraction * 100.0
                        ));
                        ui.monospace(format!("{:.3}", candidate.objectives.fee_fraction * 100.0));
                        ui.monospace(candidate.trade_count.to_string());
                        ui.label(if frontier.contains(candidate.id.as_str()) {
                            "yes"
                        } else {
                            ""
                        });
                        ui.end_row();
                    }
                });
        });
}
fn copy_report(ui: &mut egui::Ui, label: &str, report: &impl serde::Serialize) {
    if ui.button(label).clicked() {
        match serde_json::to_string_pretty(report) {
            Ok(json) if json.len() <= 32 * 1024 * 1024 => ui.ctx().copy_text(json),
            Ok(_) => {
                ui.colored_label(
                    Color32::YELLOW,
                    "Report exceeds the 32 MiB clipboard limit; use CLI output for this dataset",
                );
            }
            Err(error) => {
                ui.colored_label(Color32::YELLOW, error.to_string());
            }
        }
    }
}
fn equity_plot(ui: &mut egui::Ui, points: &[oracle::EquityPoint]) {
    if points.len() < 2 {
        ui.label("At least two simulated observations are needed for a curve");
        return;
    }
    let min = points
        .iter()
        .map(|p| p.equity)
        .fold(f64::INFINITY, f64::min);
    let max = points
        .iter()
        .map(|p| p.equity)
        .fold(f64::NEG_INFINITY, f64::max);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(200.0), 200.0),
        Sense::hover(),
    );
    let plot = rect.shrink(12.0);
    let span = (max - min).max(1e-9);
    // Downsample only the drawing. The report retains every observation.
    let stride = (points.len() / 2000).max(1);
    let line: Vec<_> = points
        .iter()
        .enumerate()
        .step_by(stride)
        .map(|(index, point)| {
            Pos2::new(
                plot.left() + plot.width() * index as f32 / (points.len() - 1) as f32,
                plot.bottom() - plot.height() * ((point.equity - min) / span) as f32,
            )
        })
        .collect();
    ui.painter()
        .rect_filled(rect, 6.0, Color32::from_rgb(12, 19, 27));
    ui.painter().add(egui::Shape::line(
        line,
        Stroke::new(1.5, Color32::from_rgb(58, 232, 165)),
    ));
    ui.label(format!(
        "Simulated equity · min {min:.2} · max {max:.2} · horizontal axis is bar index"
    ));
}
fn pareto_plot(ui: &mut egui::Ui, report: &SearchReport) {
    if report.candidates.is_empty() {
        return;
    }
    let frontier_ids: std::collections::BTreeSet<&str> = report
        .pareto_candidate_ids
        .iter()
        .map(String::as_str)
        .collect();
    let min_y = report
        .candidates
        .iter()
        .map(|c| c.objectives.net_return)
        .fold(f64::INFINITY, f64::min);
    let max_y = report
        .candidates
        .iter()
        .map(|c| c.objectives.net_return)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_x = report
        .candidates
        .iter()
        .map(|c| c.objectives.drawdown_fraction)
        .fold(0.0, f64::max)
        .max(1e-9);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(200.0), 230.0),
        Sense::hover(),
    );
    let plot = rect.shrink(12.0);
    ui.painter()
        .rect_filled(rect, 6.0, Color32::from_rgb(12, 19, 27));
    for candidate in &report.candidates {
        let point = Pos2::new(
            plot.left() + plot.width() * (candidate.objectives.drawdown_fraction / max_x) as f32,
            plot.bottom()
                - plot.height()
                    * ((candidate.objectives.net_return - min_y) / (max_y - min_y).max(1e-9))
                        as f32,
        );
        let frontier = frontier_ids.contains(candidate.id.as_str());
        ui.painter().circle_filled(
            point,
            if frontier { 4.0 } else { 2.0 },
            if frontier {
                Color32::from_rgb(207, 119, 248)
            } else {
                Color32::from_rgb(72, 100, 116)
            },
        );
    }
    ui.label(RichText::new(format!("X drawdown 0–{:.2}% · Y return {:.2}–{:.2}% · highlighted points are Pareto candidates including the fee objective",
        max_x*100.0,min_y*100.0,max_y*100.0)).small());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_local_input_reports_failure_and_worker_admission_is_single_job() {
        let directory = tempfile::tempdir().unwrap();
        let mut workspace = ResearchWorkspace::default();
        workspace.config_path = directory
            .path()
            .join("missing-config.toml")
            .display()
            .to_string();
        workspace.candles_path = directory
            .path()
            .join("missing-candles.json")
            .display()
            .to_string();
        let context = egui::Context::default();
        workspace.start(false, &context);
        workspace.start(true, &context);
        assert_eq!(workspace.running_head, Some(5));
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        while workspace.receiver.is_some() && Instant::now() < deadline {
            workspace.poll(&context);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(workspace.receiver.is_none());
        assert!(workspace.error.is_some());
        assert!(!workspace.oracle_ready());
        assert!(!workspace.genesis_ready());
    }

    #[test]
    fn blank_dataset_is_rejected_before_a_worker_is_started() {
        let mut workspace = ResearchWorkspace::default();
        workspace.start(false, &egui::Context::default());
        assert!(workspace.receiver.is_none());
        assert!(workspace.error.is_some());
    }
}
