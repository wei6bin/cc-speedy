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

/// Tmux session name for a Claude Code session: "cc-<last-2-path-segments>", max 50 chars.
pub fn cc_session_name(project_path: &str) -> String {
    let base = session_name_from_path(project_path);
    format!("cc-{}", base).chars().take(50).collect()
}

/// Tmux session name for an OpenCode session: "oc-<last-2-path-segments>", max 50 chars.
pub fn oc_session_name(project_path: &str) -> String {
    let base = session_name_from_path(project_path);
    format!("oc-{}", base).chars().take(50).collect()
}

pub fn is_inside_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// Rename a tmux window and lock the name (disable automatic-rename).
/// Also enables set-titles so the outer terminal (e.g. Windows Terminal on WSL)
/// receives the title via OSC escape sequences.
pub fn pin_window_title(session_name: &str, title: &str) {
    let _ = std::process::Command::new("tmux")
        .args(["rename-window", "-t", session_name, title])
        .status();
    let _ = std::process::Command::new("tmux")
        .args(["set-window-option", "-t", session_name, "automatic-rename", "off"])
        .status();
    // Forward title to the terminal emulator via OSC 0/2 (needed for WSL / Windows Terminal)
    let _ = std::process::Command::new("tmux")
        .args(["set-option", "-t", session_name, "set-titles", "on"])
        .status();
    let _ = std::process::Command::new("tmux")
        .args(["set-option", "-t", session_name, "set-titles-string", title])
        .status();
}

pub fn session_exists(name: &str) -> bool {
    std::process::Command::new("tmux")
        .args(["has-session", "-t", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Core helper: create-or-attach to a tmux session running `cmd`.
/// `cmd` must be a non-empty slice where `cmd[0]` is the executable.
/// Pass `detach_first = true` to always create detached then switch/attach.
fn resume_in_tmux_with_cmd(
    session_name: &str,
    project_path: &str,
    window_title: &str,
    cmd: &[&str],
) -> Result<()> {
    let start_session = |detach: bool| -> Result<()> {
        let mut builder = std::process::Command::new("tmux");
        builder.arg("new-session");
        if detach { builder.arg("-d"); }
        builder.args(["-s", session_name, "-n", window_title, "-c", project_path]);
        builder.args(cmd);
        let status = builder.status()?;
        if !status.success() {
            anyhow::bail!("tmux new-session failed: {}", status);
        }
        Ok(())
    };

    if is_inside_tmux() {
        if !session_exists(session_name) {
            start_session(true)?;
        }
        let status = std::process::Command::new("tmux")
            .args(["switch-client", "-t", session_name])
            .status()?;
        if !status.success() {
            anyhow::bail!("tmux switch-client failed: {}", status);
        }
        pin_window_title(session_name, window_title);
    } else {
        if !session_exists(session_name) {
            start_session(true)?;
        }
        pin_window_title(session_name, window_title);
        let status = std::process::Command::new("tmux")
            .args(["attach-session", "-t", session_name])
            .status()?;
        if !status.success() {
            anyhow::bail!("tmux attach-session failed: {}", status);
        }
    }
    Ok(())
}

/// Resume a Claude Code session in a named tmux session.
/// Runs `claude --resume <session_id>` (optionally with `--dangerously-skip-permissions`).
pub fn resume_in_tmux(
    session_name: &str,
    project_path: &str,
    session_id: &str,
    yolo: bool,
    window_title: &str,
) -> Result<()> {
    let mut args = vec!["claude", "--resume", session_id];
    if yolo { args.push("--dangerously-skip-permissions"); }
    resume_in_tmux_with_cmd(session_name, project_path, window_title, &args)
}

/// Resume an OpenCode session in a named tmux session.
/// Runs `opencode` in the project directory (OpenCode loads the most recent
/// session for that directory automatically).
pub fn resume_opencode_in_tmux(
    session_name: &str,
    project_path: &str,
    window_title: &str,
) -> Result<()> {
    resume_in_tmux_with_cmd(session_name, project_path, window_title, &["opencode"])
}
