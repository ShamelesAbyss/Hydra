//! LEVIATHAN EVOLUTION — native UI tab inside the unified HYDRA cockpit.

use crate::evolution::{
    self, BackupRecord, DownloadProgress, MaintenanceGate, PlatformTarget, ProvenanceStatus,
    SemVer, StagedUpdate, UpdateChannel, UpdateCheckReport, UpdateDiagnostics,
    CANONICAL_RELEASES_PAGE, DEFAULT_GITHUB_RELEASES_API, PINNED_RELEASE_SIGNING_PUBKEY_HEX,
};
use crate::telemetry::TelemetrySnapshot;
use crate::treasury::OperationalMode;
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

const SURFACE: Color32 = Color32::from_rgb(14, 17, 30);
const PANEL: Color32 = Color32::from_rgb(9, 11, 21);
const TEXT: Color32 = Color32::from_rgb(224, 236, 246);
const MUTED: Color32 = Color32::from_rgb(138, 146, 174);
const AMETHYST: Color32 = Color32::from_rgb(179, 116, 250);
const ORCHID: Color32 = Color32::from_rgb(244, 127, 224);
const EMERALD: Color32 = Color32::from_rgb(58, 232, 165);
const CYAN: Color32 = Color32::from_rgb(54, 205, 245);
const AMBER: Color32 = Color32::from_rgb(246, 188, 83);
const CRIMSON: Color32 = Color32::from_rgb(247, 95, 105);

#[derive(Debug, Default, Clone, Copy)]
pub struct EvolutionEvents {
    pub genesis_pulse: bool,
}

enum WorkerReply {
    Checked(Result<UpdateCheckReport, String>),
    Staged(Result<StagedUpdate, String>),
}

pub struct EvolutionWorkspace {
    installed: SemVer,
    platform: PlatformTarget,
    channel: UpdateChannel,
    updates_root: PathBuf,
    checking: bool,
    downloading: bool,
    progress: Arc<DownloadProgress>,
    rx: Option<Receiver<WorkerReply>>,
    report: Option<UpdateCheckReport>,
    staged: Option<StagedUpdate>,
    diagnostics: UpdateDiagnostics,
    status_banner: Option<(bool, String)>,
}

impl Default for EvolutionWorkspace {
    fn default() -> Self {
        let updates_root = evolution::updates_root_dir();
        let diagnostics = evolution::load_diagnostics(&updates_root);
        Self {
            installed: evolution::installed_version(),
            platform: PlatformTarget::current(),
            channel: UpdateChannel::default_for_installed(),
            updates_root,
            checking: false,
            downloading: false,
            progress: Arc::new(DownloadProgress::default()),
            rx: None,
            report: None,
            staged: None,
            diagnostics,
            status_banner: None,
        }
    }
}

impl EvolutionWorkspace {
    pub fn is_busy(&self) -> bool {
        self.checking || self.downloading
    }

    pub fn update_available_tag(&self) -> Option<&str> {
        self.report
            .as_ref()
            .filter(|r| r.is_update_available)
            .and_then(|r| r.candidate.as_ref())
            .map(|c| c.tag_name.as_str())
    }

    pub fn trigger_check(&mut self, ctx: &egui::Context) {
        if self.is_busy() {
            return;
        }
        self.checking = true;
        self.status_banner = None;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let channel = self.channel;
        let installed = self.installed.clone();
        let platform = self.platform;
        let root = self.updates_root.clone();
        let ctx_clone = ctx.clone();
        std::thread::Builder::new()
            .name("hydra-evolution-check".into())
            .spawn(move || {
                let res = evolution::check_for_updates(
                    DEFAULT_GITHUB_RELEASES_API,
                    channel,
                    installed,
                    platform,
                );
                match &res {
                    Ok(rep) => {
                        evolution::record_diagnostic(&root, "check_updates", true, &rep.summary)
                    }
                    Err(err) => evolution::record_diagnostic(
                        &root,
                        "check_updates",
                        false,
                        format!("{err:#}"),
                    ),
                }
                let _ = tx.send(WorkerReply::Checked(res.map_err(|e| format!("{e:#}"))));
                ctx_clone.request_repaint();
            })
            .ok();
    }

    fn trigger_download(&mut self, ctx: &egui::Context) {
        if self.is_busy() {
            return;
        }
        let Some(candidate) = self.report.as_ref().and_then(|r| r.candidate.clone()) else {
            return;
        };
        self.downloading = true;
        self.staged = None;
        self.status_banner = None;
        let progress = Arc::new(DownloadProgress::default());
        self.progress = Arc::clone(&progress);
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let root = self.updates_root.clone();
        let ctx_clone = ctx.clone();
        std::thread::Builder::new()
            .name("hydra-evolution-download".into())
            .spawn(move || {
                let res = evolution::download_and_stage_release(
                    &candidate,
                    &root,
                    PINNED_RELEASE_SIGNING_PUBKEY_HEX,
                    progress,
                    false,
                )
                .map_err(|e| format!("{e:#}"));
                let _ = tx.send(WorkerReply::Staged(res));
                ctx_clone.request_repaint();
            })
            .ok();
    }

    pub fn poll(&mut self, ctx: &egui::Context) -> EvolutionEvents {
        let mut events = EvolutionEvents::default();
        if self.is_busy() {
            ctx.request_repaint();
        }
        let mut finished = false;
        if let Some(rx) = &self.rx {
            if let Ok(reply) = rx.try_recv() {
                finished = true;
                events.genesis_pulse = true;
                match reply {
                    WorkerReply::Checked(Ok(report)) => {
                        self.checking = false;
                        self.status_banner = Some((true, report.summary.clone()));
                        self.report = Some(report);
                    }
                    WorkerReply::Checked(Err(err)) => {
                        self.checking = false;
                        self.status_banner = Some((false, format!("Update check failed: {err}")));
                    }
                    WorkerReply::Staged(Ok(staged)) => {
                        self.downloading = false;
                        self.status_banner = Some((
                            true,
                            format!(
                                "Downloaded and SHA-256 verified {} (provenance: {}).",
                                staged.asset_name,
                                staged.provenance.label()
                            ),
                        ));
                        self.staged = Some(staged);
                    }
                    WorkerReply::Staged(Err(err)) => {
                        self.downloading = false;
                        self.status_banner = Some((
                            false,
                            format!("Package download/verification failed: {err}"),
                        ));
                    }
                }
                self.diagnostics = evolution::load_diagnostics(&self.updates_root);
            }
        }
        if finished {
            self.rx = None;
        }
        events
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        mode: OperationalMode,
        snapshot: Option<&TelemetrySnapshot>,
    ) -> EvolutionEvents {
        let mut events = EvolutionEvents::default();
        let gate = MaintenanceGate::evaluate(mode, snapshot);

        // Header bar
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("LEVIATHAN EVOLUTION")
                    .monospace()
                    .strong()
                    .size(14.5)
                    .color(ORCHID),
            );
            ui.label(
                RichText::new("NATIVE UPDATE MANAGER · SHA-256 INTEGRITY · SIGNED PROVENANCE GATE")
                    .monospace()
                    .size(10.0)
                    .color(MUTED),
            );
        });
        ui.add_space(6.0);

        // Summary & channel controls
        egui::Frame::new()
            .fill(SURFACE)
            .stroke(Stroke::new(1.0, ORCHID.gamma_multiply(0.35)))
            .corner_radius(6)
            .inner_margin(10)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("INSTALLED VERSION")
                            .monospace()
                            .size(9.5)
                            .color(MUTED),
                    );
                    ui.label(
                        RichText::new(format!("v{}", self.installed))
                            .monospace()
                            .strong()
                            .color(CYAN),
                    );
                    ui.separator();
                    ui.label(RichText::new("PLATFORM").monospace().size(9.5).color(MUTED));
                    ui.label(
                        RichText::new(self.platform.display_label())
                            .monospace()
                            .color(TEXT),
                    );
                    ui.separator();
                    ui.label(RichText::new("CHANNEL").monospace().size(9.5).color(MUTED));
                    egui::ComboBox::from_id_salt("evolution-channel")
                        .selected_text(self.channel.label())
                        .show_ui(ui, |ui| {
                            for ch in [UpdateChannel::Development, UpdateChannel::Stable] {
                                ui.selectable_value(&mut self.channel, ch, ch.label());
                            }
                        });
                    ui.separator();
                    let check_btn = ui.add_enabled(
                        !self.is_busy(),
                        egui::Button::new(
                            RichText::new(if self.checking {
                                "Checking GitHub Releases…"
                            } else {
                                "Check for Updates"
                            })
                            .strong()
                            .color(ORCHID),
                        ),
                    );
                    if check_btn.clicked() {
                        self.trigger_check(ui.ctx());
                        events.genesis_pulse = true;
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "{} · Canonical Releases: {}",
                        self.channel.description(),
                        CANONICAL_RELEASES_PAGE
                    ))
                    .small()
                    .color(MUTED),
                );
            });

        ui.add_space(6.0);

        // Safety & Provenance Status Strip
        ui.columns(2, |cols| {
            egui::Frame::new()
                .fill(PANEL)
                .stroke(Stroke::new(
                    1.0,
                    if gate.safe_to_install {
                        EMERALD.gamma_multiply(0.45)
                    } else {
                        CRIMSON.gamma_multiply(0.6)
                    },
                ))
                .corner_radius(6)
                .inner_margin(9)
                .show(&mut cols[0], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new("TRADING-SAFE MAINTENANCE GATE")
                            .monospace()
                            .size(10.0)
                            .strong()
                            .color(if gate.safe_to_install { EMERALD } else { CRIMSON }),
                    );
                    if gate.safe_to_install {
                        ui.label(
                            RichText::new(
                                "SAFE — No live/guardian session, no open positions, and no unresolved execution intents.",
                            )
                            .small()
                            .color(TEXT),
                        );
                    } else {
                        for b in &gate.blockers {
                            ui.colored_label(AMBER, format!("• {b}"));
                        }
                    }
                    ui.label(
                        RichText::new(
                            "Local databases, Treasury DPAPI credentials, and risk policies are never touched; restart always boots DORMANT.",
                        )
                        .small()
                        .color(MUTED),
                    );
                });

            let signing_configured = PINNED_RELEASE_SIGNING_PUBKEY_HEX.is_some();
            egui::Frame::new()
                .fill(PANEL)
                .stroke(Stroke::new(
                    1.0,
                    if signing_configured {
                        EMERALD.gamma_multiply(0.45)
                    } else {
                        AMBER.gamma_multiply(0.55)
                    },
                ))
                .corner_radius(6)
                .inner_margin(9)
                .show(&mut cols[1], |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new("SIGNED PROVENANCE POLICY")
                            .monospace()
                            .size(10.0)
                            .strong()
                            .color(if signing_configured { EMERALD } else { AMBER }),
                    );
                    if signing_configured {
                        ui.label(
                            RichText::new("Pinned secp256k1 release-signing key active.")
                                .small()
                                .color(EMERALD),
                        );
                    } else {
                        ui.label(
                            RichText::new(
                                "Automatic installation is LOCKED until pinned release-signing key infrastructure is configured. SHA-256 verification and manual staging remain available.",
                            )
                            .small()
                            .color(AMBER),
                        );
                    }
                });
        });

        if let Some((ok, msg)) = &self.status_banner {
            ui.add_space(6.0);
            ui.colored_label(if *ok { EMERALD } else { AMBER }, msg);
        }

        // Download progress bar
        if self.downloading {
            ui.add_space(6.0);
            let frac = self.progress.fraction();
            let done = self
                .progress
                .downloaded_bytes
                .load(std::sync::atomic::Ordering::Relaxed);
            let total = self
                .progress
                .total_bytes
                .load(std::sync::atomic::Ordering::Relaxed);
            ui.horizontal(|ui| {
                ui.add(
                    egui::ProgressBar::new(frac)
                        .desired_width(360.0)
                        .text(format!(
                            "{:.1} / {:.1} MiB ({:.0}%)",
                            done as f64 / 1_048_576.0,
                            total as f64 / 1_048_576.0,
                            frac * 100.0
                        )),
                );
                if ui.button("Cancel Download").clicked() {
                    self.progress.cancel();
                }
            });
        }

        // Release Candidate Panel
        if let Some(report) = self.report.clone() {
            ui.add_space(8.0);
            if let Some(cand) = &report.candidate {
                egui::Frame::new()
                    .fill(SURFACE)
                    .stroke(Stroke::new(1.0, AMETHYST.gamma_multiply(0.4)))
                    .corner_radius(6)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(&cand.title)
                                    .strong()
                                    .size(13.5)
                                    .color(TEXT),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "[{} · {}]",
                                    cand.tag_name,
                                    if cand.prerelease { "PRERELEASE" } else { "STABLE" }
                                ))
                                .monospace()
                                .size(10.0)
                                .color(if cand.prerelease { AMBER } else { EMERALD }),
                            );
                        });
                        ui.label(
                            RichText::new(format!(
                                "Published: {} · URL: {}",
                                cand.published_at, cand.html_url
                            ))
                            .small()
                            .color(MUTED),
                        );
                        ui.add_space(4.0);
                        match (&cand.primary_asset, &cand.checksum_asset) {
                            (Some(asset), Some(sums)) => {
                                ui.label(
                                    RichText::new(format!(
                                        "Matched Package: {} ({:.2} MiB) · Checksums: {}",
                                        asset.name,
                                        asset.size as f64 / 1_048_576.0,
                                        sums.name
                                    ))
                                    .monospace()
                                    .small()
                                    .color(CYAN),
                                );
                                ui.add_space(4.0);
                                ui.horizontal_wrapped(|ui| {
                                    let dl_btn = ui.add_enabled(
                                        !self.is_busy(),
                                        egui::Button::new(
                                            RichText::new("Download & Verify Package (SHA-256)")
                                                .strong()
                                                .color(EMERALD),
                                        ),
                                    );
                                    if dl_btn.clicked() {
                                        self.trigger_download(ui.ctx());
                                        events.genesis_pulse = true;
                                    }
                                });
                            }
                            (None, _) => {
                                ui.colored_label(
                                    AMBER,
                                    format!(
                                        "No compatible binary asset found for {}.",
                                        self.platform.display_label()
                                    ),
                                );
                            }
                            (Some(_), None) => {
                                ui.colored_label(
                                    AMBER,
                                    "Release is missing SHA256SUMS.txt — unverified downloads are refused.",
                                );
                            }
                        }
                        ui.add_space(6.0);
                        ui.collapsing("Release Notes", |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("evolution-notes-scroll")
                                .max_height(150.0)
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(&cand.release_notes)
                                            .monospace()
                                            .size(11.0)
                                            .color(TEXT),
                                    );
                                });
                        });
                    });
            }
        }

        // Staged Package & Rollback Controls
        if let Some(staged) = self.staged.clone() {
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(PANEL)
                .stroke(Stroke::new(1.0, CYAN.gamma_multiply(0.45)))
                .corner_radius(6)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new("STAGED PACKAGE VERIFICATION")
                            .monospace()
                            .strong()
                            .color(CYAN),
                    );
                    ui.label(
                        RichText::new(format!("Staged file: {}", staged.staged_path.display()))
                            .monospace()
                            .small(),
                    );
                    ui.label(
                        RichText::new(format!(
                            "SHA-256: {} ({})",
                            staged.sha256,
                            if staged.sha256_verified { "MATCHED" } else { "FAILED" }
                        ))
                        .monospace()
                        .small()
                        .color(if staged.sha256_verified { EMERALD } else { CRIMSON }),
                    );
                    ui.label(
                        RichText::new(format!("Provenance: {}", staged.provenance.label()))
                            .monospace()
                            .small()
                            .color(match staged.provenance {
                                ProvenanceStatus::Verified { .. } => EMERALD,
                                ProvenanceStatus::Invalid(_) => CRIMSON,
                                _ => AMBER,
                            }),
                    );
                    ui.add_space(4.0);
                    let can_apply = staged.auto_install_allowed() && gate.safe_to_install;
                    ui.horizontal_wrapped(|ui| {
                        let apply_btn = ui.add_enabled(
                            can_apply,
                            egui::Button::new(
                                RichText::new("Apply Verified Update & Safe Restart").strong(),
                            ),
                        );
                        if apply_btn.clicked() {
                            if let Ok(exe) = std::env::current_exe() {
                                match evolution::prepare_verified_restart_script(
                                    &staged,
                                    &gate,
                                    &exe,
                                    &self.updates_root,
                                ) {
                                    Ok(script) => {
                                        self.status_banner = Some((
                                            true,
                                            format!(
                                                "Prepared verified restart handoff script at {}",
                                                script.display()
                                            ),
                                        ));
                                    }
                                    Err(err) => {
                                        self.status_banner = Some((
                                            false,
                                            format!("Install preparation refused: {err:#}"),
                                        ));
                                    }
                                }
                                self.diagnostics = evolution::load_diagnostics(&self.updates_root);
                            }
                        }
                        if !staged.auto_install_allowed() {
                            ui.label(
                                RichText::new(
                                    "Auto-install remains disabled until signed provenance is configured; you may inspect and launch the SHA-256-verified staged file manually.",
                                )
                                .small()
                                .color(MUTED),
                            );
                        }
                    });
                });
        }

        // Backup & Diagnostics section
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Create Backup of Current Executable").clicked() {
                if let Ok(exe) = std::env::current_exe() {
                    match evolution::backup_executable(
                        &exe,
                        &self.installed.to_string(),
                        &self.updates_root,
                    ) {
                        Ok(rec) => {
                            self.status_banner = Some((
                                true,
                                format!("Verified backup saved to {}", rec.backup_path.display()),
                            ));
                        }
                        Err(err) => {
                            self.status_banner = Some((false, format!("Backup failed: {err:#}")));
                        }
                    }
                    self.diagnostics = evolution::load_diagnostics(&self.updates_root);
                }
            }
            ui.label(
                RichText::new(format!(
                    "Staging & backup root: {}",
                    self.updates_root.display()
                ))
                .monospace()
                .small()
                .color(MUTED),
            );
        });

        if !self.diagnostics.backups.is_empty() {
            ui.add_space(4.0);
            ui.collapsing(
                format!("Rollback Backups ({})", self.diagnostics.backups.len()),
                |ui| {
                    let backups: Vec<BackupRecord> =
                        self.diagnostics.backups.iter().rev().cloned().collect();
                    for b in backups {
                        ui.label(
                            RichText::new(format!(
                                "{} · v{} · sha256={}… · {}",
                                b.created_at,
                                b.installed_version,
                                &b.sha256[..16.min(b.sha256.len())],
                                b.backup_path.display()
                            ))
                            .monospace()
                            .size(10.5)
                            .color(MUTED),
                        );
                    }
                },
            );
        }

        if !self.diagnostics.entries.is_empty() {
            ui.add_space(4.0);
            ui.collapsing(
                format!(
                    "Installation Diagnostics ({})",
                    self.diagnostics.entries.len()
                ),
                |ui| {
                    for entry in self.diagnostics.entries.iter().rev().take(12) {
                        ui.horizontal_wrapped(|ui| {
                            let (dot, _) =
                                ui.allocate_exact_size(Vec2::new(6.0, 6.0), egui::Sense::hover());
                            ui.painter().circle_filled(
                                dot.center(),
                                2.8,
                                if entry.ok { EMERALD } else { AMBER },
                            );
                            ui.label(
                                RichText::new(format!(
                                    "{} [{}] {}",
                                    entry.timestamp, entry.stage, entry.message
                                ))
                                .monospace()
                                .size(10.0)
                                .color(if entry.ok {
                                    TEXT
                                } else {
                                    AMBER
                                }),
                            );
                        });
                    }
                },
            );
        }

        events
    }
}
