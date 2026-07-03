//! Phase-0 SSH-remote daemon prototype.
//!
//! The remote half of the steel thread: it resolves `~/.claude`, emits a single
//! `Hello` frame, then tails session JSONL files and streams `line` /
//! `session_added` / `session_removed` frames as one JSON object per line on
//! stdout. The client end is the cc-monitor Tauri app over an SSH pipe.
//!
//! Runtime target is Linux (inotify); the code is cross-platform and compiles +
//! runs a basic file-watch smoke on Windows (`notify` is portable).
//!
//! ## Two-task design (the §5.4 slow-consumer guard)
//!
//! - The **reader** ([`watcher::spawn`]) runs the filesystem watcher on a
//!   blocking thread and pushes frames into a *bounded* channel with `try_send`
//!   (never blocking the inotify callback).
//! - The **writer** (this file's [`writer_task`]) drains the channel and writes
//!   wire lines to stdout. A slow SSH pipe back-pressures the channel — and the
//!   bound stops the back-pressure at the channel, so it never reaches the
//!   inotify reader. This split is the single most-cited Phase-0 accident
//!   source; keeping it real is the point.

mod history_query;
mod search_query;
mod watcher;
mod wire;

use std::path::PathBuf;
use tokio::io::{AsyncWriteExt, BufWriter};
use wire::{to_line, Frame};

/// Streaming wire-protocol major version, reported as `v` in the `Hello` frame.
/// Bump ONLY on a breaking wire change; additive forward-compatible frame kinds
/// (e.g. `Overflow`, #32) do NOT bump it — old parsers skip unknown kinds.
/// The monitor negotiates against its own `EXPECTED_PROTO_V` (#33).
const PROTO_VERSION: u32 = 1;

/// Daemon build id reported in the `Hello` frame (#33 version negotiation).
/// Human-readable, monotonic build/feature tag; the monitor compares it against
/// `EXPECTED_DAEMON_BUILD_ID` and warns the user when a manually-deployed daemon
/// is stale. Bump when daemon capabilities change.
/// - p1a-history  = 一次性历史查询模式（#16，--list-projects 等）
/// - p1b-overflow = + 探活精确化（#34）+ overflow 信号（#32）
/// - p1c-f20-addtime = + add-time 冒名判定（Batch5-F20：pidfile procStart 身份
///   比对为主证据，mtime 时间证据 + cmdline 白名单为 fallback，修 tmux 僵尸 tab）
const BUILD_ID: &str = "p1c-f20-addtime";

#[tokio::main]
async fn main() {
    // Log to stderr so it never corrupts the stdout wire stream.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let claude_dir = resolve_claude_dir();
    tracing::info!("claude_dir = {}", claude_dir.display());

    // issue #16：带参数 = 一次性历史查询模式，干完即退，不进流式协议。
    // 旧 daemon 不认参数会照常发 hello 进流模式——monitor 以"首行是 hello 帧"
    // 识别旧版并提示升级（优雅降级，无协议版本协商负担）。
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        // 一次性查询模式：--search 走全文搜索（#28），其余走历史查询（#16）。
        let code = match args.first().map(String::as_str) {
            Some("--search") => search_query::run(&claude_dir, &args),
            _ => history_query::run(&claude_dir, &args),
        };
        std::process::exit(code);
    }

    // (b) Emit the Hello handshake FIRST, flushed, before anything else.
    let mut stdout = BufWriter::new(tokio::io::stdout());
    let hello = Frame::Hello {
        v: PROTO_VERSION,
        build_id: BUILD_ID.to_string(),
        host_arch: std::env::consts::ARCH.to_string(),
        claude_dir: claude_dir.to_string_lossy().into_owned(),
    };
    if let Err(e) = write_frame(&mut stdout, &hello).await {
        tracing::error!("failed to write hello frame: {e}");
        return;
    }
    if let Err(e) = stdout.flush().await {
        tracing::error!("failed to flush hello frame: {e}");
        return;
    }

    // (c) Start the watcher reader; it returns the receiving half of the
    // bounded frame channel.
    let rx = watcher::spawn(claude_dir);

    // (d) Run the stdout writer until the channel closes or a signal fires.
    tokio::select! {
        _ = writer_task(stdout, rx) => {
            tracing::info!("writer task ended (channel closed)");
        }
        _ = shutdown_signal() => {
            tracing::info!("shutdown signal received; exiting");
        }
    }
}

/// The stdout writer half of the §5.4 split: drain frames and write one wire
/// line each, flushing per frame so a connected client sees them promptly.
///
/// Awaiting `recv()` here is what back-pressures the bounded channel when the
/// SSH pipe is slow; that back-pressure never reaches the inotify reader.
async fn writer_task<W: tokio::io::AsyncWrite + Unpin>(
    mut out: W,
    mut rx: tokio::sync::mpsc::Receiver<Frame>,
) {
    while let Some(frame) = rx.recv().await {
        if let Err(e) = write_frame(&mut out, &frame).await {
            // Broken pipe (client gone) is the normal end-of-life; stop quietly.
            tracing::warn!("stdout write failed ({e}); stopping writer");
            return;
        }
        if let Err(e) = out.flush().await {
            tracing::warn!("stdout flush failed ({e}); stopping writer");
            return;
        }
    }
}

/// Serialize one frame to its wire line and write it (no flush).
async fn write_frame<W: tokio::io::AsyncWrite + Unpin>(
    out: &mut W,
    frame: &Frame,
) -> std::io::Result<()> {
    let line =
        to_line(frame).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    out.write_all(line.as_bytes()).await
}

/// Resolve the Claude config directory:
/// `$CLAUDE_CONFIG_DIR` if set, else `$HOME/.claude`.
///
/// On Windows (compile/smoke only — the real target is Linux) fall back to
/// `%USERPROFILE%\.claude` when `$HOME` is unset, and finally to `.claude` in
/// the cwd so the binary still starts for a smoke test.
fn resolve_claude_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".claude");
    }
    #[cfg(windows)]
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(profile).join(".claude");
    }
    PathBuf::from(".claude")
}

/// Resolve when a SIGTERM or SIGINT (Ctrl-C) is received, for clean shutdown.
///
/// On Unix this listens for both SIGTERM and SIGINT; on other platforms it
/// falls back to Ctrl-C only (sufficient for the Windows smoke).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("failed to install SIGTERM handler: {e}");
                // Fall back to Ctrl-C only so we still shut down on SIGINT.
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        let mut sigint = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("failed to install SIGINT handler: {e}");
                let _ = sigterm.recv().await;
                return;
            }
        };
        tokio::select! {
            _ = sigterm.recv() => {}
            _ = sigint.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
