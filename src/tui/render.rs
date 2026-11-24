//! Ratatui renderer. Pure function of `DashState`; no engine touch-points.

use chrono::{DateTime, Utc};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Gauge, Paragraph, Row, Sparkline, Table, Wrap,
};
use ratatui::Frame;

use super::state::{DashState, FeedLine, FillRow, PositionRow, SignalRow};

// --- palette (HYDRA "Terminal Green + Magenta" blend from the banner) ---
const C_CYAN: Color = Color::Rgb(0, 229, 255);
const C_MAGENTA: Color = Color::Rgb(217, 70, 239);
const C_PINK: Color = Color::Rgb(236, 72, 153);
const C_GREEN: Color = Color::Rgb(74, 222, 128);
const C_RED: Color = Color::Rgb(248, 113, 113);
const C_YELLOW: Color = Color::Rgb(250, 204, 21);
const C_DIM: Color = Color::Rgb(100, 116, 139);
const C_TEXT: Color = Color::Rgb(226, 232, 240);
const C_MUTED: Color = Color::Rgb(148, 163, 184);

pub fn render(f: &mut Frame<'_>, dash: &DashState) {
    let area = f.area();

    // Top-level vertical split
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(7), // equity / pnl / risk
            Constraint::Length(9), // markets / positions
            Constraint::Length(9), // signals / fills
            Constraint::Min(5),    // feed (flex)
            Constraint::Length(6), // errors
        ])
        .split(area);

    render_header(f, root[0], dash);
    render_kpis(f, root[1], dash);
    render_markets_and_positions(f, root[2], dash);
    render_signals_and_fills(f, root[3], dash);
    render_feed(f, root[4], dash);
    render_errors(f, root[5], dash);
}

fn frame_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_DIM))
        .title(Span::styled(
            format!(" {} ", title.to_uppercase()),
            Style::default().fg(C_CYAN).add_modifier(Modifier::BOLD),
        ))
}

fn render_header(f: &mut Frame<'_>, area: Rect, d: &DashState) {
    let uptime = d
        .started_at
        .map(|s| {
            let secs = (Utc::now() - s).num_seconds().max(0);
            format!(
                "{:02}:{:02}:{:02}",
                secs / 3600,
                (secs / 60) % 60,
                secs % 60
            )
        })
        .unwrap_or_else(|| "--:--:--".into());

    let live = if d.live_trading {
        Span::styled(
            " LIVE ",
            Style::default()
                .bg(C_RED)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " DRY  ",
            Style::default()
                .bg(C_YELLOW)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
    };

    // Heartbeat indicator: dot dims when more than 6s since last beat.
    let beat_fresh = d
        .last_heartbeat
        .map(|t| (Utc::now() - t).num_milliseconds() < 6_000)
        .unwrap_or(false);
    let heartbeat = if beat_fresh {
        Span::styled("● beat", Style::default().fg(C_GREEN))
    } else {
        Span::styled("● beat", Style::default().fg(C_DIM))
    };

    // v1.5.0 QoL: braille spinner driven by wall-clock so it animates at full
    // 4fps refresh, not just on engine ticks (which fire every 1.5s).
    let spinner = spinner_glyph();

    let venues = if d.venues.is_empty() {
        "—".into()
    } else {
        d.venues.join(",")
    };
    let symbols = if d.symbols.is_empty() {
        "—".into()
    } else {
        d.symbols.join(",")
    };

    let line1 = Line::from(vec![
        Span::styled(
            "HYDRA ",
            Style::default().fg(C_MAGENTA).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("v{}", d.version), Style::default().fg(C_PINK)),
        Span::styled("  │  ", Style::default().fg(C_DIM)),
        Span::styled(
            format!("{} ", spinner),
            Style::default().fg(C_CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("session #{}", d.session_id),
            Style::default().fg(C_CYAN),
        ),
        Span::styled("  │  ", Style::default().fg(C_DIM)),
        Span::styled(format!("uptime {}", uptime), Style::default().fg(C_TEXT)),
        Span::styled("  │  ", Style::default().fg(C_DIM)),
        live,
        Span::raw("  "),
        heartbeat,
        Span::styled(format!("  tick {}", d.tick), Style::default().fg(C_MUTED)),
    ]);

    let line2 = Line::from(vec![
        Span::styled("venues: ", Style::default().fg(C_DIM)),
        Span::styled(venues, Style::default().fg(C_TEXT)),
        Span::styled("   symbols: ", Style::default().fg(C_DIM)),
        Span::styled(symbols, Style::default().fg(C_TEXT)),
        Span::styled(
            "   read-only view · Ctrl+C to stop",
            Style::default().fg(C_DIM).add_modifier(Modifier::ITALIC),
        ),
    ]);

    let p = Paragraph::new(vec![line1, line2]).block(frame_block("live"));
    f.render_widget(p, area);
}

/// Returns one of 8 braille glyphs based on the current 125ms slice of the
/// wall clock — gives a smooth ~8-frames-per-second animation regardless of
/// how often the engine ticks. Pure function, deterministic given time, so
/// adjacent renders are stable. Cheap (one syscall + modulo).
fn spinner_glyph() -> &'static str {
    const FRAMES: [&str; 8] = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    FRAMES[(now / 125 % FRAMES.len() as u128) as usize]
}

fn render_kpis(f: &mut Frame<'_>, area: Rect, d: &DashState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    // ─── Equity
    // v1.5.0 QoL: compute 5-min equity delta from history. Brings the
    // panel to life — at a glance the user sees if the bot is bleeding,
    // earning, or sideways without watching the spark.
    let eq_delta_5m: Option<(f64, f64)> = {
        let cutoff = Utc::now() - chrono::Duration::minutes(5);
        d.equity_history
            .iter()
            .find(|(t, _)| *t >= cutoff)
            .map(|(_, v0)| {
                let cur = d.equity_usd;
                let abs = cur - v0;
                let pct = if *v0 > 0.0 { abs / v0 * 100.0 } else { 0.0 };
                (abs, pct)
            })
    };
    let mut eq_spans: Vec<Span> = vec![Span::styled(
        format!("${:>10.2}", d.equity_usd),
        Style::default().fg(C_GREEN).add_modifier(Modifier::BOLD),
    )];
    if let Some((abs, pct)) = eq_delta_5m {
        let sign = if abs >= 0.0 { "+" } else { "-" };
        let color = if abs >= 0.0 { C_GREEN } else { C_RED };
        eq_spans.push(Span::styled(
            format!(
                "  {}${:.2} ({}{:.2}%)  5m",
                sign,
                abs.abs(),
                sign,
                pct.abs()
            ),
            Style::default().fg(color),
        ));
    } else {
        eq_spans.push(Span::styled(
            "  · waiting for history",
            Style::default().fg(C_DIM).add_modifier(Modifier::ITALIC),
        ));
    }
    let eq_line = Line::from(eq_spans);
    let spark_data: Vec<u64> = {
        let points: Vec<f64> = d.equity_history.iter().map(|(_, v)| *v).collect();
        scale_for_sparkline(&points)
    };
    let spark = Sparkline::default()
        .data(&spark_data)
        .style(Style::default().fg(C_CYAN))
        .bar_set(symbols::bar::NINE_LEVELS);
    // draw equity number + spark stacked
    let eq_block = frame_block("equity");
    let inner = eq_block.inner(cols[0]);
    f.render_widget(eq_block, cols[0]);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);
    f.render_widget(Paragraph::new(eq_line), rows[0]);
    f.render_widget(spark, rows[1]);

    // ─── Session P&L
    let pnl_color = if d.session_realized_usd >= 0.0 {
        C_GREEN
    } else {
        C_RED
    };
    let pnl_sign = if d.session_realized_usd >= 0.0 {
        "+"
    } else {
        "-"
    };
    let daily_color = if d.risk.daily_realized_usd >= 0.0 {
        C_GREEN
    } else {
        C_RED
    };
    let weekly_color = if d.risk.weekly_realized_usd >= 0.0 {
        C_GREEN
    } else {
        C_RED
    };
    let pnl_lines = vec![
        Line::from(vec![
            Span::styled("session  ", Style::default().fg(C_DIM)),
            Span::styled(
                format!("{}${:.2}", pnl_sign, d.session_realized_usd.abs()),
                Style::default().fg(pnl_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("   trades {}", d.session_trades),
                Style::default().fg(C_MUTED),
            ),
        ]),
        Line::from(vec![
            Span::styled("daily    ", Style::default().fg(C_DIM)),
            Span::styled(
                signed_usd(d.risk.daily_realized_usd),
                Style::default().fg(daily_color),
            ),
        ]),
        Line::from(vec![
            Span::styled("weekly   ", Style::default().fg(C_DIM)),
            Span::styled(
                signed_usd(d.risk.weekly_realized_usd),
                Style::default().fg(weekly_color),
            ),
        ]),
        Line::from(vec![
            Span::styled("risk mult", Style::default().fg(C_DIM)),
            Span::raw(" "),
            Span::styled(
                format!("{:.2}×", d.risk.risk_mult),
                Style::default().fg(C_YELLOW).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  base {:.2}%", d.risk.default_risk_pct),
                Style::default().fg(C_MUTED),
            ),
        ]),
    ];
    f.render_widget(Paragraph::new(pnl_lines).block(frame_block("pnl")), cols[1]);

    // ─── Risk dashboard
    let exposure_pct = {
        let eq = d.equity_usd.max(1.0);
        d.risk.open_exposure_usd / eq * 100.0
    };
    let consec_pct = if d.risk.max_consecutive_losses > 0 {
        (d.risk.consec_losses as f64 / d.risk.max_consecutive_losses as f64 * 100.0).min(100.0)
    } else {
        0.0
    };

    let risk_block = frame_block("risk");
    let inner = risk_block.inner(cols[2]);
    f.render_widget(risk_block, cols[2]);
    let sub = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1); 5])
        .split(inner);

    // Exposure gauge
    let exp_clean = clean_pct(exposure_pct);
    let exp_gauge = Gauge::default()
        .gauge_style(gauge_color(exp_clean, d.risk.max_open_exposure_pct))
        .ratio((exp_clean / d.risk.max_open_exposure_pct.max(0.01)).clamp(0.0, 1.0))
        .label(format!(
            "exposure {:.0}% / {:.0}% cap",
            exp_clean, d.risk.max_open_exposure_pct
        ));
    f.render_widget(exp_gauge, sub[0]);

    // Consec losses gauge
    let cl_gauge = Gauge::default()
        .gauge_style(gauge_color(consec_pct, 100.0))
        .ratio((consec_pct / 100.0).clamp(0.0, 1.0))
        .label(format!(
            "consec loss {}/{}",
            d.risk.consec_losses, d.risk.max_consecutive_losses
        ));
    f.render_widget(cl_gauge, sub[1]);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("daily cap  ", Style::default().fg(C_DIM)),
            Span::styled(
                format!("{:.1}%", d.risk.max_daily_loss_pct),
                Style::default().fg(C_YELLOW),
            ),
            Span::styled("   weekly cap  ", Style::default().fg(C_DIM)),
            Span::styled(
                format!("{:.1}%", d.risk.max_weekly_loss_pct),
                Style::default().fg(C_YELLOW),
            ),
        ])),
        sub[2],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("per-mkt cap ", Style::default().fg(C_DIM)),
            Span::styled(
                format!("${:.0}", d.risk.max_notional_per_market_usd),
                Style::default().fg(C_YELLOW),
            ),
        ])),
        sub[3],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("open exposure ", Style::default().fg(C_DIM)),
            Span::styled(
                if d.risk.open_exposure_usd.abs() < 0.005 {
                    "$0.00".into()
                } else {
                    format!("${:.2}", d.risk.open_exposure_usd)
                },
                Style::default().fg(C_TEXT),
            ),
        ])),
        sub[4],
    );
}

fn render_markets_and_positions(f: &mut Frame<'_>, area: Rect, d: &DashState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    // Markets table
    let header = Row::new(vec![
        "sym", "mid", "spread", "atr%", "regime", "tf", "mult", "trend",
    ])
    .style(Style::default().fg(C_MUTED).add_modifier(Modifier::BOLD));
    let mut rows: Vec<Row> = Vec::new();
    for m in &d.markets {
        let spread_txt = m
            .spread_bps
            .map(|s| format!("{:.1}bp", s))
            .unwrap_or_else(|| "—".into());
        let regime_style = regime_style(&m.regime);
        let trend_line = animated_sparkline(&m.price_history);
        let tf = m.regime_view.clone().unwrap_or_else(|| "—".into());
        let (mult_txt, mult_style) = match m.adaptive_mult {
            Some((mult, samples)) if samples > 0 => {
                let style = if (mult - 1.0).abs() < 0.01 {
                    Style::default().fg(C_MUTED)
                } else if mult > 1.0 {
                    Style::default().fg(C_GREEN)
                } else {
                    Style::default().fg(C_YELLOW)
                };
                (format!("×{:.2}", mult), style)
            }
            _ => ("—".into(), Style::default().fg(C_DIM)),
        };
        rows.push(Row::new(vec![
            Cell::from(m.symbol.clone())
                .style(Style::default().fg(C_TEXT).add_modifier(Modifier::BOLD)),
            Cell::from(format_price(m.mid)).style(Style::default().fg(C_TEXT)),
            Cell::from(spread_txt).style(Style::default().fg(C_MUTED)),
            Cell::from(format!("{:.2}%", m.atr_pct)).style(Style::default().fg(C_MUTED)),
            Cell::from(m.regime.clone()).style(regime_style),
            Cell::from(tf).style(Style::default().fg(C_DIM)),
            Cell::from(mult_txt).style(mult_style),
            Cell::from(trend_line),
        ]));
    }
    if rows.is_empty() {
        rows.push(Row::new(vec![Cell::from("waiting for market data…")
            .style(
                Style::default().fg(C_DIM).add_modifier(Modifier::ITALIC),
            )]));
    }
    let widths = [
        Constraint::Length(6),
        Constraint::Length(11),
        Constraint::Length(8),
        Constraint::Length(6),
        Constraint::Length(8),
        Constraint::Length(9),
        Constraint::Length(6),
        Constraint::Min(14),
    ];
    let mkt_table = Table::new(rows, widths)
        .header(header)
        .block(frame_block("markets"));
    f.render_widget(mkt_table, cols[0]);

    // Positions table
    render_positions(f, cols[1], &d.positions);
}

fn render_positions(f: &mut Frame<'_>, area: Rect, positions: &[PositionRow]) {
    let header = Row::new(vec![
        "sym", "side", "size", "entry", "mark", "upnl", "age", "status", "strategy",
    ])
    .style(Style::default().fg(C_MUTED).add_modifier(Modifier::BOLD));
    let mut rows: Vec<Row> = Vec::new();
    for p in positions {
        let side_style = if p.side == "LONG" {
            Style::default().fg(C_GREEN).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(C_RED).add_modifier(Modifier::BOLD)
        };
        let pnl_style = if p.unrealized_pnl_usd >= 0.0 {
            Style::default().fg(C_GREEN)
        } else {
            Style::default().fg(C_RED)
        };
        let age_secs = (Utc::now() - p.opened_at).num_seconds().max(0);
        let age_str = if age_secs < 60 {
            format!("{}s", age_secs)
        } else if age_secs < 3600 {
            format!("{}m", age_secs / 60)
        } else {
            format!("{}h{:02}m", age_secs / 3600, (age_secs / 60) % 60)
        };
        // ── v1.4.2 quality-of-life badges ─────────────────────────────────
        // Computed from data already in PositionRow; no engine changes.
        // SHIELD = chandelier defended this position to break-even or better.
        // CLOCK  = position age past 75% of the time-stop ceiling (~135min of 180).
        // FIRE   = winner running with > $0.50 unrealized AND stop above entry.
        let (status_glyphs, status_style) = position_status(p, age_secs);
        rows.push(Row::new(vec![
            Cell::from(p.symbol.clone())
                .style(Style::default().fg(C_TEXT).add_modifier(Modifier::BOLD)),
            Cell::from(p.side.clone()).style(side_style),
            Cell::from(format!("{:.4}", p.size)).style(Style::default().fg(C_TEXT)),
            Cell::from(format_price(p.entry_price)).style(Style::default().fg(C_MUTED)),
            Cell::from(format_price(p.mark_price)).style(Style::default().fg(C_TEXT)),
            Cell::from(signed_usd(p.unrealized_pnl_usd)).style(pnl_style),
            Cell::from(age_str).style(Style::default().fg(C_MUTED)),
            Cell::from(status_glyphs).style(status_style),
            Cell::from(p.strategy.clone().unwrap_or_default()).style(Style::default().fg(C_DIM)),
        ]));
    }
    if rows.is_empty() {
        rows.push(Row::new(vec![Cell::from("no open positions").style(
            Style::default().fg(C_DIM).add_modifier(Modifier::ITALIC),
        )]));
    }
    let widths = [
        Constraint::Length(6),
        Constraint::Length(5),
        Constraint::Length(8),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(10),
        Constraint::Length(7),
        Constraint::Length(8),
        Constraint::Min(10),
    ];
    let t = Table::new(rows, widths)
        .header(header)
        .block(frame_block("open positions"));
    f.render_widget(t, area);
}

/// Pure helper: returns the badge glyphs + a style hint for the given position.
/// Uses ASCII-friendly indicators (no emoji) so it renders identically across
/// every terminal font (PowerShell, Alacritty, iTerm2, tmux, etc.).
fn position_status(p: &PositionRow, age_secs: i64) -> (String, Style) {
    let mut parts: Vec<&str> = Vec::new();
    let is_long = p.side == "LONG";

    // BE+: chandelier has pulled the stop to entry or beyond.
    let be_locked = match p.stop_price {
        Some(s) if is_long => s >= p.entry_price,
        Some(s) => s <= p.entry_price,
        None => false,
    };
    if be_locked {
        parts.push("BE+");
    }

    // HOT: meaningful unrealized profit AND stop is defending it.
    if be_locked && p.unrealized_pnl_usd > 0.50 {
        parts.push("HOT");
    }

    // OLD: position age > 75% of the 180-min time-stop default.
    if age_secs > 135 * 60 {
        parts.push("OLD");
    }

    // RISK: still in initial-risk territory (no BE lock, position older than 5m).
    if !be_locked && age_secs > 300 {
        parts.push("RISK");
    }

    let style = if parts.iter().any(|s| *s == "HOT") {
        Style::default().fg(C_GREEN).add_modifier(Modifier::BOLD)
    } else if parts.iter().any(|s| *s == "BE+") {
        Style::default().fg(C_CYAN)
    } else if parts.iter().any(|s| *s == "OLD") {
        Style::default().fg(C_YELLOW)
    } else if parts.iter().any(|s| *s == "RISK") {
        Style::default().fg(C_RED)
    } else {
        Style::default().fg(C_DIM)
    };

    let txt = if parts.is_empty() {
        "—".to_string()
    } else {
        parts.join(" ")
    };
    (txt, style)
}

fn render_signals_and_fills(f: &mut Frame<'_>, area: Rect, d: &DashState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Signals
    render_signals(f, cols[0], &d.signals);
    // Fills
    render_fills(f, cols[1], &d.fills);
}

fn render_signals(f: &mut Frame<'_>, area: Rect, signals: &std::collections::VecDeque<SignalRow>) {
    let header = Row::new(vec!["time", "strategy", "sym", "side", "edge", "outcome"])
        .style(Style::default().fg(C_MUTED).add_modifier(Modifier::BOLD));
    let mut rows: Vec<Row> = Vec::new();
    for s in signals.iter().rev().take(10) {
        let side_style = if s.side == "LONG" {
            Style::default().fg(C_GREEN)
        } else if s.side == "SHORT" {
            Style::default().fg(C_RED)
        } else {
            Style::default().fg(C_DIM)
        };
        let outcome_style = if s.outcome.starts_with("block") {
            Style::default().fg(C_YELLOW)
        } else if s.outcome == "filled" {
            Style::default().fg(C_GREEN)
        } else {
            Style::default().fg(C_CYAN)
        };
        rows.push(Row::new(vec![
            Cell::from(hhmmss(s.ts)).style(Style::default().fg(C_DIM)),
            Cell::from(s.strategy.clone()).style(Style::default().fg(C_MAGENTA)),
            Cell::from(s.symbol.clone()).style(Style::default().fg(C_TEXT)),
            Cell::from(s.side.clone()).style(side_style),
            Cell::from(format!("{:.0}bp", s.edge_bps)).style(Style::default().fg(C_MUTED)),
            Cell::from(s.outcome.clone()).style(outcome_style),
        ]));
    }
    if rows.is_empty() {
        rows.push(Row::new(vec![Cell::from("no signals yet").style(
            Style::default().fg(C_DIM).add_modifier(Modifier::ITALIC),
        )]));
    }
    let widths = [
        Constraint::Length(8),
        Constraint::Length(15),
        Constraint::Length(5),
        Constraint::Length(6),
        Constraint::Length(7),
        Constraint::Min(10),
    ];
    let t = Table::new(rows, widths)
        .header(header)
        .block(frame_block("signals · last 10"));
    f.render_widget(t, area);
}

fn render_fills(f: &mut Frame<'_>, area: Rect, fills: &std::collections::VecDeque<FillRow>) {
    let header = Row::new(vec![
        "time", "sym", "side", "size", "price", "kind", "realized",
    ])
    .style(Style::default().fg(C_MUTED).add_modifier(Modifier::BOLD));
    let mut rows: Vec<Row> = Vec::new();
    for s in fills.iter().rev().take(10) {
        let side_style = if s.side == "LONG" {
            Style::default().fg(C_GREEN)
        } else {
            Style::default().fg(C_RED)
        };
        let realized_txt = s.realized_usd.map(signed_usd).unwrap_or_else(|| "—".into());
        let realized_style = match s.realized_usd {
            Some(v) if v >= 0.0 => Style::default().fg(C_GREEN),
            Some(_) => Style::default().fg(C_RED),
            None => Style::default().fg(C_DIM),
        };
        rows.push(Row::new(vec![
            Cell::from(hhmmss(s.ts)).style(Style::default().fg(C_DIM)),
            Cell::from(s.symbol.clone()).style(Style::default().fg(C_TEXT)),
            Cell::from(s.side.clone()).style(side_style),
            Cell::from(format!("{:.4}", s.size)).style(Style::default().fg(C_MUTED)),
            Cell::from(format_price(s.price)).style(Style::default().fg(C_TEXT)),
            Cell::from(s.kind.clone()).style(Style::default().fg(C_CYAN)),
            Cell::from(realized_txt).style(realized_style),
        ]));
    }
    if rows.is_empty() {
        rows.push(Row::new(vec![Cell::from("no fills yet").style(
            Style::default().fg(C_DIM).add_modifier(Modifier::ITALIC),
        )]));
    }
    let widths = [
        Constraint::Length(8),
        Constraint::Length(5),
        Constraint::Length(5),
        Constraint::Length(8),
        Constraint::Length(12),
        Constraint::Length(10),
        Constraint::Min(8),
    ];
    let t = Table::new(rows, widths)
        .header(header)
        .block(frame_block("fills · last 10"));
    f.render_widget(t, area);
}

fn render_feed(f: &mut Frame<'_>, area: Rect, d: &DashState) {
    let block = frame_block("feed");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let visible = inner.height as usize;
    let take = visible.max(1);
    let mut lines: Vec<Line> = Vec::with_capacity(take);
    let slice: Vec<&FeedLine> = d.feed.iter().rev().take(take).collect();
    for line in slice.iter().rev() {
        let lvl_style = match line.level.as_str() {
            "ERROR" => Style::default().fg(C_RED).add_modifier(Modifier::BOLD),
            "WARN" => Style::default().fg(C_YELLOW),
            "INFO" => Style::default().fg(C_CYAN),
            _ => Style::default().fg(C_DIM),
        };
        let trimmed = truncate(&line.message, inner.width.saturating_sub(22) as usize);
        lines.push(Line::from(vec![
            Span::styled(hhmmss(line.ts), Style::default().fg(C_DIM)),
            Span::raw(" "),
            Span::styled(format!("{:<5}", line.level), lvl_style),
            Span::raw(" "),
            Span::styled(trimmed, Style::default().fg(C_TEXT)),
        ]));
    }
    let p = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(p, inner);
}

fn render_errors(f: &mut Frame<'_>, area: Rect, d: &DashState) {
    let header = Row::new(vec!["time", "kind", "message"])
        .style(Style::default().fg(C_MUTED).add_modifier(Modifier::BOLD));
    let mut rows: Vec<Row> = Vec::new();
    for e in d.errors.iter().rev().take(5) {
        rows.push(Row::new(vec![
            Cell::from(hhmmss(e.ts)).style(Style::default().fg(C_DIM)),
            Cell::from(e.kind.clone()).style(Style::default().fg(C_YELLOW)),
            Cell::from(truncate(&e.message, 200)).style(Style::default().fg(C_TEXT)),
        ]));
    }
    if rows.is_empty() {
        rows.push(Row::new(vec![Cell::from("no errors").style(
            Style::default().fg(C_DIM).add_modifier(Modifier::ITALIC),
        )]));
    }
    let widths = [
        Constraint::Length(8),
        Constraint::Length(16),
        Constraint::Min(20),
    ];
    let t = Table::new(rows, widths)
        .header(header)
        .block(frame_block("errors · last 5").title_alignment(Alignment::Left));
    f.render_widget(t, area);
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn hhmmss(ts: DateTime<Utc>) -> String {
    let local = ts.with_timezone(&chrono::Local);
    local.format("%H:%M:%S").to_string()
}

fn format_price(px: f64) -> String {
    if !px.is_finite() || px == 0.0 {
        return "—".into();
    }
    if px >= 1000.0 {
        format!("{:>10.2}", px)
    } else if px >= 1.0 {
        format!("{:>10.4}", px)
    } else {
        format!("{:>10.6}", px)
    }
}

fn signed_usd(v: f64) -> String {
    // Snap tiny magnitudes to clean zero so we never render "-$0.00".
    if v.abs() < 0.005 {
        return "+$0.00".into();
    }
    if v >= 0.0 {
        format!("+${:.2}", v)
    } else {
        format!("-${:.2}", v.abs())
    }
}

fn clean_pct(v: f64) -> f64 {
    // Treat floating-point fuzz around zero as exactly zero for display.
    if v.abs() < 0.05 {
        0.0
    } else {
        v
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn regime_style(regime: &str) -> Style {
    match regime {
        "Trending" => Style::default().fg(C_GREEN).add_modifier(Modifier::BOLD),
        "VolSpike" => Style::default().fg(C_YELLOW).add_modifier(Modifier::BOLD),
        "Choppy" => Style::default().fg(C_CYAN),
        "Panic" => Style::default().fg(C_RED).add_modifier(Modifier::BOLD),
        "LowLiquidity" => Style::default().fg(C_MUTED),
        "NoTrade" => Style::default().fg(C_DIM),
        _ => Style::default().fg(C_MUTED),
    }
}

fn gauge_color(value_pct: f64, cap_pct: f64) -> Style {
    let ratio = if cap_pct > 0.0 {
        value_pct / cap_pct
    } else {
        0.0
    };
    let fg = if ratio < 0.5 {
        C_GREEN
    } else if ratio < 0.8 {
        C_YELLOW
    } else {
        C_RED
    };
    Style::default().fg(fg).bg(Color::Rgb(30, 41, 59))
}

/// Compact inline sparkline using unicode block characters, for use in cells.
/// v1.5.0 QoL: animated, color-graded sparkline that *moves* between
/// engine ticks. Three layered effects make the trend column feel alive:
///
///   1. **Direction colour** — last vs first price colours the whole spark
///      green (rising), red (falling), or cyan (flat).
///   2. **Brightness gradient** — older bars are dimmed, newer bars are
///      bright; the eye naturally follows the right edge as new data lands.
///   3. **Pulsing comet head** — the rightmost cell breathes via wall-clock
///      `sin` between full-bright and bold-bright every ~1.2s. The animation
///      runs at the TUI's full 4fps refresh, decoupled from the 1.5s engine
///      tick, so there's always something moving even on quiet markets.
///
/// Followed by a colour-matched ▲ / ▼ / ◆ glyph + signed % delta so the
/// magnitude is readable at a glance.
fn animated_sparkline(prices: &std::collections::VecDeque<f64>) -> Line<'static> {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if prices.is_empty() {
        return Line::from(Span::styled(
            "· · ·",
            Style::default().fg(C_DIM).add_modifier(Modifier::ITALIC),
        ));
    }
    let n = prices.len();
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &p in prices {
        if p < min {
            min = p;
        }
        if p > max {
            max = p;
        }
    }
    let range = (max - min).max(1e-9);
    let first = *prices.front().unwrap();
    let last = *prices.back().unwrap();
    let pct = if first > 0.0 {
        (last - first) / first * 100.0
    } else {
        0.0
    };

    // Direction palette — base colour from price action, with a brighter
    // companion shade used for the leading edge "comet head".
    let (base, bright, glyph) = if pct > 0.05 {
        (C_GREEN, Color::Rgb(134, 239, 172), "▲")
    } else if pct < -0.05 {
        (C_RED, Color::Rgb(252, 165, 165), "▼")
    } else {
        (C_CYAN, Color::Rgb(165, 243, 252), "◆")
    };

    // Comet-head pulse: 0.0 = base, 1.0 = bright. Sine over a 1200ms cycle.
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0);
    let phase = (now_ms / 1200.0) * std::f64::consts::TAU;
    let pulse = (phase.sin() * 0.5 + 0.5).clamp(0.0, 1.0);

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(n + 2);
    for (i, &p) in prices.iter().enumerate() {
        let block_idx = ((p - min) / range * 7.0).round() as usize;
        let ch = BLOCKS[block_idx.min(7)];
        let pos = i as f64 / (n.max(1) as f64); // 0.0 (oldest) → 1.0 (newest)

        // Last 2 cells = "comet head" — pulse between base and bright.
        let style = if i + 2 >= n {
            let head_color = if pulse > 0.5 { bright } else { base };
            let mut s = Style::default().fg(head_color);
            if i + 1 == n {
                s = s.add_modifier(Modifier::BOLD);
            }
            s
        } else if pos < 0.35 {
            // Older history: dim
            Style::default().fg(C_DIM)
        } else {
            // Mid-history: base colour, no modifiers
            Style::default().fg(base)
        };
        spans.push(Span::styled(ch.to_string(), style));
    }

    // Trailing direction glyph + delta %
    spans.push(Span::raw(" "));
    let glyph_style = Style::default().fg(base).add_modifier(Modifier::BOLD);
    spans.push(Span::styled(glyph, glyph_style));
    spans.push(Span::styled(
        format!("{:+.2}%", pct),
        Style::default().fg(base),
    ));

    Line::from(spans)
}

/// Scale a raw equity series to u64s in [0, 100] for ratatui's Sparkline widget.
/// When the series is flat (min == max), render a flat line at mid-height so
/// the panel isn't visually empty during long no-trade windows.
fn scale_for_sparkline(points: &[f64]) -> Vec<u64> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &v in points {
        if v.is_finite() {
            if v < min {
                min = v;
            }
            if v > max {
                max = v;
            }
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return Vec::new();
    }
    let range = max - min;
    if range.abs() < 1e-6 {
        // Flat series → render as a mid-height line.
        return vec![50u64; points.len()];
    }
    points
        .iter()
        .map(|&v| {
            if !v.is_finite() {
                return 0u64;
            }
            let scaled = ((v - min) / range * 100.0).clamp(0.0, 100.0);
            scaled.round() as u64
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_pos(side: &str, entry: f64, stop: Option<f64>, mark: f64, upnl: f64) -> PositionRow {
        PositionRow {
            venue: "hyperliquid".into(),
            symbol: "ETH".into(),
            side: side.into(),
            size: 0.04,
            entry_price: entry,
            mark_price: mark,
            unrealized_pnl_usd: upnl,
            stop_price: stop,
            take_profit: None,
            opened_at: Utc::now(),
            strategy: None,
        }
    }

    #[test]
    fn fresh_position_has_no_badge() {
        let p = make_pos("LONG", 100.0, Some(98.0), 100.5, 0.02);
        let (txt, _) = position_status(&p, 30);
        assert_eq!(txt, "—");
    }

    #[test]
    fn long_with_stop_above_entry_shows_be_plus() {
        let p = make_pos("LONG", 100.0, Some(101.0), 102.0, 0.08);
        let (txt, _) = position_status(&p, 60);
        assert!(txt.contains("BE+"));
    }

    #[test]
    fn short_with_stop_below_entry_shows_be_plus() {
        let p = make_pos("SHORT", 100.0, Some(99.0), 98.0, 0.08);
        let (txt, _) = position_status(&p, 60);
        assert!(txt.contains("BE+"));
    }

    #[test]
    fn winner_with_protection_shows_hot_and_be_plus() {
        // BE locked AND >$0.50 unrealized → both badges.
        let p = make_pos("LONG", 100.0, Some(101.0), 105.0, 0.75);
        let (txt, _) = position_status(&p, 60);
        assert!(txt.contains("BE+"), "got '{}'", txt);
        assert!(txt.contains("HOT"), "got '{}'", txt);
    }

    #[test]
    fn old_position_in_initial_risk_shows_old_and_risk() {
        // 2.5h old, stop still below entry on a long → OLD + RISK.
        let p = make_pos("LONG", 100.0, Some(98.0), 99.5, -0.02);
        let (txt, _) = position_status(&p, 9_000); // 150 minutes
        assert!(txt.contains("OLD"));
        assert!(txt.contains("RISK"));
    }

    #[test]
    fn no_stop_means_no_be_plus() {
        let p = make_pos("LONG", 100.0, None, 102.0, 0.08);
        let (txt, _) = position_status(&p, 600);
        assert!(!txt.contains("BE+"));
        assert!(txt.contains("RISK")); // age > 5min, no protection
    }
}
