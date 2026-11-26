//! Native LEVIATHAN TREASURY settings tab, THE OFFERING onboarding ritual,
//! truthful 12-metric portfolio surface, and Awakening the Beast operational controls.
//!
//! Private keys are masked in the UI, scrubbed immediately after OS-protected storage,
//! and never transmitted over any network call or emitted to logs/telemetry.

use crate::{
    telemetry::{self, TelemetrySnapshot},
    treasury::{
        self, CapabilityStage, GateState, HyperliquidVerification, MetricProvenance,
        OperationalMode, ProtectionBackend, ReadinessReport, TreasuryPortfolioSummary,
        TreasuryProfile, TreasuryStore, AUTONOMOUS_CAPABILITIES, OFFERING_ACKNOWLEDGMENT,
    },
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, RichText, Sense, Shape, Stroke, Vec2};
use std::sync::mpsc::{self, Receiver};

const SURFACE: Color32 = Color32::from_rgb(13, 16, 29);
const DEEP: Color32 = Color32::from_rgb(8, 10, 20);
const TEXT: Color32 = Color32::from_rgb(224, 236, 246);
const MUTED: Color32 = Color32::from_rgb(142, 150, 178);
const AMETHYST: Color32 = Color32::from_rgb(184, 122, 255);
const GOLD: Color32 = Color32::from_rgb(245, 194, 92);
const EMERALD: Color32 = Color32::from_rgb(58, 232, 165);
const NEXUS_BLUE: Color32 = Color32::from_rgb(73, 151, 255);
const CYAN: Color32 = Color32::from_rgb(54, 205, 245);
const CRIMSON: Color32 = Color32::from_rgb(247, 95, 105);

#[derive(Clone, Copy, PartialEq, Eq)]
enum TreasurySection {
    Offering,
    Portfolio,
    Awakening,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TreasuryEvents {
    /// True when genuine wallet configuration or venue verification just succeeded.
    pub offering_illuminated: bool,
    /// True when emergency suspension was triggered this frame.
    pub emergency_suspended: bool,
}

pub struct TreasuryWorkspace {
    store: Option<TreasuryStore>,
    profile: Option<TreasuryProfile>,
    has_protected_key: bool,
    section: TreasurySection,

    // Onboarding / replacement inputs (masked; scrubbed on save/cancel/drop).
    wallet_address_input: String,
    private_key_input: String,
    reveal_key: bool,
    risk_ack_checked: bool,
    wallet_auth_checked: bool,
    replacing_credentials: bool,
    confirm_removal: bool,

    // Operational mode & background verification.
    mode: OperationalMode,
    mode_notice: Option<String>,
    status_message: Option<String>,
    error_message: Option<String>,
    verify_rx: Option<Receiver<Result<HyperliquidVerification, String>>>,
    verification: Option<HyperliquidVerification>,
    integrity_verified: bool,
}

impl Default for TreasuryWorkspace {
    fn default() -> Self {
        let store = TreasuryStore::default_store().ok();
        let (profile, has_key, init_err) = match &store {
            Some(s) => match s.load_profile() {
                Ok(p) => (p, s.has_protected_key(), None),
                Err(e) => (
                    None,
                    s.has_protected_key(),
                    Some(telemetry::redact_diagnostic(&format!("{e:#}"))),
                ),
            },
            None => (
                None,
                false,
                Some("OS state directory unavailable for Treasury credential storage.".into()),
            ),
        };
        let mode = if profile.as_ref().is_some_and(|p| p.emergency_suspended) {
            OperationalMode::Suspended
        } else {
            OperationalMode::Dormant
        };
        let wallet_address_input = profile
            .as_ref()
            .map(|p| p.dedicated_wallet_address.clone())
            .unwrap_or_default();
        let risk_ack_checked = profile.as_ref().is_some_and(|p| p.risk_acknowledged);
        let wallet_auth_checked = profile.as_ref().is_some_and(|p| p.wallet_authorized);

        Self {
            store,
            profile,
            has_protected_key: has_key,
            section: TreasurySection::Offering,
            wallet_address_input,
            private_key_input: String::new(),
            reveal_key: false,
            risk_ack_checked,
            wallet_auth_checked,
            replacing_credentials: false,
            confirm_removal: false,
            mode,
            mode_notice: None,
            status_message: None,
            error_message: init_err,
            verify_rx: None,
            verification: None,
            integrity_verified: false,
        }
    }
}

impl Drop for TreasuryWorkspace {
    fn drop(&mut self) {
        scrub_string(&mut self.private_key_input);
    }
}

fn scrub_string(s: &mut String) {
    // SAFETY: zeroing UTF-8 buffer with ASCII NUL bytes keeps valid UTF-8 before clear.
    unsafe {
        for b in s.as_bytes_mut() {
            std::ptr::write_volatile(b, 0);
        }
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    s.clear();
}

impl TreasuryWorkspace {
    pub fn mode(&self) -> OperationalMode {
        self.mode
    }
    pub fn is_suspended(&self) -> bool {
        self.mode == OperationalMode::Suspended
            || self.profile.as_ref().is_some_and(|p| p.emergency_suspended)
    }
    pub fn is_wallet_configured(&self) -> bool {
        self.profile.is_some() && self.has_protected_key
    }
    pub fn is_venue_verified(&self) -> bool {
        self.verification.is_some()
            && self.profile.as_ref().is_some_and(|p| {
                self.verification.as_ref().is_some_and(|v| {
                    v.dedicated_wallet_address == p.dedicated_wallet_address
                        && v.derived_signer_address == p.derived_signer_address
                })
            })
    }
    pub fn verifying(&self) -> bool {
        self.verify_rx.is_some()
    }
    pub fn profile(&self) -> Option<&TreasuryProfile> {
        self.profile.as_ref()
    }
    pub fn verification(&self) -> Option<&HyperliquidVerification> {
        self.verification.as_ref()
    }

    pub fn readiness(&self, snapshot: Option<&TelemetrySnapshot>) -> ReadinessReport {
        ReadinessReport::evaluate(
            self.profile.as_ref(),
            self.has_protected_key,
            self.verification.as_ref(),
            snapshot,
        )
    }

    /// Prominent one-click emergency suspension callable from anywhere in the command center.
    pub fn trigger_emergency_suspension(&mut self, reason: &str) {
        self.mode = OperationalMode::Suspended;
        self.wallet_auth_checked = false;
        self.mode_notice = Some(
            "EMERGENCY SUSPENSION ACTIVE — all new trading activity blocked. Protective position management remains available where appropriate.".into(),
        );
        if let Some(store) = &self.store {
            if self.profile.is_some() {
                match store.update_flags(
                    self.risk_ack_checked,
                    false,
                    true,
                    Some(reason.to_owned()),
                ) {
                    Ok(p) => {
                        self.profile = Some(p);
                        self.error_message = None;
                    }
                    Err(e) => {
                        self.error_message = Some(telemetry::redact_diagnostic(&format!("{e:#}")));
                    }
                }
            }
        }
    }

    /// Clear emergency suspension and return safely to DORMANT (never straight to live).
    pub fn clear_emergency_suspension(&mut self) {
        self.mode = OperationalMode::Dormant;
        self.mode_notice =
            Some("Emergency suspension cleared. Mode returned safely to DORMANT.".into());
        if let Some(store) = &self.store {
            if self.profile.is_some() {
                match store.update_flags(self.risk_ack_checked, false, false, None) {
                    Ok(p) => {
                        self.profile = Some(p);
                        self.wallet_auth_checked = false;
                        self.error_message = None;
                    }
                    Err(e) => {
                        self.error_message = Some(telemetry::redact_diagnostic(&format!("{e:#}")));
                    }
                }
            }
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context) -> TreasuryEvents {
        let mut events = TreasuryEvents::default();
        let recv = self.verify_rx.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(res) => Some(res),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "Hyperliquid verification worker ended unexpectedly.".into(),
            )),
        });
        if let Some(result) = recv {
            self.verify_rx = None;
            match result {
                Ok(v) => {
                    self.status_message = Some(format!(
                        "Hyperliquid account verified ({} mode · equity ${:.2} · available collateral ${:.2} · {} ms).",
                        v.account_mode, v.account_equity_usd, v.withdrawable_collateral_usd, v.latency_ms
                    ));
                    self.error_message = None;
                    self.verification = Some(v);
                    events.offering_illuminated = true;
                }
                Err(err) => {
                    self.error_message = Some(err);
                }
            }
            ctx.request_repaint();
        }
        events
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: Option<&TelemetrySnapshot>,
        time: f32,
        motion: bool,
    ) -> TreasuryEvents {
        let mut events = TreasuryEvents::default();
        let readiness = self.readiness(snapshot);

        // Top Treasury sub-navigation + status strip.
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("LEVIATHAN TREASURY")
                    .monospace()
                    .size(14.0)
                    .strong()
                    .color(AMETHYST),
            );
            ui.label(
                RichText::new("DEDICATED WALLET VAULT  ·  THE OFFERING  ·  OPERATIONAL CONTROL")
                    .monospace()
                    .size(10.0)
                    .color(MUTED),
            );
            ui.separator();
            if ui
                .selectable_label(
                    self.section == TreasurySection::Offering,
                    RichText::new("THE OFFERING").monospace().color(GOLD),
                )
                .clicked()
            {
                self.section = TreasurySection::Offering;
            }
            if ui
                .selectable_label(
                    self.section == TreasurySection::Portfolio,
                    RichText::new("TREASURY & PORTFOLIO")
                        .monospace()
                        .color(EMERALD),
                )
                .clicked()
            {
                self.section = TreasurySection::Portfolio;
            }
            if ui
                .selectable_label(
                    self.section == TreasurySection::Awakening,
                    RichText::new("AWAKENING & AUTONOMY")
                        .monospace()
                        .color(AMETHYST),
                )
                .clicked()
            {
                self.section = TreasurySection::Awakening;
            }
        });
        ui.add_space(6.0);

        match self.section {
            TreasurySection::Offering => {
                self.show_offering(ui, &readiness, time, motion, &mut events);
            }
            TreasurySection::Portfolio => {
                self.show_portfolio(ui, snapshot);
            }
            TreasurySection::Awakening => {
                self.show_awakening(ui, &readiness, &mut events);
            }
        }
        events
    }

    fn show_offering(
        &mut self,
        ui: &mut egui::Ui,
        readiness: &ReadinessReport,
        time: f32,
        motion: bool,
        events: &mut TreasuryEvents,
    ) {
        // 1. Ceremonial obsidian-amethyst Offering Altar visualizer (Treasury -> Heart -> AEGIS & NEXUS).
        let validated = self.is_wallet_configured();
        let verified = self.is_venue_verified();
        offering_altar_banner(ui, validated, verified, self.is_suspended(), time, motion);
        ui.add_space(8.0);

        // 2. Dedicated wallet warning banner (never primary personal wallet).
        egui::Frame::new()
            .fill(Color32::from_rgb(24, 17, 34))
            .stroke(Stroke::new(1.0, GOLD.gamma_multiply(0.55)))
            .corner_radius(6)
            .inner_margin(10)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new("DEDICATED TRADING WALLET REQUIREMENT")
                        .monospace()
                        .size(11.0)
                        .strong()
                        .color(GOLD),
                );
                ui.label(
                    RichText::new(
                        "Create and fund a dedicated cryptocurrency wallet exclusively for Hydra. Never connect your primary personal wallet. Private signing keys stay strictly on this machine under OS-protected storage and are never transmitted to AI services, telemetry, logs, or developers. Automatic withdrawals and arbitrary transfers are permanently disabled.",
                    )
                    .small()
                    .color(TEXT),
                );
            });
        ui.add_space(8.0);

        if let Some(msg) = &self.status_message {
            ui.colored_label(EMERALD, msg);
        }
        if let Some(err) = &self.error_message {
            ui.colored_label(CRIMSON, format!("TREASURY DIAGNOSTIC · {err}"));
        }

        // 3. Configured Vault Overview OR Onboarding / Replacement Form.
        let show_form = !self.is_wallet_configured() || self.replacing_credentials;
        if let Some(profile) = self.profile.clone() {
            egui::Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(
                    1.0,
                    if verified {
                        EMERALD.gamma_multiply(0.6)
                    } else {
                        AMETHYST.gamma_multiply(0.5)
                    },
                ))
                .corner_radius(6)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new("VAULTED DEDICATED WALLET")
                                .monospace()
                                .strong()
                                .color(AMETHYST),
                        );
                        badge(
                            ui,
                            if verified {
                                "HYPERLIQUID VERIFIED"
                            } else if self.has_protected_key {
                                "KEY VAULTED (UNVERIFIED ON VENUE)"
                            } else {
                                "PROTECTED KEY MISSING"
                            },
                            if verified {
                                EMERALD
                            } else if self.has_protected_key {
                                GOLD
                            } else {
                                CRIMSON
                            },
                        );
                        badge(
                            ui,
                            if profile.wallet_authorized {
                                "WALLET AUTHORIZED"
                            } else {
                                "AUTHORIZATION OFF"
                            },
                            if profile.wallet_authorized {
                                CYAN
                            } else {
                                MUTED
                            },
                        );
                    });
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Dedicated wallet:").small().color(MUTED));
                        ui.monospace(&profile.dedicated_wallet_address);
                        ui.label(RichText::new("  |  Derived signer:").small().color(MUTED));
                        ui.monospace(treasury::mask_address(&profile.derived_signer_address));
                    });
                    ui.label(
                        RichText::new(format!(
                            "Relationship: {}  ·  Protection: {}  ·  Private key: •••••••••••••••••••• (OS-protected)",
                            profile.signer_relationship.label(),
                            profile.protection_backend.label()
                        ))
                        .small()
                        .color(MUTED),
                    );
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(
                                self.verify_rx.is_none() && self.has_protected_key,
                                egui::Button::new("Verify Hyperliquid Account"),
                            )
                            .on_hover_text(
                                "Read-only public-address query to Hyperliquid /info. Private signing key never touches the network.",
                            )
                            .clicked()
                        {
                            self.start_hyperliquid_verification(&profile, ui.ctx());
                        }
                        if ui.button("Check Local Vault Integrity").clicked() {
                            if let Some(store) = &self.store {
                                match store.verify_vault_integrity() {
                                    Ok(addr) => {
                                        self.integrity_verified = true;
                                        self.status_message = Some(format!(
                                            "Local OS-protected key decrypted and matched signer {}.",
                                            treasury::mask_address(&addr)
                                        ));
                                        self.error_message = None;
                                    }
                                    Err(e) => {
                                        self.integrity_verified = false;
                                        self.error_message =
                                            Some(telemetry::redact_diagnostic(&format!("{e:#}")));
                                    }
                                }
                            }
                        }
                        if ui
                            .button(if self.replacing_credentials {
                                "Cancel Replacement"
                            } else {
                                "Replace Credentials"
                            })
                            .clicked()
                        {
                            self.replacing_credentials = !self.replacing_credentials;
                            scrub_string(&mut self.private_key_input);
                            self.reveal_key = false;
                        }
                        if !self.confirm_removal {
                            if ui.button("Remove Credentials").clicked() {
                                self.confirm_removal = true;
                            }
                        } else {
                            if ui
                                .button(RichText::new("Confirm Permanent Wipe").color(CRIMSON))
                                .clicked()
                            {
                                if let Some(store) = &self.store {
                                    match store.remove_credentials() {
                                        Ok(()) => {
                                            self.profile = None;
                                            self.has_protected_key = false;
                                            self.verification = None;
                                            self.integrity_verified = false;
                                            self.wallet_auth_checked = false;
                                            self.replacing_credentials = false;
                                            self.confirm_removal = false;
                                            scrub_string(&mut self.private_key_input);
                                            self.status_message = Some(
                                                "Dedicated wallet credentials overwritten and removed from Treasury.".into(),
                                            );
                                            self.error_message = None;
                                        }
                                        Err(e) => {
                                            self.error_message = Some(
                                                telemetry::redact_diagnostic(&format!("{e:#}")),
                                            );
                                        }
                                    }
                                }
                            }
                            if ui.button("Cancel").clicked() {
                                self.confirm_removal = false;
                            }
                        }
                    });
                    if self.verify_rx.is_some() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Verifying dedicated Hyperliquid account (public address only)...");
                        });
                    }

                    // Explicit authorization & acknowledgment controls for existing vaulted wallet.
                    if !show_form {
                        ui.separator();
                        let prev_ack = self.risk_ack_checked;
                        let prev_auth = self.wallet_auth_checked;
                        offering_acknowledgment_box(ui, &mut self.risk_ack_checked);
                        ui.add_enabled_ui(self.risk_ack_checked && !self.is_suspended(), |ui| {
                            ui.checkbox(
                                &mut self.wallet_auth_checked,
                                "Explicitly authorize this dedicated wallet for Hydra (live AWAKENED mode remains gated until all readiness checks pass)",
                            );
                        });
                        if prev_ack != self.risk_ack_checked || prev_auth != self.wallet_auth_checked
                        {
                            if !self.risk_ack_checked {
                                self.wallet_auth_checked = false;
                            }
                            if let Some(store) = &self.store {
                                match store.update_flags(
                                    self.risk_ack_checked,
                                    self.wallet_auth_checked,
                                    self.is_suspended(),
                                    None,
                                ) {
                                    Ok(updated) => {
                                        self.profile = Some(updated);
                                        self.error_message = None;
                                    }
                                    Err(e) => {
                                        self.error_message =
                                            Some(telemetry::redact_diagnostic(&format!("{e:#}")));
                                    }
                                }
                            }
                        }
                    }
                });
            ui.add_space(8.0);
        }

        if show_form {
            egui::Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, AMETHYST.gamma_multiply(0.55)))
                .corner_radius(6)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(if self.replacing_credentials {
                            "REPLACE DEDICATED WALLET CREDENTIALS"
                        } else {
                            "THE OFFERING — ENTRUST A DEDICATED WALLET"
                        })
                        .monospace()
                        .size(12.0)
                        .strong()
                        .color(AMETHYST),
                    );
                    ui.label(
                        RichText::new(format!(
                            "Storage protection on this host: {}",
                            ProtectionBackend::current().label()
                        ))
                        .small()
                        .color(MUTED),
                    );
                    ui.add_space(6.0);

                    egui::Grid::new("treasury-offering-grid")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Dedicated Wallet Public Address");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.wallet_address_input)
                                    .hint_text("0x... (dedicated Hyperliquid trading address)")
                                    .desired_width(440.0),
                            );
                            ui.end_row();

                            ui.label("Private Signing Key (Masked)");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.private_key_input)
                                        .password(!self.reveal_key)
                                        .hint_text("64-hex-char secp256k1 key (stored OS-protected)")
                                        .desired_width(360.0),
                                );
                                ui.checkbox(&mut self.reveal_key, "Show");
                            });
                            ui.end_row();
                        });

                    // Live local format feedback (never prints key material).
                    if !self.wallet_address_input.trim().is_empty()
                        && !self.private_key_input.trim().is_empty()
                    {
                        match treasury::validate_wallet_inputs(
                            &self.wallet_address_input,
                            &self.private_key_input,
                        ) {
                            Ok(v) => {
                                ui.colored_label(
                                    EMERALD,
                                    format!(
                                        "Local identity valid · {} · derived signer {}",
                                        v.signer_relationship.label(),
                                        treasury::mask_address(&v.derived_signer_address)
                                    ),
                                );
                            }
                            Err(e) => {
                                ui.colored_label(GOLD, format!("Validation pending: {e}"));
                            }
                        }
                    }

                    ui.add_space(6.0);
                    offering_acknowledgment_box(ui, &mut self.risk_ack_checked);
                    ui.add_enabled_ui(self.risk_ack_checked, |ui| {
                        ui.checkbox(
                            &mut self.wallet_auth_checked,
                            "Explicitly authorize this dedicated wallet after validation",
                        );
                    });
                    ui.add_space(6.0);

                    if ui
                        .button(
                            RichText::new("Complete The Offering · Save to OS-Protected Vault")
                                .strong()
                                .color(Color32::from_rgb(244, 232, 255)),
                        )
                        .clicked()
                    {
                        if let Some(store) = &self.store {
                            match store.save_offering(
                                &self.wallet_address_input,
                                &self.private_key_input,
                                self.risk_ack_checked,
                                self.wallet_auth_checked,
                            ) {
                                Ok(profile) => {
                                    scrub_string(&mut self.private_key_input);
                                    self.reveal_key = false;
                                    self.replacing_credentials = false;
                                    self.verification = None;
                                    self.has_protected_key = true;
                                    self.integrity_verified = true;
                                    self.status_message = Some(format!(
                                        "The Offering accepted: dedicated wallet {} sealed in {}. Verify Hyperliquid account next.",
                                        treasury::mask_address(&profile.dedicated_wallet_address),
                                        profile.protection_backend.label()
                                    ));
                                    self.profile = Some(profile);
                                    self.error_message = None;
                                    events.offering_illuminated = true;
                                }
                                Err(e) => {
                                    self.error_message =
                                        Some(telemetry::redact_diagnostic(&format!("{e:#}")));
                                }
                            }
                        } else {
                            self.error_message =
                                Some("Treasury OS credential vault is unavailable.".into());
                        }
                    }
                });
            ui.add_space(8.0);
        }

        // 4. Connection Diagnostics & Verification Details.
        if let Some(v) = &self.verification {
            egui::Frame::new()
                .fill(DEEP)
                .stroke(Stroke::new(0.9, EMERALD.gamma_multiply(0.45)))
                .corner_radius(6)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new("HYPERLIQUID VERIFICATION DIAGNOSTICS")
                            .monospace()
                            .size(11.0)
                            .strong()
                            .color(EMERALD),
                    );
                    ui.label(format!(
                        "Verified at {} · latency {} ms · account mode {} · equity ${:.2} · available collateral ${:.2} · margin used ${:.2} · open positions {}",
                        v.verified_at,
                        v.latency_ms,
                        v.account_mode,
                        v.account_equity_usd,
                        v.withdrawable_collateral_usd,
                        v.margin_used_usd,
                        v.open_positions_count
                    ));
                    for d in &v.diagnostics {
                        ui.label(RichText::new(format!("• {d}")).small().color(MUTED));
                    }
                });
            ui.add_space(6.0);
        }

        // 5. Readiness Gate summary directly visible on The Offering tab.
        readiness_gate_panel(ui, readiness);
    }

    fn show_portfolio(&mut self, ui: &mut egui::Ui, snapshot: Option<&TelemetrySnapshot>) {
        let summary = TreasuryPortfolioSummary::build(self.verification.as_ref(), snapshot);
        ui.label(
            RichText::new(
                "Authoritative financial view. Unavailable values remain explicitly unavailable; external deposits and withdrawals are never counted as trading profit.",
            )
            .small()
            .color(MUTED),
        );
        ui.add_space(6.0);

        ui.horizontal_wrapped(|ui| {
            for metric in &summary.metrics {
                let accent = match metric.provenance {
                    MetricProvenance::VerifiedVenue => EMERALD,
                    MetricProvenance::RecordedLocal => CYAN,
                    MetricProvenance::Unavailable => MUTED,
                };
                egui::Frame::new()
                    .fill(SURFACE)
                    .stroke(Stroke::new(0.9, accent.gamma_multiply(0.38)))
                    .corner_radius(6)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_min_width(198.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(metric.label).small().strong().color(TEXT));
                            badge(ui, metric.provenance.badge(), accent);
                        });
                        ui.label(
                            RichText::new(&metric.value)
                                .monospace()
                                .size(16.0)
                                .strong()
                                .color(accent),
                        );
                        ui.label(RichText::new(&metric.note).small().color(MUTED));
                    });
            }
        });
    }

    fn show_awakening(
        &mut self,
        ui: &mut egui::Ui,
        readiness: &ReadinessReport,
        events: &mut TreasuryEvents,
    ) {
        // Emergency Suspension prominent safety deck.
        egui::Frame::new()
            .fill(Color32::from_rgb(28, 12, 16))
            .stroke(Stroke::new(
                1.2,
                if self.is_suspended() {
                    CRIMSON
                } else {
                    CRIMSON.gamma_multiply(0.5)
                },
            ))
            .corner_radius(6)
            .inner_margin(10)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("EMERGENCY TRADING SUSPENSION")
                            .monospace()
                            .strong()
                            .color(CRIMSON),
                    );
                    if self.is_suspended() {
                        badge(ui, "SUSPENSION LATCHED", CRIMSON);
                        if ui.button("Clear Suspension -> Return to DORMANT").clicked() {
                            self.clear_emergency_suspension();
                        }
                    } else if ui
                        .button(
                            RichText::new("ACTIVATE EMERGENCY SUSPEND")
                                .strong()
                                .color(CRIMSON),
                        )
                        .clicked()
                    {
                        self.trigger_emergency_suspension("Operator emergency suspension");
                        events.emergency_suspended = true;
                    }
                });
                ui.label(
                    RichText::new(
                        "Blocks all new trading activity immediately. Protective position management remains available where appropriate. The trading engine operates independently and does not require the graphical interface to remain responsive.",
                    )
                    .small()
                    .color(MUTED),
                );
            });
        ui.add_space(8.0);

        // Six operational modes selector.
        ui.label(
            RichText::new("AWAKENING THE BEAST — OPERATIONAL MODE")
                .monospace()
                .size(11.5)
                .strong()
                .color(AMETHYST),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            for mode in OperationalMode::ALL {
                let selected = self.mode == mode;
                let color = mode_color(mode);
                let locked = mode == OperationalMode::Awakened && !readiness.can_awaken;
                let resp = egui::Frame::new()
                    .fill(if selected {
                        Color32::from_rgb(24, 20, 42)
                    } else {
                        SURFACE
                    })
                    .stroke(Stroke::new(
                        if selected { 1.4 } else { 0.8 },
                        if selected {
                            color
                        } else {
                            color.gamma_multiply(0.35)
                        },
                    ))
                    .corner_radius(6)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.set_min_width(175.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(mode.label()).monospace().strong().color(color));
                            if locked {
                                badge(ui, "GATED", GOLD);
                            }
                        });
                        ui.label(RichText::new(mode.summary()).small().color(MUTED));
                    });
                let click = ui.interact(
                    resp.response.rect,
                    ui.id().with(("op-mode", mode.label())),
                    Sense::click(),
                );
                if click.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if click.clicked() {
                    if mode == OperationalMode::Suspended {
                        self.trigger_emergency_suspension("Operator selected SUSPENDED mode");
                        events.emergency_suspended = true;
                    } else if self.is_suspended() {
                        self.mode_notice = Some(
                            "Clear Emergency Suspension explicitly before leaving SUSPENDED mode."
                                .into(),
                        );
                    } else if mode == OperationalMode::Awakened && !readiness.can_awaken {
                        self.mode_notice = Some(
                            "AWAKENED refused: all Treasury, risk, and unattended certification gates must pass before live operation can be authorized.".into(),
                        );
                    } else {
                        self.mode = mode;
                        self.mode_notice = Some(format!(
                            "Operational posture set to {} (read-only cockpit; headless engine live authorization remains independently gated).",
                            mode.label()
                        ));
                    }
                }
            }
        });
        if let Some(notice) = &self.mode_notice {
            ui.add_space(4.0);
            ui.colored_label(GOLD, notice);
        }

        ui.add_space(8.0);
        readiness_gate_panel(ui, readiness);
        ui.add_space(8.0);

        // Autonomous Leviathan Architecture Readiness Table.
        ui.label(
            RichText::new("AUTONOMOUS LEVIATHAN ARCHITECTURE MATRIX")
                .monospace()
                .size(11.5)
                .strong()
                .color(CYAN),
        );
        ui.label(
            RichText::new(
                "Engine autonomy is decoupled from GUI responsiveness. Unattended operation remains disabled until all pending settlement, recovery, IPC, and watchdog gates pass.",
            )
            .small()
            .color(MUTED),
        );
        ui.add_space(4.0);
        egui::Grid::new("autonomous-capabilities-grid")
            .striped(true)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.strong("Capability");
                ui.strong("Owner");
                ui.strong("Status");
                ui.strong("Current Engineering Reality");
                ui.end_row();
                for cap in AUTONOMOUS_CAPABILITIES {
                    ui.label(cap.name);
                    ui.monospace(cap.head);
                    let col = match cap.stage {
                        CapabilityStage::Implemented => EMERALD,
                        CapabilityStage::Partial => GOLD,
                        CapabilityStage::Pending => MUTED,
                    };
                    ui.label(
                        RichText::new(cap.stage.label())
                            .monospace()
                            .small()
                            .color(col),
                    );
                    ui.label(RichText::new(cap.detail).small().color(MUTED));
                    ui.end_row();
                }
            });
    }

    fn start_hyperliquid_verification(&mut self, profile: &TreasuryProfile, ctx: &egui::Context) {
        let wallet = profile.dedicated_wallet_address.clone();
        let signer = profile.derived_signer_address.clone();
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        self.verify_rx = Some(rx);
        self.error_message = None;
        self.status_message = None;
        std::thread::Builder::new()
            .name("hydra-treasury-verify".into())
            .spawn(move || {
                let res = treasury::verify_hyperliquid_account(&wallet, &signer)
                    .map_err(|e| telemetry::redact_diagnostic(&format!("{e:#}")));
                let _ = tx.send(res);
                ctx.request_repaint();
            })
            .ok();
    }
}

pub fn mode_color(mode: OperationalMode) -> Color32 {
    match mode {
        OperationalMode::Dormant => Color32::from_rgb(154, 162, 192),
        OperationalMode::Observation => CYAN,
        OperationalMode::Simulation => Color32::from_rgb(208, 194, 248),
        OperationalMode::Awakened => EMERALD,
        OperationalMode::Guardian => GOLD,
        OperationalMode::Suspended => CRIMSON,
    }
}

fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(Color32::from_rgb(14, 16, 28))
        .stroke(Stroke::new(0.8, color.gamma_multiply(0.5)))
        .corner_radius(3)
        .inner_margin(egui::Margin::symmetric(5, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).monospace().size(9.0).color(color));
        });
}

fn offering_acknowledgment_box(ui: &mut egui::Ui, checked: &mut bool) {
    egui::Frame::new()
        .fill(DEEP)
        .stroke(Stroke::new(1.0, AMETHYST.gamma_multiply(0.55)))
        .corner_radius(5)
        .inner_margin(9)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(format!("\"{OFFERING_ACKNOWLEDGMENT}\""))
                    .italics()
                    .color(Color32::from_rgb(230, 218, 248)),
            );
            ui.add_space(4.0);
            ui.checkbox(
                checked,
                "I acknowledge this risk disclosure and confirm this is a dedicated wallet funded only with capital I am prepared to lose.",
            );
        });
}

fn readiness_gate_panel(ui: &mut egui::Ui, readiness: &ReadinessReport) {
    egui::Frame::new()
        .fill(DEEP)
        .stroke(Stroke::new(0.9, Color32::from_rgb(48, 40, 78)))
        .corner_radius(6)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("AUTONOMOUS READINESS GATES")
                        .monospace()
                        .size(11.0)
                        .strong()
                        .color(AMETHYST),
                );
                badge(
                    ui,
                    if readiness.can_awaken {
                        "READY"
                    } else {
                        "AWAKENED LOCKED"
                    },
                    if readiness.can_awaken { EMERALD } else { GOLD },
                );
            });
            ui.add_space(4.0);
            for check in &readiness.checks {
                let col = match check.state {
                    GateState::Passed => EMERALD,
                    GateState::Missing => GOLD,
                    GateState::Blocked => CRIMSON,
                    GateState::PendingCertification => AMETHYST,
                };
                ui.horizontal_wrapped(|ui| {
                    badge(ui, check.state.label(), col);
                    ui.label(RichText::new(check.name).strong().small().color(TEXT));
                    ui.label(
                        RichText::new(format!("— {}", check.detail))
                            .small()
                            .color(MUTED),
                    );
                });
            }
        });
}

/// Ceremonial custom-painted Offering Altar showing Treasury -> Amethyst Heart -> AEGIS & NEXUS.
fn offering_altar_banner(
    ui: &mut egui::Ui,
    validated: bool,
    verified: bool,
    suspended: bool,
    time: f32,
    motion: bool,
) {
    let width = ui.available_width().max(320.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 96.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 7.0, Color32::from_rgb(8, 10, 21));
    p.rect_stroke(
        rect,
        7.0,
        Stroke::new(
            1.0,
            if suspended {
                CRIMSON.gamma_multiply(0.65)
            } else if verified {
                EMERALD.gamma_multiply(0.65)
            } else if validated {
                AMETHYST.gamma_multiply(0.75)
            } else {
                Color32::from_rgb(46, 38, 72)
            },
        ),
        egui::StrokeKind::Inside,
    );

    let cy = rect.center().y;
    let left_x = rect.left() + (rect.width() * 0.16).clamp(64.0, 170.0);
    let heart_x = rect.center().x;
    let right_x = rect.right() - (rect.width() * 0.16).clamp(64.0, 170.0);
    let treasury_pt = Pos2::new(left_x, cy);
    let heart_pt = Pos2::new(heart_x, cy);
    let aegis_pt = Pos2::new(right_x, cy - 22.0);
    let nexus_pt = Pos2::new(right_x, cy + 22.0);

    let active_flow = (validated || verified) && !suspended;
    let t = if motion && time.is_finite() {
        time.rem_euclid(3600.0)
    } else {
        0.0
    };
    let pulse = if motion {
        (t * 2.2).sin() * 0.5 + 0.5
    } else {
        0.5
    };

    // Conduit paths: Treasury -> Amethyst Heart -> AEGIS & NEXUS.
    let line_alpha = if active_flow { 185 } else { 52 };
    p.line_segment(
        [treasury_pt, heart_pt],
        Stroke::new(
            if active_flow { 2.2 } else { 1.1 },
            Color32::from_rgba_unmultiplied(184, 122, 255, line_alpha),
        ),
    );
    p.line_segment(
        [heart_pt, aegis_pt],
        Stroke::new(
            if active_flow { 2.0 } else { 1.0 },
            Color32::from_rgba_unmultiplied(58, 232, 165, line_alpha),
        ),
    );
    p.line_segment(
        [heart_pt, nexus_pt],
        Stroke::new(
            if active_flow { 2.0 } else { 1.0 },
            Color32::from_rgba_unmultiplied(73, 151, 255, line_alpha),
        ),
    );

    // Animated light packets ONLY when genuine wallet validation / venue verification succeeded.
    if active_flow && motion {
        let u = (t * 0.55).fract();
        let pkt_in = treasury_pt.lerp(heart_pt, u);
        p.circle_filled(pkt_in, 4.0, Color32::from_rgb(236, 206, 255));
        let u2 = (t * 0.55 + 0.5).fract();
        p.circle_filled(heart_pt.lerp(aegis_pt, u2), 3.4, EMERALD);
        p.circle_filled(heart_pt.lerp(nexus_pt, u2), 3.4, NEXUS_BLUE);
    }

    // Center Amethyst Heart crystal sigil.
    let glow_r = if active_flow {
        30.0 + pulse * 8.0
    } else {
        18.0
    };
    p.circle_filled(
        heart_pt,
        glow_r,
        Color32::from_rgba_unmultiplied(176, 108, 250, if active_flow { 48 } else { 18 }),
    );
    p.add(Shape::convex_polygon(
        vec![
            heart_pt + Vec2::new(0.0, -21.0),
            heart_pt + Vec2::new(16.0, 0.0),
            heart_pt + Vec2::new(0.0, 21.0),
            heart_pt + Vec2::new(-16.0, 0.0),
        ],
        if active_flow {
            Color32::from_rgb(142, 82, 224)
        } else {
            Color32::from_rgb(44, 28, 72)
        },
        Stroke::new(1.3, Color32::from_rgb(220, 192, 255)),
    ));
    p.circle_filled(
        heart_pt,
        if active_flow { 4.2 } else { 2.2 },
        Color32::from_rgb(248, 238, 255),
    );

    // Treasury Vault node (left) & AEGIS/NEXUS nodes (right).
    p.circle_filled(
        treasury_pt,
        10.0,
        if active_flow {
            Color32::from_rgb(64, 42, 92)
        } else {
            Color32::from_rgb(24, 22, 38)
        },
    );
    p.circle_stroke(
        treasury_pt,
        10.0,
        Stroke::new(1.3, if active_flow { GOLD } else { MUTED }),
    );
    p.text(
        treasury_pt + Vec2::new(0.0, 24.0),
        Align2::CENTER_CENTER,
        "TREASURY VAULT",
        FontId::monospace(9.5),
        if active_flow { GOLD } else { MUTED },
    );

    p.circle_filled(aegis_pt, 6.5, Color32::from_rgb(16, 46, 38));
    p.circle_stroke(aegis_pt, 6.5, Stroke::new(1.2, EMERALD));
    p.text(
        aegis_pt + Vec2::new(14.0, 0.0),
        Align2::LEFT_CENTER,
        "AEGIS · RISK",
        FontId::monospace(9.5),
        EMERALD,
    );

    p.circle_filled(nexus_pt, 6.5, Color32::from_rgb(16, 34, 62));
    p.circle_stroke(nexus_pt, 6.5, Stroke::new(1.2, NEXUS_BLUE));
    p.text(
        nexus_pt + Vec2::new(14.0, 0.0),
        Align2::LEFT_CENTER,
        "NEXUS · SIGNER",
        FontId::monospace(9.5),
        NEXUS_BLUE,
    );

    let title = if suspended {
        "EMERGENCY SUSPENSION ACTIVE"
    } else if verified {
        "THE OFFERING · VENUE VERIFIED"
    } else if validated {
        "THE OFFERING · DEDICATED WALLET SEALED"
    } else {
        "THE OFFERING · AWAITING DEDICATED WALLET"
    };
    p.text(
        Pos2::new(heart_x, rect.top() + 14.0),
        Align2::CENTER_CENTER,
        title,
        FontId::monospace(10.0),
        if suspended {
            CRIMSON
        } else if verified {
            EMERALD
        } else if validated {
            Color32::from_rgb(225, 198, 255)
        } else {
            MUTED
        },
    );
}
