//! Thin wrapper around the official `obsidian` CLI bundled with Obsidian.app.
//!
//! All public functions either succeed or return a typed `Error` describing
//! one of three discrete failure modes. Callers map these to user-facing
//! strings or stderr logs as appropriate.

use std::process::Command;

#[derive(Debug)]
pub enum Error {
    /// The `obsidian` binary could not be invoked (not on PATH or not executable).
    CliMissing,
    /// The CLI is reachable but no Obsidian instance is running or the named vault
    /// is not open.
    NotRunning,
    /// The command itself returned a non-zero exit. The first line of stderr is
    /// captured for surfacing to the user.
    CommandFailed { stderr_first_line: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::CliMissing => write!(f, "Obsidian CLI not installed"),
            Error::NotRunning => write!(f, "Obsidian not running — open the vault first"),
            Error::CommandFailed { stderr_first_line } => {
                write!(f, "Obsidian: {}", stderr_first_line)
            }
        }
    }
}

impl std::error::Error for Error {}

/// Escape a value for use as the right-hand side of a `key=value` argument
/// passed to `obsidian`. Per the CLI's own escape rules: `\\` for backslash,
/// `\"` for double-quote, `\n` for newline, `\t` for tab. Backslash is escaped
/// first so the other replacements don't double-escape its expansions.
pub fn escape_arg_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str(r#"\""#),
            '\n' => out.push_str(r"\n"),
            '\t' => out.push_str(r"\t"),
            _ => out.push(ch),
        }
    }
    out
}

/// Returns true iff the `obsidian` binary is on PATH and responds to `--help`.
pub fn is_available() -> bool {
    Command::new("obsidian")
        .arg("--help")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Build a JS expression suitable for `obsidian eval code=...` that returns
/// `true` if today's daily note exists AND already contains `marker`, else
/// `false`. Marker is escaped to remain inside the JS string literal.
pub fn build_dedupe_eval_code(marker: &str) -> String {
    // Escape backslash first, then double-quote, for embedding in JS string literal.
    let escaped: String = marker
        .chars()
        .flat_map(|c| match c {
            '\\' => vec!['\\', '\\'],
            '"' => vec!['\\', '"'],
            '\n' => vec!['\\', 'n'],
            other => vec![other],
        })
        .collect();
    format!(
        r#"(()=>{{const t=window.moment().format('YYYY-MM-DD');const f=app.vault.getMarkdownFiles().find(x=>x.basename===t);return !!(f && (await app.vault.read(f)).includes("{}"))}})()"#,
        escaped,
    )
}

/// Append a single line of content to today's daily note in `vault`. If
/// `dedupe_marker` is `Some(s)` and today's daily note already contains `s`,
/// the call is a no-op (idempotent). The CLI auto-creates today's daily note
/// if it doesn't yet exist.
pub fn daily_append(vault: &str, content: &str, dedupe_marker: Option<&str>) -> Result<(), Error> {
    if !is_available() {
        return Err(Error::CliMissing);
    }

    if let Some(marker) = dedupe_marker {
        let code = build_dedupe_eval_code(marker);
        let probe = Command::new("obsidian")
            .arg(format!("vault={}", vault))
            .arg("eval")
            .arg(format!("code={}", escape_arg_value(&code)))
            .output()
            .map_err(|_| Error::CliMissing)?;
        if !probe.status.success() {
            // Vault not open or eval failed — surface as NotRunning.
            return Err(Error::NotRunning);
        }
        let stdout = String::from_utf8_lossy(&probe.stdout);
        let last = stdout.lines().last().map(|l| l.trim()).unwrap_or("");
        if last == "=> true" {
            return Ok(()); // already there; nothing to do
        }
    }

    let out = Command::new("obsidian")
        .arg(format!("vault={}", vault))
        .arg("daily:append")
        .arg(format!("content={}", escape_arg_value(content)))
        .output()
        .map_err(|_| Error::CliMissing)?;

    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let line = stderr.lines().next().unwrap_or("(no stderr)").to_string();
        Err(Error::CommandFailed {
            stderr_first_line: line,
        })
    }
}

/// Probe whether the named vault is currently open in a running Obsidian
/// instance. Returns false if the CLI is missing, the app isn't running,
/// the vault isn't open, or the eval otherwise fails.
pub fn vault_is_running(vault: &str) -> bool {
    let output = Command::new("obsidian")
        .arg(format!("vault={}", vault))
        .arg("eval")
        .arg(r#"code=app.vault.getName()"#)
        .output();
    match output {
        Ok(o) if o.status.success() => {
            // CLI prints "=> <value>" — accept anything non-empty.
            !String::from_utf8_lossy(&o.stdout).trim().is_empty()
        }
        _ => false,
    }
}
