#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "Hydra read-only desktop cockpit")]
struct Options {
    #[arg(long, default_value = "data/hydra.db")]
    db: PathBuf,
}

fn main() -> eframe::Result {
    hydra::cockpit::run(Options::parse().db)
}
