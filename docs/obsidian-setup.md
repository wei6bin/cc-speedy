# Obsidian CLI setup

cc-speedy uses the official `obsidian` command-line tool that ships with the
Obsidian desktop app (Obsidian ≥ 1.10). This is **not** a separate download —
it's built into the installer and you toggle it on inside Obsidian.

## Enable the CLI

1. Open Obsidian on your platform.
2. Settings → General → **Command line interface** → toggle on.
3. Follow the on-screen registration prompt.

After registration:

| OS | Where the CLI lands |
|----|-----|
| macOS   | `/usr/local/bin/obsidian` (symlink) |
| Linux   | `~/.local/bin/obsidian` |
| Windows | `Obsidian.com` next to `Obsidian.exe`; the installer adds it to PATH |

Verify:

```sh
obsidian --help
obsidian eval code="app.vault.getName()"
```

The second command prints the active vault's name. If both work, cc-speedy is
ready to use the CLI features.

## WSL users

The Linux side of WSL doesn't see Windows-side PATH automatically. Drop a
small wrapper at `~/.local/bin/obsidian`:

```bash
#!/usr/bin/env bash
set -euo pipefail
CANDIDATES=(
  "/mnt/c/Users/$(whoami)/AppData/Local/Programs/Obsidian/Obsidian.com"
  "/mnt/c/Program Files/Obsidian/Obsidian.com"
)
for c in "${CANDIDATES[@]}"; do
  [[ -x "$c" ]] && exec "$c" "$@"
done
echo "obsidian: redirector not found — adjust CANDIDATES" >&2
exit 1
```

`chmod +x ~/.local/bin/obsidian` and you're done.

## Required: vault must be open

The CLI talks to a *running* Obsidian instance. Make sure the vault you've
configured in cc-speedy (`s` settings panel) is currently open in Obsidian
when cc-speedy tries to push.

## Required for daily-note features

cc-speedy uses `obsidian daily:append` to push session lines into today's
daily note. Out of the box this works against Obsidian's built-in Daily Notes
core plugin (enabled by default). If you've disabled Daily Notes, re-enable
it under Settings → Core plugins.

## Configuring cc-speedy

Press `s` in cc-speedy to open the Settings panel:

- **Vault path** — absolute path to the vault directory (existing setting).
- **Vault name** — the name Obsidian uses internally. If left blank, cc-speedy
  defaults to the basename of the vault path. If your vault directory and
  Obsidian-side vault name differ, set this explicitly.
- **Push to daily note** — toggle on/off. Off disables the daily-note push
  without affecting the per-session note export.

## Troubleshooting

| Status flash | Meaning |
|----|----|
| `Obsidian CLI not installed` | `obsidian --help` failed. Re-run the registration step in Settings. |
| `Obsidian not running — open the vault first` | The CLI is reachable but no instance is running, or the configured vault isn't open. |
| `Obsidian: <message>` | The CLI returned non-zero. The message is the first line of stderr. |

The per-session Markdown file is always written regardless of CLI status — the
CLI integrations are purely additive.
