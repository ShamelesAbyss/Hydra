//! Native offline planning, confidence and portfolio analysis. No engine writes.
use crate::{
    cognition_shadow::{self, RegimeAssessment, RegimeInferenceConfig},
    core::{Candle, Side},
    diagnostics::{self, DiagnosticKind, DiagnosticReport},
    fang_planner::{self, BoundedGridConfig, BoundedGridPlan, GridSpacingMode},
    portfolio_research::{self, MarginPositionInput, MarginStressReport},
    research, telemetry,
};
use eframe::egui::{self, Color32, Pos2, RichText, Sense, Stroke, Vec2};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::{
    path::Path,
    str::FromStr,
    sync::mpsc::{self, Receiver},
};

pub struct DiagnosticWorkspace {
    last_head: Option<usize>,
    path: String,
    anchor: bool,
    receiver: Option<Receiver<Result<DiagnosticReport, String>>>,
    running_head: Option<usize>,
    report: Option<DiagnosticReport>,
    error: Option<String>,
    // FANG bounded grid interactive controls
    grid_symbol: String,
    grid_long: bool,
    grid_geometric: bool,
    grid_anchor_px: String,
    grid_step_bps: u32,
    grid_levels: usize,
    grid_cap_usd: String,
    grid_mult_tenths: u32,
    grid_tp_bps: u32,
    grid_stop_bps: u32,
    grid_plan: Option<Result<BoundedGridPlan, String>>,
    // AEGIS liquidation & margin stress simulator controls
    stress_equity: String,
    stress_symbol: String,
    stress_long: bool,
    stress_qty: String,
    stress_mark: String,
    stress_mmr_pct: String,
    stress_report: Option<Result<MarginStressReport, String>>,
    // COGNITION causal market-regime inference controls
    regime_candles_path: String,
    regime_lookback: usize,
    regime_result: Option<Result<RegimeAssessment, String>>,
}

impl Default for DiagnosticWorkspace {
    fn default() -> Self {
        Self {
            last_head: None,
            path: String::new(),
            anchor: false,
            receiver: None,
            running_head: None,
            report: None,
            error: None,
            grid_symbol: "ETH".into(),
            grid_long: true,
            grid_geometric: true,
            grid_anchor_px: "3000".into(),
            grid_step_bps: 100,
            grid_levels: 5,
            grid_cap_usd: "1000".into(),
            grid_mult_tenths: 12,
            grid_tp_bps: 150,
            grid_stop_bps: 200,
            grid_plan: None,
            stress_equity: "2000".into(),
            stress_symbol: "BTC".into(),
            stress_long: true,
            stress_qty: "0.05".into(),
            stress_mark: "100000".into(),
            stress_mmr_pct: "2.5".into(),
            stress_report: None,
            regime_candles_path: String::new(),
            regime_lookback: 24,
            regime_result: None,
        }
    }
}
impl DiagnosticWorkspace {
    pub fn running_head(&self) -> Option<usize> {
        self.running_head
    }
    pub fn report(&self) -> Option<&DiagnosticReport> {
        self.report.as_ref()
    }
    pub fn report_head(&self) -> Option<usize> {
        self.report.as_ref().map(|r| match r {
            DiagnosticReport::Fang(_) | DiagnosticReport::EmaAnchor(_) => 1,
            DiagnosticReport::Cognition(_) => 2,
            DiagnosticReport::Aegis(_) => 3,
        })
    }
    pub fn error_head(&self) -> Option<usize> {
        if self.error.is_some() {
            self.last_head
        } else {
            None
        }
    }

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
            self.show_bounded_grid_planner(ui);
            ui.separator();
        } else if head == 2 {
            self.show_cognition_regime_inference(ui);
            ui.separator();
        } else if head == 3 {
            self.show_aegis_stress_simulator(ui);
            ui.separator();
        }
        ui.add_enabled_ui(self.receiver.is_none(), |ui| {
            ui.horizontal(|ui| {
                ui.label("Snapshot JSON");
                ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(430.0));
                if ui.button("Evaluate diagnostics").clicked() {
                    self.last_head = Some(head);
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

    fn show_bounded_grid_planner(&mut self, ui: &mut egui::Ui) {
        ui.collapsing(
            RichText::new("FANG Bounded Deterministic Grid Planner (Anti-Martingale / Hard-Cap)")
                .strong()
                .color(Color32::from_rgb(247, 95, 105)),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Symbol:");
                    ui.add(egui::TextEdit::singleline(&mut self.grid_symbol).desired_width(64.0));
                    ui.selectable_value(&mut self.grid_long, true, "LONG");
                    ui.selectable_value(&mut self.grid_long, false, "SHORT");
                    ui.selectable_value(&mut self.grid_geometric, true, "Geometric");
                    ui.selectable_value(&mut self.grid_geometric, false, "Arithmetic");
                    ui.label("Anchor $:");
                    ui.add(egui::TextEdit::singleline(&mut self.grid_anchor_px).desired_width(72.0));
                    ui.label("Cap $:");
                    ui.add(egui::TextEdit::singleline(&mut self.grid_cap_usd).desired_width(72.0));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Levels (1..20):");
                    ui.add(egui::DragValue::new(&mut self.grid_levels).range(1..=20));
                    ui.label("Step bps:");
                    ui.add(egui::DragValue::new(&mut self.grid_step_bps).range(5..=2500));
                    ui.label("Mult (0.5..1.5x):");
                    ui.add(egui::DragValue::new(&mut self.grid_mult_tenths).range(5..=15));
                    ui.label("TP bps:");
                    ui.add(egui::DragValue::new(&mut self.grid_tp_bps).range(10..=5000));
                    ui.label("Stop buf bps:");
                    ui.add(egui::DragValue::new(&mut self.grid_stop_bps).range(10..=5000));
                    if ui.button("Compute Bounded Grid").clicked() {
                        let parsed = (|| -> anyhow::Result<BoundedGridPlan> {
                            let cfg = BoundedGridConfig {
                                symbol: self.grid_symbol.trim().to_string(),
                                side: if self.grid_long { Side::Long } else { Side::Short },
                                anchor_price: Decimal::from_str(self.grid_anchor_px.trim())?,
                                step_bps: Decimal::from(self.grid_step_bps),
                                levels: self.grid_levels,
                                spacing: if self.grid_geometric {
                                    GridSpacingMode::Geometric
                                } else {
                                    GridSpacingMode::Arithmetic
                                },
                                max_total_notional_usd: Decimal::from_str(self.grid_cap_usd.trim())?,
                                size_multiplier: Decimal::new(self.grid_mult_tenths as i64, 1),
                                take_profit_bps: Decimal::from(self.grid_tp_bps),
                                stop_buffer_bps: Decimal::from(self.grid_stop_bps),
                            };
                            fang_planner::plan_bounded_grid(&cfg)
                        })();
                        self.grid_plan = Some(parsed.map_err(|e| format!("{e:#}")));
                    }
                });
                match &self.grid_plan {
                    Some(Ok(plan)) => {
                        ui.label(
                            RichText::new(format!(
                                "Planned Notional: ${} / Cap ${} · Worst Avg Entry: {} · Hard Stop: {} · Span: {} bps (Research Only)",
                                plan.total_planned_notional_usd,
                                plan.max_total_notional_usd,
                                plan.worst_case_avg_entry,
                                plan.protective_stop_price,
                                plan.max_grid_span_bps
                            ))
                            .monospace()
                            .color(Color32::from_rgb(58, 232, 165)),
                        );
                        egui::Grid::new("fang_grid_levels").striped(true).show(ui, |ui| {
                            for h in [
                                "Rung", "Entry Px", "Notional $", "Qty", "Cum $", "Avg Entry",
                                "Take-Profit",
                            ] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for lv in &plan.levels {
                                ui.monospace(format!("#{}", lv.level_index));
                                ui.monospace(lv.entry_price.to_string());
                                ui.monospace(lv.level_notional_usd.to_string());
                                ui.monospace(lv.level_quantity.to_string());
                                ui.monospace(lv.cumulative_notional_usd.to_string());
                                ui.monospace(lv.weighted_avg_entry.to_string());
                                ui.monospace(lv.take_profit_price.to_string());
                                ui.end_row();
                            }
                        });
                        if !plan.levels.is_empty() {
                            let max_notional = plan
                                .levels
                                .iter()
                                .filter_map(|l| l.level_notional_usd.to_f32())
                                .fold(1.0_f32, f32::max)
                                .max(1e-3);
                            let h_px = (plan.levels.len() as f32 * 15.0 + 16.0).clamp(48.0, 220.0);
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width().max(260.0), h_px),
                                Sense::hover(),
                            );
                            let p = ui.painter_at(rect);
                            p.rect_filled(rect, 5, Color32::from_rgb(12, 14, 24));
                            let pad = 8.0;
                            for (i, lv) in plan.levels.iter().enumerate() {
                                let y = rect.top() + pad + i as f32 * 15.0;
                                let w_frac = (lv.level_notional_usd.to_f32().unwrap_or(0.0)
                                    / max_notional)
                                    .clamp(0.05, 1.0);
                                let bar_w = (rect.width() - 160.0).max(40.0) * w_frac;
                                let bar_rect = egui::Rect::from_min_size(
                                    Pos2::new(rect.left() + 145.0, y + 1.0),
                                    Vec2::new(bar_w, 10.0),
                                );
                                p.rect_filled(
                                    bar_rect,
                                    2.0,
                                    Color32::from_rgba_unmultiplied(247, 95, 105, 175),
                                );
                                p.text(
                                    Pos2::new(rect.left() + 8.0, y + 6.0),
                                    egui::Align2::LEFT_CENTER,
                                    format!("#{} @ {}", lv.level_index, lv.entry_price),
                                    egui::FontId::monospace(10.0),
                                    Color32::from_rgb(220, 226, 244),
                                );
                            }
                        }
                    }
                    Some(Err(err)) => {
                        ui.colored_label(Color32::LIGHT_RED, err);
                    }
                    None => {}
                }
            },
        );
    }

    fn show_aegis_stress_simulator(&mut self, ui: &mut egui::Ui) {
        ui.collapsing(
            RichText::new("AEGIS Liquidation Distance & Multi-Shock Margin Stress Matrix")
                .strong()
                .color(Color32::from_rgb(58, 232, 165)),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Equity $:");
                    ui.add(egui::TextEdit::singleline(&mut self.stress_equity).desired_width(72.0));
                    ui.label("Symbol:");
                    ui.add(egui::TextEdit::singleline(&mut self.stress_symbol).desired_width(60.0));
                    ui.selectable_value(&mut self.stress_long, true, "LONG");
                    ui.selectable_value(&mut self.stress_long, false, "SHORT");
                    ui.label("Qty:");
                    ui.add(egui::TextEdit::singleline(&mut self.stress_qty).desired_width(64.0));
                    ui.label("Mark $:");
                    ui.add(egui::TextEdit::singleline(&mut self.stress_mark).desired_width(76.0));
                    ui.label("MMR %:");
                    ui.add(egui::TextEdit::singleline(&mut self.stress_mmr_pct).desired_width(48.0));
                    if ui.button("Run Stress Matrix").clicked() {
                        let res = (|| -> anyhow::Result<MarginStressReport> {
                            let eq = Decimal::from_str(self.stress_equity.trim())?;
                            let qty = Decimal::from_str(self.stress_qty.trim())?;
                            let mark = Decimal::from_str(self.stress_mark.trim())?;
                            let mmr_pct = Decimal::from_str(self.stress_mmr_pct.trim())?;
                            let pos = MarginPositionInput {
                                symbol: self.stress_symbol.trim().to_string(),
                                is_long: self.stress_long,
                                quantity: qty,
                                entry_price: mark,
                                mark_price: mark,
                                maintenance_margin_rate: mmr_pct / Decimal::from(100u32),
                            };
                            let shocks = [
                                Decimal::from(5u32),
                                Decimal::from(10u32),
                                Decimal::from(20u32),
                                Decimal::from(35u32),
                            ];
                            portfolio_research::evaluate_margin_stress(eq, &[pos], &shocks)
                        })();
                        self.stress_report = Some(res.map_err(|e| format!("{e:#}")));
                    }
                });
                match &self.stress_report {
                    Some(Ok(rep)) => {
                        ui.label(
                            RichText::new(format!(
                                "Equity: ${} · Notional: ${} · Gross Leverage: {}x · Maint Margin: ${}",
                                rep.account_equity_usd,
                                rep.total_notional_usd,
                                rep.gross_leverage,
                                rep.total_maintenance_margin_usd
                            ))
                            .monospace(),
                        );
                        for p in &rep.positions {
                            ui.label(
                                RichText::new(format!(
                                    "{} {} · Est. Liq Px: {} ({}% buffer from mark)",
                                    p.symbol,
                                    p.side_label,
                                    p.estimated_liquidation_price,
                                    p.distance_to_liquidation_pct
                                ))
                                .monospace()
                                .color(Color32::from_rgb(54, 205, 245)),
                            );
                        }
                        egui::Grid::new("aegis_stress_grid").striped(true).show(ui, |ui| {
                            for h in [
                                "Adverse Shock",
                                "Est. Loss $",
                                "Stressed Equity $",
                                "Margin Buffer $",
                                "Status",
                            ] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for s in &rep.scenarios {
                                ui.monospace(format!("-{}%", s.adverse_shock_pct));
                                ui.monospace(format!("-${}", s.estimated_loss_usd));
                                ui.monospace(format!("${}", s.stressed_equity_usd));
                                ui.monospace(format!("${}", s.margin_buffer_usd));
                                ui.colored_label(
                                    if s.maintenance_breached {
                                        Color32::LIGHT_RED
                                    } else {
                                        Color32::from_rgb(58, 232, 165)
                                    },
                                    if s.maintenance_breached {
                                        "MAINT BREACH"
                                    } else {
                                        "SURVIVES"
                                    },
                                );
                                ui.end_row();
                            }
                        });
                        if !rep.scenarios.is_empty() {
                            let eq_f = rep.account_equity_usd.to_f32().unwrap_or(1.0).max(1.0);
                            let h_px = (rep.scenarios.len() as f32 * 18.0 + 16.0).clamp(48.0, 140.0);
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width().max(260.0), h_px),
                                Sense::hover(),
                            );
                            let p = ui.painter_at(rect);
                            p.rect_filled(rect, 5, Color32::from_rgb(12, 14, 24));
                            for (i, s) in rep.scenarios.iter().enumerate() {
                                let y = rect.top() + 8.0 + i as f32 * 18.0;
                                let eq_frac = (s.stressed_equity_usd.to_f32().unwrap_or(0.0)
                                    / eq_f)
                                    .clamp(0.0, 1.0);
                                let col = if s.maintenance_breached {
                                    Color32::from_rgb(247, 95, 105)
                                } else {
                                    Color32::from_rgb(58, 232, 165)
                                };
                                let bar_w = (rect.width() - 130.0).max(40.0) * eq_frac;
                                p.rect_filled(
                                    egui::Rect::from_min_size(
                                        Pos2::new(rect.left() + 115.0, y + 2.0),
                                        Vec2::new(bar_w.max(3.0), 11.0),
                                    ),
                                    2.0,
                                    col.gamma_multiply(0.85),
                                );
                                p.text(
                                    Pos2::new(rect.left() + 8.0, y + 7.5),
                                    egui::Align2::LEFT_CENTER,
                                    format!("-{}% shock", s.adverse_shock_pct),
                                    egui::FontId::monospace(10.0),
                                    Color32::from_rgb(218, 224, 242),
                                );
                            }
                        }
                    }
                    Some(Err(err)) => {
                        ui.colored_label(Color32::LIGHT_RED, err);
                    }
                    None => {}
                }
            },
        );
    }

    fn show_cognition_regime_inference(&mut self, ui: &mut egui::Ui) {
        ui.collapsing(
            RichText::new("COGNITION Causal Market-Regime Inference (Zero-Lookahead)")
                .strong()
                .color(Color32::from_rgb(173, 133, 255)),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Candles JSON:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.regime_candles_path)
                            .desired_width(340.0),
                    );
                    ui.label("Lookback bars:");
                    ui.add(egui::DragValue::new(&mut self.regime_lookback).range(4..=500));
                    if ui.button("Infer Regime").clicked() {
                        let res = research::read_json::<Vec<Candle>>(Path::new(
                            self.regime_candles_path.trim(),
                        ))
                        .and_then(|candles| {
                            let cfg = RegimeInferenceConfig {
                                lookback_bars: self.regime_lookback,
                                ..Default::default()
                            };
                            cognition_shadow::infer_regime(&candles, &cfg)
                        });
                        self.regime_result =
                            Some(res.map_err(|e| telemetry::redact_diagnostic(&format!("{e:#}"))));
                    }
                });
                match &self.regime_result {
                    Some(Ok(r)) => {
                        ui.label(
                            RichText::new(format!(
                                "Inferred Regime: {} · Bars: {} · Efficiency: {:.3} · Net Return: {:+.2}% · Bar Log-Ret Std: {:.4}",
                                r.regime.label(),
                                r.bars_evaluated,
                                r.efficiency_ratio,
                                r.net_return_pct,
                                r.bar_log_return_std
                            ))
                            .monospace()
                            .color(Color32::from_rgb(173, 133, 255)),
                        );
                    }
                    Some(Err(err)) => {
                        ui.colored_label(Color32::LIGHT_RED, err);
                    }
                    None => {}
                }
            },
        );
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
