# cc-monitor

> **Read-only output renderer for Claude Code CLI** — Tauri 2 + Vanilla TypeScript, Windows desktop app
>
> English · [中文](./README.md) | License: MIT | Platform: Windows 10/11 | Current: v3.2.0

Renders the real-time conversation written by Claude Code CLI to `~/.claude/projects/*.jsonl` with a modern UI: Markdown / LaTeX / syntax highlighting / collapsible tool-call cards / auto multi-tab management / history browsing & resume / **branch from any past turn**. **Fully read-only, zero intrusion** (does not **modify** any existing Claude Code file; the only two explicit user writes are deleting a session and branching from a turn — the latter only **adds** a new session file, leaving the original untouched).

**Project status**: Stable & in use. 365 backend + remote-daemon + vendor `code-picture-core` (25) tests + frontend node pure-function (many groups) & vitest+jsdom DOM tests (595) + an e2e suite, strict tsc type-check, **CI green across 4 jobs** (Rust `cargo test` [incl. `-p code-picture-core`] + frontend `npm test` [+ advisory eslint/stylelint + a coverage floor] + remote-daemon `cargo test` + an e2e-scripts health smoke [shellcheck/py_compile] — `npm test` gates only the frontend). **Batch 15/16 in development (unreleased, see [CHANGELOG](CHANGELOG.md) [Unreleased])**; last release **v3.2.0** (**Multi-account: isolated yet synced + per-session account switching + in-app account deploy wizard (#68/#69)**; earlier **Batch 14: SSH/SFTP/tmux remote integration (F41-F60)** — one-click remote resume (launches a terminal) / multi-address failover (happy-eyeballs racing) / SFTP file panel (browse·up/download·edit) / one-click public-key push / tmux attach & right-click screen preview / ProxyJump jump hosts / bulk import & aggregate from ~/.ssh/config / local port-forward console / daemonless fallback reads / "Claude finished a turn" system notification / tool-card file paths → SFTP locate; v2.22.2: **⚙ mislabel fixed** — bg-spare claiming the parent sid could demote an interactive session into a mis-mounted gear tab; kind conflicts now resolve deterministically; **remote stream-mode degradation fixed** — installers since v2.19 shipped without the daemon identity manifest, silently hiding bg sessions & re-introducing congestion; manifest embedded + hello-driven self-heal + visible degradation toast; v2.22.0: **message-stream virtualization** #35 — long sessions no longer lag (off-viewport layout/paint skipped + precise height estimates), history viewer opens a 37MB session in 1.1s (was 65.5s), cold start 24s→4s, live tabs load earlier messages on scroll-up; **right-click Resume on archived tabs**; `cc` first-bind race fixed — new shells no longer stall 800ms); v2.21.0: (**customizable resume command** (cc/cct), sidebar-drag / h-scroll / remote-↗ / ccm-installer fixes; v2.20.0: **vertical left tab bar** — drag-resize / narrow-window collapse, tabs no longer slide under the top-right icons; **background-clone sessions flagged in history** with a ⚙ badge so you resume the real session; + v2.19.1 fix: queued input messages no longer misfolded as ESC-rewinds #36); v2.19.0: (**remote congestion eliminated** — history via side-channel snapshots + a dedicated real-time tail, 46MB ≈ 4.6s with zero overflow (verified E2E); **newest messages load first**; **remote session status lights** now match local; correct skeleton/bg/focus rebuild after F5), now covering **SSH remote mode** (aggregate local + multiple remote machines in one window, #15/#17/#18/#20/#30/#31) — incl. **daemon auto-deploy + one-click install/uninstall** (embedded musl binaries pushed over SFTP #29; per-machine cards install/uninstall the daemon & ccm helper with install-location hints), **remote full-text search** (#28), **remote history delete / one-click resume** (since F41: tab right-click / history ↺ launches the remote terminal directly, falling back to clipboard copy), **history grouped/collapsible by machine** (#30/#31), **version negotiation + congestion toasts** (#32/#33), **session status lights** (#23), **local session auto-revive on resume** (crash/exit → archived, `/resume` restores without F5), visible AskUserQuestion options / API errors (#21), single-key shortcuts + tab tear-off, and more. See [CHANGELOG](CHANGELOG.md) + [doc/ARCHITECTURE.md](doc/ARCHITECTURE.md).

---

## Features

### Real-time rendering
- Watches `~/.claude/projects/**/*.jsonl`; new lines appear in window within 200ms
- Multi-tab: one tab per active Claude session, title `[project] aiTitle`
- After a session exits, its tab is archived (grayed out), closable via `W` / middle-click / `×`; a **local session auto-revives to live when you `/resume` it** (no F5 needed)
- **Tab in independent window** (issue #10): right-click a tab → "Open in new window" / `N`, **or just drag the tab below the tab bar and drop** (tear-off), mirrors the session into a standalone read-only window (dual-monitor / long-running tasks), synced live with the main window
- **Session status lights** (issue #23): each local tab's status dot reflects Claude's live state — 🟢 running / 🟡 waiting for your decision (permission / dialog, breathing blink) / 🔴 done, awaiting input; the agents expander gives each subagent its own light

### SSH remote mode (issue #15)
- Aggregate local + **multiple** remote-machine (NanoPi / any Linux / WSL) Claude sessions in **one window**; remote tabs are prefixed `[host]`; history browser groups/filters by machine (#30/#31)
- A remote daemon streams sessions back over SSH live; **auto-reconnect** on drop (exponential backoff 2→30s, #17), seq-dedup catch-up on reconnect
- **Daemon auto-deploy** (#29): cc-monitor embeds cross-compiled aarch64/x86_64 musl daemon binaries and pushes the right one over SFTP on connect (build_id version-gated) — zero manual deploy
- **Remote full-text search** (#28): the top-bar full-text search covers remote session content via a daemon server-side `--search`, hits tagged `[host]`
- **Remote history delete** (SFTP remove, double-confirm) / **one-click resume**: right-click a tab / history `↺` launches a remote terminal running `claude --resume` (wt.exe first, falls back to copying the command to the clipboard on failure)
- **Per-machine cards** in settings: one-click **install / uninstall daemon** and **install / uninstall the `ccm` helper** (into the remote `~/.bashrc`), with **install-location hints** (daemon → `~/.cc-monitor/bin/`, ccm → `~/.bashrc` marker block); cards collapse to the machine name
- **Version negotiation** (#33) + **slow-consumer overflow signal** (#32): a daemon/client build_id mismatch or a congested pipe surfaces a remote-health toast
- Remote tabs can also ↗ raise their terminal (#18)
- Deployment: [doc/REMOTE-PHASE0-DEPLOY.md](doc/REMOTE-PHASE0-DEPLOY.md) (auto-deploy + manual fallback)

**Batch 14 remote enhancements** (F41–F60):

- **One-click resume / resume-tmux / launch new Claude**: right-click a tab or use history `↺` to spawn a remote terminal directly — no manual command paste; tmux sessions go through send-keys, and you can start a fresh Claude session on the selected machine in one click
- **Multi-address failover** (happy-eyeballs): when a machine has several addresses, race them concurrently and take the first to connect
- **SFTP file panel**: browse / upload-download (with progress + cancel) / mkdir·rename·delete / directory bookmarks / open-terminal-here / edit small files in-panel — reachable from each machine card's "Files" entry; a remote path in a tool card jumps straight to its SFTP location
- **Tab attach tmux / right-click preview of the remote tmux screen**: find which tmux session Claude runs in and attach in one click, or grab a read-only snapshot of the current screen
- **One-click public-key push**: append your local public key to the remote `~/.ssh/authorized_keys` for passwordless login
- **ProxyJump / ssh-config bulk import**: reach an intranet target through a jump host; bulk-import from `~/.ssh/config` with smart aggregation of a machine's multiple addresses
- **Local port-forward console** (`-L`): forward ports over the existing SSH connection, started/stopped from one place
- **Daemonless degraded read**: read remote sessions via plain `tail` polling without a daemon (a capability subset, honestly surfaced)
- **Turn-complete notification**: a system notification when a remote session finishes a turn; **fingerprint reset**: reset a remote host-key when it changes

### Multi-account (isolated yet synced, #68/#69)
- **Isolation + sharing**: manage multiple Claude Code accounts on one remote — each with its own `CLAUDE_CONFIG_DIR`/`.credentials.json` (both run at once, no mutual kick), while skills/memory/history/settings/plugins are shared live (symlinked to one shared library)
- **Settings "Accounts" group**: lists the remote's accounts (name/email/logged-in), the current one carries a chip/badge
- **Per-session account for launch / Resume**: choose which account's config-dir a session starts with (remote-first)
- **Graceful exit on account switch**: switching account restarts the session — request a graceful exit first (`Escape` to interrupt the turn → `/exit` → bounded wait → fallback kill), then relaunch with the new account's config-dir
- **In-app account deploy wizard**: a settings wizard steps through the `cc-acct-iso` isolate/sync pipeline (read-only status via the daemon; credential/login/sync moves go through a real terminal window), with a "login terminal" button per account
- **Note**: multi-account read-only queries need the remote daemon at its latest build (auto-redeployed on connect, or reinstall per-machine in settings)

### Rich rendering
- **Markdown**: GFM + tables + task lists (marked.js)
- **LaTeX**: `$...$` inline, `$$...$$` block (KaTeX)
- **Syntax highlighting**: 30+ common languages (highlight.js/common)
- **Tool calls**: `tool_use` + `tool_result` merged into one collapsible card; long output gets a nested second-level collapse
- **Code-change diffs** (issue #14): `Edit` / `Write` / `MultiEdit` tools expand into line-level red/green diffs (instead of raw JSON), long ones auto-collapse with "show full"; any anomaly falls back to the raw JSON
- **subagent**: `Task` / `Agent` tool calls auto-embed the sub-JSONL (lazy-loaded)
- **/compact summary**: shown collapsed
- **User input prefix cards**: `!cmd` bash mode renders as a terminal-style command card plus stdout/stderr output cards (stderr tinted red, long output collapsible); `/xxx` slash commands render as compact cards compatible with both old and new CLI tag orders; anything unrecognized falls through verbatim
- **Code copy**: top-right "copy" button on every code block

### History browser
- Toolbar `◷` button / `H` to toggle; grouped by working directory
- Project groups **collapsed by default**; expand triggers **lazy load** of all sessions in that project
- **Full-text search** (issue #6): a "full-text" mode searches message content across all sessions; hits highlighted, click to jump into the read-only viewer and locate; optional "include tool content", filter by scope/time
- Per-row actions: `★/☆` star, `✎` rename (Chinese supported), `–/+` hide, `↺` resume (v2.8.1: `cc --resume` in a new **PowerShell** window, falls back to `claude`; loads your profile so proxy/env apply), `✕` delete (confirm twice; jsonl actually removed)
- Clicking a session opens a **read-only viewer**
- **Branch from a turn (F62)**: in the read-only viewer, hover any turn (your prompt / Claude's reply) → a `⑂` appears top-right; click it to copy "start → this turn" into a **new session** (matching Claude's native `/branch` `forkedFrom` format, **original untouched**), then a toast lets you `resume` the branch in a new terminal. Fills the gap that the built-in `/branch` only forks at the current point. Local sessions only (not shown for remote)

### Settings panel (,)

Five collapsible groups (only "Behavior" expanded by default):

- **Behavior**: auto-follow which tab the user is typing into; whether to bring monitor window to front on auto-switch
- **Shortcuts**: built-in editor to customize all 22 available action chords
- **Data sources & integration**: configurable Claude data location (three-tier fallback: settings > `$CLAUDE_CONFIG_DIR` > `~/.claude`) + one-click install for the PowerShell `__ccm_bind` helper
- **Appearance**: 13 tokens (fonts + colors), live preview, persisted to `~/.claude/claudecode-frontend/config.json`
- **Diagnostics & storage**: tracing level toggle + log file path + transparent listing of every persisted data path

### Terminal focus (optional)
- Each live tab has a ↗ button / backtick key to bring the corresponding terminal window to front
- Requires installing the PowerShell integration (one-click from settings panel); see "PowerShell Integration" below

### Shortcuts

| Key | Action |
|---|---|
| **]** / **[** | Next / previous tab |
| **1** .. **9** | Jump to tab N |
| **W** | Close current archived tab |
| **E** | Open the current tab's working directory in Explorer |
| **`** (backtick) | Bring current tab's terminal to front |
| **H** | Toggle history browser |
| **,** | Open settings panel |
| **M** | Minimize main window |
| **F11** | Toggle **true fullscreen** (borderless, covers taskbar; a non-letter key, so it works under a CJK IME too) |
| **N** | Open current tab in an independent window (issue #10; or drag the tab out below the tab bar) |
| **T** | Toggle Task panel |
| **Esc** | Close topmost overlay (read-only viewer → history view → settings panel) |

> **Defaults are all single keys** — cc-monitor is a read-only monitor window, no modifier needed. When an editable field (search / rename input) is focused, shortcuts yield to typing. Every chord is customizable in Settings → Shortcuts; two behavior/panel toggles are unbound by default and can be assigned a key there.
>
> ⚠ **Chinese / East-Asian IME**: in a CJK input mode the bare letter keys (**W / E / H / M / N / T**) are swallowed by the IME at the OS layer before the app sees them — switch to English input first, or rebind them to `Ctrl`/`Alt` combos or non-character keys (e.g. `Delete`) in the shortcut editor. Digits `1`–`9`, `[` `]`, `` ` ``, `Esc`, and the mouse `×` are unaffected.

---

## Installation

### Requirements
- Windows 11 / 10 (1809+)
- [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (bundled with Win11; needs manual install on Win10)
- [Claude Code CLI](https://github.com/anthropics/claude-code) installed and run at least once

### Download

Get the latest version from [Releases](https://github.com/bo0Zeng/cc-monitor/releases). File names like `cc-monitor_<version>_x64-setup.exe`:

- `*-setup.exe` — NSIS installer (recommended for regular users)
- `*_zh-CN.msi` — MSI bundle (for enterprise IT deployment)

Double-click to run; Windows SmartScreen will show "unknown publisher" on first launch (we don't sign). Choose "More info → Run anyway".

### First use

1. Launch `cc-monitor.exe`
2. Run `claude` in any terminal (a tab will appear instantly in cc-monitor)
3. Type in claude → user/assistant messages appear in cc-monitor within 200ms
4. Want Tab ↗ focus-switch? See "PowerShell Integration" below

---

## PowerShell Integration (optional)

To make **Tab ↗ / `Ctrl+\`` focus-switching** accurately raise the right terminal window, you need to install an `__ccm_bind` helper into your PowerShell profile.

1. Open cc-monitor → `Ctrl+,` settings → **PowerShell integration**
2. Pick a profile location (5 options dropdown):
   - `PowerShell 5.1 - $PROFILE` — installs to `Microsoft.PowerShell_profile.ps1` (CurrentUserCurrentHost); only `powershell.exe` console reads it
   - **`PowerShell 5.1 - All hosts (profile.ps1)`** ⭐ recommended — VSCode terminal / ISE / SSH all work
   - PowerShell 7.x: same two options
   - Custom path
3. **"Also install cc wrapper" is unchecked by default** — installs only `__ccm_bind` helper, doesn't touch your existing commands
4. Click [Install] → restart PowerShell
5. In your own claude-launching wrapper (function / alias), add `__ccm_bind` at the top

If you want cc-monitor to create a `cc` wrapper for you: check "Also install cc wrapper", and it installs `function cc { __ccm_bind; & claude $args }`. Use `cc` to start claude. **Warning**: this will override any existing same-named function in your profile.

Optionally check "Auto-launch monitor when starting claude via cc".

**Safety guarantees**: [Install] backs up the original profile to `<profile>.ccm-backup-<timestamp>` before writing, verifies length after write, and auto-rolls-back if anything goes wrong. Uses Win32 `ReplaceFileW` API to preserve original NTFS ACL. Settings persist to localStorage.

Skipping this is fine — ↗ / `Ctrl+\`` just won't work; real-time rendering / tabs / history all work normally.

---

## Troubleshooting

| Symptom | Diagnose |
|---|---|
| "WebView2 Runtime not found" on launch | Install [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) |
| Tab doesn't appear after running claude | Check `~/.claude/sessions/<PID>.json` exists |
| Tab ↗ / `Ctrl+\`` doesn't raise terminal | PowerShell integration not installed, or wrapper doesn't call `__ccm_bind` |
| After installing cc integration, `cc` times out | monitor isn't running; start monitor first, or check "Auto-launch monitor" in settings |
| `Access to the path … is denied` on PS startup | NTFS ACL overwritten by an older version (v1.7.10+ fixes this). Run in admin PS: `icacls "<profile>" /grant "$env:USERDOMAIN\$env:USERNAME:(F)"` |
| History `↺` resume fails | v2.8.1+ runs `cc`/`claude --resume` inside PowerShell: ensure your PowerShell profile installs `cc` (or `claude` is in PATH); the resume window now loads your profile so proxy/`cc` settings apply |
| Claude data in a non-default location | Settings → Data → Claude data directory; or set `CLAUDE_CONFIG_DIR` env and restart |

---

## Documentation

The detailed documentation is in **Chinese**. See [README.md](README.md) for the full doc tree. Key entries:

| Doc | Audience | Content |
|---|---|---|
| [doc/ARCHITECTURE.md](doc/ARCHITECTURE.md) | New contributors | Data flow + module map + design layering |
| [doc/IPC-PROTOCOL.md](doc/IPC-PROTOCOL.md) | Protocol changers | Cross-process file IPC + sessions/status + remote wire schemas + handshake timing |
| [doc/REMOTE-PHASE0-DEPLOY.md](doc/REMOTE-PHASE0-DEPLOY.md) | Remote deployers | SSH remote daemon auto-deploy (#29) + manual deploy runbook (issue #15) |
| [doc/INVARIANTS.md](doc/INVARIANTS.md) | Everyone | Global invariants (zero intrusion / encoding / ACL / ordering) |
| [doc/CONTRIBUTING.md](doc/CONTRIBUTING.md) | Contributors | Checklists + cookbook (add IPC / jsonl type / setting / hotkey) |
| [doc/DEVELOPMENT.md](doc/DEVELOPMENT.md) | Developers | Dev environment + port conflict / debugging |
| [doc/BUILDING.md](doc/BUILDING.md) | Releasers | Production build + packaging + Code Signing |
| [doc/RELEASING.md](doc/RELEASING.md) | Releasers | Release SOP + how to write the CHANGELOG |
| [CHANGELOG.md](CHANGELOG.md) | Upgrade users | Version history |

English translations of these docs are not yet complete. Contributions welcome.

---

## License

[MIT](LICENSE) © 2026 cc-monitor contributors
