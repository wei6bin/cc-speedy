use anyhow::Result;
use dirs::home_dir;
use serde_json::{json, Value};
use std::path::Path;

pub fn build_hook_entry(binary_path: &str) -> Value {
    // Quote the binary path to handle spaces; "summarize" is a literal safe subcommand
    let command = format!("\"{}\" summarize", binary_path.replace('"', "\\\""));
    json!({
        "hooks": [{
            "type": "command",
            "command": command
        }]
    })
}

/// Install hook to a specific settings file path (used in tests)
pub fn install_to(settings_path: &Path, binary_path: &str) -> Result<()> {
    // Read existing settings, distinguishing "not found" from other IO errors.
    // If the file doesn't exist yet we start from an empty object; any other
    // read failure (e.g. permissions) is surfaced as an error rather than
    // silently overwriting the file.
    let content = match std::fs::read_to_string(settings_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}".to_string(),
        Err(e) => return Err(anyhow::anyhow!("cannot read {:?}: {}", settings_path, e)),
    };
    let mut settings: Value = serde_json::from_str(&content)?;

    // Build the entry first so the idempotency check uses the exact same command string
    let entry = build_hook_entry(binary_path);
    let hook_cmd = entry["hooks"][0]["command"].as_str().unwrap_or("").to_string();

    // Check if already installed — idempotent
    if let Some(existing) = settings["hooks"]["SessionEnd"].as_array() {
        for e in existing {
            if let Some(hooks) = e["hooks"].as_array() {
                for h in hooks {
                    if h["command"].as_str() == Some(hook_cmd.as_str()) {
                        println!("cc-speedy: SessionEnd hook already installed.");
                        return Ok(());
                    }
                }
            }
        }
    }

    // Append to existing SessionEnd array or create it
    if let Some(arr) = settings["hooks"]["SessionEnd"].as_array_mut() {
        arr.push(entry);
    } else {
        // Ensure hooks object exists
        if settings["hooks"].is_null() || !settings["hooks"].is_object() {
            settings["hooks"] = json!({});
        }
        settings["hooks"]["SessionEnd"] = json!([entry]);
    }

    let pretty = serde_json::to_string_pretty(&settings)?;
    std::fs::write(settings_path, pretty)?;
    println!("cc-speedy: SessionEnd hook installed in {:?}", settings_path);
    Ok(())
}

/// Install hook to the real ~/.claude/settings.json
pub fn run() -> Result<()> {
    let settings_path = home_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?
        .join(".claude")
        .join("settings.json");

    let binary = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "cc-speedy".to_string());

    install_to(&settings_path, &binary)
}
