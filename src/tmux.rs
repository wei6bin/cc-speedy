use anyhow::Result;

/// Derive tmux session name: last 2 path segments joined with "-", sanitized, max 50 chars
pub fn session_name_from_path(path: &str) -> String {
    let parts: Vec<&str> = path
        .trim_end_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let name = match parts.len() {
        0 => "cc-speedy".to_string(),
        1 => parts[0].to_string(),
        n => format!("{}-{}", parts[n - 2], parts[n - 1]),
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
        .args([
            "set-window-option",
            "-t",
            session_name,
            "automatic-rename",
            "off",
        ])
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
/// If `context` is `Some`, the text is pasted into the session (via bracketed paste)
/// ~1.5s after the session is created, so the agent has time to boot.
fn resume_in_tmux_with_cmd(
    session_name: &str,
    project_path: &str,
    window_title: &str,
    cmd: &[&str],
    context: Option<&str>,
) -> Result<()> {
    let start_session = |detach: bool| -> Result<()> {
        let mut builder = std::process::Command::new("tmux");
        builder.arg("new-session");
        if detach {
            builder.arg("-d");
        }
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
        if let Some(ctx) = context {
            let _ = schedule_paste_into_session(session_name, ctx);
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
        if let Some(ctx) = context {
            let _ = schedule_paste_into_session(session_name, ctx);
        }
        let status = std::process::Command::new("tmux")
            .args(["attach-session", "-t", session_name])
            .status()?;
        if !status.success() {
            anyhow::bail!("tmux attach-session failed: {}", status);
        }
    }
    Ok(())
}

/// Schedule a bracketed-paste of `text` into `session_name` ~1.5s from now.
/// Works by writing the text to a tempfile and forking a detached `sh` that
/// sleeps, loads the file into a tmux buffer, pastes it with `-p` (bracketed
/// paste — TUI agents treat it as a single paste, not typed Enter), then
/// deletes the buffer and tempfile. Returns immediately; the paste fires
/// asynchronously while the user is attached to the session.
fn schedule_paste_into_session(session_name: &str, text: &str) -> Result<()> {
    use std::io::Write;
    let mut tf = tempfile::Builder::new()
        .prefix("cc-speedy-ctx-")
        .suffix(".txt")
        .tempfile()?;
    tf.write_all(text.as_bytes())?;
    // Persist the tempfile so the detached shell can read it after we return.
    // The shell is responsible for `rm -f`ing it after paste.
    let path = tf.into_temp_path().keep()?;
    let file = path.to_string_lossy().to_string();

    let buf = format!("cc-speedy-ctx-{}", session_name);
    // Session name, buffer name, and tempfile path are all tightly controlled
    // (sanitized session names, tempfile crate paths) — single-quote wrapping
    // is sufficient and no user-provided text passes through the shell.
    let script = format!(
        "sleep 1.5 && \
         tmux load-buffer -b '{buf}' '{file}' && \
         tmux paste-buffer -p -b '{buf}' -t '{session}' ; \
         tmux delete-buffer -b '{buf}' 2>/dev/null ; \
         rm -f '{file}'",
        buf = buf,
        file = file,
        session = session_name,
    );

    std::process::Command::new("sh")
        .args(["-c", &script])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

/// Unique tmux session name for a brand-new CC conversation (timestamp suffix avoids collisions).
pub fn new_cc_session_name(project_path: &str) -> String {
    let base = session_name_from_path(project_path);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("cc-new-{}-{}", base, ts % 100_000)
        .chars()
        .take(50)
        .collect()
}

/// Unique tmux session name for a brand-new OC conversation.
pub fn new_oc_session_name(project_path: &str) -> String {
    let base = session_name_from_path(project_path);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("oc-new-{}-{}", base, ts % 100_000)
        .chars()
        .take(50)
        .collect()
}

/// Start a fresh Claude Code conversation (no --resume) in a new tmux session.
/// If `context` is `Some`, the text is pasted into the session after the agent starts.
pub fn new_cc_in_tmux(
    session_name: &str,
    project_path: &str,
    yolo: bool,
    window_title: &str,
    context: Option<&str>,
) -> Result<()> {
    let mut args = vec!["claude"];
    if yolo {
        args.push("--dangerously-skip-permissions");
    }
    resume_in_tmux_with_cmd(session_name, project_path, window_title, &args, context)
}

/// Start a fresh OpenCode conversation (no --session) in a new tmux session.
pub fn new_oc_in_tmux(
    session_name: &str,
    project_path: &str,
    window_title: &str,
    context: Option<&str>,
) -> Result<()> {
    resume_in_tmux_with_cmd(
        session_name,
        project_path,
        window_title,
        &["opencode"],
        context,
    )
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
    if yolo {
        args.push("--dangerously-skip-permissions");
    }
    resume_in_tmux_with_cmd(session_name, project_path, window_title, &args, None)
}

/// Resume an OpenCode session in a named tmux session.
/// Runs `opencode --session <session_id>` to resume the specific session.
pub fn resume_opencode_in_tmux(
    session_name: &str,
    project_path: &str,
    session_id: &str,
    window_title: &str,
) -> Result<()> {
    resume_in_tmux_with_cmd(
        session_name,
        project_path,
        window_title,
        &["opencode", "--session", session_id],
        None,
    )
}

/// Tmux session name for a Copilot session: "co-<last-2-path-segments>", max 50 chars.
pub fn copilot_session_name(project_path: &str) -> String {
    let base = session_name_from_path(project_path);
    format!("co-{}", base).chars().take(50).collect()
}

/// Unique tmux session name for a brand-new Copilot conversation.
pub fn new_copilot_session_name(project_path: &str) -> String {
    let base = session_name_from_path(project_path);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("co-new-{}-{}", base, ts % 100_000)
        .chars()
        .take(50)
        .collect()
}

/// Resume a Copilot session in a named tmux session.
/// `yolo = true` adds `--allow-all` (Copilot's equivalent of --dangerously-skip-permissions).
pub fn resume_copilot_in_tmux(
    session_name: &str,
    project_path: &str,
    session_id: &str,
    yolo: bool,
    window_title: &str,
) -> Result<()> {
    let resume_arg = format!("--resume={}", session_id);
    let mut args = vec!["copilot"];
    if yolo {
        args.push("--allow-all");
    }
    args.push(&resume_arg);
    resume_in_tmux_with_cmd(session_name, project_path, window_title, &args, None)
}

/// Start a fresh Copilot conversation in a new tmux session.
pub fn new_copilot_in_tmux(
    session_name: &str,
    project_path: &str,
    yolo: bool,
    window_title: &str,
    context: Option<&str>,
) -> Result<()> {
    let mut args = vec!["copilot"];
    if yolo {
        args.push("--allow-all");
    }
    resume_in_tmux_with_cmd(session_name, project_path, window_title, &args, context)
}
