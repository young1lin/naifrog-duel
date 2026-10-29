//! Minimal dedicated server entry point; no renderer or Bevy dependency.
use naifrog_duel::server::{Config, run};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run(Config::from_env()?).await?;
    Ok(())
}
