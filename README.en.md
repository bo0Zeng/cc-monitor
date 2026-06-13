# cc-monitor

> **Read-only output renderer for Claude Code CLI** — Tauri 2 + Vanilla TypeScript, Windows desktop app
>
> English · [中文](./README.md) | License: MIT | Platform: Windows 10/11 | Current: v2.9.6

Renders the real-time conversation written by Claude Code CLI to `~/.claude/projects/*.jsonl` with a modern UI: Markdown / LaTeX / syntax highlighting / collapsible tool-call cards / auto multi-tab management / history browsing & resume. **Fully read-only, zero intrusion** (does not modify any Claude Code file; the only exception is when the user **explicitly** clicks delete in the history browser).

**Project status**: Stable & in use. 130 backend + 17 remote-daemon + 3 frontend pure-function tests, strict tsc type-check, CI green. Current release **v2.9.6**, now covering **SSH remote mode** (aggregate local + remote-machine sessions in one window, #15/#17/#18/#20), **session status lights** (live busy/waiting/idle, #23), visible AskUserQuestion options / API errors (#21), single-key shortcuts + tab tear-off, and more. See [CHANGELOG](CHANGELOG.md) + [doc/ARCHITECTURE.md](doc/ARCHITECTURE.md).

---

## Features

### Real-time rendering
- Watches `~/.claude/projects/**/*.jsonl`; new lines appear in window within 200ms
- Multi-tab: one tab per active Claude session, title `[project] aiTitle`
- After a session exits, its tab is archived (grayed out), closable via `W`
- **Tab in independent window** (issue #10): right-click a tab → "Open in new window" / `N`, **or just drag the tab below the tab bar and drop** (tear-off), mirrors the session into a standalone read-only window (dual-monitor / long-running tasks), synced live with the main window
- **Session status lights** (issue #23): each local tab's status dot reflects Claude's live state — 🟢 running / 🟡 waiting for your decision (permission / dialog, breathing blink) / 🔴 done, awaiting input; the agents expander gives each subagent its own light

### SSH remote mode (issue #15)
- Aggregate local + remote-machine (NanoPi / any Linux / WSL) Claude sessions in **one window**; remote tabs are prefixed `[host]`
- A remote daemon streams sessions back over SSH live; **auto-reconnect** on drop (exponential backoff 2→30s, #17), seq-dedup catch-up on reconnect
- Remote tabs can also ↗ raise their terminal (#18)
- Deployment: [doc/REMOTE-PHASE0-DEPLOY.md](doc/REMOTE-PHASE0-DEPLOY.md)

### Rich rendering
- **Markdown**: GFM + tables + task lists (marked.js)
- **LaTeX**: `$...$` inline, `$$...$$` block (KaTeX)
- **Syntax highlighting**: 30+ common languages (highlight.js/common)
- **Tool calls**: `tool_use` + `tool_result` merged into one collapsible card; long output gets a nested second-level collapse
- **subagent**: `Task` / `Agent` tool calls auto-embed the sub-JSONL (lazy-loaded)
- **/compact summary**: shown collapsed
- **Code copy**: top-right "copy" button on every code block

### History browser
- Toolbar `◷` button / `H` to toggle; grouped by working directory
- Project groups **collapsed by default**; expand triggers **lazy load** of all sessions in that project
- **Full-text search** (issue #6): a "full-text" mode searches message content across all sessions; hits highlighted, click to jump into the read-only viewer and locate; optional "include tool content", filter by scope/time
- Per-row actions: `★/☆` star, `✎` rename (Chinese supported), `–/+` hide, `↺` resume (v2.8.1: `cc --resume` in a new **PowerShell** window, falls back to `claude`; loads your profile so proxy/env apply), `✕` delete (confirm twice; jsonl actually removed)
- Clicking a session opens a **read-only viewer**

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
| **N** | Open current tab in an independent window (issue #10; or drag the tab out below the tab bar) |
| **T** | Toggle Task panel |
| **Esc** | Close topmost overlay (read-only viewer → history view → settings panel) |

> **Defaults are all single keys** — cc-monitor is a read-only monitor window, no modifier needed. When an editable field (search / rename input) is focused, shortcuts yield to typing. Every chord is customizable in Settings → Shortcuts; two behavior/panel toggles are unbound by default and can be assigned a key there.

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
| [doc/REMOTE-PHASE0-DEPLOY.md](doc/REMOTE-PHASE0-DEPLOY.md) | Remote deployers | SSH remote daemon manual deploy runbook (issue #15) |
| [doc/INVARIANTS.md](doc/INVARIANTS.md) | Everyone | Global invariants (zero intrusion / encoding / ACL / ordering) |
| [doc/CONTRIBUTING.md](doc/CONTRIBUTING.md) | Contributors | Checklists + cookbook (add IPC / jsonl type / setting / hotkey) |
| [doc/DEVELOPMENT.md](doc/DEVELOPMENT.md) | Developers | Dev environment + port conflict / debugging |
| [doc/BUILDING.md](doc/BUILDING.md) | Releasers | Production build + packaging + Code Signing |
| [CHANGELOG.md](CHANGELOG.md) | Upgrade users | Version history |

English translations of these docs are not yet complete. Contributions welcome.

---

## License

[MIT](LICENSE) © 2026 cc-monitor contributors
