use anyhow::Result;
use cc_speedy::{summary, install, tui, update};

#[tokio::main]
async fn main() -> Result<()> {
    // Strip proxy/override env vars (e.g. from openclaw or similar wrappers) so
    // every child process — `tmux`, `claude --print`, `claude`, `opencode`,
    // `copilot` — uses the user's default Claude subscription, not whatever
    // endpoint the parent shell pointed us at. Mirrors the intent of the
    // `alias claude='env -u ANTHROPIC_AUTH_TOKEN ... claude'` pattern.
    for var in ["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_BASE_URL", "ANTHROPIC_MODEL", "ANTHROPIC_API_KEY"] {
        std::env::remove_var(var);
    }

    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("--version") | Some("-V") | Some("-v") => {
            println!("cc-speedy {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("summarize") => summary::run_hook().await,
        Some("install")   => install::run(),
        Some("update")    => update::run().await,
        _                 => tui::run().await,
    }
}
