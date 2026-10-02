# Security Policy

cc-monitor is a desktop viewer and launcher for Claude Code sessions, on the local machine
and on remote machines over SSH. It reads Claude Code's session records, and it writes files
when you ask it to — the full list is under *What it writes* below.

## Reporting a vulnerability

Please report security issues **privately** — do **not** open a public issue until a
fix is available:

- Open a [GitHub security advisory](https://github.com/bo0Zeng/cc-monitor/security/advisories/new) (preferred), **or**
- Email the maintainer (see the address in the git commit history / `git log`).

We aim to acknowledge reports within a few days.

## What it writes

Each machine's page in the app (Settings → Machines → Footprint) lists every path cc-monitor
writes on that machine. By kind:

- **Its own home, `~/.cc-monitor/`**: settings, logs, the backend binary (`bin/ccm`), the
  account store (`accounts/`), backups, and credentials (API keys, relay keys). Credential
  files are readable only by you when they are created.
- **The backend on a remote machine**: connecting installs `~/.cc-monitor/bin/ccm` there, and
  replaces it when its version does not match.
- **Claude Code's directory, only on an explicit action**: forking a session creates a new
  session file (it never overwrites one); deleting a session removes its file; installing or
  removing a skill or an MCP server writes that skill's folder or that server's entry in
  Claude Code's config (`.claude.json`, or a project's `.mcp.json`); enabling multiple accounts
  moves the current login files into the account store and links the shared items back.
  `~/.claude/settings.json` is never written — the app shows a snippet for you to paste.
- **Shell startup files**: setting up the shell integration adds a fenced block to your shell
  rc file (bash / zsh) or PowerShell profile, after backing the file up; removing it strips
  exactly that block. The alias list is a file of its own under `~/.cc-monitor/`.
- **SSH keys**: pushing your public key adds it to the remote `~/.ssh/authorized_keys`.
- **Files you change in the file window**: create, rename, delete, copy, upload, download,
  change permissions — where you point it (the remote machine you opened, this computer for
  downloads, another machine you copy to), with that user's permissions.

## Scope notes

- **SSH host keys** use trust-on-first-use (TOFU) by default; the first connection is
  MITM-capable. The UI warns loudly and offers one-click fingerprint pinning — pre-share
  or pin a `SHA256:` fingerprint for sensitive setups.
- **Untrusted content** (CLI / model output) is sanitized with DOMPurify before any
  `innerHTML` rendering.
- **Releases are unsigned.** Verify your download against the published `SHA256SUMS.txt`
  (Windows) or `SHA256SUMS-linux.txt` (Linux).
- Dependency advisories: CI gates `npm audit` on **production** deps (`--omit=dev`). For
  Rust, run `cargo audit` locally. Known **accepted residuals** (no actionable upstream fix,
  low risk for this app):
  - `rsa` **RUSTSEC-2023-0071** (Marvin Attack — RSA decryption timing sidechannel): pulled
    transitively by `russh`; no patched `rsa` release exists upstream. Only used for SSH
    host-key / auth against **user-controlled** hosts → minimal exposure.
  - **gtk-rs** crates (`atk` / `gdk` / `gtk` …) flagged *unmaintained*: these are Tauri's
    **Linux** GUI bindings. They are compiled into the Linux build (the `.deb` and the bare
    `cc-monitor` binary) and are not used on Windows.
