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
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    time::Instant,
};

pub struct MultiMarketLabResult {
    pub portfolio: crate::oracle_research::PortfolioResearchReport,
    pub walk_forward: crate::oracle_research::PurgedWalkForwardReport,
    pub bootstrap: crate::oracle_research::BootstrapInterval,
    pub verdict: crate::qualification::StrategyAcceptanceVerdict,
    pub fingerprint: String,
}

enum Completed {
    Oracle(ReplayReport),
    Validation(crate::oracle_validation::ValidationReport),
    Genesis(SearchReport),
    Assessment(crate::genesis_oos::AssessmentReport),
    GenesisStep {
        progress: crate::genesis::SearchProgress,
        output: String,
    },
    MultiMarket(Box<MultiMarketLabResult>),
    Nsga2(Box<crate::genesis_nsga2::Nsga2CampaignReport>),
}
pub struct ResearchWorkspace {
    db_path: Option<PathBuf>,
    cancel_flag: Option<Arc<AtomicBool>>,
    multi_dataset_path: String,
    multi_folds: usize,
    multi_embargo: usize,
    multi_result: Option<MultiMarketLabResult>,
    nsga2_result: Option<crate::genesis_nsga2::Nsga2CampaignReport>,
    assessment_mode: bool,
    held_out_path: String,
    assessment_options_path: String,
    assessment_limit: usize,
    assessment: Option<crate::genesis_oos::AssessmentReport>,
    last_head: Option<usize>,
    checkpoint_mode: bool,
    checkpoint_input: String,
    checkpoint_output: String,
    generations_per_step: usize,
    saved_checkpoint: Option<(crate::genesis::SearchCheckpoint, String, bool)>,
    validation_mode: bool,
    validation_options_path: String,
    validation: Option<crate::oracle_validation::ValidationReport>,
    dataset_interval_ms: i64,
    dataset_audit: Option<Result<crate::oracle_validation::DatasetIntegrityReport, String>>,
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
            db_path: None,
            cancel_flag: None,
            multi_dataset_path: String::new(),
            multi_folds: 3,
            multi_embargo: 4,
            multi_result: None,
            nsga2_result: None,
            assessment_mode: false,
            held_out_path: String::new(),
            assessment_options_path: String::new(),
            assessment_limit: 4,
            assessment: None,
            last_head: None,
            checkpoint_mode: false,
            checkpoint_input: String::new(),
            checkpoint_output: String::new(),
            generations_per_step: 1,
            saved_checkpoint: None,
            validation_mode: false,
            validation_options_path: String::new(),
            validation: None,
            dataset_interval_ms: 3_600_000,
            dataset_audit: None,
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
    pub fn set_db_path(&mut self, path: PathBuf) {
        self.db_path = Some(path);
    }

    pub fn running_head(&self) -> Option<usize> {
        self.running_head
    }
    pub fn error_head(&self) -> Option<usize> {
        if self.error.is_some() {
            self.last_head
        } else {
            None
        }
    }
    pub fn brief(&self, head: usize) -> Option<String> {
        if head == 5 {
            if self.validation_mode {
                self.validation
                    .as_ref()
                    .map(|r| format!("{} cost scenarios / held-out folds", r.scenarios.len()))
            } else {
                self.oracle
                    .as_ref()
                    .map(|r| format!("{} bars / {} simulated trades", r.bar_count, r.trades.len()))
            }
        } else if self.assessment_mode {
            self.assessment
                .as_ref()
                .map(|r| format!("{} held-out candidates", r.assessments.len()))
        } else {
            self.genesis.as_ref().map(|r| {
                format!(
                    "{} generations / {} candidates",
                    r.generations.len(),
                    r.candidates.len()
                )
            })
        }
    }

    pub fn oracle_ready(&self) -> bool {
        self.oracle.is_some() || self.validation.is_some()
    }
    pub fn genesis_ready(&self) -> bool {
        self.genesis.is_some() || self.assessment.is_some()
    }
    pub fn latest_oracle_summary(&self) -> Option<(String, f64, f64, usize)> {
        if let Some(r) = &self.oracle {
            let ret = if r.initial_equity > 0.0 {
                ((r.final_equity - r.initial_equity) / r.initial_equity) * 100.0
            } else {
                0.0
            };
            return Some((
                r.symbol.clone(),
                ret,
                r.max_drawdown_fraction * 100.0,
                r.trades.len(),
            ));
        }
        self.validation.as_ref().map(|v| {
            let first = v.scenarios.first();
            (
                first
                    .map(|s| s.scenario.name.clone())
                    .unwrap_or_else(|| "walk-forward".into()),
                first
                    .map(|s| s.summary.mean_fold_return * 100.0)
                    .unwrap_or(0.0),
                first
                    .map(|s| s.summary.worst_fold_drawdown_fraction * 100.0)
                    .unwrap_or(0.0),
                first.map(|s| s.summary.trade_count).unwrap_or(0),
            )
        })
    }
    pub fn latest_genesis_summary(&self) -> Option<(u64, usize, f64)> {
        self.genesis.as_ref().map(|r| {
            let best_ret = r
                .candidates
                .iter()
                .map(|c| c.objectives.net_return * 100.0)
                .fold(f64::NEG_INFINITY, f64::max);
            (
                r.generations.len() as u64,
                r.pareto_candidate_ids.len(),
                if best_ret.is_finite() { best_ret } else { 0.0 },
            )
        })
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
        let was_cancelled = self
            .cancel_flag
            .as_ref()
            .is_some_and(|f| f.load(Ordering::SeqCst));
        let head = self.running_head.take();
        self.receiver = None;
        self.cancel_flag = None;
        self.started = None;
        if was_cancelled {
            self.error = Some("Research job cancelled before accepting results".into());
            ctx.request_repaint();
            return head;
        }
        match result {
            Ok(Completed::Validation(report)) => {
                self.validation = Some(report);
                self.error = None;
            }
            Ok(Completed::Oracle(report)) => {
                self.oracle = Some(report);
                self.error = None;
            }
            Ok(Completed::GenesisStep { progress, output }) => {
                self.saved_checkpoint = Some((progress.checkpoint, output, progress.complete));
                self.genesis = Some(progress.report);
                self.error = None;
            }
            Ok(Completed::Assessment(report)) => {
                self.assessment = Some(report);
                self.error = None;
            }
            Ok(Completed::Genesis(report)) => {
                self.genesis = Some(report);
                self.error = None;
            }
            Ok(Completed::MultiMarket(res)) => {
                if let Some(ref db_p) = self.db_path {
                    if let Ok(st) = crate::storage::Storage::open(db_p) {
                        let _ = crate::oracle_research::persist_oracle_research_event(
                            &st,
                            &format!("lab-oracle-{}", chrono::Utc::now().timestamp()),
                            "default",
                            &res.fingerprint,
                            &res.portfolio,
                        );
                    }
                }
                self.multi_result = Some(*res);
                self.error = None;
            }
            Ok(Completed::Nsga2(rep)) => {
                if let Some(ref db_p) = self.db_path {
                    if let Ok(st) = crate::storage::Storage::open(db_p) {
                        let _ = crate::genesis_nsga2::persist_genesis_research_event(
                            &st,
                            &format!("lab-nsga2-{}", chrono::Utc::now().timestamp()),
                            &rep,
                        );
                    }
                }
                self.nsga2_result = Some(*rep);
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
            if !optimizer {
                ui.checkbox(
                    &mut self.validation_mode,
                    "Rolling held-out / cost scenarios",
                );
            }
            path(ui, "Strategy TOML", &mut self.config_path);
            path(ui, "Candles JSON", &mut self.candles_path);
            if !optimizer {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Expected bar interval (ms):");
                    ui.add(
                        egui::DragValue::new(&mut self.dataset_interval_ms)
                            .range(1_000..=86_400_000)
                            .speed(1_000),
                    );
                    if ui.button("Audit Dataset Integrity & SHA-256").clicked() {
                        let res = research::read_json::<Vec<Candle>>(Path::new(
                            self.candles_path.trim(),
                        ))
                        .and_then(|bars| {
                            crate::oracle_validation::audit_candle_dataset(
                                &bars,
                                self.dataset_interval_ms,
                            )
                        });
                        self.dataset_audit =
                            Some(res.map_err(|e| telemetry::redact_diagnostic(&format!("{e:#}"))));
                    }
                });
                match &self.dataset_audit {
                    Some(Ok(rep)) => {
                        ui.label(
                            RichText::new(format!(
                                "Integrity: {} · {} bars · coverage {:.1}% · gaps {} · invalid OHLC {} · spikes {} · sha256={}…",
                                if rep.integrity_ok { "PASS" } else { "ISSUES FOUND" },
                                rep.bar_count,
                                rep.coverage_fraction * 100.0,
                                rep.missing_interval_gaps,
                                rep.invalid_ohlc_bars,
                                rep.extreme_return_spikes,
                                &rep.dataset_sha256[..16]
                            ))
                            .monospace()
                            .color(if rep.integrity_ok {
                                Color32::from_rgb(58, 232, 165)
                            } else {
                                Color32::from_rgb(246, 188, 83)
                            }),
                        );
                    }
                    Some(Err(err)) => {
                        ui.colored_label(Color32::LIGHT_RED, err);
                    }
                    None => {}
                }
            }
            if optimizer {
                if ui.checkbox(&mut self.checkpoint_mode, "Resumable generation steps").changed() && self.checkpoint_mode {self.assessment_mode=false;}
                if ui.checkbox(&mut self.assessment_mode,"Assess frozen candidates / later held-out data").changed() && self.assessment_mode {self.checkpoint_mode=false;}
                if self.assessment_mode {
                    path(ui,"Completed training checkpoint",&mut self.checkpoint_input);
                    path(ui,"Later held-out candles JSON",&mut self.held_out_path);
                    path(ui,"Validation options JSON (optional)",&mut self.assessment_options_path);
                    ui.horizontal(|ui|{ui.label("Training-frontier candidate limit");ui.add(egui::DragValue::new(&mut self.assessment_limit).range(1..=16));});
                    ui.label("Candles JSON above is TRAINING data. All held-out history/test bars must be later.");
                }
                if self.checkpoint_mode {
                    path(
                        ui,
                        "Input checkpoint (blank starts new)",
                        &mut self.checkpoint_input,
                    );
                    path(
                        ui,
                        "New output checkpoint (must not exist)",
                        &mut self.checkpoint_output,
                    );
                    ui.horizontal(|ui| {
                        ui.label("Generations this step");
                        ui.add(egui::DragValue::new(&mut self.generations_per_step).range(1..=100));
                    });
                }
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
            } else if self.validation_mode {
                path(
                    ui,
                    "Validation options JSON (optional)",
                    &mut self.validation_options_path,
                );
                ui.label(
                    "Fixed parameters; disjoint held-out windows; independent capital per fold.",
                );
                if ui.button("Copy default validation options JSON").clicked() {
                    if let Ok(json) = serde_json::to_string_pretty(
                        &crate::oracle_validation::ValidationOptions::default(),
                    ) {
                        ui.ctx().copy_text(json);
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
                if self.assessment_mode {"Assess frozen training frontier"} else if self.checkpoint_mode {
                    "Run optimizer step / save checkpoint"
                } else {
                    "Run bounded optimization"
                }
            } else {
                if self.validation_mode {
                    "Run held-out validation"
                } else {
                    "Run backtest"
                }
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
                if ui.button("Cancel Job").clicked() {
                    if let Some(ref flag) = self.cancel_flag {
                        flag.store(true, Ordering::SeqCst);
                    }
                    self.receiver = None;
                    self.cancel_flag = None;
                    self.running_head = None;
                    self.started = None;
                    self.error = Some("Research job cancelled".into());
                }
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
                if self.assessment_mode {
                    self.assessment.is_some()
                } else {
                    self.genesis.is_some()
                }
            } else if self.validation_mode {
                self.validation.is_some()
            } else {
                self.oracle.is_some()
            })
        {
            ui.label("Displayed results belong to the last completed job.");
        }
        if optimizer {
            if let Some((checkpoint, path, complete)) = &self.saved_checkpoint {
                ui.label(format!(
                    "Last saved checkpoint: {path} / {} complete generations / complete {complete}",
                    checkpoint.state.generations.len()
                ));
                ui.label("Use this as the next input with a new output filename. Checksum is not authenticity or financial proof.");
                copy_report(ui, "Copy last saved checkpoint JSON", checkpoint);
            }
            if self.assessment_mode {
                if let Some(report) = &self.assessment {
                    assessment_report(ui, report);
                }
            } else if let Some(report) = &self.genesis {
                genesis_report(ui, report);
            }
        } else if self.validation_mode {
            if let Some(report) = &self.validation {
                validation_report(ui, report);
            }
        } else if let Some(report) = &self.oracle {
            oracle_report(ui, report);
        }
        ui.separator();
        ui.collapsing(
            RichText::new("ORACLE Multi-Market & Purged Walk-Forward / GENESIS NSGA-II Laboratory")
                .strong()
                .color(Color32::from_rgb(179, 116, 250)),
            |ui| {
                ui.add_enabled_ui(self.receiver.is_none(), |ui| {
                    path(ui, "MultiMarketDataset JSON", &mut self.multi_dataset_path);
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Folds:");
                        ui.add(egui::DragValue::new(&mut self.multi_folds).range(2..=8));
                        ui.label("Embargo bars:");
                        ui.add(egui::DragValue::new(&mut self.multi_embargo).range(1..=48));
                        if ui
                            .button("Run Multi-Market Evaluation + Scientific Gate")
                            .clicked()
                        {
                            self.start_multi_market(false, ui.ctx());
                        }
                        if ui.button("Run Bounded NSGA-II Campaign").clicked() {
                            self.start_multi_market(true, ui.ctx());
                        }
                    });
                });
                if let Some(ref res) = self.multi_result {
                    ui.label(
                        RichText::new(format!(
                            "SIMULATED MULTI-MARKET · {} trades · net={:+.2}% · excess_vs_BH={:+.2}% · maxDD={:.2}% · worst_OOS_DD={:.2}% · boot_p05={:+.2}% · fp={}…",
                            res.portfolio.metrics.trades_count,
                            res.portfolio.metrics.net_return_pct,
                            res.portfolio.excess_over_buy_and_hold_pct,
                            res.portfolio.metrics.max_drawdown_pct,
                            res.walk_forward.worst_oos_drawdown_pct,
                            res.bootstrap.p05_mean_return_pct,
                            &res.fingerprint[..16.min(res.fingerprint.len())]
                        ))
                        .monospace()
                        .small(),
                    );
                    match &res.verdict {
                        crate::qualification::StrategyAcceptanceVerdict::EligibleForMonitoredPilot { summary } => {
                            ui.colored_label(Color32::from_rgb(58, 232, 165), summary);
                        }
                        crate::qualification::StrategyAcceptanceVerdict::RejectedKeepDisabled { reasons } => {
                            ui.colored_label(
                                Color32::from_rgb(246, 188, 83),
                                format!("REJECTED (keep disabled): {}", reasons.join("; ")),
                            );
                        }
                    }
                }
                if let Some(ref rep) = self.nsga2_result {
                    ui.label(
                        RichText::new(format!(
                            "NSGA-II (workers={}, auto_deploy={}) · pareto={} · held_out={} · eval={}",
                            rep.workers_used,
                            rep.auto_deployment_authorized,
                            rep.pareto_front.len(),
                            rep.held_out_assessments.len(),
                            rep.checkpoint.evaluated.len()
                        ))
                        .monospace()
                        .small()
                        .color(Color32::from_rgb(54, 205, 245)),
                    );
                }
            },
        );
    }
    fn start_multi_market(&mut self, nsga2: bool, ctx: &egui::Context) {
        if self.receiver.is_some() {
            return;
        }
        let ds_path = self.multi_dataset_path.trim().to_string();
        if ds_path.is_empty() {
            self.error = Some("Select a MultiMarketDataset JSON path".into());
            return;
        }
        let folds = self.multi_folds.max(2);
        let embargo = self.multi_embargo.max(1);
        let flag = Arc::new(AtomicBool::new(false));
        let flag_bg = flag.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        let context = ctx.clone();
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .max(2);
        let max_workers = cores.saturating_sub(1).clamp(1, 4);
        let result = std::thread::Builder::new()
            .name("hydra-lab-multi-market".into())
            .spawn(move || {
                let mut sched = crate::qualification::ResourceBudgetScheduler::new(cores, 1).ok();
                let acquired = sched.as_mut().is_some_and(|s| {
                    s.try_acquire(crate::qualification::WorkloadPriority::ResearchBackground)
                });
                let res = (|| -> anyhow::Result<Completed> {
                    let ds: crate::oracle_research::MultiMarketDataset =
                        research::read_json(Path::new(&ds_path))?;
                    let (timeline, _) = ds.validate_and_fingerprint()?;
                    let fang_cfg = crate::fang_engine::UnifiedStrategyConfig::default();
                    let exec = crate::oracle_research::ExecutionModelConfig {
                        warmup_bars: 12,
                        latency_bars: 1,
                        ..Default::default()
                    };
                    if flag_bg.load(Ordering::SeqCst) {
                        anyhow::bail!("Cancelled before execution");
                    }
                    if nsga2 {
                        let train_bars = (timeline.len() / 2)
                            .max(24)
                            .min(timeline.len().saturating_sub(embargo + 16));
                        let opts = crate::genesis_nsga2::Nsga2Options {
                            max_workers,
                            ..Default::default()
                        };
                        let rep = crate::genesis_nsga2::run_nsga2_campaign(
                            &ds, train_bars, embargo, &opts, None,
                        )?;
                        Ok(Completed::Nsga2(Box::new(rep)))
                    } else {
                        let fp = ds.experiment_fingerprint(&fang_cfg, &exec)?;
                        let portfolio =
                            crate::oracle_research::simulate_portfolio(&ds, &fang_cfg, &exec)?;
                        let test_bars = (timeline.len() / (folds + 2)).max(exec.warmup_bars + 4);
                        let train_bars = timeline
                            .len()
                            .saturating_sub(folds * test_bars + embargo)
                            .max(exec.warmup_bars + 2);
                        let wf = crate::oracle_research::walk_forward_purged_cv(
                            &ds, train_bars, embargo, test_bars, test_bars, &fang_cfg, &exec,
                        )?;
                        let mut oos_bars = Vec::new();
                        for f in &wf.folds {
                            if flag_bg.load(Ordering::SeqCst) {
                                anyhow::bail!("Cancelled by operator");
                            }
                            let fold_ds = ds.slice_range(f.test_range.0, f.test_range.1)?;
                            let (_, bars) =
                                crate::oracle_research::simulate_portfolio_with_bar_returns(
                                    &fold_ds, &fang_cfg, &exec,
                                )?;
                            oos_bars.extend(bars);
                        }
                        let boot = if oos_bars.len() < 10 {
                            crate::oracle_research::BootstrapInterval {
                                samples: oos_bars.len(),
                                block_len: 1,
                                resamples: 100,
                                p05_mean_return_pct: -100.0,
                                median_mean_return_pct: 0.0,
                                p95_mean_return_pct: 0.0,
                            }
                        } else {
                            let bl = 6.min(oos_bars.len() / 2).max(1);
                            crate::oracle_research::block_bootstrap_ci(&oos_bars, bl, 200, 7)?
                        };
                        let verdict = crate::qualification::evaluate_scientific_strategy_acceptance(
                            &portfolio,
                            &wf,
                            &boot,
                            &crate::qualification::ScientificAcceptanceCriteria::default(),
                        );
                        Ok(Completed::MultiMarket(Box::new(MultiMarketLabResult {
                            portfolio,
                            walk_forward: wf,
                            bootstrap: boot,
                            verdict,
                            fingerprint: fp,
                        })))
                    }
                })()
                .map_err(|e| telemetry::redact_diagnostic(&format!("{e:#}")));
                if acquired {
                    if let Some(ref mut s) = sched {
                        s.release(crate::qualification::WorkloadPriority::ResearchBackground);
                    }
                }
                let _ = tx.send(res);
                context.request_repaint();
            });
        match result {
            Ok(_) => {
                self.cancel_flag = Some(flag);
                self.receiver = Some(rx);
                self.running_head = Some(if nsga2 { 6 } else { 5 });
                self.started = Some(Instant::now());
                self.error = None;
            }
            Err(error) => self.error = Some(format!("Could not start laboratory worker: {error}")),
        }
    }
    fn start(&mut self, optimizer: bool, ctx: &egui::Context) {
        if self.receiver.is_some() {
            return;
        }
        self.last_head = Some(if optimizer { 6 } else { 5 });
        if self.candles_path.trim().is_empty() || self.config_path.trim().is_empty() {
            self.error = Some("Select both strategy configuration and candle dataset paths".into());
            return;
        }
        if optimizer && self.checkpoint_mode && self.checkpoint_output.trim().is_empty() {
            self.error = Some(
                "Choose a new output checkpoint path; existing files are never overwritten".into(),
            );
            return;
        }
        let assessment_mode = optimizer && self.assessment_mode;
        if assessment_mode
            && (self.checkpoint_input.trim().is_empty() || self.held_out_path.trim().is_empty())
        {
            self.error =
                Some("Choose a completed training checkpoint and later held-out dataset".into());
            return;
        }
        let held_out_path = self.held_out_path.clone();
        let assessment_options_path = self.assessment_options_path.clone();
        let assessment_limit = self.assessment_limit;
        let checkpoint_mode = optimizer && self.checkpoint_mode;
        let checkpoint_input = self.checkpoint_input.clone();
        let checkpoint_output = self.checkpoint_output.clone();
        let generations_per_step = self.generations_per_step;
        let config = self.config_path.clone();
        let candles = self.candles_path.clone();
        let validation = !optimizer && self.validation_mode;
        let options = if validation {
            self.validation_options_path.clone()
        } else if optimizer {
            self.genesis_options_path.clone()
        } else {
            self.oracle_options_path.clone()
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let context = ctx.clone();
        let flag = Arc::new(AtomicBool::new(false));
        let result = std::thread::Builder::new()
            .name("hydra-offline-research".into())
            .spawn(move || {
                let result =
                    (|| -> anyhow::Result<Completed> {
                        let cfg = AppConfig::load_research(&config, None)?;
                        let candles: Vec<Candle> = research::read_json(Path::new(&candles))?;
                        if validation {
                            let options = if options.trim().is_empty() {
                                crate::oracle_validation::ValidationOptions::default()
                            } else {
                                research::read_json(Path::new(&options))?
                            };
                            Ok(Completed::Validation(crate::oracle_validation::evaluate(
                                &cfg, &candles, &options,
                            )?))
                        } else if optimizer {
                            let options: SearchOptions = if options.trim().is_empty() {
                                SearchOptions::default()
                            } else {
                                research::read_json(Path::new(&options))?
                            };
                            if assessment_mode {
                                let checkpoint =
                                    research::read_json::<crate::genesis::SearchCheckpoint>(
                                        Path::new(&checkpoint_input),
                                    )?;
                                let held_out =
                                    research::read_json::<Vec<Candle>>(Path::new(&held_out_path))?;
                                let validation = if assessment_options_path.trim().is_empty() {
                                    crate::oracle_validation::ValidationOptions::default()
                                } else {
                                    research::read_json(Path::new(&assessment_options_path))?
                                };
                                Ok(Completed::Assessment(crate::genesis_oos::evaluate(
                                    &cfg,
                                    &candles,
                                    &held_out,
                                    &options,
                                    &checkpoint,
                                    &validation,
                                    assessment_limit,
                                )?))
                            } else if checkpoint_mode {
                                let checkpoint = if checkpoint_input.trim().is_empty() {
                                    None
                                } else {
                                    Some(research::read_json::<crate::genesis::SearchCheckpoint>(
                                        Path::new(&checkpoint_input),
                                    )?)
                                };
                                let progress = genesis::advance(
                                    &cfg,
                                    &candles,
                                    &options,
                                    checkpoint.as_ref(),
                                    generations_per_step,
                                )?;
                                crate::genesis_checkpoint::save_new(
                                    Path::new(&checkpoint_output),
                                    &progress.checkpoint,
                                )?;
                                Ok(Completed::GenesisStep {
                                    progress,
                                    output: checkpoint_output,
                                })
                            } else {
                                Ok(Completed::Genesis(genesis::optimize(
                                    &cfg, &candles, &options,
                                )?))
                            }
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
                self.cancel_flag = Some(flag);
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

fn validation_report(ui: &mut egui::Ui, report: &crate::oracle_validation::ValidationReport) {
    ui.heading("ORACLE / held-out cost scenarios");
    ui.label(
        "SIMULATED · fixed configuration · no fitting or automatic selection · folds reset capital",
    );
    ui.label(format!(
        "{} input bars | {} unused trailing bars",
        report.input_bar_count, report.unused_trailing_bars
    ));
    copy_report(ui, "Copy validation report JSON", report);
    for limitation in &report.limitations {
        ui.label(limitation);
    }
    egui::ScrollArea::vertical().id_salt("validation_scenarios").show(ui,|ui| {
        for scenario in &report.scenarios {
            ui.push_id(&scenario.scenario.name,|ui| {
                let s=&scenario.summary;
                ui.heading(&scenario.scenario.name);
                ui.label(format!("Fee {} bps | slip {} bps | {} folds | {} trades",scenario.scenario.fee_bps,scenario.scenario.slippage_bps,s.fold_count,s.trade_count));
                ui.label(format!("Mean fold {:.2}% | worst fold {:.2}% | worst DD {:.2}% | independent net sum {:.4}",s.mean_fold_return*100.0,s.worst_fold_return*100.0,s.worst_fold_drawdown_fraction*100.0,s.pooled_net_pnl));
                for fold in &scenario.folds {
                    ui.collapsing(format!("Fold {} | test [{}..{}) | {} to {} ms",fold.index+1,fold.test_start_index,fold.test_end_index_exclusive,fold.test_start_ms,fold.test_end_ms),|ui| {
                        ui.push_id(fold.index,|ui| oracle_report(ui,&fold.replay));
                    });
                }
            });
        }
    });
}

fn assessment_report(ui: &mut egui::Ui, report: &crate::genesis_oos::AssessmentReport) {
    ui.heading("GENESIS / frozen frontier held-out assessment");
    ui.label("SIMULATED · no automatic selection/deployment · caller state not authenticated");
    ui.label(format!("Training ends {} ms / held-out {} to {} ms / {} assessed / {} frontier candidates unassessed",report.training_end_ms,report.held_out_start_ms,report.held_out_end_ms,report.assessments.len(),report.unassessed_frontier_candidates));
    ui.label(&report.selection_basis);
    copy_report(ui, "Copy held-out candidate report JSON", report);
    ui.collapsing("Assessment limitations", |ui| {
        for limitation in &report.limitations {
            ui.label(limitation);
        }
    });
    egui::ScrollArea::vertical().id_salt("candidate_assessment").show(ui,|ui| {
        for candidate in &report.assessments {ui.push_id(&candidate.candidate_id,|ui| {
            ui.collapsing(&candidate.candidate_id,|ui| {
                ui.label(format!("Fixed parameters {:?}; training objectives are caller checkpoint research state, not financial evidence",candidate.parameters));
                validation_report(ui,&candidate.held_out);
            });
        });}
    });
}
