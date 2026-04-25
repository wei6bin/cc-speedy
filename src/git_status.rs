use std::process::Command;
use std::time::Duration;

/// Live git state of a project directory, as rendered in the session list.
/// Branch string is populated for Clean/Dirty; omitted for NoGit/Error.
#[derive(Clone, Debug, PartialEq)]
pub enum GitStatus {
    Clean {
        branch: String,
    },
    Dirty {
        branch: String,
    },
    NoGit,
    /// Timeout, git binary missing, or any other failure.
    Error,
}

impl GitStatus {
    pub fn branch(&self) -> Option<&str> {
        match self {
            GitStatus::Clean { branch } | GitStatus::Dirty { branch } => Some(branch.as_str()),
            _ => None,
        }
    }
}

/// Run `git -C <path> status --porcelain --branch` with a timeout.
/// Blocking — callers should wrap in `tokio::task::spawn_blocking`.
pub fn check(path: &str, timeout_ms: u64) -> GitStatus {
    let mut child = match Command::new("git")
        .args(["-C", path, "status", "--porcelain", "--branch"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return GitStatus::Error,
    };

    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    // Non-zero exit → non-repo (or git error). Treat as NoGit.
                    return GitStatus::NoGit;
                }
                break;
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return GitStatus::Error;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => return GitStatus::Error,
        }
    }

    let mut stdout = String::new();
    if let Some(mut out) = child.stdout.take() {
        use std::io::Read;
        let _ = out.read_to_string(&mut stdout);
    }
    parse_porcelain(&stdout)
}

/// Parse `git status --porcelain --branch` stdout.
/// First line is `## <branch>[...]` (or `## HEAD (no branch)` when detached).
/// Any additional lines indicate dirty state (modifications or untracked).
pub fn parse_porcelain(stdout: &str) -> GitStatus {
    let mut lines = stdout.lines();
    let Some(branch_line) = lines.next() else {
        return GitStatus::Error;
    };

    let Some(rest) = branch_line.strip_prefix("## ") else {
        return GitStatus::Error;
    };

    // Branch is the portion before the first space (which starts tracking info
    // like `...origin/x [ahead 1]`) OR the whole thing for detached HEAD.
    let branch = if let Some((b, _)) = rest.split_once("...") {
        b.to_string()
    } else {
        rest.to_string()
    };

    let has_changes = lines.any(|l| !l.is_empty());
    if has_changes {
        GitStatus::Dirty { branch }
    } else {
        GitStatus::Clean { branch }
    }
}
