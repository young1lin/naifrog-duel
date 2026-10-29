//! Minimal dedicated server entry point; no renderer or Bevy dependency.
use naifrog_duel::server::{Config, run};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|argument| argument == "--version") {
        println!(
            "naifrog-server {} (protocol {})",
            naifrog_duel::VERSION,
            naifrog_duel::net::PROTOCOL_VERSION
        );
        return Ok(());
    }
    run(Config::from_env()?).await?;
    Ok(())
}
