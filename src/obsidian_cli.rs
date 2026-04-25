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
