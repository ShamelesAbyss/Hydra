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

const BG: Color32 = Color32::from_rgb(6, 10, 15);
const PANEL: Color32 = Color32::from_rgb(12, 19, 27);
const TEXT: Color32 = Color32::from_rgb(219, 234, 241);
const MUTED: Color32 = Color32::from_rgb(119, 144, 157);
const EMERALD: Color32 = Color32::from_rgb(58, 232, 165);
const CYAN: Color32 = Color32::from_rgb(54, 205, 245);
const AMBER: Color32 = Color32::from_rgb(246, 188, 83);
const HEADS: [(&str, &str, Color32); 7] = [
    ("HUNTER", "Observed instruments", CYAN),
    ("FANG", "Strategy signal history", AMBER),
    (
        "COGNITION",
        "Learning provenance",
        Color32::from_rgb(173, 133, 255),
    ),
    ("AEGIS", "Persistent risk authority", EMERALD),
    (
        "NEXUS",
        "Execution and fill evidence",
        Color32::from_rgb(73, 151, 255),
    ),
    (
        "ORACLE",
        "Historical research status",
        Color32::from_rgb(237, 129, 189),
    ),
    (
        "GENESIS",
        "Optimization status",
        Color32::from_rgb(207, 119, 248),
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
    pub fn new(cc: &eframe::CreationContext<'_>, path: PathBuf) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BG;
        visuals.window_fill = PANEL;
        visuals.override_text_color = Some(TEXT);
        visuals.selection.bg_fill = Color32::from_rgb(18, 69, 63);
        visuals.widgets.noninteractive.bg_fill = PANEL;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(31, 49, 62));
        cc.egui_ctx.set_visuals(visuals);
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
    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("HYDRA").size(29.0).strong().color(EMERALD));
            ui.label(RichText::new("PROJECT LEVIATHAN").size(15.0).color(TEXT));
            ui.separator();
            ui.label(
                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .monospace()
                    .color(MUTED),
            );
            ui.label(RichText::new("READ ONLY").strong().color(CYAN));
            ui.label(RichText::new("No wallet access · no exchange writes").color(MUTED));
        });
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
    fn navigation(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        nav(
            ui,
            &mut self.workspace,
            Workspace::Overview,
            "COMMAND CENTER",
            EMERALD,
        );
        ui.add_space(10.0);
        ui.label(RichText::new("SEVEN HEADS").small().color(MUTED));
        for (i, (name, description, color)) in HEADS.iter().enumerate() {
            if ui
                .selectable_label(
                    self.workspace == Workspace::Head(i),
                    RichText::new(*name).color(*color).strong(),
                )
                .clicked()
            {
                self.workspace = Workspace::Head(i);
            }
            ui.label(RichText::new(*description).small().color(MUTED));
            ui.add_space(8.0);
        }
        ui.separator();
        nav(
            ui,
            &mut self.workspace,
            Workspace::Positions,
            "PORTFOLIO / POSITIONS",
            TEXT,
        );
        nav(
            ui,
            &mut self.workspace,
            Workspace::BlackBox,
            "BLACK BOX",
            TEXT,
        );
        nav(
            ui,
            &mut self.workspace,
            Workspace::Source,
            "SOURCE / SCHEMA",
            TEXT,
        );
        ui.add_space(16.0);
        ui.label(RichText::new("Viewer lifecycle is independent of the engine. Closing this window sends no stop or trading command.").small().color(MUTED));
    }
    fn body(&mut self, ui: &mut egui::Ui) {
        let Some(data) = self.snapshot.clone() else {
            ui.add_space(50.0);
            ui.heading("Connect to your local engine history");
            ui.label(
                "Enter the path to an existing Hydra SQLite database and select Open read-only.",
            );
            ui.label("No credentials are requested. Recorded data is displayed only after a successful, consistent read.");
            ui.add_space(20.0);
            self.hydra(ui, None);
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
                    5 | 6 => {
                        ui.colored_label(AMBER, "Research service unavailable in this build.");
                        ui.label(if index==5 {"A deterministic historical execution simulator is not connected. No backtests, equity curves or simulated trades are invented here."}else{"An evolutionary optimizer is not connected. No populations, generations, candidate returns or automatic deployment controls are invented here."});
                        ui.add_space(16.0);
                        ui.label("Available source evidence");
                        schema(ui, &data);
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
        self.hydra(ui, Some(data));
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
    fn hydra(&mut self, ui: &mut egui::Ui, data: Option<&TelemetrySnapshot>) {
        let width = ui.available_width().max(400.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 260.0), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 10, PANEL);
        for i in 0..=20 {
            let x = rect.left() + rect.width() * i as f32 / 20.0;
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(0.5, Color32::from_rgb(17, 35, 43)),
            );
        }
        let anchor = Pos2::new(rect.center().x, rect.bottom() - 38.0);
        for (i, (name, _, color)) in HEADS.iter().enumerate() {
            let offset = i as f32 - 3.0;
            let target = Pos2::new(
                rect.center().x + offset * rect.width() * 0.13,
                rect.top() + 58.0 + offset.abs() * 13.0,
            );
            let pulse = if self.animate_updates {
                self.changed_at[i]
                    .map(|at| 1.0 - (at.elapsed().as_secs_f32() / 3.0).min(1.0))
                    .unwrap_or(0.0)
            } else {
                0.0
            };
            let available = data.is_some_and(|d| match i {
                0 | 1 => d.has_table("signals"),
                2 => d.has_table("trade_context"),
                3 => d.has_table("aegis_accounts"),
                4 => d.has_table("execution_intents"),
                _ => false,
            });
            let active_color = if available {
                *color
            } else {
                Color32::from_rgb(66, 76, 94)
            };
            let mut points = Vec::new();
            for j in 0..=24 {
                let t = j as f32 / 24.0;
                let bend = (t * std::f32::consts::PI).sin() * offset * 12.0;
                points.push(Pos2::new(
                    anchor.x + (target.x - anchor.x) * t + bend,
                    anchor.y + (target.y - anchor.y) * t,
                ));
            }
            painter.add(egui::Shape::line(
                points.clone(),
                Stroke::new(7.0, active_color.gamma_multiply(0.10 + pulse * 0.10)),
            ));
            painter.add(egui::Shape::line(
                points.clone(),
                Stroke::new(1.5, active_color.gamma_multiply(0.7)),
            ));
            if pulse > 0.0 {
                let index = ((1.0 - pulse) * 24.0) as usize;
                painter.circle_filled(points[index.min(24)], 3.0, active_color);
                ui.ctx().request_repaint_after(Duration::from_millis(33));
            }
            let head = vec![
                target + Vec2::new(0.0, -21.0),
                target + Vec2::new(20.0, -6.0),
                target + Vec2::new(13.0, 16.0),
                target + Vec2::new(0.0, 23.0),
                target + Vec2::new(-13.0, 16.0),
                target + Vec2::new(-20.0, -6.0),
            ];
            painter.add(egui::Shape::convex_polygon(
                head,
                Color32::from_rgb(10, 23, 29),
                Stroke::new(
                    if self.workspace == Workspace::Head(i) {
                        2.5
                    } else {
                        1.0
                    },
                    active_color,
                ),
            ));
            painter.line_segment(
                [
                    target + Vec2::new(-12.0, -4.0),
                    target + Vec2::new(-4.0, 0.0),
                ],
                Stroke::new(2.0, active_color),
            );
            painter.line_segment(
                [target + Vec2::new(12.0, -4.0), target + Vec2::new(4.0, 0.0)],
                Stroke::new(2.0, active_color),
            );
            painter.text(
                target + Vec2::new(0.0, -35.0),
                egui::Align2::CENTER_CENTER,
                *name,
                FontId::monospace(11.0),
                active_color,
            );
            let hit = egui::Rect::from_center_size(target, Vec2::new(65.0, 95.0));
            if ui
                .interact(hit, ui.id().with(("head", i)), Sense::click())
                .on_hover_text(HEADS[i].1)
                .clicked()
            {
                self.workspace = Workspace::Head(i);
            }
        }
        painter.circle_filled(anchor, 15.0, Color32::from_rgb(15, 45, 41));
        painter.circle_stroke(anchor, 15.0, Stroke::new(1.5, EMERALD));
        painter.text(
            Pos2::new(rect.left() + 14.0, rect.bottom() - 15.0),
            egui::Align2::LEFT_CENTER,
            "SEVEN HEADS / ONE ENGINE",
            FontId::monospace(11.0),
            EMERALD,
        );
        painter.text(
            Pos2::new(rect.right() - 14.0, rect.bottom() - 15.0),
            egui::Align2::RIGHT_CENTER,
            "PULSES = OBSERVED DATABASE CHANGES",
            FontId::monospace(10.0),
            MUTED,
        );
    }
}

impl eframe::App for Cockpit {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.receive();
        if !self.paused && !self.loading && self.last_request.elapsed() >= Duration::from_secs(1) {
            self.refresh();
        }
        if ui.ctx().input(|i| i.key_pressed(egui::Key::F11)) {
            self.fullscreen = !self.fullscreen;
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
        }
        ui.ctx().request_repaint_after(Duration::from_secs(1));
        egui::Panel::top("header").show(ui, |ui| self.header(ui));
        egui::Panel::bottom("footer").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(if self.paused {
                        "VIEWER PAUSED"
                    } else if self.loading {
                        "READING LOCAL SNAPSHOT"
                    } else {
                        "LOCAL READ-ONLY SNAPSHOT"
                    })
                    .monospace()
                    .color(CYAN),
                );
                if let Some(last) = self.last_received {
                    ui.label(
                        RichText::new(format!("received {:.1}s ago", last.elapsed().as_secs_f32()))
                            .small()
                            .color(MUTED),
                    );
                }
                ui.label(
                    RichText::new("F11 full screen · refresh ≤1 Hz · no wallet signing")
                        .small()
                        .color(MUTED),
                );
            });
        });
        egui::Panel::left("navigation")
            .resizable(true)
            .size_range(205.0..=330.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.navigation(ui));
            });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.body(ui);
                });
        });
    }
}

pub fn run(path: PathBuf) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 940.0])
            .with_min_inner_size([980.0, 680.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "HYDRA — PROJECT LEVIATHAN",
        options,
        Box::new(move |cc| Ok(Box::new(Cockpit::new(cc, path)))),
    )
}

fn nav(ui: &mut egui::Ui, current: &mut Workspace, target: Workspace, label: &str, color: Color32) {
    if ui
        .selectable_label(
            *current == target,
            RichText::new(label).color(color).strong(),
        )
        .clicked()
    {
        *current = target;
    }
}
fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::default()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, Color32::from_rgb(28, 50, 61)))
        .corner_radius(8)
        .inner_margin(12)
        .show(ui, contents);
}
fn metric(ui: &mut egui::Ui, label: &str, value: &str, color: Color32) {
    card(ui, |ui| {
        ui.set_min_width(135.0);
        ui.label(RichText::new(label).small().color(MUTED));
        ui.label(RichText::new(value).size(23.0).monospace().color(color));
    });
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
        Vec2::new(ui.available_width().max(300.0), 120.0),
        Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6, PANEL);
    let plot = rect.shrink2(Vec2::new(12.0, 14.0));
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
    painter.add(egui::Shape::line(line, Stroke::new(1.5, EMERALD)));
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
