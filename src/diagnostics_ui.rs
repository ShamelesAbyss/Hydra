//! Native offline planning, confidence and portfolio analysis. No engine writes.
use crate::{
    diagnostics::{self, DiagnosticKind, DiagnosticReport},
    telemetry,
};
use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};
use rust_decimal::prelude::ToPrimitive;
use std::{
    path::Path,
    sync::mpsc::{self, Receiver},
};

#[derive(Default)]
pub struct DiagnosticWorkspace {
    path: String,
    anchor: bool,
    receiver: Option<Receiver<Result<DiagnosticReport, String>>>,
    running_head: Option<usize>,
    report: Option<DiagnosticReport>,
    error: Option<String>,
}
impl DiagnosticWorkspace {
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<usize> {
        let result = self.receiver.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Diagnostic worker ended without a report".into()))
            }
        })?;
        self.receiver = None;
        let head = self.running_head.take();
        match result {
            Ok(report) => {
                self.report = Some(report);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        ctx.request_repaint();
        head
    }
    pub fn show(&mut self, ui: &mut egui::Ui, head: usize) {
        ui.colored_label(
            Color32::from_rgb(246, 188, 83),
            "OFFLINE DIAGNOSTICS / CALLER DATA UNVERIFIED",
        );
        ui.label("Research only. No financial evidence certification, execution, risk release or live-learning authority.");
        let mut kind = match head {
            1 => DiagnosticKind::Fang,
            2 => DiagnosticKind::Cognition,
            _ => DiagnosticKind::Aegis,
        };
        if head == 1 {
            ui.add_enabled_ui(self.receiver.is_none(), |ui| {
                ui.checkbox(&mut self.anchor, "Basic EMA Anchor research");
            });
            if self.anchor {
                kind = DiagnosticKind::EmaAnchor;
            }
        }
        ui.add_enabled_ui(self.receiver.is_none(), |ui| {
            ui.horizontal(|ui| {
                ui.label("Snapshot JSON");
                ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(430.0));
                if ui.button("Evaluate diagnostics").clicked() {
                    let path = self.path.clone();
                    let context = ui.ctx().clone();
                    let (tx, rx) = mpsc::sync_channel(1);
                    match std::thread::Builder::new()
                        .name("hydra-offline-diagnostics".into())
                        .spawn(move || {
                            let r =
                                diagnostics::evaluate(kind, Path::new(&path)).map_err(|error| {
                                    telemetry::redact_diagnostic(&format!("{error:#}"))
                                });
                            let _ = tx.send(r);
                            context.request_repaint();
                        }) {
                        Ok(_) => {
                            self.receiver = Some(rx);
                            self.running_head = Some(head);
                            self.error = None;
                        }
                        Err(error) => {
                            self.error = Some(format!("Could not start diagnostic worker: {error}"))
                        }
                    }
                }
            });
            if ui.button("Copy empty request template").clicked() {
                match diagnostics::template(kind) {
                    Ok(json) => ui.ctx().copy_text(json),
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
        });
        if self.receiver.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Evaluating bounded local inputs...");
            });
        }
        if let Some(error) = &self.error {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
        ui.separator();
        match kind {
            DiagnosticKind::Fang => {
                ui.label("Deterministic arbitration with exact external capacity budgets and one owner per symbol. Shared pure kernel; live/replay consumers are not integrated yet.");
            }
            DiagnosticKind::EmaAnchor => {
                ui.label("Completed-bar EMA bands and bounded price-only suggestions. No Passivbot parity, sizing, adaptive grids or orders.");
            }
            DiagnosticKind::Cognition => {
                ui.label("Strict prior-outcome shadow gates, recency weighting and sample-aware descriptive intervals. Regimes are caller labels; verified-net claims are refused.");
            }
            DiagnosticKind::Aegis => {
                ui.label("Decimal exposure totals, explicit unknowns and budget breaches; historical correlation requires exactly aligned return timestamps. This is separate from live AEGIS enforcement.");
            }
        }
        let Some(report) = &self.report else {
            ui.label(
                "No diagnostic report loaded. Empty templates contain no fictional observations.",
            );
            return;
        };
        let matches = matches!(
            (kind, report),
            (DiagnosticKind::Fang, DiagnosticReport::Fang(_))
                | (DiagnosticKind::EmaAnchor, DiagnosticReport::EmaAnchor(_))
                | (DiagnosticKind::Cognition, DiagnosticReport::Cognition(_))
                | (DiagnosticKind::Aegis, DiagnosticReport::Aegis(_))
        );
        if !matches {
            ui.label("A previous report exists for another diagnostic. Run this diagnostic to display its results.");
            return;
        }
        if self.receiver.is_some() || self.error.is_some() {
            ui.label("Displayed report belongs to the last completed job.");
        }
        if ui.button("Copy complete report JSON").clicked() {
            if let Ok(json) = serde_json::to_string_pretty(report) {
                ui.ctx().copy_text(json);
            }
        }
        egui::ScrollArea::vertical().id_salt("diagnostic_results").show(ui,|ui|match report {
            DiagnosticReport::Fang(r)=>{
                ui.heading(format!("{} selected / {} rejected",r.selected.len(),r.rejected.len()));
                ui.label(format!("External notional allocated {} / remaining {}",r.allocated_notional,r.remaining_notional));
                for c in &r.selected {ui.label(format!("{} / {} / {:?} / notional {}",c.signal.symbol,c.signal.strategy,c.signal.side,c.requested_notional));
                    let fraction=c.requested_notional.checked_div(r.allocated_notional).and_then(|v|v.to_f32()).unwrap_or(0.0);ui.add(egui::ProgressBar::new(fraction).text(&c.id));}
                ui.collapsing("Rejection explanations",|ui|{for c in &r.rejected {ui.label(format!("{}: {:?}",c.id,c.reason));}});
                limitations(ui,&r.limitations);
            },
            DiagnosticReport::EmaAnchor(r)=>{
                ui.heading("Basic EMA Anchor / completed bars");
                ui.label(format!("{} bars / as-of {} ms",r.bar_count,r.as_of_ms));
                ui.label(format!("Fast EMA {:.6} / slow EMA {:.6} / anchor {:.6}",r.fast_ema,r.slow_ema,r.anchor));
                ui.label(format!("Directional deviation {:.3} bps / bounded price suggestion {:?}",r.directional_deviation_bps,r.suggested_entry_price));
                limitations(ui,&r.limitations);
            },
            DiagnosticReport::Cognition(r)=>{
                ui.heading("Strategy attribution / shadow comparison");
                ui.label(format!("{} supplied / {} evaluated opportunities / as-of {} ms",r.supplied_samples,r.evaluated_opportunities,r.as_of_ms));
                for c in &r.comparisons {ui.label(format!("{:?}: baseline sum {:.4} R / shadow sum {:.4} R / {} passed of {} (not portfolio returns)",c.source,c.baseline_normalized_r_sum,c.shadow_normalized_r_sum,c.shadow_passed_samples,c.baseline_samples));}
                for g in &r.groups {ui.push_id((&g.strategy,&g.regime,format!("{:?}",g.source)),|ui|{
                    ui.label(format!("{} / {} / {:?} / n {} / effective n {:.2} / {}",g.strategy,g.regime,g.source,g.estimate.samples,g.estimate.effective_samples,if g.estimate.sufficient_samples {"sample threshold met"} else {"insufficient samples"}));
                    interval(ui,&g.estimate);
                });}
                ui.collapsing("Strict prior-outcome decisions",|ui|{for d in r.decisions.iter().take(256) {ui.label(format!("{}: {} / {}",d.sample_id,if d.shadow_pass {"pass"} else {"skip"},d.reason));}ui.label("Display limited to 256 decisions; complete JSON retains all.");});
                limitations(ui,&r.limitations);
            },
            DiagnosticReport::Aegis(r)=>{
                ui.heading("Portfolio exposure / supplied-budget analysis");
                ui.label(format!("Account label {} / as-of {} ms / budget result {:?} (unverified)",r.account_id,r.as_of_ms,r.within_supplied_budgets));
                if let Some(t)=&r.totals {ui.label(format!("Gross {} / net {} / long {} / short {} USD",t.gross_usd,t.net_usd,t.long_usd,t.short_usd));}
                for unknown in &r.unknowns {ui.colored_label(Color32::YELLOW,format!("UNKNOWN: {unknown}"));}
                for breach in &r.breaches {ui.colored_label(Color32::LIGHT_RED,format!("BREACH: {breach}"));}
                for asset in &r.assets {ui.label(format!("{} / gross {:?} / equity fraction {:?} / fresh {}",asset.symbol,asset.gross_usd,asset.equity_fraction,asset.fresh));}
                ui.collapsing("Aligned historical correlation",|ui|{for pair in &r.correlations {
                    ui.horizontal(|ui| {let color=match pair.pearson {Some(v) if v>0.0=>Color32::from_rgb(255,(180.0*(1.0-v)) as u8,100),Some(v)=>Color32::from_rgb(70,(180.0*(-v)) as u8,220),None=>Color32::GRAY};
                        let(rect,_)=ui.allocate_exact_size(Vec2::splat(14.0),Sense::hover());ui.painter().rect_filled(rect,2.0,color);
                        ui.label(format!("{} / {} / rho {:?} / n {} / {}",pair.first,pair.second,pair.pearson,pair.aligned_samples,pair.unavailable_reason.as_deref().unwrap_or("aligned sample estimate")));
                    });
                }});
                limitations(ui,&r.limitations);
            },
        });
    }
}
fn limitations(ui: &mut egui::Ui, values: &[String]) {
    ui.collapsing("Method and limitations", |ui| {
        for text in values {
            ui.label(text);
        }
    });
}
fn interval(ui: &mut egui::Ui, e: &crate::cognition_shadow::ConfidenceEstimate) {
    let (Some(low), Some(mean), Some(high)) = (
        e.approximate_lower_bound_r,
        e.weighted_mean_r,
        e.approximate_upper_bound_r,
    ) else {
        ui.label("Descriptive interval unavailable");
        return;
    };
    ui.label(format!(
        "Mean {mean:.4} R / approximate interval [{low:.4}, {high:.4}] (no probability guarantee)"
    ));
    let limit = low.abs().max(high.abs()).max(1e-9);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().min(700.0), 24.0),
        Sense::hover(),
    );
    let x = |v: f64| rect.center().x + (v / limit) as f32 * rect.width() * 0.48;
    ui.painter().line_segment(
        [
            Pos2::new(x(0.0), rect.top()),
            Pos2::new(x(0.0), rect.bottom()),
        ],
        Stroke::new(1.0, Color32::GRAY),
    );
    ui.painter().line_segment(
        [
            Pos2::new(x(low), rect.center().y),
            Pos2::new(x(high), rect.center().y),
        ],
        Stroke::new(3.0, Color32::from_rgb(173, 133, 255)),
    );
    ui.painter()
        .circle_filled(Pos2::new(x(mean), rect.center().y), 4.0, Color32::WHITE);
}
