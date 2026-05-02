use anyhow::Result;
use clap::Parser;
use hydra::cli::Cli;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let cli = Cli::parse();
    hydra::cli::dispatch(cli).await
}
