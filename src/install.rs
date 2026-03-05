use anyhow::Result;
use dirs::home_dir;
use serde_json::{json, Value};

pub fn build_hook_entry(binary_path: &str) -> Value {
    json!({
        "hooks": [{
            "type": "command",
            "command": format!("{} summarize", binary_path)
        }]
    })
}

pub fn run() -> Result<()> {
    println!("install stub");
    Ok(())
}
