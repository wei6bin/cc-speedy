use anyhow::Result;
use cc_speedy::{summary, install, tui};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("--version") | Some("-V") => {
            println!("cc-speedy {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("summarize") => summary::run_hook().await,
        Some("install")   => install::run(),
        _                 => tui::run().await,
    }
}
