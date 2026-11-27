//! Market perception workspace (offline snapshots + read-only public Hyperliquid radar).
//! Never accesses wallets, private keys, or order submission.
use crate::{
    hunter::{
        self, HunterPolicy, HunterReport, HunterRequest, PublicHunterPolicy, PublicHunterReport,
        HYPERLIQUID_PUBLIC_INFO_URL,
    },
    research, telemetry,
};
use eframe::egui::{self, Color32, Pos2, RichText, Sense, Stroke, Vec2};
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
    public_rx: Option<Receiver<Result<PublicHunterReport, String>>>,
    public_report: Option<PublicHunterReport>,
    public_error: Option<String>,
    public_policy: PublicHunterPolicy,
    error: Option<String>,
}
impl HunterWorkspace {
    pub fn running(&self) -> bool {
        self.receiver.is_some() || self.public_rx.is_some()
    }
    pub fn report(&self) -> Option<&HunterReport> {
        self.report.as_ref()
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref().or(self.public_error.as_deref())
    }

    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        let mut changed = false;
        if let Some(result) = self.receiver.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Snapshot worker ended without a report".into()))
            }
        }) {
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
            changed = true;
        }
        if let Some(result) = self.public_rx.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Public radar worker ended without a report".into()))
            }
        }) {
            self.public_rx = None;
            match result {
                Ok(report) => {
                    self.public_report = Some(report);
                    self.public_error = None;
                }
                Err(error) => {
                    self.public_error = Some(error);
                }
            }
            changed = true;
        }
        if changed {
            ctx.request_repaint();
        }
        changed
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.colored_label(
            Color32::from_rgb(54, 205, 245),
            "HUNTER RADAR · PUBLIC HYPERLIQUID DISCOVERY & OFFLINE SNAPSHOTS",
        );
        ui.label("Eligibility, liquidity, open interest, impact spread, and funding costs. Scores are heuristics, never profit forecasts.");
        ui.label("Read-only market perception: no wallet credentials, signing, capital allocation, or position rotation.");
        ui.add_space(4.0);

        // Live read-only public Hyperliquid acquisition bar
        ui.add_enabled_ui(self.public_rx.is_none(), |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(
                        RichText::new("Fetch Live Public Hyperliquid Radar (Read-Only)")
                            .strong()
                            .color(Color32::from_rgb(54, 205, 245)),
                    )
                    .clicked()
                {
                    let policy = self.public_policy.clone();
                    let prev_watch = self
                        .public_report
                        .as_ref()
                        .map(|r| r.watchlist.clone())
                        .unwrap_or_default();
                    let ctx = ui.ctx().clone();
                    let (tx, rx) = mpsc::channel();
                    self.public_rx = Some(rx);
                    self.public_error = None;
                    std::thread::Builder::new()
                        .name("hydra-hunter-public".into())
                        .spawn(move || {
                            let res = hunter::fetch_hyperliquid_public_snapshot(
                                HYPERLIQUID_PUBLIC_INFO_URL,
                                &policy,
                                &prev_watch,
                                &BTreeSet::new(),
                            )
                            .map_err(|e| telemetry::redact_diagnostic(&format!("{e:#}")));
                            let _ = tx.send(res);
                            ctx.request_repaint();
                        })
                        .ok();
                }
                ui.label("Slots:");
                ui.add(egui::DragValue::new(&mut self.public_policy.slots).range(1..=20));
                ui.label("Min 24h Vol ($M):");
                let mut vol_m = self.public_policy.min_volume_24h_usd / 1_000_000.0;
                if ui
                    .add(
                        egui::DragValue::new(&mut vol_m)
                            .range(0.1..=500.0)
                            .speed(0.5),
                    )
                    .changed()
                {
                    self.public_policy.min_volume_24h_usd = vol_m * 1_000_000.0;
                }
            });
        });
        if self.public_rx.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Querying public Hyperliquid metaAndAssetCtxs (read-only)...");
            });
        }
        if let Some(err) = &self.public_error {
            ui.colored_label(Color32::LIGHT_RED, format!("Public radar error: {err}"));
        }
        if let Some(pub_rep) = &self.public_report {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!(
                    "PUBLIC RADAR ({}): {} perps / {} eligible / {} selected",
                    pub_rep.acquired_at_rfc3339,
                    pub_rep.total_markets,
                    pub_rep.eligible_markets,
                    pub_rep.watchlist.len()
                ));
                ui.colored_label(
                    Color32::from_rgb(54, 205, 245),
                    format!("Watchlist: {}", pub_rep.watchlist.join(" · ")),
                );
            });
            egui::ScrollArea::vertical()
                .id_salt("hunter-public-grid")
                .max_height(210.0)
                .show(ui, |ui| {
                    egui::Grid::new("hunter_public_table")
                        .striped(true)
                        .show(ui, |ui| {
                            for h in [
                                "Symbol",
                                "Status",
                                "Score",
                                "Mark Px",
                                "24h %",
                                "24h Vol ($M)",
                                "OI ($M)",
                                "Spread bps",
                                "Fund bps/h",
                                "Max Lev",
                            ] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for item in pub_rep.ranked.iter().take(40) {
                                let o = &item.observation;
                                ui.monospace(&o.symbol);
                                ui.colored_label(
                                    if item.selected {
                                        Color32::from_rgb(54, 205, 245)
                                    } else if item.eligible {
                                        Color32::from_rgb(58, 232, 165)
                                    } else {
                                        Color32::GRAY
                                    },
                                    if item.selected {
                                        "SELECTED"
                                    } else if item.eligible {
                                        "ELIGIBLE"
                                    } else {
                                        "FILTERED"
                                    },
                                );
                                ui.monospace(
                                    item.score
                                        .map(|s| format!("{s:.1}"))
                                        .unwrap_or_else(|| "—".into()),
                                );
                                ui.monospace(format!("{:.4}", o.mark_px));
                                ui.monospace(
                                    o.change_24h_pct
                                        .map(|c| format!("{c:+.2}%"))
                                        .unwrap_or_else(|| "—".into()),
                                );
                                ui.monospace(format!("{:.2}", o.volume_24h_usd / 1_000_000.0));
                                ui.monospace(format!("{:.2}", o.open_interest_usd / 1_000_000.0));
                                ui.monospace(
                                    o.impact_spread_bps
                                        .map(|s| format!("{s:.2}"))
                                        .unwrap_or_else(|| "—".into()),
                                );
                                ui.monospace(format!("{:+.3}", o.funding_bps_1h));
                                ui.monospace(format!("{}x", o.max_leverage));
                                ui.end_row();
                            }
                        });
                });
            if !pub_rep.ranked.is_empty() {
                let top: Vec<_> = pub_rep
                    .ranked
                    .iter()
                    .filter(|r| r.score.is_some())
                    .take(8)
                    .collect();
                if !top.is_empty() {
                    let max_oi = top
                        .iter()
                        .map(|r| r.observation.open_interest_usd)
                        .fold(1.0_f64, f64::max)
                        .max(1.0);
                    let h_px = (top.len() as f32 * 16.0 + 16.0).clamp(48.0, 160.0);
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width().max(260.0), h_px),
                        Sense::hover(),
                    );
                    let p = ui.painter_at(rect);
                    p.rect_filled(rect, 5, Color32::from_rgb(12, 14, 24));
                    for (i, item) in top.iter().enumerate() {
                        let y = rect.top() + 8.0 + i as f32 * 16.0;
                        let score_frac =
                            (item.score.unwrap_or(0.0) / 100.0).clamp(0.04, 1.0) as f32;
                        let oi_frac =
                            (item.observation.open_interest_usd / max_oi).clamp(0.02, 1.0) as f32;
                        let avail = (rect.width() - 125.0).max(40.0);
                        p.rect_filled(
                            egui::Rect::from_min_size(
                                Pos2::new(rect.left() + 110.0, y + 1.0),
                                Vec2::new(avail * score_frac, 6.0),
                            ),
                            2.0,
                            Color32::from_rgb(54, 205, 245),
                        );
                        p.rect_filled(
                            egui::Rect::from_min_size(
                                Pos2::new(rect.left() + 110.0, y + 8.0),
                                Vec2::new(avail * oi_frac, 4.0),
                            ),
                            1.5,
                            Color32::from_rgb(179, 116, 250),
                        );
                        p.text(
                            Pos2::new(rect.left() + 8.0, y + 6.0),
                            egui::Align2::LEFT_CENTER,
                            format!(
                                "{} {:.1}",
                                item.observation.symbol,
                                item.score.unwrap_or(0.0)
                            ),
                            egui::FontId::monospace(10.0),
                            Color32::from_rgb(220, 228, 244),
                        );
                    }
                }
            }
            ui.separator();
        }
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
