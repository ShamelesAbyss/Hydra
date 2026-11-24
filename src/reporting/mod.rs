//! Session header/footer reporting and on-disk summary.

use anyhow::Result;
use chrono::{DateTime, Utc};
use comfy_table::{presets::UTF8_BORDERS_ONLY, ContentArrangement, Table};
use owo_colors::OwoColorize;
use std::path::PathBuf;

use crate::config::AppConfig;
use crate::storage::Storage;
use crate::utils::banner;

pub struct SessionReporter;

impl SessionReporter {
    pub fn print_header(cfg: &AppConfig, storage: &Storage) -> Result<()> {
        let prev = storage.list_sessions(1)?;

        banner::section("session start");
        println!(
            " {:<14} {}",
            "time".bright_cyan(),
            Utc::now().to_rfc3339().bright_white()
        );
        println!(
            " {:<14} {}",
            "live_trading".bright_cyan(),
            if cfg.live_trading {
                " LIVE ".on_bright_red().white().bold().to_string()
                    + " real orders will be submitted"
            } else {
                " DRY ".on_yellow().black().bold().to_string() + " ENABLE_LIVE_TRADING=false"
            }
        );
        println!(
            " {:<14} {}",
            "db".bright_cyan(),
            cfg.db_path.display().to_string().dimmed()
        );
        println!(
            " {:<14} {:?}",
            "strategies".bright_cyan(),
            cfg.strategies.global.enabled_strategies
        );

        if let Some(p) = prev.first() {
            println!(
                " {:<14} #{} ({} → {})",
                "prior".bright_cyan(),
                p.id,
                p.started_at.to_rfc3339().dimmed(),
                p.ended_at
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_else(|| "LIVE".bright_green().to_string())
            );
        }
        let open = storage.list_open_positions()?;
        if !open.is_empty() {
            println!(
                " {:<14} {} positions",
                "recovered".bright_cyan(),
                open.len().to_string().bright_yellow().bold()
            );
            for p in &open {
                let side = if p.side == "LONG" {
                    p.side.bright_green().to_string()
                } else {
                    p.side.bright_red().to_string()
                };
                println!(
                    "   {} {:<12} {:<6} {:<5} size={:.6} entry={:.4}",
                    "└─".dimmed(),
                    p.venue.bright_cyan(),
                    p.symbol,
                    side,
                    p.size,
                    p.entry_price
                );
            }
        } else {
            println!(" {:<14} none", "open_pos".bright_cyan());
        }
        println!();
        Ok(())
    }

    pub fn print_footer(
        cfg: &AppConfig,
        storage: &Storage,
        session_id: i64,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        reason: &str,
    ) -> Result<()> {
        let runtime = (end - start).num_seconds();
        let sessions = storage.list_sessions(1)?;
        let s = sessions.iter().find(|s| s.id == session_id).cloned();
        let pos = storage.list_open_positions()?;
        let realized = s.as_ref().map(|s| s.realized_usd).unwrap_or(0.0);
        let trades = s.as_ref().map(|s| s.trades).unwrap_or(0);

        banner::section("session end");
        let mut t = Table::new();
        t.load_preset(UTF8_BORDERS_ONLY)
            .set_content_arrangement(ContentArrangement::Dynamic);
        t.set_header(vec![
            "Field".bright_white().bold().to_string(),
            "Value".bright_white().bold().to_string(),
        ]);
        let realized_str = if realized >= 0.0 {
            format!("+${:.2}", realized).bright_green().to_string()
        } else {
            format!("-${:.2}", realized.abs()).bright_red().to_string()
        };
        t.add_row(vec![
            "session_id".bright_cyan().to_string(),
            session_id.to_string(),
        ]);
        t.add_row(vec!["start".bright_cyan().to_string(), start.to_rfc3339()]);
        t.add_row(vec!["end".bright_cyan().to_string(), end.to_rfc3339()]);
        t.add_row(vec![
            "runtime_secs".bright_cyan().to_string(),
            runtime.to_string(),
        ]);
        t.add_row(vec![
            "shutdown_reason".bright_cyan().to_string(),
            reason.bright_yellow().to_string(),
        ]);
        t.add_row(vec![
            "accepted_orders".bright_cyan().to_string(),
            trades.to_string(),
        ]);
        t.add_row(vec!["realized_usd".bright_cyan().to_string(), realized_str]);
        t.add_row(vec![
            "open_positions".bright_cyan().to_string(),
            pos.len().to_string(),
        ]);
        println!("{t}");

        let out_dir: PathBuf = cfg.data_dir.join("session_reports");
        std::fs::create_dir_all(&out_dir).ok();
        let path = out_dir.join(format!("session_{session_id}.json"));
        let payload = serde_json::json!({
            "session_id": session_id,
            "start": start.to_rfc3339(),
            "end":   end.to_rfc3339(),
            "runtime_secs": runtime,
            "shutdown_reason": reason,
            "trades": trades,
            "realized_usd": realized,
            "open_positions_remaining": pos.len(),
            "config": {
                "live_trading": cfg.live_trading,
            }
        });
        std::fs::write(&path, serde_json::to_vec_pretty(&payload)?)?;
        println!(
            " {} wrote {}",
            "✔".bright_green(),
            path.display().to_string().dimmed()
        );
        Ok(())
    }
}
