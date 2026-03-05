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

pub fn resume_in_tmux(session_name: &str, project_path: &str, session_id: &str, yolo: bool) -> Result<()> {
    // Pass claude args directly — never via "sh -c" to avoid shell injection
    let new_session = |detach: bool| -> Result<()> {
        let mut cmd = std::process::Command::new("tmux");
        cmd.arg("new-session");
        if detach { cmd.arg("-d"); }
        let mut args: Vec<&str> = vec!["-s", session_name, "-c", project_path,
                                       "claude", "--resume", session_id];
        if yolo { args.push("--dangerously-skip-permissions"); }
        cmd.args(&args);
        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("tmux new-session failed: {}", status);
        }
        Ok(())
    };

    if is_inside_tmux() {
        if session_exists(session_name) {
            let status = std::process::Command::new("tmux")
                .args(["switch-client", "-t", session_name])
                .status()?;
            if !status.success() {
                anyhow::bail!("tmux switch-client failed: {}", status);
            }
        } else {
            new_session(true)?;
            let status = std::process::Command::new("tmux")
                .args(["switch-client", "-t", session_name])
                .status()?;
            if !status.success() {
                anyhow::bail!("tmux switch-client failed: {}", status);
            }
        }
    } else {
        new_session(false)?;
    }
    Ok(())
}
