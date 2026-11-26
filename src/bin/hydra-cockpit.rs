#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "Hydra Leviathan unified native command center")]
struct Options {
    #[arg(long, default_value = "data/hydra.db")]
    db: PathBuf,
    #[arg(long,value_enum,default_value_t=hydra::desktop_renderer::Backend::Auto)]
    renderer: hydra::desktop_renderer::Backend,
    #[arg(long)]
    fullscreen: bool,
    #[arg(long,value_enum,default_value_t=hydra::leviathan_scene::SceneQuality::High)]
    quality: hydra::leviathan_scene::SceneQuality,
    #[arg(long)]
    reduced_motion: bool,
    #[arg(long,default_value_t=60,value_parser=clap::value_parser!(u32).range(15..=144))]
    frame_cap: u32,
    /// Use OS decorations instead of the integrated borderless frame.
    #[arg(long)]
    decorated: bool,
}

fn main() -> eframe::Result {
    let options = Options::parse();
    hydra::cockpit::run_configured(
        options.db,
        options.renderer,
        options.fullscreen,
        !options.decorated,
        options.quality,
        options.reduced_motion,
        options.frame_cap,
    )
}
