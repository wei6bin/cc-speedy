use std::path::PathBuf;

/// Path to the OpenCode SQLite database (~/.local/share/opencode/opencode.db).
/// Returns None if the data_local_dir cannot be determined.
pub fn opencode_db_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("opencode").join("opencode.db"))
}
