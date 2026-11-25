//! Health checks + heartbeat. v1.4.3: ETH RPC checks removed.

use anyhow::Result;
use comfy_table::{presets::UTF8_BORDERS_ONLY, ContentArrangement, Table};
use owo_colors::OwoColorize;
use std::time::Instant;

use crate::adapters::make_venue;
use crate::config::AppConfig;
use crate::core::VenueKind;
use crate::storage::Storage;

pub struct HealthReport {
    pub ok: bool,
    pub checks: Vec<(String, bool, String)>,
}

impl HealthReport {
    pub async fn gather(cfg: &AppConfig, storage: &Storage) -> Self {
        let mut checks: Vec<(String, bool, String)> = Vec::new();

        // DB
        let db_ok = storage.list_sessions(1).is_ok();
        checks.push(("sqlite".into(), db_ok, cfg.db_path.display().to_string()));

        // Hyperliquid info
        let t = Instant::now();
        let hl = match make_venue(VenueKind::Hyperliquid, cfg).await {
            Ok(v) => match v.ping().await {
                Ok(_) => (true, format!("mainnet  ({}ms)", t.elapsed().as_millis())),
                Err(e) => (false, e.to_string()),
            },
            Err(e) => (false, e.to_string()),
        };
        checks.push(("hyperliquid".into(), hl.0, hl.1));

        // Key presence
        checks.push((
            "hl_key_present".into(),
            cfg.hl_private_key.is_some(),
            if cfg.hl_private_key.is_some() {
                "set".into()
            } else {
                "missing".into()
            },
        ));

        // Address derived from HL key (sanity)
        if let Some(k) = &cfg.hl_private_key {
            if let Ok(address) = crate::hyperliquid_wire::derive_address(k) {
                checks.push(("trading_address".into(), true, address));
            }
        }

        let ok = checks.iter().all(|(_, v, _)| *v);
        Self { ok, checks }
    }

    pub fn print(&self) {
        crate::utils::banner::section("core subsystems");
        let mut t = Table::new();
        t.load_preset(UTF8_BORDERS_ONLY)
            .set_content_arrangement(ContentArrangement::Dynamic);
        t.set_header(vec![
            "Check".bright_white().bold().to_string(),
            "Status".bright_white().bold().to_string(),
            "Detail".bright_white().bold().to_string(),
        ]);
        for (k, ok, d) in &self.checks {
            let s = if *ok {
                "✓ PASS".bright_green().bold().to_string()
            } else {
                "✗ FAIL".bright_red().bold().to_string()
            };
            t.add_row(vec![k.clone(), s, d.clone()]);
        }
        println!("{t}");

        println!();
        if self.ok {
            println!(
                " {}  {}",
                "●".bright_green().bold(),
                "OVERALL: HEALTHY".bright_green().bold()
            );
        } else {
            println!(
                " {}  {}",
                "●".bright_red().bold(),
                "OVERALL: DEGRADED".bright_red().bold()
            );
        }
    }
}

/// In-process heartbeat (persisted periodically).
pub struct Heartbeat {
    last_beat: std::time::Instant,
    ticks: u64,
}
impl Heartbeat {
    pub fn new() -> Self {
        Self {
            last_beat: std::time::Instant::now(),
            ticks: 0,
        }
    }
    pub fn beat(&mut self, _kind: &str) {
        self.ticks = self.ticks.wrapping_add(1);
        self.last_beat = std::time::Instant::now();
    }
    pub fn persist(&self, storage: &Storage, session_id: i64) -> Result<()> {
        storage.heartbeat(session_id, "engine_tick")
    }
}
