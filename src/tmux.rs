use anyhow::Result;

/// Derive tmux session name: last 2 path segments joined with "-", sanitized, max 50 chars
pub fn session_name_from_path(path: &str) -> String {
    let parts: Vec<&str> = path.trim_end_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let name = match parts.len() {
        0 => "cc-speedy".to_string(),
        1 => parts[0].to_string(),
        n => format!("{}-{}", parts[n-2], parts[n-1]),
    };
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(50)
        .collect()
}

pub fn is_inside_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

pub fn session_exists(name: &str) -> bool {
    std::process::Command::new("tmux")
        .args(["has-session", "-t", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn resume_in_tmux(session_name: &str, project_path: &str, session_id: &str) -> Result<()> {
    let claude_cmd = format!("claude --resume {}", session_id);
    if is_inside_tmux() {
        if session_exists(session_name) {
            std::process::Command::new("tmux")
                .args(["switch-client", "-t", session_name])
                .status()?;
        } else {
            std::process::Command::new("tmux")
                .args(["new-session", "-d", "-s", session_name, "-c", project_path, &claude_cmd])
                .status()?;
            std::process::Command::new("tmux")
                .args(["switch-client", "-t", session_name])
                .status()?;
        }
    } else {
        std::process::Command::new("tmux")
            .args(["new-session", "-s", session_name, "-c", project_path, &claude_cmd])
            .status()?;
    }
    Ok(())
}
