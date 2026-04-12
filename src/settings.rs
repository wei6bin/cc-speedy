use anyhow::Result;
use rusqlite::Connection;

#[derive(Debug, Clone, Default)]
pub struct AppSettings {
    pub obsidian_kb_path: Option<String>,
}

/// Load all settings from DB into AppSettings.
pub fn load(conn: &Connection) -> AppSettings {
    AppSettings {
        obsidian_kb_path: crate::store::get_setting(conn, "obsidian_kb_path"),
    }
}

/// Validate that path exists and is a directory, then persist to DB.
pub fn save_obsidian_path(conn: &Connection, path: &str) -> Result<()> {
    let meta = std::fs::metadata(path)
        .map_err(|_| anyhow::anyhow!("Path does not exist: {}", path))?;
    if !meta.is_dir() {
        anyhow::bail!("Path is not a directory: {}", path);
    }
    crate::store::set_setting(conn, "obsidian_kb_path", path)?;
    Ok(())
}
