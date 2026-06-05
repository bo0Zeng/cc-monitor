# cc-monitor-remote (Phase-0 prototype)

Prototype daemon for the cc-monitor SSH-remote feature (issue #15).

- **Linux-only at runtime.** It tails `~/.claude` session JSONL files and
  streams them to a connected client. Step 2 adds the inotify-based watcher.
- **Built natively on the target** (e.g. a Raspberry Pi) — there is no
  cross-compile. Build it on the same box where it will run.
- **Standalone crate.** It is intentionally *not* part of a Cargo workspace and
  is not referenced by any root `Cargo.toml`, so the Windows/Tauri CI never
  tries to compile it.

The full build-on-Pi + `scp` deploy runbook lands in Step 7.

## Wire protocol

One UTF-8 JSON object per line, terminated by `\n`, no bare `\n`/`\r` inside the
object. See `src/wire.rs` for the `Frame` types (`hello`, `line`,
`session_added`, `session_removed`).
