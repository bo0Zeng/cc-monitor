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
mod resolve_query;
mod search_query;
mod turn_detect;
mod usage_query;
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
/// is stale (staleness 提示 + 部署确认)。
///
/// **与 F66 `capabilities` 两轴正交（§26）**：`build_id` = daemon 的**身份/构建版本**
/// （改了 daemon 二进制就该 bump，用于 staleness + 部署确认）；`capabilities` = 该版本
/// **声明支持什么能力**（用于运行时门控发哪些 flag）。两者不混——bump build_id 是
/// 「我是新构建」，声明 capability 是「我这个构建支持 X」。
///
/// **★ F66 待 bump（发版前一套动作，Phase G 记账）**：F66 给 daemon 加了 `capabilities`
/// 声明（wire.rs Hello + 下面的 `CAPABILITIES`），是新构建 → **发版前应 bump 到 p1i-xxx**。
/// 但 bump **必须与 re-zigbuild 内嵌二进制 + 更新 `embedded-daemons/*.build_id` 清单一套做**
/// （只 bump 源码不 re-embed = 源码 build_id 与内嵌清单不一致的半 bump，更糟）。当前 p1h
/// 不 bump **良性**：monitor 乐观路径照发 flag、旧内嵌二进制自 F24/F25 起就剥
/// `--with-bg`/`--tail-only`，不死循环、不降级（Phase G 双 agent 核实）。
/// - p1a-history  = 一次性历史查询模式（#16，--list-projects 等）
/// - p1b-overflow = + 探活精确化（#34）+ overflow 信号（#32）
/// - p1c-f20-addtime = + add-time 冒名判定（Batch5-F20：pidfile procStart 身份
///   比对为主证据，mtime 时间证据 + cmdline 白名单为 fallback，修 tmux 僵尸 tab）
/// - p1d-lifecycle = + kind 交互性门（Batch6-F21：kind:"bg" 后台任务不成 tab）
///   + sid 原地变更 removed + 同 sid 多 PID 引用计数（Batch6-F22）
/// - p1e-bg-tree = + --with-bg 放行 bg 会话、session_added 帧附 session_kind/cwd/name
///   （Batch7-F24，additive 向后兼容）
/// - p1f-tail-snapshot = + --tail-only（连接不重放历史，seq=行号语义）、
///   session_added 帧附 path（Batch8-F25，历史改走 monitor 旁路 --read-session 快照）
/// - p1g-status-tail = + session_status 帧/宣告带初始 status（Batch9-F27 远端红绿灯）、
///   --read-session-tail 尾部优先查询（Batch9-F30）
/// - p1h-bg-badge = --list-sessions 输出附 isBg（记录级 sessionKind:"bg" 探测，
///   Batch11-F32 历史 ⚙ 徽标；additive，查询字段无版本门控问题）
/// - p1i-line-offset = Line 帧附 `byte_offset`（daemon-01/gap#2，累计原始字节、逐字节对齐 aterm
///   `LineFramer`：计 CRLF `\r`、含 `\n`、残行不计；给 offset 续拉/截断检测。additive、不 bump
///   PROTO_VERSION——旧 client 忽略、旧 daemon 缺字段 client 得 0）
/// - p1j-offset-resume = + `--read-session-from-offset <path> <offset>` 一次性查询（daemon-02/
///   Phase 1）：从字节 offset 透传 [offset,EOF] = aterm `tail -c +(offset+1)`；配 p1i 的
///   `byte_offset` 做重连/断线 offset 续拉。additive 子命令（旧 daemon 报 unknown arg、client 降级）
/// - p1k-resolve-rpc = + `--resolve` advisor RPC（daemon-04/Phase 1）：读 stdin ResumeSpec JSON →
///   出 stdout CommandPlan JSON（camelCase，caps 复用 aterm `SessionCapabilities` 4 名），错误
///   exit2+stderr `{code,message}`。契约与 aterm cc-bus 对齐定死。additive 子命令、advisory 零 handle
/// - p1l-audit-fixes = daemon Phase 1 三视角代码审查修复（daemon-05）：一次性查询模式不再向 stderr
///   打 info（`--resolve` 错误信封 stderr 纯 `{code,message}`）；resolve base(launchCandidate) 补
///   shell-safe 校验（B2 对称化，新错误码 `unsafe_launch_candidate`）；stdin `.take(1MiB)` 兜 DoS。
///   纯查询/流协议 wire 不变——非破坏、无 PROTO_VERSION bump。
/// - p1m-hello-emits = phase② 联调（daemon-08）：Hello 加 `emits:[帧 kind]`（additive，与 capabilities
///   正交、不受 §26）——aterm 门控消费。现声明 line/session_added/session_status/session_removed/
///   overflow；turn_end 待其帧接线后加。additive、无 PROTO_VERSION bump。
/// - p1n-turn-end = phase② 联调（daemon-09）：`process_jsonl` 每见 turn-end 记录发 `Frame::TurnEnd
///   {sid,uuid}`（raw-per-record、方案 C 不 dedup；判词 `turn_detect` 对拍 aterm TurnDetector）；
///   `turn_end` 加进 EMITS。dedup 视界在 aterm rolling+debounce baselineByPath。additive、无 bump。
const BUILD_ID: &str = "p1n-turn-end";

/// F66（#58③）：本构建**声明支持的能力 token**（hello 帧 `capabilities` 字段）。
/// monitor 按此决定发 `--with-bg`/`--tail-only`，不再靠 build_id 精确匹配去猜
/// （闭合 2026-07-09「漏拷身份清单 → 确认不了 → 全降级」事故：能力由 daemon 自己
/// 声明，即使清单丢失也照开）。
///
/// **加法式，两轴正交**：加新能力就往这里加 token（旧 monitor 忽略未知 token）；
/// **绝不为此 bump `PROTO_VERSION`**（那是破坏性变更专用，会把每台旧 daemon 误判
/// Incompatible）。build_id 继续管 staleness / 重部署提示，与能力正交。
///
/// **§26 死循环护栏（硬约束）**：只声明本 daemon **会在一次性查询判定前剥离对应
/// flag** 的能力——即每个 token 必须有 `split_stream_flags`（`:76`）里对应的剥离分支。
/// `bg`→`--with-bg`、`tail-only`→`--tail-only`，二者 `split_stream_flags` 都剥。
/// 加新能力 token 时，必须同时给它的 flag 加剥离分支，否则声明它 = 埋死循环
/// （monitor 发对应 flag → 本 daemon 不剥 → 当查询退出 → 无 hello → 重连死循环）。
/// **此硬约束由 `every_capability_token_is_strippable` 测试代码强制**（不再只是约定）。
const CAPABILITIES: &[&str] = &["bg", "tail-only"];

/// phase②（daemon-08）：本 daemon **会发射的帧 kind 集**（snake_case），填进 `Hello.emits`——
/// aterm 门控消费（emits 含 kind → 依赖该帧；不含 → 回退 β/watchdog）。**与 `CAPABILITIES` 正交**：
/// emits 是纯发射声明、无对应流 flag、不受 §26 护栏（见 `wire.rs` Hello.emits）。`turn_end` 待其帧
/// 发射接线（daemon-08+）后加入——**在此登记 = 承诺 daemon 真发该帧**，勿提前声明未接线的帧。
const EMITS: &[&str] = &[
    "line",
    "session_added",
    "session_status",
    "session_removed",
    "overflow",
    "turn_end", // daemon-09：process_jsonl 已发 TurnEnd（登记=承诺真发，已接线）
];

/// Batch7-F24/Batch8-F25：从 argv 剥离流模式 flag（`--with-bg` / `--tail-only`），
/// 返回（剩余参数, with_bg, tail_only）。**必须在一次性查询模式判定之前调用**
/// （INVARIANT §26：flag 落进 query 分支 → daemon 打印查询结果退出 → monitor
/// 无 hello 死循环）。
fn split_stream_flags(mut args: Vec<String>) -> (Vec<String>, bool, bool) {
    let with_bg = args.iter().any(|a| a == "--with-bg");
    let tail_only = args.iter().any(|a| a == "--tail-only");
    args.retain(|a| a != "--with-bg" && a != "--tail-only");
    (args, with_bg, tail_only)
}

// 本测块紧邻被测的 split_stream_flags（就近可读）、不挪文件尾；显式 allow 让 clippy
// --all-targets 净（审计：门槛此前只跑默认 target、漏 test-target lint）。
#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod stream_flag_tests {
    use super::split_stream_flags;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    /// F66（#58③）★ §26 死循环护栏的**代码强制**：`CAPABILITIES` 里每个能力 token 的
    /// CLI flag 都必须被 `split_stream_flags` 剥离——否则声明它 = 埋 monitor 侧死循环
    /// （monitor 发该 flag → 本 daemon 不剥 → 当一次性查询退出 → 无 hello → 重连死循环）。
    /// 加新能力 token 时，若忘了在此登记它的 flag、或忘了给 `split_stream_flags` 加剥离
    /// 分支，本测试红。把审计指出的「约定强制」拉回「代码强制」。
    #[test]
    fn every_capability_token_is_strippable() {
        // token → 它对应的 CLI flag（加新能力时同步扩这张表）
        fn flag_of(token: &str) -> &'static str {
            match token {
                "bg" => "--with-bg",
                "tail-only" => "--tail-only",
                other => panic!(
                    "CAPABILITIES 声明了 token `{other}` 但此处无 flag 映射——加新能力必须在此登记它的 flag 并确认 split_stream_flags 剥离它（否则埋 §26 死循环）"
                ),
            }
        }
        for &token in super::CAPABILITIES {
            let flag = flag_of(token);
            let (rest, _, _) = split_stream_flags(v(&[flag]));
            assert!(
                rest.is_empty(),
                "能力 token `{token}` 的 flag `{flag}` 未被 split_stream_flags 剥离 → §26 死循环"
            );
        }
    }

    /// F25 DoD ③：流模式 flag 剥离后不残留（不会误入查询模式判定）。
    #[test]
    fn flags_are_stripped_and_detected() {
        let (rest, bg, tail) = split_stream_flags(v(&["--with-bg", "--tail-only"]));
        assert!(
            rest.is_empty(),
            "剥净 → 流模式（!args.is_empty() 为 false）"
        );
        assert!(bg);
        assert!(tail);
        let (rest, bg, tail) = split_stream_flags(v(&["--tail-only"]));
        assert!(rest.is_empty());
        assert!(!bg);
        assert!(tail);
        let (rest, bg, tail) = split_stream_flags(v(&[]));
        assert!(rest.is_empty());
        assert!(!bg);
        assert!(!tail);
    }

    /// 查询参数与流 flag 互不干扰：查询参数原样保留（顺带守住"flag 混进查询
    /// 命令行也不会破坏查询"的边角）。
    #[test]
    fn query_args_pass_through() {
        let (rest, bg, tail) =
            split_stream_flags(v(&["--read-session", "/p/s.jsonl", "--with-bg"]));
        assert_eq!(rest, v(&["--read-session", "/p/s.jsonl"]));
        assert!(bg);
        assert!(!tail);
    }
}

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

    // issue #16：带参数 = 一次性历史查询模式，干完即退，不进流式协议。
    // 旧 daemon 不认参数会照常发 hello 进流模式——monitor 以"首行是 hello 帧"
    // 识别旧版并提示升级（优雅降级，无协议版本协商负担）。
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Batch7-F24/Batch8-F25：流模式 flag 集合，先剥离再判一次性查询模式
    // （否则误入 query 分支——INVARIANT §26）。纯函数化供单测（审计 D）。
    let (args_rest, with_bg, tail_only) = split_stream_flags(args);
    let args = args_rest;
    if !args.is_empty() {
        // 一次性查询模式：--search 全文搜索（#28）/ --usage 用量聚合（F88a-remote）/
        // --resolve advisor（daemon-04，读 stdin ResumeSpec→stdout CommandPlan），其余走历史查询（#16）。
        let code = match args.first().map(String::as_str) {
            Some("--search") => search_query::run(&claude_dir, &args),
            Some("--usage") => usage_query::run(&claude_dir, &args),
            Some("--resolve") => resolve_query::run(&claude_dir, &args),
            _ => history_query::run(&claude_dir, &args),
        };
        std::process::exit(code);
    }

    // 一次性查询已 exit；到此必是流模式。claude_dir 日志放此（审计 correctness-重要①：
    // --resolve/一次性查询模式 stderr 只承载结构化错误/查询结果，不掺 info——兑现协议 v1 §3
    // 「错误 exit2 + stderr 纯 {code,message} JSON」，客户端可整段 JSON-parse stderr）。
    tracing::info!("claude_dir = {}", claude_dir.display());

    // (b) Emit the Hello handshake FIRST, flushed, before anything else.
    let mut stdout = BufWriter::new(tokio::io::stdout());
    let hello = Frame::Hello {
        v: PROTO_VERSION,
        build_id: BUILD_ID.to_string(),
        host_arch: std::env::consts::ARCH.to_string(),
        claude_dir: claude_dir.to_string_lossy().into_owned(),
        capabilities: CAPABILITIES.iter().map(|s| s.to_string()).collect(),
        emits: EMITS.iter().map(|s| s.to_string()).collect(),
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
    let rx = watcher::spawn(claude_dir, with_bg, tail_only);

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
