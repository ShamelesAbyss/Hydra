//! Native read-only command center. Signing and execution are deliberately absent.
use crate::telemetry::{self, TelemetrySnapshot};
use eframe::egui::{self, Color32, FontId, Pos2, RichText, Sense, Stroke, Vec2};
use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, SyncSender},
        Arc,
    },
    time::{Duration, Instant},
};

const BG: Color32 = Color32::from_rgb(4, 6, 12);
const PANEL: Color32 = Color32::from_rgb(9, 11, 21);
const SURFACE: Color32 = Color32::from_rgb(14, 17, 30);
const TEXT: Color32 = Color32::from_rgb(224, 236, 246);
const MUTED: Color32 = Color32::from_rgb(138, 146, 174);
const AMETHYST: Color32 = Color32::from_rgb(179, 116, 250);
const EMERALD: Color32 = Color32::from_rgb(58, 232, 165);
const CYAN: Color32 = Color32::from_rgb(54, 205, 245);
const AMBER: Color32 = Color32::from_rgb(246, 188, 83);
const HEADS: [(&str, &str, Color32); 7] = [
    ("HUNTER", "Market scanning & ranking", CYAN),
    (
        "FANG",
        "Strategy planning & intent",
        Color32::from_rgb(247, 95, 105),
    ),
    (
        "COGNITION",
        "Shadow learning & context",
        Color32::from_rgb(173, 133, 255),
    ),
    ("AEGIS", "Risk protection & shields", EMERALD),
    (
        "NEXUS",
        "Execution & ledger evidence",
        Color32::from_rgb(73, 151, 255),
    ),
    (
        "ORACLE",
        "Backtesting & validation",
        Color32::from_rgb(208, 199, 247),
    ),
    (
        "GENESIS",
        "Optimization & evolution",
        Color32::from_rgb(244, 127, 224),
    ),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Workspace {
    Overview,
    Head(usize),
    Positions,
    BlackBox,
    Source,
}
type SnapshotResponse = (PathBuf, Result<TelemetrySnapshot, String>);

pub struct Cockpit {
    started: Instant,
    last_frame: Option<Instant>,
    last_frame_number: Option<u64>,
    frame_stats: crate::leviathan_state::FrameStats,
    quality: crate::leviathan_scene::SceneQuality,
    reduced_motion: bool,
    frame_limit: u32,
    details_height: f32,
    target_details_height: f32,
    hovered_head: Option<usize>,
    show_graphics: bool,
    borderless: bool,
    renderer: String,
    wgpu_atmosphere: bool,
    research_changed_at: [Option<Instant>; 7],

    research: crate::research_ui::ResearchWorkspace,
    hunter: crate::hunter_ui::HunterWorkspace,
    diagnostics: crate::diagnostics_ui::DiagnosticWorkspace,
    database_input: String,
    active_path: PathBuf,
    requests: SyncSender<PathBuf>,
    responses: Receiver<SnapshotResponse>,
    snapshot: Option<Arc<TelemetrySnapshot>>,
    error: Option<String>,
    loading: bool,
    paused: bool,
    last_request: Instant,
    last_received: Option<Instant>,
    workspace: Workspace,
    filter: String,
    animate_updates: bool,
    changed_at: [Option<Instant>; 7],
    fullscreen: bool,
}

impl Cockpit {
    pub fn configure_presentation(
        &mut self,
        quality: crate::leviathan_scene::SceneQuality,
        reduced_motion: bool,
        frame_limit: u32,
    ) {
        self.quality = quality;
        self.reduced_motion = reduced_motion;
        self.frame_limit = frame_limit.clamp(15, 144);
        self.frame_stats.clear();
    }
    pub fn measured_frames(&self) -> Option<crate::leviathan_state::FrameSummary> {
        self.frame_stats.summary()
    }
    pub fn renderer_info(&self) -> &str {
        &self.renderer
    }

    pub fn new(cc: &eframe::CreationContext<'_>, path: PathBuf) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BG;
        visuals.window_fill = PANEL;
        visuals.extreme_bg_color = Color32::from_rgb(6, 8, 16);
        visuals.override_text_color = Some(TEXT);
        visuals.selection.bg_fill = Color32::from_rgb(64, 38, 96);
        visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(162, 114, 232));
        visuals.widgets.noninteractive.bg_fill = PANEL;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(38, 34, 62));
        visuals.widgets.inactive.bg_fill = SURFACE;
        visuals.widgets.inactive.weak_bg_fill = SURFACE;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(48, 42, 78));
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(24, 24, 46);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(24, 24, 46);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(138, 96, 212));
        visuals.widgets.active.bg_fill = Color32::from_rgb(44, 30, 74);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, AMETHYST);
        cc.egui_ctx.set_visuals(visuals);
        let mut style = cc.egui_ctx.style_of(egui::Theme::Dark).as_ref().clone();
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.button_padding = Vec2::new(10.0, 4.5);
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(12.5));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, FontId::proportional(19.5));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, FontId::monospace(12.0));
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);

        #[cfg(feature = "desktop-wgpu")]
        let wgpu_atmosphere = crate::leviathan_shader::register_wgpu_atmosphere(cc);
        #[cfg(not(feature = "desktop-wgpu"))]
        let wgpu_atmosphere = false;

        let (requests, rx) = mpsc::sync_channel::<PathBuf>(1);
        let (tx, responses) = mpsc::sync_channel::<SnapshotResponse>(1);
        let ctx = cc.egui_ctx.clone();
        // At most one worker, one pending request and one bounded response.
        std::thread::Builder::new()
            .name("hydra-readonly-telemetry".into())
            .spawn(move || {
                while let Ok(path) = rx.recv() {
                    let result = telemetry::load_snapshot(&path, 256)
                        .map_err(|e| telemetry::redact_diagnostic(&format!("{e:#}")));
                    if tx.send((path, result)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("create read-only telemetry worker");
        let mut app = Self {
            started: Instant::now(),
            last_frame: None,
            last_frame_number: None,
            frame_stats: crate::leviathan_state::FrameStats::default(),
            quality: crate::leviathan_scene::SceneQuality::High,
            reduced_motion: false,
            frame_limit: 60,
            details_height: 270.0,
            target_details_height: 270.0,
            hovered_head: None,
            show_graphics: false,
            borderless: true,
            renderer: crate::desktop_renderer::renderer_description(cc),
            wgpu_atmosphere,
            research_changed_at: [None; 7],

            research: crate::research_ui::ResearchWorkspace::default(),
            hunter: crate::hunter_ui::HunterWorkspace::default(),
            diagnostics: crate::diagnostics_ui::DiagnosticWorkspace::default(),
            database_input: path.display().to_string(),
            active_path: path,
            requests,
            responses,
            snapshot: None,
            error: None,
            loading: false,
            paused: false,
            last_request: Instant::now(),
            last_received: None,
            workspace: Workspace::Overview,
            filter: String::new(),
            animate_updates: true,
            changed_at: [None; 7],
            fullscreen: false,
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        if self.requests.try_send(self.active_path.clone()).is_ok() {
            self.loading = true;
            self.last_request = Instant::now();
        }
    }
    fn receive(&mut self) {
        while let Ok((path, result)) = self.responses.try_recv() {
            if path != self.active_path {
                continue;
            }
            self.loading = false;
            match result {
                Ok(snapshot) => {
                    if let Some(previous) = &self.snapshot {
                        let before = fingerprints(previous);
                        let after = fingerprints(&snapshot);
                        for i in 0..7 {
                            if before[i] != after[i] {
                                self.changed_at[i] = Some(Instant::now());
                            }
                        }
                    }
                    self.snapshot = Some(Arc::new(snapshot));
                    self.error = None;
                    self.last_received = Some(Instant::now());
                }
                Err(error) => self.error = Some(error),
            }
        }
    }
    fn source_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("SQLite source");
            ui.add(egui::TextEdit::singleline(&mut self.database_input).desired_width(420.0));
            if ui.button("Open read-only").clicked() {
                let path = PathBuf::from(self.database_input.trim());
                if path != self.active_path {
                    self.active_path = path;
                    self.snapshot = None;
                    self.error = None;
                    self.last_received = None;
                }
                self.refresh();
            }
            if ui.button("Refresh").clicked() {
                self.refresh();
            }
            ui.checkbox(&mut self.paused, "Pause viewer");
            ui.checkbox(&mut self.animate_updates, "Update pulses");
        });
        if let Some(error) = &self.error {
            ui.colored_label(AMBER, format!("SOURCE UNAVAILABLE · {error}"));
        } else if let Some(data) = &self.snapshot {
            ui.label(RichText::new(format!("Consistent read snapshot · {} · {} rows maximum per view · engine connectivity unverified",data.captured_at,data.row_limit)).small().color(MUTED));
        } else {
            ui.label(
                RichText::new(
                    "Choose an existing Hydra database. The viewer never creates or migrates it.",
                )
                .color(MUTED),
            );
        }
    }
    fn body(&mut self, ui: &mut egui::Ui) {
        if self.workspace == Workspace::Source
            || (self.workspace == Workspace::Overview && self.snapshot.is_none())
        {
            self.source_controls(ui);
        }

        if let Workspace::Head(index @ (1..=3)) = self.workspace {
            ui.heading(RichText::new(HEADS[index].0).color(HEADS[index].2));
            self.diagnostics.show(ui, index);
            if let Some(data) = self.snapshot.clone() {
                ui.collapsing("Recorded engine history",|ui| match index {
                    1=>signals(ui,&data,&self.filter),
                    2=>{
                        metric(ui,"Legacy learning context records",&count_text(&data,"trade_context"),HEADS[2].2);
                        ui.colored_label(AMBER,"Historical context may contain estimated outcomes. This viewer does not certify those outcomes or feed them into learning.");
                        ui.label("The authoritative ledger remains separate until complete fill, fee and funding attribution is proven.");
                        signals(ui,&data,&self.filter);
                    },
                    _=>risk(ui,&data),
                });
            }
            return;
        }

        if self.workspace == Workspace::Head(0) {
            ui.heading(RichText::new("HUNTER / MARKET RADAR").color(CYAN));
            self.hunter.show(ui);
            if let Some(data) = self.snapshot.clone() {
                ui.collapsing("Observed engine instruments", |ui| {
                    observed_markets(ui, &data)
                });
            }
            return;
        }
        if let Workspace::Head(index @ (5 | 6)) = self.workspace {
            ui.heading(RichText::new(HEADS[index].0).color(HEADS[index].2));
            self.research.show(ui, index == 6);
            return;
        }
        let Some(data) = self.snapshot.clone() else {
            ui.add_space(6.0);
            ui.heading("Connect to your local engine history");
            ui.label(
                "Enter the path to an existing Hydra SQLite database and select Open read-only.",
            );
            ui.label("No credentials are requested. Recorded data is displayed only after a successful, consistent read.");
            ui.add_space(12.0);
            if !std::path::Path::new(&self.database_input).exists() {
                ui.label("First launch? Initialize empty local history at the path above. Trading remains disabled.");
                if ui.button("Initialize empty local history").clicked() {
                    match crate::local_setup::initialize_history(std::path::Path::new(
                        &self.database_input,
                    )) {
                        Ok(()) => {
                            self.active_path = PathBuf::from(&self.database_input);
                            self.error = None;
                            self.refresh();
                        }
                        Err(error) => {
                            self.error = Some(telemetry::redact_diagnostic(&format!("{error:#}")))
                        }
                    }
                }
            }
            ui.add_space(20.0);

            return;
        };
        if self.error.is_some() {
            ui.colored_label(
                AMBER,
                "Showing the last successful snapshot; this view may be stale.",
            );
        }
        match self.workspace {
            Workspace::Overview => {
                self.overview(ui, &data);
            }
            Workspace::Head(index) => {
                let (name, description, color) = HEADS[index];
                ui.heading(RichText::new(name).color(color));
                ui.label(description);
                ui.separator();
                match index {
                    0 => observed_markets(ui, &data),
                    1 => {
                        ui.label("Persisted strategy decisions. No current strategy activity is inferred from history.");
                        signals(ui, &data, &self.filter);
                    }
                    2 => {
                        metric(
                            ui,
                            "Legacy learning context records",
                            &count_text(&data, "trade_context"),
                            color,
                        );
                        ui.colored_label(AMBER,"Historical context may contain estimated outcomes. This viewer does not certify those outcomes or feed them into learning.");
                        ui.label("The authoritative ledger remains separate until complete fill, fee and funding attribution is proven.");
                        signals(ui, &data, &self.filter);
                    }
                    3 => risk(ui, &data),
                    4 => {
                        self.search(ui);
                        journal(ui, &data, &self.filter);
                        ui.add_space(16.0);
                        fills(ui, &data, &self.filter);
                    }
                    _ => {}
                }
            }
            Workspace::Positions => {
                ui.heading("Portfolio / positions");
                positions(ui, &data);
                ui.add_space(20.0);
                equity_chart(ui, &data);
            }
            Workspace::BlackBox => {
                ui.heading("Black box");
                self.search(ui);
                events(ui, &data, &self.filter);
            }
            Workspace::Source => {
                ui.heading("Read-only research surface · schema v1");
                ui.monospace(&data.database_path);
                ui.label(format!("Consistency: {}", data.consistency));
                if ui.button("Copy redacted telemetry JSON").clicked() {
                    match serde_json::to_string_pretty(data.as_ref()) {
                        Ok(json) => ui.ctx().copy_text(json),
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                for diagnostic in &data.diagnostics {
                    ui.label(RichText::new(diagnostic).color(MUTED));
                }
                ui.separator();
                schema(ui, &data);
            }
        }
    }
    fn search(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Search this bounded view");
            ui.text_edit_singleline(&mut self.filter);
            if ui.button("Clear").clicked() {
                self.filter.clear();
            }
        });
        ui.add_space(8.0);
    }
    fn overview(&mut self, ui: &mut egui::Ui, data: &TelemetrySnapshot) {
        ui.horizontal_wrapped(|ui| {
            metric(
                ui,
                "Open position rows",
                &data.positions.len().to_string(),
                EMERALD,
            );
            metric(
                ui,
                "Recent execution rows",
                &data.intents.len().to_string(),
                CYAN,
            );
            metric(
                ui,
                "Ledger fill records",
                &count_text(data, "ledger_fills"),
                HEADS[4].2,
            );
            metric(
                ui,
                "Funding records",
                &count_text(data, "ledger_funding"),
                HEADS[2].2,
            );
        });

        if let Some(session) = &data.latest_session {
            ui.label(
                RichText::new(format!(
                    "Last persisted session #{} · {} · configuration={} · {}",
                    session.id,
                    session.started_at,
                    if session.configured_live {
                        "LIVE"
                    } else {
                        "DRY RUN"
                    },
                    if session.ended_at.is_some() {
                        "ended"
                    } else {
                        "end not recorded"
                    }
                ))
                .color(MUTED),
            );
            ui.label(
                RichText::new("This historical session flag is not current live authorization.")
                    .small()
                    .color(AMBER),
            );
        }
        ui.add_space(12.0);
        equity_chart(ui, data);
        ui.add_space(12.0);
        ui.heading("Latest persisted decisions");
        signals(ui, data, "");
    }
    fn head_states(&self) -> [crate::leviathan_state::HeadState; 7] {
        use crate::leviathan_state::{HeadInputs, HeadState};
        let mut evidence: [HeadInputs; 7] = std::array::from_fn(|_| HeadInputs {
            source_available: self.snapshot.is_some(),
            waiting_for_data: true,
            ..Default::default()
        });
        if let Some(data) = &self.snapshot {
            evidence[0].observations_available = !data.signals.is_empty();
            evidence[1].observations_available = !data.signals.is_empty();
            evidence[2].observations_available =
                data.count("trade_context").is_some_and(|v| v.value > 0);
            evidence[3].observations_available = !data.risk_accounts.is_empty();
            evidence[3].blocked = data
                .risk_accounts
                .iter()
                .any(|v| v.breaker_reason.is_some());
            evidence[4].observations_available = !data.intents.is_empty() || !data.fills.is_empty();
            evidence[4].warning = data.intents.iter().any(|v| {
                matches!(
                    v.state.as_str(),
                    "submitted" | "unresolved" | "unknown" | "ambiguous"
                )
            });
        }
        evidence[4].faulted = self.error.is_some();
        evidence[0].job_running = self.hunter.running();
        evidence[0].research_result_available = self.hunter.report().is_some();
        evidence[0].faulted = self.hunter.error().is_some();
        if let Some(head) = self.diagnostics.running_head() {
            evidence[head].job_running = true;
        }
        if let Some(head) = self.diagnostics.report_head() {
            evidence[head].research_result_available = true;
        }
        if let Some(head) = self.diagnostics.error_head() {
            evidence[head].faulted = true;
        }
        if let Some(crate::diagnostics::DiagnosticReport::Aegis(r)) = self.diagnostics.report() {
            evidence[3].warning = !r.breaches.is_empty() || !r.unknowns.is_empty();
        }
        if let Some(head) = self.research.running_head() {
            evidence[head].job_running = true;
        }
        if let Some(head) = self.research.error_head() {
            evidence[head].faulted = true;
        }
        evidence[5].last_result_simulated = self.research.oracle_ready();
        evidence[6].last_result_simulated = self.research.genesis_ready();
        evidence.map(HeadState::classify)
    }
    fn active_jobs(&self) -> usize {
        usize::from(self.hunter.running())
            + usize::from(self.diagnostics.running_head().is_some())
            + usize::from(self.research.running_head().is_some())
    }
    fn command_bar(&mut self, ui: &mut egui::Ui, states: &[crate::leviathan_state::HeadState; 7]) {
        let drag = ui.interact(
            ui.max_rect(),
            ui.id().with("native-title-drag"),
            Sense::click_and_drag(),
        );
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }
        ui.horizontal(|ui| {
            // Miniature glowing amethyst Leviathan crest emblem.
            let (crest_rect, _) = ui.allocate_exact_size(Vec2::new(22.0, 24.0), Sense::hover());
            let cp = ui.painter_at(crest_rect);
            let cc = crest_rect.center();
            cp.circle_filled(cc, 9.0, Color32::from_rgba_unmultiplied(166, 98, 245, 38));
            cp.add(egui::Shape::convex_polygon(
                vec![
                    cc + Vec2::new(0.0, -9.5),
                    cc + Vec2::new(7.5, 0.0),
                    cc + Vec2::new(0.0, 9.5),
                    cc + Vec2::new(-7.5, 0.0),
                ],
                Color32::from_rgb(130, 76, 206),
                Stroke::new(1.1, Color32::from_rgb(220, 190, 255)),
            ));
            cp.circle_filled(cc, 2.2, Color32::from_rgb(245, 232, 255));

            ui.label(
                RichText::new("HYDRA")
                    .size(26.0)
                    .strong()
                    .color(Color32::from_rgb(214, 186, 255)),
            );
            ui.label(
                RichText::new("PROJECT  /  LEVIATHAN")
                    .size(15.0)
                    .color(Color32::from_rgb(196, 210, 228)),
            );
            ui.label(
                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .monospace()
                    .small()
                    .color(MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("×")
                    .on_hover_text("Close cockpit; the independent engine receives no stop command")
                    .clicked()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("□").on_hover_text("Maximize / restore").clicked() {
                    let value = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Maximized(!value));
                }
                if ui.button("—").on_hover_text("Minimize cockpit").clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
                if ui
                    .selectable_label(self.show_graphics, "Graphics")
                    .clicked()
                {
                    self.show_graphics = !self.show_graphics;
                }
                if ui
                    .button(if self.fullscreen {
                        "Windowed"
                    } else {
                        "Fullscreen"
                    })
                    .clicked()
                {
                    self.fullscreen = !self.fullscreen;
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
                }
                ui.label(
                    RichText::new(chrono::Local::now().format("%H:%M:%S").to_string())
                        .monospace()
                        .color(Color32::from_rgb(168, 174, 202)),
                );
            });
        });
        ui.horizontal_wrapped(|ui| {
            pill(ui, "READ ONLY / LOCAL RESEARCH", CYAN);
            pill(ui, "ENGINE LINK UNVERIFIED", AMBER);
            if self.loading {
                pill(ui, "READING LOCAL HISTORY", MUTED);
            } else if self.snapshot.is_some() {
                pill(ui, "HISTORY AVAILABLE", EMERALD);
            } else {
                pill(ui, "NO LOCAL HISTORY", MUTED);
            }
            if self.wgpu_atmosphere {
                pill(ui, "WGPU SHADER", AMETHYST);
            } else if self.renderer.starts_with("WGPU") {
                pill(ui, "WGPU", AMETHYST);
            } else {
                pill(ui, "GLOW", Color32::from_rgb(156, 166, 204));
            }
            if self.active_jobs() > 0 {
                pill(
                    ui,
                    &format!("{} LOCAL JOBS", self.active_jobs()),
                    Color32::from_rgb(198, 156, 255),
                );
            }
            let alerts = states.iter().filter(|s| s.is_alert()).count();
            if alerts > 0 {
                pill(ui, &format!("{alerts} HEAD ALERTS"), AMBER);
            }
            if let Some(frame) = self.frame_stats.summary() {
                ui.label(
                    RichText::new(format!(
                        "UI {:.1}/s  •  build {:.2} ms",
                        frame.median_updates_per_second, frame.ui_build_p50_ms
                    ))
                    .monospace()
                    .small()
                    .color(MUTED),
                )
                .on_hover_text("Observed UI cadence and UI-build CPU duration; not GPU execution timing or a trading benchmark.");
            }
        });
        if ui.available_width() < 1800.0 {
            ui.horizontal_wrapped(|ui| {
                for (index, (name, _, color)) in HEADS.iter().enumerate() {
                    if ui
                        .selectable_label(
                            self.workspace == Workspace::Head(index),
                            RichText::new(*name).small().color(*color),
                        )
                        .clicked()
                    {
                        self.workspace = Workspace::Head(index);
                    }
                }
            });
        }
    }
    fn intelligence_region(
        &mut self,
        ui: &mut egui::Ui,
        states: &[crate::leviathan_state::HeadState; 7],
    ) -> Option<usize> {
        let mut rail_hovered = None;
        section_title(ui, "INTELLIGENCE", Color32::from_rgb(184, 152, 238));
        ui.label(
            RichText::new("SEVEN HEADS / ONE ORGANISM")
                .monospace()
                .small()
                .color(MUTED),
        );
        ui.add_space(6.0);
        for (index, (name, description, color)) in HEADS.iter().enumerate() {
            let selected = self.workspace == Workspace::Head(index);
            let is_alert = states[index].is_alert();
            let is_active = states[index] == crate::leviathan_state::HeadState::Researching;
            let bar_color = if is_alert { AMBER } else { *color };
            let card_fill = if selected {
                Color32::from_rgb(21, 18, 38)
            } else if self.hovered_head == Some(index) {
                Color32::from_rgb(16, 17, 32)
            } else {
                Color32::from_rgb(11, 13, 24)
            };
            let stroke_col = if selected {
                bar_color.gamma_multiply(0.65)
            } else if is_alert {
                AMBER.gamma_multiply(0.55)
            } else {
                Color32::from_rgb(36, 32, 58)
            };
            let frame_resp = egui::Frame::new()
                .fill(card_fill)
                .stroke(Stroke::new(if selected { 1.2 } else { 0.8 }, stroke_col))
                .corner_radius(5)
                .inner_margin(egui::Margin {
                    left: 10,
                    right: 8,
                    top: 6,
                    bottom: 6,
                })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(*name)
                                .monospace()
                                .strong()
                                .size(12.0)
                                .color(*color),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let dot_color = if is_alert {
                                AMBER
                            } else if is_active {
                                *color
                            } else {
                                Color32::from_rgb(86, 94, 124)
                            };
                            let (dot_rect, _) =
                                ui.allocate_exact_size(Vec2::new(6.0, 6.0), Sense::hover());
                            ui.painter()
                                .circle_filled(dot_rect.center(), 3.0, dot_color);
                        });
                    });
                    ui.label(
                        RichText::new(states[index].label())
                            .monospace()
                            .size(9.5)
                            .color(if is_alert {
                                AMBER
                            } else if is_active {
                                Color32::from_rgb(225, 236, 250)
                            } else {
                                MUTED
                            }),
                    );
                    ui.label(
                        RichText::new(*description)
                            .size(10.5)
                            .color(Color32::from_rgb(108, 118, 148)),
                    );
                });
            let rect = frame_resp.response.rect;
            // Vertical activation strip on left edge of card.
            let strip = egui::Rect::from_min_max(
                rect.left_top() + Vec2::new(1.0, 3.0),
                Pos2::new(rect.left() + 3.5, rect.bottom() - 3.0),
            );
            ui.painter().rect_filled(
                strip,
                1.5,
                if selected || is_active || is_alert || self.hovered_head == Some(index) {
                    bar_color
                } else {
                    bar_color.gamma_multiply(0.35)
                },
            );
            let click_resp = ui.interact(
                rect,
                ui.id().with(("intel-head-card", index)),
                Sense::click(),
            );
            if click_resp.hovered() {
                rail_hovered = Some(index);
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if click_resp.clicked() {
                self.workspace = Workspace::Head(index);
            }
            click_resp.on_hover_text(states[index].detail());
            ui.add_space(4.0);
        }

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(SURFACE)
            .stroke(Stroke::new(0.8, Color32::from_rgb(38, 34, 62)))
            .corner_radius(5)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if let Some(r) = self.hunter.report() {
                    ui.label(
                        RichText::new("RESEARCH WATCHLIST")
                            .monospace()
                            .small()
                            .color(CYAN),
                    );
                    ui.label(if r.watchlist.is_empty() {
                        "No assets selected".into()
                    } else {
                        r.watchlist.join(" · ")
                    });
                    ui.label(
                        RichText::new(format!("{} supplied observations", r.ranked.len()))
                            .small()
                            .color(MUTED),
                    );
                } else {
                    ui.label(
                        RichText::new("RESEARCH SNAPSHOT")
                            .monospace()
                            .size(10.0)
                            .color(Color32::from_rgb(138, 152, 186)),
                    );
                    ui.label(
                        RichText::new("Market snapshot not loaded")
                            .small()
                            .color(MUTED),
                    );
                }
                if let Some(crate::diagnostics::DiagnosticReport::Cognition(r)) =
                    self.diagnostics.report()
                {
                    ui.label(
                        RichText::new(format!(
                            "Shadow research: {} supplied samples",
                            r.supplied_samples
                        ))
                        .small()
                        .color(HEADS[2].2),
                    );
                }
            });
        ui.add_space(8.0);
        nav(
            ui,
            &mut self.workspace,
            Workspace::Overview,
            "COMMAND OVERVIEW",
            TEXT,
        );
        nav(
            ui,
            &mut self.workspace,
            Workspace::BlackBox,
            "EVENT TIMELINE",
            TEXT,
        );
        nav(
            ui,
            &mut self.workspace,
            Workspace::Source,
            "SOURCE / CONNECTION",
            TEXT,
        );
        rail_hovered
    }
    fn protection_region(&mut self, ui: &mut egui::Ui) {
        section_title(ui, "PROTECTION / EXECUTION", EMERALD);
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(Color32::from_rgb(24, 18, 14))
            .stroke(Stroke::new(1.0, AMBER.gamma_multiply(0.45)))
            .corner_radius(5)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new("READINESS NOT CERTIFIED")
                        .monospace()
                        .size(11.0)
                        .strong()
                        .color(AMBER),
                );
                ui.label(
                    RichText::new("No wallet access. No order or risk-release controls.")
                        .small()
                        .color(Color32::from_rgb(184, 174, 160)),
                );
            });
        ui.add_space(10.0);
        section_title(ui, "OBSERVED LOCAL EVIDENCE", HEADS[4].2);
        ui.add_space(4.0);
        if let Some(data) = &self.snapshot {
            let pos_val = data.positions.len().to_string();
            let intent_val = data.intents.len().to_string();
            let fills_val = count_text(data, "ledger_fills");
            let scope_val = if data.risk_accounts.is_empty() {
                "unavailable".to_string()
            } else {
                format!("{} bound", data.risk_accounts.len())
            };
            ui.columns(2, |cols| {
                mini_stat(&mut cols[0], "POSITIONS", &pos_val, EMERALD);
                mini_stat(&mut cols[1], "INTENTS", &intent_val, CYAN);
            });
            ui.add_space(4.0);
            ui.columns(2, |cols| {
                mini_stat(&mut cols[0], "LEDGER FILLS", &fills_val, HEADS[4].2);
                mini_stat(&mut cols[1], "ACCOUNT SCOPE", &scope_val, AMETHYST);
            });
            for account in data.risk_accounts.iter().take(3) {
                ui.add_space(6.0);
                egui::Frame::new()
                    .fill(SURFACE)
                    .stroke(Stroke::new(
                        0.9,
                        if account.breaker_reason.is_some() {
                            AMBER.gamma_multiply(0.6)
                        } else {
                            Color32::from_rgb(38, 34, 62)
                        },
                    ))
                    .corner_radius(5)
                    .inner_margin(7)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new("RECORDED ACCOUNT SCOPE")
                                .monospace()
                                .size(9.5)
                                .color(MUTED),
                        );
                        ui.monospace(&account.account);
                        ui.label(format!(
                            "{}{} active reservations",
                            account.active_reservations.value,
                            if account.active_reservations.capped {
                                "+"
                            } else {
                                ""
                            }
                        ));
                        ui.label(format!(
                            "Reserved notional: {}",
                            account.reserved_notional.as_deref().unwrap_or("unknown")
                        ));
                        if let Some(reason) = &account.breaker_reason {
                            ui.colored_label(AMBER, format!("BREAKER: {reason}"));
                        }
                    });
            }
            for intent in data.intents.iter().take(4) {
                let state =
                    crate::leviathan_state::LifecycleEvidence::from_recorded_status(&intent.state);
                ui.add_space(4.0);
                egui::Frame::new()
                    .fill(Color32::from_rgb(12, 14, 25))
                    .stroke(Stroke::new(0.8, Color32::from_rgb(36, 36, 62)))
                    .corner_radius(4)
                    .inner_margin(6)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(format!("{} / {}", intent.symbol, intent.state))
                                .monospace()
                                .small(),
                        );
                        ui.label(RichText::new(state.label()).small().color(MUTED));
                    });
            }
        } else {
            egui::Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(0.8, Color32::from_rgb(36, 34, 58)))
                .corner_radius(5)
                .inner_margin(8)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(
                            "Portfolio, reservations and execution evidence unavailable.",
                        )
                        .small()
                        .color(MUTED),
                    );
                });
        }
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Portfolio / positions").clicked() {
                self.workspace = Workspace::Positions;
            }
            if ui.button("Risk diagnostics").clicked() {
                self.workspace = Workspace::Head(3);
            }
        });
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);
        section_title(ui, "SOURCE FRESHNESS", Color32::from_rgb(186, 168, 224));
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(SURFACE)
            .stroke(Stroke::new(0.8, Color32::from_rgb(38, 34, 62)))
            .corner_radius(5)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let dot_col = if self.error.is_some() {
                        AMBER
                    } else if self.last_received.is_some() {
                        EMERALD
                    } else {
                        MUTED
                    };
                    let (r, _) = ui.allocate_exact_size(Vec2::new(7.0, 7.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 3.2, dot_col);
                    if let Some(last) = self.last_received {
                        ui.label(
                            RichText::new(format!(
                                "Local read {:.1}s ago",
                                last.elapsed().as_secs_f32()
                            ))
                            .monospace()
                            .small(),
                        );
                    } else {
                        ui.label(
                            RichText::new("No successful local read")
                                .small()
                                .color(MUTED),
                        );
                    }
                });
                ui.label(
                    RichText::new(
                        "A database read is not an engine heartbeat or current exchange state.",
                    )
                    .small()
                    .color(MUTED),
                );
            });
        if self.error.is_some() {
            ui.add_space(4.0);
            ui.colored_label(AMBER, "Local source unavailable / read failed");
            if ui.button("Inspect source details").clicked() {
                self.workspace = Workspace::Source;
            }
        }
        for head in [5, 6] {
            if let Some(brief) = self.research.brief(head) {
                ui.add_space(6.0);
                egui::Frame::new()
                    .fill(SURFACE)
                    .stroke(Stroke::new(0.8, HEADS[head].2.gamma_multiply(0.4)))
                    .corner_radius(5)
                    .inner_margin(7)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(HEADS[head].0)
                                .monospace()
                                .strong()
                                .color(HEADS[head].2),
                        );
                        ui.label(RichText::new(brief).small().color(MUTED));
                    });
            }
        }
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        section_title(ui, "ENERGY SIGNAL KEY", AMETHYST);
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(SURFACE)
            .stroke(Stroke::new(0.8, Color32::from_rgb(38, 34, 62)))
            .corner_radius(5)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for (col, label, desc) in [
                    (CYAN, "COLORED STREAM", "Active local research job"),
                    (
                        Color32::from_rgb(186, 182, 214),
                        "SILVER PIPS",
                        "Local history snapshot delta",
                    ),
                    (AMBER, "AMBER BARRIER", "Warning, blocked or faulted"),
                    (AMETHYST, "HEART PULSE", "Idle decorative organism life"),
                ] {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(7.0, 7.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 3.0, col);
                        ui.label(RichText::new(label).monospace().size(9.5).color(col));
                    });
                    ui.label(RichText::new(desc).small().color(MUTED));
                    ui.add_space(3.0);
                }
            });
    }
    fn graphics_controls(&mut self, ui: &mut egui::Ui) {
        use crate::leviathan_scene::SceneQuality;
        ui.horizontal_wrapped(|ui| {
            ui.strong("PRESENTATION");
            egui::ComboBox::from_id_salt("scene-quality")
                .selected_text(format!("{:?}", self.quality))
                .show_ui(ui, |ui| {
                    for q in [SceneQuality::Low, SceneQuality::Medium, SceneQuality::High] {
                        ui.selectable_value(&mut self.quality, q, format!("{q:?}"));
                    }
                });
            ui.checkbox(&mut self.reduced_motion, "Reduced motion");
            ui.checkbox(&mut self.animate_updates, "Event energy");
            egui::ComboBox::from_id_salt("frame-cadence")
                .selected_text(format!("{} updates/s target", self.frame_limit))
                .show_ui(ui, |ui| {
                    for fps in [15, 30, 60, 120, 144] {
                        if ui
                            .selectable_value(
                                &mut self.frame_limit,
                                fps,
                                format!("{fps} updates/s"),
                            )
                            .clicked()
                        {
                            self.frame_stats.clear();
                        }
                    }
                });
            if ui
                .button(if self.borderless {
                    "Use window frame"
                } else {
                    "Use borderless window"
                })
                .clicked()
            {
                self.borderless = !self.borderless;
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Decorations(!self.borderless));
            }
            if ui.button("Close graphics options").clicked() {
                self.show_graphics = false;
            }
        });
        ui.label(RichText::new(&self.renderer).small().color(MUTED));
        ui.label(RichText::new("Idle breathing/atmosphere are decorative. Colored energy = actual local research; silver pips = recorded-history changes. Neither proves a live engine link.").small().color(MUTED));
        if let Some(s) = self.frame_stats.summary() {
            ui.label(format!("Observed {} UI samples · cadence p50 {:.2} / p95 {:.2} ms · UI-build CPU p50 {:.2} / p95 {:.2} ms · GPU timing unavailable",s.samples,s.cadence_p50_ms,s.cadence_p95_ms,s.ui_build_p50_ms,s.ui_build_p95_ms));
        }
        ui.separator();
    }
}

impl eframe::App for Cockpit {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let frame_start = Instant::now();
        let frame_number = ui.ctx().cumulative_frame_nr();
        let new_frame = self.last_frame_number != Some(frame_number);
        let cadence = if new_frame {
            self.last_frame_number = Some(frame_number);
            self.last_frame
                .replace(frame_start)
                .map(|last| frame_start.saturating_duration_since(last))
        } else {
            None
        };
        if let Some(index) = self.diagnostics.poll(ui.ctx()) {
            self.research_changed_at[index] = Some(frame_start);
        }
        if self.hunter.poll(ui.ctx()) {
            self.research_changed_at[0] = Some(frame_start);
        }
        if let Some(index) = self.research.poll(ui.ctx()) {
            self.research_changed_at[index] = Some(frame_start);
        }
        for (index, key) in [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
            egui::Key::Num6,
            egui::Key::Num7,
        ]
        .iter()
        .enumerate()
        {
            if ui.ctx().input(|i| i.modifiers.alt && i.key_pressed(*key)) {
                self.workspace = Workspace::Head(index);
            }
        }
        self.receive();
        if !self.paused && !self.loading && self.last_request.elapsed() >= Duration::from_secs(1) {
            self.refresh();
        }
        if ui.ctx().input(|i| i.key_pressed(egui::Key::F11)) {
            self.fullscreen = !self.fullscreen;
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
        }
        // Smooth damped choreography for drawer height presets (instant when reduced motion).
        if (self.details_height - self.target_details_height).abs() > 0.5 {
            if self.reduced_motion {
                self.details_height = self.target_details_height;
            } else {
                self.details_height += (self.target_details_height - self.details_height) * 0.22;
            }
        }
        let states = self.head_states();
        let details_accent = match self.workspace {
            Workspace::Head(i) => HEADS[i].2,
            Workspace::Positions => EMERALD,
            Workspace::BlackBox => CYAN,
            Workspace::Source => AMBER,
            Workspace::Overview => AMETHYST,
        };
        let mut next_hovered_head = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG).inner_margin(8))
            .show(ui, |ui| {
                let plan = crate::leviathan_layout::UnifiedLayout::for_rect(
                    ui.max_rect(),
                    self.details_height,
                );
                region(ui, plan.command, "command", None, |ui| {
                    self.command_bar(ui, &states);
                });
                if let Some(rect) = plan.left {
                    region(ui, rect, "intelligence", Some(AMETHYST), |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("intelligence-scroll")
                            .show(ui, |ui| {
                                if let Some(h) = self.intelligence_region(ui, &states) {
                                    next_hovered_head = Some(h);
                                }
                            });
                    });
                }
                if let Some(rect) = plan.right {
                    region(ui, rect, "protection", Some(EMERALD), |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("protection-scroll")
                            .show(ui, |ui| self.protection_region(ui));
                    });
                }
                region(ui, plan.canvas, "leviathan", None, |ui| {
                    let heads = std::array::from_fn(|index| {
                        let running = if index == 0 {
                            self.hunter.running()
                        } else {
                            self.diagnostics.running_head() == Some(index)
                                || self.research.running_head() == Some(index)
                        };
                        let age = self.research_changed_at[index]
                            .map(|at| frame_start.saturating_duration_since(at));
                        let pulse = if self.animate_updates {
                            crate::leviathan_state::event_pulse(age, self.reduced_motion)
                        } else {
                            0.0
                        };
                        crate::leviathan_scene::HeadPresentation {
                            name: HEADS[index].0,
                            status: states[index].label(),
                            accent: if states[index]
                                == crate::leviathan_state::HeadState::Faulted
                            {
                                Color32::from_rgb(255, 90, 119)
                            } else {
                                HEADS[index].2
                            },
                            observation: if self.animate_updates {
                                crate::leviathan_state::event_pulse(
                                    self.changed_at[index]
                                        .map(|at| frame_start.saturating_duration_since(at)),
                                    self.reduced_motion,
                                )
                            } else {
                                0.0
                            },
                            activity: if self.reduced_motion || !self.animate_updates {
                                0.0
                            } else {
                                pulse.max(if running { 0.65 } else { 0.0 })
                            },
                            warning: states[index].is_alert(),
                        }
                    });
                    let heart = if self.active_jobs() > 0 {
                        "LOCAL RESEARCH / ENGINE LINK UNVERIFIED"
                    } else if self.snapshot.is_some() {
                        "LOCAL HISTORY / EXECUTION READINESS UNVERIFIED"
                    } else {
                        "WAITING FOR DATA / ENGINE LINK UNVERIFIED"
                    };
                    let input = crate::leviathan_scene::SceneInput {
                        heads: &heads,
                        selected: match self.workspace {
                            Workspace::Head(i) => Some(i),
                            _ => None,
                        },
                        hovered: self.hovered_head,
                        time: self.started.elapsed().as_secs_f32(),
                        motion: !self.reduced_motion,
                        quality: self.quality,
                        heart_status: heart,
                        wgpu_atmosphere: self.wgpu_atmosphere,
                    };
                    let scene_resp = crate::leviathan_scene::show_interactive(ui, &input);
                    if scene_resp.hovered.is_some() {
                        next_hovered_head = scene_resp.hovered;
                    }
                    if let Some(index) = scene_resp.clicked {
                        self.workspace = Workspace::Head(index);
                    }
                });
                let handle = egui::Rect::from_min_max(
                    egui::pos2(plan.details.left(), plan.details.top() - 6.0),
                    egui::pos2(plan.details.right(), plan.details.top() + 3.0),
                );
                let response = ui
                    .interact(handle, ui.id().with("analysis-resize"), Sense::drag())
                    .on_hover_cursor(egui::CursorIcon::ResizeVertical);
                if response.dragged() {
                    let h = (self.details_height - response.drag_delta().y).clamp(100.0, 900.0);
                    self.details_height = h;
                    self.target_details_height = h;
                }
                // Luminous subtle drag-grip bar along top edge of the lower workspace.
                let grip_w = 72.0;
                let grip_y = plan.details.top() - 1.5;
                let grip_col = if response.hovered() || response.dragged() {
                    details_accent
                } else {
                    details_accent.gamma_multiply(0.45)
                };
                ui.painter().line_segment(
                    [
                        Pos2::new(plan.details.center().x - grip_w * 0.5, grip_y),
                        Pos2::new(plan.details.center().x + grip_w * 0.5, grip_y),
                    ],
                    Stroke::new(2.0, grip_col),
                );

                region(ui, plan.details, "details", Some(details_accent), |ui| {
                    let content_h = plan.bounds.height();
                    ui.horizontal_wrapped(|ui| {
                        section_title(
                            ui,
                            match self.workspace {
                                Workspace::Overview => "COMMAND OVERVIEW",
                                Workspace::Head(i) => HEADS[i].0,
                                Workspace::Positions => "PORTFOLIO / POSITIONS",
                                Workspace::BlackBox => "EVENT TIMELINE",
                                Workspace::Source => "SOURCE / CONNECTION",
                            },
                            details_accent,
                        );
                        ui.label(
                            RichText::new("INLINE WORKSPACE / DRAG TOP EDGE")
                                .monospace()
                                .size(9.5)
                                .color(MUTED),
                        );
                        if ui
                            .selectable_label(self.workspace == Workspace::Overview, "Overview")
                            .clicked()
                        {
                            self.workspace = Workspace::Overview;
                        }
                        if ui
                            .selectable_label(self.workspace == Workspace::Source, "Source")
                            .clicked()
                        {
                            self.workspace = Workspace::Source;
                        }
                        if ui
                            .selectable_label(self.workspace == Workspace::BlackBox, "Timeline")
                            .clicked()
                        {
                            self.workspace = Workspace::BlackBox;
                        }
                        if ui
                            .selectable_label(self.workspace == Workspace::Positions, "Positions")
                            .clicked()
                        {
                            self.workspace = Workspace::Positions;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("Expand")
                                .on_hover_text("Expand lower analytical workspace")
                                .clicked()
                            {
                                self.target_details_height = (content_h * 0.40).clamp(240.0, 620.0);
                            }
                            if ui
                                .small_button("Balanced")
                                .on_hover_text("Restore balanced workspace height")
                                .clicked()
                            {
                                self.target_details_height = (content_h * 0.25).clamp(180.0, 340.0);
                            }
                            if ui
                                .small_button("Compact")
                                .on_hover_text("Maximize space for the seven-headed Leviathan")
                                .clicked()
                            {
                                self.target_details_height = (content_h * 0.16).clamp(120.0, 220.0);
                            }
                        });
                    });
                    ui.add_space(4.0);
                    egui::ScrollArea::both()
                        .id_salt("integrated-workspace")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if self.show_graphics {
                                self.graphics_controls(ui);
                            }
                            self.body(ui);
                        });
                });
                region(ui, plan.footer, "footer", None, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new("SEVEN HEADS. ONE HEART. ONE LEVIATHAN.")
                                .monospace()
                                .small()
                                .color(Color32::from_rgb(194, 168, 236)),
                        );
                        ui.label(
                            RichText::new(
                                "Alt+1…7 heads · F11 fullscreen · idle motion decorative · colored research / silver history",
                            )
                            .small()
                            .color(MUTED),
                        );
                        if self.paused {
                            ui.colored_label(AMBER, "HISTORY VIEW PAUSED");
                        }
                    });
                });
            });
        self.hovered_head = next_hovered_head;
        let build = frame_start.elapsed();
        if let Some(cadence) = cadence {
            self.frame_stats.record(cadence, build);
        }
        let period = if self.reduced_motion {
            Duration::from_secs(1)
        } else {
            Duration::from_secs_f64(1.0 / self.frame_limit.clamp(15, 144) as f64)
        };
        ui.ctx()
            .request_repaint_after(period.saturating_sub(frame_start.elapsed()));
    }
}
pub fn run(path: PathBuf) -> eframe::Result {
    run_with_options(path, crate::desktop_renderer::Backend::Auto, false, true)
}
pub fn run_with_options(
    path: PathBuf,
    backend: crate::desktop_renderer::Backend,
    fullscreen: bool,
    borderless: bool,
) -> eframe::Result {
    run_configured(
        path,
        backend,
        fullscreen,
        borderless,
        crate::leviathan_scene::SceneQuality::High,
        false,
        60,
    )
}
pub fn run_configured(
    path: PathBuf,
    backend: crate::desktop_renderer::Backend,
    fullscreen: bool,
    borderless: bool,
    quality: crate::leviathan_scene::SceneQuality,
    reduced_motion: bool,
    frame_limit: u32,
) -> eframe::Result {
    let attempts = crate::desktop_renderer::attempts(backend)
        .map_err(|error| eframe::Error::AppCreation(error.into()))?;
    let mut failure = None;
    for backend in attempts {
        let options = crate::desktop_renderer::native_options(backend, fullscreen, borderless)
            .map_err(|error| eframe::Error::AppCreation(error.into()))?;
        let database = path.clone();
        match eframe::run_native(
            "HYDRA — PROJECT LEVIATHAN",
            options,
            Box::new(move |cc| {
                let mut app = Cockpit::new(cc, database);
                app.fullscreen = fullscreen;
                app.borderless = borderless;
                app.configure_presentation(quality, reduced_motion, frame_limit);
                Ok(Box::new(app))
            }),
        ) {
            Ok(()) => return Ok(()),
            Err(error) => {
                eprintln!("Native {} startup failed: {}", backend.name(), error);
                failure = Some(error);
            }
        }
    }
    Err(failure.unwrap_or_else(|| {
        eframe::Error::AppCreation("No native renderer attempt was available".into())
    }))
}
fn region(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id: &str,
    accent: Option<Color32>,
    contents: impl FnOnce(&mut egui::Ui),
) {
    if rect.width() < 1.0 || rect.height() < 1.0 {
        return;
    }
    let framed = accent.is_some();
    if let Some(accent_col) = accent {
        let painter = ui.painter();
        painter.rect_filled(rect, 7.0, PANEL);
        painter.rect_stroke(
            rect,
            7.0,
            Stroke::new(1.0, Color32::from_rgb(44, 36, 70)),
            egui::StrokeKind::Inside,
        );
        // Subtle luminous top accent hairline + corner brackets.
        let top_line_col =
            Color32::from_rgba_unmultiplied(accent_col.r(), accent_col.g(), accent_col.b(), 95);
        painter.line_segment(
            [
                Pos2::new(rect.left() + 14.0, rect.top() + 0.5),
                Pos2::new(rect.right() - 14.0, rect.top() + 0.5),
            ],
            Stroke::new(1.2, top_line_col),
        );
        let corner_col =
            Color32::from_rgba_unmultiplied(accent_col.r(), accent_col.g(), accent_col.b(), 165);
        let arm = 9.0;
        for (cx, cy, dx, dy) in [
            (rect.left() + 1.0, rect.top() + 1.0, 1.0, 1.0),
            (rect.right() - 1.0, rect.top() + 1.0, -1.0, 1.0),
            (rect.left() + 1.0, rect.bottom() - 1.0, 1.0, -1.0),
            (rect.right() - 1.0, rect.bottom() - 1.0, -1.0, -1.0),
        ] {
            painter.line_segment(
                [Pos2::new(cx, cy), Pos2::new(cx + dx * arm, cy)],
                Stroke::new(1.2, corner_col),
            );
            painter.line_segment(
                [Pos2::new(cx, cy), Pos2::new(cx, cy + dy * arm)],
                Stroke::new(1.2, corner_col),
            );
        }
    }
    let inner = rect.shrink(if framed { 12.0 } else { 2.0 });
    ui.scope_builder(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            ui.set_clip_rect(rect);
            contents(ui);
        },
    );
}
fn section_title(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.label(
        RichText::new(text)
            .monospace()
            .size(11.5)
            .strong()
            .color(color),
    );
}
fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(Color32::from_rgb(13, 15, 27))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.42)))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                let (dot, _) = ui.allocate_exact_size(Vec2::new(5.0, 5.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 2.3, color);
                ui.label(RichText::new(text).monospace().size(9.5).color(color));
            });
        });
}
fn mini_stat(ui: &mut egui::Ui, label: &str, value: &str, color: Color32) {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(0.8, Color32::from_rgb(38, 36, 62)))
        .corner_radius(5)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(label).monospace().size(8.5).color(MUTED));
            ui.label(
                RichText::new(value)
                    .monospace()
                    .size(13.5)
                    .strong()
                    .color(color),
            );
        });
}

fn nav(ui: &mut egui::Ui, current: &mut Workspace, target: Workspace, label: &str, color: Color32) {
    let selected = *current == target;
    let resp = egui::Frame::new()
        .fill(if selected {
            Color32::from_rgb(36, 24, 62)
        } else {
            Color32::from_rgb(12, 14, 25)
        })
        .stroke(Stroke::new(
            0.9,
            if selected {
                AMETHYST.gamma_multiply(0.75)
            } else {
                Color32::from_rgb(36, 34, 58)
            },
        ))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(9, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(label)
                    .monospace()
                    .size(10.5)
                    .strong()
                    .color(if selected {
                        Color32::from_rgb(236, 224, 255)
                    } else {
                        color
                    }),
            );
        });
    let click = ui.interact(
        resp.response.rect,
        ui.id().with(("nav-btn", label)),
        Sense::click(),
    );
    if click.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if click.clicked() {
        *current = target;
    }
    ui.add_space(4.0);
}
fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::default()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, Color32::from_rgb(42, 42, 72)))
        .corner_radius(7)
        .inner_margin(12)
        .show(ui, contents);
}
fn metric(ui: &mut egui::Ui, label: &str, value: &str, color: Color32) {
    let resp = egui::Frame::default()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.32)))
        .corner_radius(7)
        .inner_margin(egui::Margin {
            left: 14,
            right: 12,
            top: 10,
            bottom: 10,
        })
        .show(ui, |ui| {
            ui.set_min_width(148.0);
            ui.label(RichText::new(label).small().color(MUTED));
            ui.label(
                RichText::new(value)
                    .size(22.0)
                    .monospace()
                    .strong()
                    .color(color),
            );
        });
    let r = resp.response.rect;
    ui.painter().rect_filled(
        egui::Rect::from_min_max(
            r.left_top() + Vec2::new(2.0, 6.0),
            Pos2::new(r.left() + 4.5, r.bottom() - 6.0),
        ),
        1.5,
        color,
    );
}
fn count_text(data: &TelemetrySnapshot, table: &str) -> String {
    data.count(table)
        .map(|c| format!("{}{}", c.value, if c.capped { "+" } else { "" }))
        .unwrap_or_else(|| "unavailable".into())
}
fn matches(filter: &str, values: &[&str]) -> bool {
    filter.is_empty()
        || values
            .iter()
            .any(|v| v.to_lowercase().contains(&filter.to_lowercase()))
}
fn fingerprints(data: &TelemetrySnapshot) -> [String; 7] {
    [
        data.signals
            .iter()
            .map(|s| s.symbol.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(","),
        data.signals
            .first()
            .map(|s| format!("{}{}{}", s.at, s.symbol, s.strategy))
            .unwrap_or_default(),
        count_text(data, "trade_context"),
        serde_json::to_string(&data.risk_accounts).unwrap_or_default(),
        data.intents
            .iter()
            .map(|i| format!("{}:{}", i.id, i.state))
            .collect::<Vec<_>>()
            .join("|"),
        String::new(),
        String::new(),
    ]
}
fn positions(ui: &mut egui::Ui, data: &TelemetrySnapshot) {
    ui.label(RichText::new("Persisted local positions · account attribution is inherited from the engine database · marks and current unrealized PnL unavailable").color(MUTED));
    if data.positions.is_empty() {
        ui.label("No open position rows in this snapshot.");
        return;
    }
    egui::Grid::new("positions").striped(true).show(ui, |ui| {
        for title in ["Instrument", "Side", "Quantity", "Entry", "Stop", "Owner"] {
            ui.strong(title);
        }
        ui.end_row();
        for p in &data.positions {
            ui.monospace(&p.symbol);
            ui.colored_label(if p.side == "LONG" { EMERALD } else { AMBER }, &p.side);
            ui.monospace(format!("{:.8}", p.size));
            ui.monospace(format!("{:.5}", p.entry_price));
            ui.monospace(
                p.stop_price
                    .map(|v| format!("{v:.5}"))
                    .unwrap_or_else(|| "unavailable".into()),
            );
            ui.label(p.strategy.as_deref().unwrap_or("unattributed"));
            ui.end_row();
        }
    });
}
fn journal(ui: &mut egui::Ui, data: &TelemetrySnapshot, filter: &str) {
    ui.heading("Durable execution journal");
    ui.label(
        RichText::new("Resting, submitted and unknown outcomes are not confirmed fills.")
            .color(MUTED),
    );
    egui::Grid::new("journal").striped(true).show(ui, |ui| {
        for title in ["Updated", "Instrument", "State", "Client intent ID"] {
            ui.strong(title);
        }
        ui.end_row();
        for row in &data.intents {
            if !matches(filter, &[&row.symbol, &row.state, &row.id]) {
                continue;
            }
            ui.monospace(&row.updated_at);
            ui.monospace(&row.symbol);
            ui.colored_label(
                if matches!(row.state.as_str(), "applied" | "rejected") {
                    MUTED
                } else {
                    AMBER
                },
                &row.state,
            );
            ui.monospace(&row.id);
            ui.end_row();
        }
    });
}
fn fills(ui: &mut egui::Ui, data: &TelemetrySnapshot, filter: &str) {
    ui.heading("Exchange fill evidence");
    if !data.has_table("ledger_fills") {
        ui.label("Authoritative fill ledger schema unavailable.");
        return;
    }
    ui.label(RichText::new("Fees retain their actual currency. A fill subtotal is not a reconciled round-trip net PnL.").color(MUTED));
    egui::Grid::new("fills").striped(true).show(ui, |ui| {
        for title in [
            "Scope",
            "Instrument",
            "Order / fill",
            "Quantity",
            "Price",
            "Fee",
        ] {
            ui.strong(title);
        }
        ui.end_row();
        for f in &data.fills {
            if !matches(filter, &[&f.scope, &f.symbol, &f.order_id, &f.fill_id]) {
                continue;
            }
            ui.monospace(&f.scope);
            ui.monospace(&f.symbol);
            ui.monospace(format!("{} / {}", f.order_id, f.fill_id));
            ui.monospace(&f.quantity);
            ui.monospace(&f.price);
            ui.monospace(match (&f.fee_amount, &f.fee_currency) {
                (Some(a), Some(c)) => format!("{a} {c}"),
                _ => "unknown".into(),
            });
            ui.end_row();
        }
    });
}
fn signals(ui: &mut egui::Ui, data: &TelemetrySnapshot, filter: &str) {
    if data.signals.is_empty() {
        ui.label("No recorded signals in this snapshot.");
        return;
    }
    egui::Grid::new("signals").striped(true).show(ui, |ui| {
        for title in [
            "Time",
            "Instrument",
            "Strategy",
            "Side",
            "Confidence",
            "Recorded explanation",
        ] {
            ui.strong(title);
        }
        ui.end_row();
        for s in &data.signals {
            if !matches(filter, &[&s.symbol, &s.strategy, &s.reason]) {
                continue;
            }
            ui.monospace(&s.at);
            ui.monospace(&s.symbol);
            ui.label(&s.strategy);
            ui.label(&s.side);
            ui.monospace(format!("{:.1}%", s.confidence * 100.0));
            ui.label(&s.reason);
            ui.end_row();
        }
    });
}
fn events(ui: &mut egui::Ui, data: &TelemetrySnapshot, filter: &str) {
    egui::Grid::new("events").striped(true).show(ui, |ui| {
        for title in ["Time", "Source", "Classification", "Redacted diagnostic"] {
            ui.strong(title);
        }
        ui.end_row();
        for e in &data.events {
            if !matches(filter, &[&e.subsystem, &e.classification, &e.detail]) {
                continue;
            }
            ui.monospace(&e.at);
            ui.colored_label(
                if e.subsystem == "AEGIS" {
                    EMERALD
                } else {
                    CYAN
                },
                &e.subsystem,
            );
            ui.label(&e.classification);
            ui.label(&e.detail);
            ui.end_row();
        }
    });
}
fn observed_markets(ui: &mut egui::Ui, data: &TelemetrySnapshot) {
    ui.label(RichText::new("Instruments observed in the recent persisted signal window. Liquidity/funding ranking and dynamic discovery are not implemented here.").color(MUTED));
    let mut markets = std::collections::BTreeMap::<&str, (usize, &str)>::new();
    for s in &data.signals {
        let value = markets.entry(&s.symbol).or_insert((0, &s.at));
        value.0 += 1;
    }
    egui::Grid::new("observed").striped(true).show(ui, |ui| {
        for t in ["Instrument", "Recent signal rows", "Latest recorded signal"] {
            ui.strong(t);
        }
        ui.end_row();
        for (market, (count, latest)) in markets {
            ui.monospace(market);
            ui.monospace(count.to_string());
            ui.monospace(latest);
            ui.end_row();
        }
    });
}
fn risk(ui: &mut egui::Ui, data: &TelemetrySnapshot) {
    ui.label(RichText::new("Persistent policy and reservation evidence, separated by account. No risk action is available from this viewer.").color(MUTED));
    if !data.has_table("aegis_accounts") {
        ui.label("Aegis persistent schema unavailable.");
        return;
    }
    for account in &data.risk_accounts {
        card(ui, |ui| {
            ui.monospace(&account.account);
            ui.colored_label(
                if account.breaker_reason.is_some() {
                    AMBER
                } else {
                    EMERALD
                },
                account
                    .breaker_reason
                    .as_deref()
                    .unwrap_or("No persisted breaker latched · current readiness unverified"),
            );
            ui.label(format!(
                "Recorded equity peak: {} · snapshot revision: {}",
                account.high_water_equity,
                account
                    .snapshot_revision
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "unavailable".into())
            ));
            ui.label(format!(
                "Active reservations: {}{} · reserved notional: {}",
                account.active_reservations.value,
                if account.active_reservations.capped {
                    "+"
                } else {
                    ""
                },
                account
                    .reserved_notional
                    .as_deref()
                    .unwrap_or("unavailable")
            ));
        });
        ui.add_space(8.0);
    }
}
fn schema(ui: &mut egui::Ui, data: &TelemetrySnapshot) {
    egui::Grid::new("schema").striped(true).show(ui, |ui| {
        ui.strong("Source table");
        ui.strong("Availability");
        ui.strong("Bounded record count");
        ui.end_row();
        for table in &data.tables {
            ui.monospace(&table.name);
            ui.colored_label(
                if table.available { EMERALD } else { MUTED },
                if table.available {
                    "present"
                } else {
                    "unavailable"
                },
            );
            ui.monospace(count_text(data, &table.name));
            ui.end_row();
        }
    });
}
fn equity_chart(ui: &mut egui::Ui, data: &TelemetrySnapshot) {
    ui.heading("Recorded Hyperliquid equity");
    ui.label(RichText::new("Historical database snapshots · account attribution unavailable · not a current account balance").small().color(MUTED));
    let points: Vec<_> = data
        .equity
        .iter()
        .filter(|p| p.venue == "hyperliquid")
        .collect();
    if points.len() < 2 {
        ui.label("At least two recorded snapshots are needed for a historical curve.");
        return;
    }
    let min = points
        .iter()
        .map(|p| p.recorded_equity)
        .fold(f64::INFINITY, f64::min);
    let max = points
        .iter()
        .map(|p| p.recorded_equity)
        .fold(f64::NEG_INFINITY, f64::max);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(300.0), 130.0),
        Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 7, SURFACE);
    painter.rect_stroke(
        rect,
        7,
        Stroke::new(1.0, Color32::from_rgb(38, 44, 68)),
        egui::StrokeKind::Inside,
    );
    let plot = rect.shrink2(Vec2::new(14.0, 14.0));
    // Subtle reference gridlines.
    for row in 0..=3 {
        let y = plot.top() + plot.height() * row as f32 / 3.0;
        painter.line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(0.6, Color32::from_rgba_unmultiplied(128, 136, 174, 34)),
        );
    }
    let span = (max - min).max(1e-9);
    let line: Vec<_> = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            Pos2::new(
                plot.left() + plot.width() * i as f32 / (points.len() - 1) as f32,
                plot.bottom() - plot.height() * ((p.recorded_equity - min) / span) as f32,
            )
        })
        .collect();
    // Subtle translucent emerald ribbon under the recorded equity curve.
    for pair in line.windows(2) {
        painter.add(egui::Shape::convex_polygon(
            vec![
                pair[0],
                pair[1],
                Pos2::new(pair[1].x, plot.bottom()),
                Pos2::new(pair[0].x, plot.bottom()),
            ],
            Color32::from_rgba_unmultiplied(58, 232, 165, 22),
            Stroke::NONE,
        ));
    }
    painter.add(egui::Shape::line(
        line.clone(),
        Stroke::new(4.5, Color32::from_rgba_unmultiplied(58, 232, 165, 36)),
    ));
    painter.add(egui::Shape::line(line.clone(), Stroke::new(1.8, EMERALD)));
    if let Some(last) = line.last() {
        painter.circle_filled(*last, 3.0, EMERALD);
    }
    response.on_hover_text(format!("{} snapshots, ordered by persistence. Visible min {:.2}, max {:.2}. Vertical axis spans the visible min/max; points use equal spacing, not elapsed time.",points.len(),min,max));
    ui.horizontal_wrapped(|ui| {
        ui.monospace(format!("min {min:.2} / max {max:.2}"));
        ui.label(
            RichText::new(format!(
                "{} → {}",
                points.first().unwrap().at,
                points.last().unwrap().at
            ))
            .small()
            .color(MUTED),
        );
    });
}
