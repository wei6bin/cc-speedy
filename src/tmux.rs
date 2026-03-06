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

pub fn resume_in_tmux(session_name: &str, project_path: &str, session_id: &str, yolo: bool, window_title: &str) -> Result<()> {
    // Pass claude args directly — never via "sh -c" to avoid shell injection
    let new_session = |detach: bool| -> Result<()> {
        let mut cmd = std::process::Command::new("tmux");
        cmd.arg("new-session");
        if detach { cmd.arg("-d"); }
        // -n sets the window name at creation; must come before the command separator
        let mut args: Vec<&str> = vec!["-s", session_name, "-n", window_title, "-c", project_path,
                                       "claude", "--resume", session_id];
        if yolo { args.push("--dangerously-skip-permissions"); }
        cmd.args(&args);
        let status = cmd.status()?;
        if !status.success() {
            anyhow::bail!("tmux new-session failed: {}", status);
        }
        Ok(())
    };

    let pin_window_title = |target: &str| pin_window_title(target, window_title);

    if is_inside_tmux() {
        if session_exists(session_name) {
            let status = std::process::Command::new("tmux")
                .args(["switch-client", "-t", session_name])
                .status()?;
            if !status.success() {
                anyhow::bail!("tmux switch-client failed: {}", status);
            }
            pin_window_title(session_name);
        } else {
            new_session(true)?;
            let status = std::process::Command::new("tmux")
                .args(["switch-client", "-t", session_name])
                .status()?;
            if !status.success() {
                anyhow::bail!("tmux switch-client failed: {}", status);
            }
            pin_window_title(session_name);
        }
    } else {
        // new_session(false) attaches and blocks — set title before attaching via -n flag
        // pin automatic-rename off before attaching so it survives process start
        new_session(true)?; // create detached first
        pin_window_title(session_name);
        // now attach
        let status = std::process::Command::new("tmux")
            .args(["attach-session", "-t", session_name])
            .status()?;
        if !status.success() {
            anyhow::bail!("tmux attach-session failed: {}", status);
        }
    }
    Ok(())
}
