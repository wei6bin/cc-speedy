mod tui;

use anyhow::Result;
use cc_speedy::{summary, install};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("summarize") => summary::run_hook().await,
        Some("install")   => install::run(),
        _                 => tui::run().await,
    }
}
