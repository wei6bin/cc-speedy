use anyhow::Result;
use rusqlite::Connection;

#[derive(Debug, Clone)]
pub struct AppSettings {
    pub obsidian_kb_path: Option<String>,
    pub obsidian_vault_name: Option<String>,
    pub obsidian_daily_push: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            obsidian_kb_path: None,
            obsidian_vault_name: None,
            obsidian_daily_push: true,
        }
    }
}

impl AppSettings {
    /// Vault name to use for CLI calls. Returns the configured value if non-empty,
    /// otherwise the basename of `obsidian_kb_path`, otherwise `None`.
    pub fn effective_vault_name(&self) -> Option<String> {
        if let Some(n) = self.obsidian_vault_name.as_deref() {
            if !n.is_empty() {
                return Some(n.to_owned());
            }
        }
        self.obsidian_kb_path.as_deref().and_then(|p| {
            std::path::Path::new(p)
                .file_name()
                .and_then(|x| x.to_str())
                .map(|x| x.to_owned())
        })
    }
}

/// Load all settings from DB into AppSettings.
pub fn load(conn: &Connection) -> AppSettings {
    AppSettings {
        obsidian_kb_path: crate::store::get_setting(conn, "obsidian_kb_path"),
        obsidian_vault_name: crate::store::get_setting(conn, "obsidian_vault_name"),
        obsidian_daily_push: crate::store::get_setting_bool(conn, "obsidian_daily_push", true),
    }
}

/// Validate that path exists and is a directory, then persist to DB.
pub fn save_obsidian_path(conn: &Connection, path: &str) -> Result<()> {
    let meta =
        std::fs::metadata(path).map_err(|_| anyhow::anyhow!("Path does not exist: {}", path))?;
    if !meta.is_dir() {
        anyhow::bail!("Path is not a directory: {}", path);
    }
    crate::store::set_setting(conn, "obsidian_kb_path", path)?;
    Ok(())
}

/// Persist the Obsidian vault name. Trims whitespace; empty string deletes
/// the setting (so subsequent loads see `None` and `effective_vault_name`
/// falls back to the path basename).
pub fn save_obsidian_vault_name(conn: &Connection, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        crate::store::clear_setting(conn, "obsidian_vault_name")
    } else {
        crate::store::set_setting(conn, "obsidian_vault_name", name)
    }
}

/// Persist the "push to today's daily note" toggle.
pub fn save_obsidian_daily_push(conn: &Connection, value: bool) -> Result<()> {
    crate::store::set_setting_bool(conn, "obsidian_daily_push", value)
}
