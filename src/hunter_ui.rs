//! Offline market selection workspace. Never connects to a wallet or exchange.
use crate::{
    hunter::{self, HunterPolicy, HunterReport, HunterRequest},
    research, telemetry,
};
use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};
use std::{
    collections::BTreeSet,
    path::Path,
    sync::mpsc::{self, Receiver},
};

#[derive(Default)]
pub struct HunterWorkspace {
    path: String,
    filter: String,
    selected_only: bool,
    receiver: Option<Receiver<Result<HunterReport, String>>>,
    report: Option<HunterReport>,
    error: Option<String>,
}
impl HunterWorkspace {
    pub fn running(&self) -> bool {
        self.receiver.is_some()
    }
    pub fn report(&self) -> Option<&HunterReport> {
        self.report.as_ref()
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        let result = self.receiver.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Snapshot worker ended without a report".into()))
            }
        });
        let Some(result) = result else {
            return false;
        };
        self.receiver = None;
        match result {
            Ok(report) => {
                self.report = Some(report);
                self.error = None;
            }
            Err(error) => {
                self.error = Some(error);
            }
        }
        ctx.request_repaint();
        true
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.colored_label(
            Color32::from_rgb(54, 205, 245),
            "OFFLINE SNAPSHOT / RESEARCH WATCHLIST",
        );
        ui.label("Eligibility, liquidity and execution costs. Scores are heuristics, never profit forecasts.");
        ui.label("No trading authority, capital allocation or position rotation. Alt+1 through Alt+7 switches heads.");
        ui.add_enabled_ui(self.receiver.is_none(), |ui| {
            ui.horizontal(|ui| {
                ui.label("Snapshot JSON");
                ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(450.0));
                if ui.button("Evaluate snapshot").clicked() {
                    let path = self.path.clone();
                    let ctx = ui.ctx().clone();
                    let (tx, rx) = mpsc::channel();
                    self.receiver = Some(rx);
                    self.error = None;
                    std::thread::spawn(move || {
                        let result = research::read_json::<HunterRequest>(Path::new(&path))
                            .and_then(|request| hunter::evaluate(&request))
                            .map_err(|error| telemetry::redact_diagnostic(&error.to_string()));
                        let _ = tx.send(result);
                        ctx.request_repaint();
                    });
                }
            });
            if ui.button("Copy empty snapshot template").clicked() {
                let request = HunterRequest {
                    as_of_ms: 0,
                    source: "operator-supplied offline snapshot".into(),
                    approved: BTreeSet::new(),
                    excluded: BTreeSet::new(),
                    current_watchlist: vec![],
                    policy: HunterPolicy::default(),
                    markets: vec![],
                };
                if let Ok(json) = serde_json::to_string_pretty(&request) {
                    ui.ctx().copy_text(json);
                }
            }
        });
        if self.receiver.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Evaluating bounded local snapshot...");
            });
        }
        if let Some(error) = &self.error {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
        ui.separator();
        let Some(report) = &self.report else {
            ui.heading("Inspect the opportunity set before selecting assets");
            ui.label("Load your own timestamped observations. Missing metrics, unapproved assets and stale observations fail eligibility.");
            ui.label("Required: volume, depth, spread, volatility, directionality, funding, open interest, coverage, age and EMA readiness.");
            ui.label("Rotation uses deterministic score ordering and a configurable switch margin to reduce watchlist churn.");
            ui.label("No snapshot loaded. No market values or opportunity scores are inferred.");
            return;
        };
        ui.label(format!(
            "User source label: {} | snapshot as-of: {} ms (not current-time verification)",
            report.source, report.as_of_ms
        ));
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!(
                "{} markets / {} eligible / {} selected",
                report.ranked.len(),
                report.ranked.iter().filter(|m| m.eligible).count(),
                report.watchlist.len()
            ));
            ui.label(format!("Watchlist: {}", report.watchlist.join(", ")));
        });
        ui.collapsing("Liquidity / spread map", |ui| {
            ui.label("X: spread (basis points) | Y: log10 depth (USD). Cyan: selected; gray: other valid points.");
            let points: Vec<_> = report.ranked.iter().filter_map(|m| {
                let (Some(spread),Some(depth)) = (m.observation.spread_bps,m.observation.depth_usd) else { return None; };
                (spread.is_finite() && spread >= 0.0 && depth.is_finite() && depth > 0.0).then_some((m,spread,depth.log10()))
            }).collect();
            if points.is_empty() { ui.label("No valid depth/spread observations."); return; }
            let max_x = points.iter().map(|p| p.1).fold(1.0_f64, f64::max);
            let min_y = points.iter().map(|p| p.2).fold(f64::INFINITY, f64::min);
            let max_y = points.iter().map(|p| p.2).fold(f64::NEG_INFINITY, f64::max);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(1.0),170.0), Sense::hover());
            let plot = rect.shrink(10.0);
            ui.painter().line_segment([plot.left_bottom(),plot.right_bottom()],Stroke::new(1.0,Color32::GRAY));
            ui.painter().line_segment([plot.left_bottom(),plot.left_top()],Stroke::new(1.0,Color32::GRAY));
            for (m,x,y) in points {
                let pos = Pos2::new(plot.left() + (x/max_x) as f32*plot.width(),plot.bottom()-((y-min_y)/(max_y-min_y).max(1.0)) as f32*plot.height());
                ui.painter().circle_filled(pos,4.0,if m.selected {Color32::from_rgb(54,205,245)} else {Color32::GRAY});
                let response = ui.interact(egui::Rect::from_center_size(pos,Vec2::splat(12.0)),ui.id().with(&m.observation.symbol),Sense::hover());
                response.on_hover_text(format!("{} | spread {:.3} bps | depth {:.2} USD",m.observation.symbol,x,m.observation.depth_usd.unwrap()));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Filter");
            ui.text_edit_singleline(&mut self.filter);
            ui.checkbox(&mut self.selected_only, "Selected only");
        });
        let query = self.filter.to_ascii_lowercase();
        egui::ScrollArea::vertical().max_height(360.0).show(ui,|ui| {
            for m in report.ranked.iter().filter(|m| (!self.selected_only || m.selected) && m.observation.symbol.to_ascii_lowercase().contains(&query)) {
                ui.collapsing(format!("{} {} | {}",if m.selected {"SELECTED"} else if m.eligible {"ELIGIBLE"} else {"BLOCKED"},m.observation.symbol,m.score.map(|s|format!("{s:.2} / 100")).unwrap_or_else(||"unscored".into())),|ui| {
                    if let Some(score)=m.score { ui.add(egui::ProgressBar::new((score/100.0) as f32).text("Selection heuristic")); }
                    ui.label(format!("Volume {:?} USD | depth {:?} USD | spread {:?} bps | funding {:?} bps",m.observation.volume_24h_usd,m.observation.depth_usd,m.observation.spread_bps,m.observation.funding_bps));
                    ui.label(format!("Directionality {:?} | OI {:?} USD | coverage {:?} | age {:?} days",m.observation.directionality,m.observation.open_interest_usd,m.observation.coverage_fraction,m.observation.age_days));
                    ui.label(format!("Volatility {:?}% | EMA ready {} | observed at {} ms",m.observation.volatility_pct,m.observation.ema_ready,m.observation.observed_at_ms));
                    for reason in &m.reasons { ui.colored_label(Color32::LIGHT_RED,reason); }
                });
            }
        });
        ui.collapsing("Method and limitations", |ui| {
            for limitation in &report.limitations {
                ui.label(limitation);
            }
        });
    }
}
