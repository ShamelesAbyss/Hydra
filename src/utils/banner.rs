//! ASCII banner + boot splash.
//!
//! Prints a bright, colour-graded HYDRA mark on every start plus a mini
//! "system online" indicator strip.

use owo_colors::OwoColorize;

pub fn print_banner() {
    // Gradient palette (teal → purple → magenta). Applied row-by-row.
    let palette: [(u8, u8, u8); 7] = [
        (0, 229, 255), // bright cyan
        (0, 200, 255),
        (96, 165, 250),
        (167, 139, 250),
        (217, 70, 239),  // magenta
        (236, 72, 153),  // pink
        (248, 113, 113), // coral
    ];

    let art = [
        r"    ██╗  ██╗██╗   ██╗██████╗ ██████╗  █████╗",
        r"    ██║  ██║╚██╗ ██╔╝██╔══██╗██╔══██╗██╔══██╗",
        r"    ███████║ ╚████╔╝ ██║  ██║██████╔╝███████║",
        r"    ██╔══██║  ╚██╔╝  ██║  ██║██╔══██╗██╔══██║",
        r"    ██║  ██║   ██║   ██████╔╝██║  ██║██║  ██║",
        r"    ╚═╝  ╚═╝   ╚═╝   ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝",
        r"    ─────── live-fire trading engine ───────",
    ];

    println!();
    for (line, (r, g, b)) in art.iter().zip(palette.iter()) {
        println!("{}", line.truecolor(*r, *g, *b).bold());
    }

    println!(
        "{}",
        "         HYPERLIQUID  ·  v1.4.5"
            .truecolor(148, 163, 184)
            .italic()
    );
    println!(
        "{}",
        "    180+ perps · compounding · mainnet · single-venue lean".truecolor(100, 116, 139)
    );
    println!();
}

/// Small status bullet strip printed after the banner or before a command.
pub fn status_strip(items: &[(&str, bool)]) {
    let mut parts: Vec<String> = Vec::new();
    for (label, ok) in items {
        let dot = if *ok {
            "●".bright_green().to_string()
        } else {
            "●".bright_red().to_string()
        };
        parts.push(format!("{} {}", dot, label.bright_white()));
    }
    println!("  {}", parts.join("   "));
    println!();
}

/// Thin horizontal separator.
pub fn rule() {
    println!("{}", "─".repeat(66).truecolor(71, 85, 105));
}

/// Section heading used inside reports.
pub fn section(title: &str) {
    println!();
    println!(
        "{}  {}",
        "▸".bright_cyan().bold(),
        title.to_uppercase().bright_white().bold()
    );
    rule();
}
