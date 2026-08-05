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
//! - The **reader** ([`observe::watcher::spawn`]) runs the filesystem watcher on a
//!   blocking thread and pushes frames into a *bounded* channel with `try_send`
//!   (never blocking the inotify callback).
//! - The **writer** (this file's [`writer_task`]) drains the channel and writes
//!   wire lines to stdout. A slow SSH pipe back-pressures the channel — and the
//!   bound stops the back-pressure at the channel, so it never reaches the
//!   inotify reader. This split is the single most-cited Phase-0 accident
//!   source; keeping it real is the point.

mod build_id_guard; // E77：加了子命令必须 bump BUILD_ID（内部整体 #[cfg(test)]，生产构建为空）
mod common; // U2：两边都要、又不含平台原语的纯工具（§0.5-6 打掉了「三分够用」那个判断）
mod control; // U3：控制面 —— 会改变世界（写盘 / 改 tmux server / 发信号），或产出改变世界的计划
#[cfg(test)]
mod guard_support; // U-1：各条源码扫描型守卫共用的「只留生产段」剥法（仅测试构建）
mod inbound; // U6b-1：流连接上的入方向（信封 / 分派 / 取消）
mod layering_guard; // U3：§1.1 第二条解耦线的机器判据（observe↔control 方向与条数）
mod no_timer_guard; // P6：零定时器护栏（内部整体 #[cfg(test)]，生产构建为空）
mod observe; // U3：观测面 —— 读，不改变世界
mod platform; // U2：唯一允许平台原语与平台 cfg 的层（§1.1 第一条解耦线）
mod protocol_doc_guard; // U6a：IPC-PROTOCOL.md 与真实协议面的对拍
mod readonly_guard; // F08a：daemon 只读机器护栏（内部整体 #[cfg(test)]，生产构建为空）
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
/// - p1o-codex-dg = Phase 2D Codex 泛化（DG3 wire additive agent_kind/liveness_confidence/codex_dir/kinds、
///   DG4 turn-end 检测器、DG5 `--usage` per-kind、DG6 resume）。全 additive、**不 bump PROTO_VERSION**；
///   bump BUILD_ID 给含 DG3-6 的 daemon 独立身份（Phase G 审计 I2：防"同 id 不同内容"静默陈旧）。
///   Codex live 监视/判活（DG1/DG2）暂停、未接线。
/// - p1p-tmux-frame = B2：watch_loop 周期本机 `tmux ls` 发 `TmuxSessions` 帧（+EMITS "tmux_sessions"），
///   替 monitor 每 8s 新建 SSH 跑 tmux ls 的对账刷屏。additive、**不 bump PROTO_VERSION**。
/// - p1q-accounts = A2：多账号只读三命令 `--list-accounts` / `--session-accounts` /
///   `--account-trust`（cc-acct-iso manifest 的消费侧；账号=一个 CLAUDE_CONFIG_DIR）。
///   纯一次性查询、零写入、不 shell out；**不动** PROTO_VERSION / CAPABILITIES / EMITS。
///   bump BUILD_ID 只为给"含账号命令"的 daemon 独立身份，旧版遇到新命令会
///   `unknown argument` exit 2，monitor 侧按"功能不可用"优雅降级。
/// - p1r-event-liveness = zero-poll-liveness P0-P6：判活信号全部换成内核事件
///   （pidfile inotify + pidfd 看进程死 · socket 目录 inotify 看 server 生死复活 ·
///   tmux hook → `--tmux-notify` → SIGUSR1 看会话开关），两条轮询（判活 2s tick /
///   tmux 8s tick）都已删除，生产段零定时器（`no_timer_guard.rs` 钉住）。
///   wire 两处 additive、**不 bump PROTO_VERSION**：`TmuxSessions` 加
///   `observation`（有会话时省略 ⇒ 载荷逐字节不变）+ 新帧 `TmuxSessionClosed`（进 EMITS）。
///   **bump BUILD_ID 是必须的**：旧 daemon 报同一个 id 就不会被判 stale、不自动重装，
///   整轮改动会在已部署的远端**休眠**（本条正是 P1 记档里点名、P5 漏做、P7 补上的那次 bump）。
/// - p1t-removal-cause = **修 v3.4.0 发出去的一个真 bug**：`--account-trust-zero`
///   在 `accounts_query.rs` 里实现完整，但本文件的 match 漏列它 ⇒ 落进 `_` 臂走历史查询
///   ⇒ 回 `unknown argument` + exit 2，而 monitor 的账号 0 信任预检**真的在发这条命令**。
///   **必须 bump**：不 bump 的话已部署的 v3.4.0 daemon 不被判 stale、不会自动换掉，
///   修了也到不了用户手上（P5 漏做、P7 补上的那一课）。
/// - p1u-fork-session = **G2/G6（branch-anywhere）新增 `--fork-session`**：daemon 第一次
///   有写盘能力（`fork_write.rs`，`readonly_guard` 两层白名单只放行它一个模块）。
///   **必须 bump**：monitor 侧的远端分叉命令要靠这个 id 判 stale 才会自动重装；不 bump
///   的话已部署的 daemon 报同一个 id ⇒ 不判 stale ⇒ 不重装 ⇒ 用户点远端 `⑂` 永远只拿到
///   「版本过旧，请重新部署」。**这条是 Phase G 审计当场抓出来的** —— 上面 p1r/p1t 两段
///   逐字写着这课，本轮仍然漏了，说明「加子命令」这一步该有机检而不是靠记性（登记 E77）。
/// - p1v-attachable = **E73**：`SessionAdded` 帧 additive 加 `attachable`（来自 pidfile 的同名布尔）。
///   `session_kind` 此前把两件事压在一个轴上 —— ①「该不该在 UI 出现」②「attach 进去对人有没有
///   意义」。SDK / 脚本驱动的会话正好「①要②不要」：它**有** tmux、`@ccm_sid` 也对，但
///   `stdin=DEVNULL`，用户敲的字会被脚本吃掉。省略 = true（存量零迁移）。
///   **必须 bump**：monitor 要靠新 daemon 才拿得到这个字段；不 bump 就不判 stale、不重装。
///   （wire 是 additive、旧 monitor 忽略未知字段 ⇒ **不 bump PROTO_VERSION**。）
const BUILD_ID: &str = "p1v-attachable";

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
    "turn_end",      // daemon-09：process_jsonl 已发 TurnEnd（登记=承诺真发，已接线）
    "tmux_sessions", // B2：watch_loop 周期本地 tmux ls 发 TmuxSessions（登记=承诺真发，已接线）
    // P5：与上一份快照差分算出的**正向死亡帧**。登记 = 承诺真发（已接线，见 watcher.rs
    // 的 `diff_closed`）。monitor 收到即 retire、绕过 miss 计数；旧 monitor 忽略未知 kind。
    "tmux_session_closed",
];

// U6b-2 **argv 三分表**：daemon 认识的每个 `--token` 恰好属于其中一类。
//
// # 为什么要有这张表
//
// 在它之前是**二分**：剥掉流 flag，剩下非空就当一次性查询。后果实测：
//
// ```text
// $ cc-monitor-remote --some-future-flag
// cc-monitor-remote query error: unknown argument: --some-future-flag
// rc=2
// ```
//
// **未知 flag 在流位置 ⇒ exit 2、一个字节都不输出、没有 hello。** monitor 那头看到的
// 和「daemon 崩了」无法区分 ⇒ 重连 ⇒ 发同一个 flag ⇒ **死循环**。这正是 2026-07-09
// 事故的形状。`every_capability_token_is_strippable` 挡不住它——那条只覆盖**与已声明
// 能力绑定**的 flag，「monitor 因为别的原因发了个新 flag」不在它的判据里。
//
// # 这张表**漏一项**的后果比旧行为更糟，所以必须有完备性机检
//
// 判据改成「`args[0]` ∈ [`SUBCOMMANDS`] ⇒ 查询模式」之后，漏登记一条子命令不再是
// exit 2（吵，但看得见），而是**那条子命令静默变成「起了个流」**——调用方拿到一堆
// jsonl 行而不是查询结果。这是 v3.4.0 `--account-trust-zero` 漏登记那次事故的**加强版**。
// ⇒ `every_dispatched_token_is_classified` 是本组的核心交付，不是附属品。

/// ① 流模式 flag：出现即剥离并置位，**不影响模式判定**。
const STREAM_FLAGS: &[&str] = &["--with-bg", "--tail-only"];

/// ② 一次性查询子命令：**只有 `args[0]` 是其中之一才进查询模式**。
const SUBCOMMANDS: &[&str] = &[
    "--account-trust",
    "--account-trust-zero",
    "--fork-session",
    "--list-accounts",
    "--list-projects",
    "--list-sessions",
    "--read-session",
    "--read-session-from-offset",
    "--read-session-tail",
    "--resolve",
    "--search",
    "--session-accounts",
    "--tmux-notify",
    "--usage",
];

/// ③ 子命令自己的选项：只在某条 [`SUBCOMMANDS`] 之后才有意义，daemon 顶层不解释它们。
const SUBCOMMAND_OPTIONS: &[&str] = &[
    "--accts-dir",
    "--after-ms",
    "--include-tools",
    "--limit",
    "--scope",
];

/// 从 argv 剥离流模式 flag，返回（剩余参数, with_bg, tail_only）。
///
/// **必须在一次性查询模式判定之前调用**（INVARIANT §26）。
fn split_stream_flags(mut args: Vec<String>) -> (Vec<String>, bool, bool) {
    let with_bg = args.iter().any(|a| a == "--with-bg");
    let tail_only = args.iter().any(|a| a == "--tail-only");
    args.retain(|a| !STREAM_FLAGS.contains(&a.as_str()));
    (args, with_bg, tail_only)
}

/// 剥完流 flag 之后：这些参数该进查询模式，还是该进流模式？
///
/// 返回 `true` = 一次性查询。判据是 **`args[0]` 是不是一条已登记的子命令**，
/// 不再是「非空即查询」。
///
/// # 未知 `--flag` 忽略，未知**裸参数**仍报错
///
/// 两者要分开：
/// - 未知 `--flag`：可能是**新版 monitor 发给旧版 daemon** 的。忽略它 + 一行 warn，
///   照常进流模式发 hello ⇒ monitor 拿得到握手、能看出对面旧、可以降级。
///   这条不能追溯修好**已经部署**的旧 daemon，但它让**下一个** flag 的新增是安全的。
/// - 未知裸参数（不以 `--` 开头）：那是明确的调用错误。任何未来协议都不会把裸参数放 `args[0]`，
///   静默吞掉只会让人查半天。**仍旧落进查询分支报 `unknown argument` + exit 2。**
fn is_query_mode(args: &[String]) -> bool {
    match args.first() {
        None => false,
        Some(first) => {
            if SUBCOMMANDS.contains(&first.as_str()) {
                return true;
            }
            // 不是子命令：这两种都算**明确的调用错误**，交给查询分支报 unknown argument + exit 2。
            //
            // - 还有任何一个**裸参数**（不以 `--` 开头）：任何未来协议都不会这么发。
            // - `args[0]` 是个**子命令选项**（如 `--scope`）：选项脱离了它的子命令，
            //   静默当成起流会让调用方拿到一堆 jsonl 行还以为查询成功了。
            if args.iter().any(|a| !a.starts_with("--"))
                || SUBCOMMAND_OPTIONS.contains(&first.as_str())
            {
                return true;
            }
            for a in args {
                tracing::warn!(
                    "未知 flag {a}：本 daemon 不认识它，已忽略并照常进流模式。\
                     （若这是新版 monitor 的新能力，请升级 daemon。）"
                );
            }
            false
        }
    }
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
    if is_query_mode(&args) {
        // 一次性查询模式：--search 全文搜索（#28）/ --usage 用量聚合（F88a-remote）/
        // --resolve advisor（daemon-04，读 stdin ResumeSpec→stdout CommandPlan），其余走历史查询（#16）。
        let code = match args.first().map(String::as_str) {
            // P4b：hook 子进程走这条 —— 校验身份后给 daemon 发 SIGUSR1，**不碰文件系统**。
            Some("--tmux-notify") => control::tmux_hook::notify(&args),
            Some("--search") => observe::search_query::run(&claude_dir, &args),
            Some("--usage") => observe::usage_query::run(&claude_dir, &args),
            Some("--resolve") => control::resolve_query::run(&claude_dir, &args),
            // G2（branch-anywhere）：从指定消息处分叉出一个新会话文件。
            // **daemon 唯一的写盘入口**，护栏白名单层单独盯着它（readonly_guard）。
            Some("--fork-session") => control::fork_write::run(&claude_dir, &args),
            // ★ 这几个字面量必须与 `observe::accounts_query::run` 自己认的子命令**完全一致**。
            // v3.4.0 出过一次事故：`--account-trust-zero` 在 accounts_query 里实现完整，
            // 但这里漏列 ⇒ 落进下面的 `_` 臂走历史查询 ⇒ `unknown argument` + exit 2，
            // 而 monitor 的账号 0 路径**真的在发这条命令**。
            // 测试当时抓不到，是因为它们直接调 `observe::accounts_query::run`、**绕过了本处调度**。
            // 现由 `observe::accounts_query::tests::main_dispatches_every_subcommand_we_handle` 钉住。
            Some("--list-accounts")
            | Some("--session-accounts")
            | Some("--account-trust")
            | Some("--account-trust-zero") => observe::accounts_query::run(&claude_dir, &args),
            _ => observe::history_query::run(&claude_dir, &args),
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
        // DG3 wire 面已建，但 Codex **发现**（DG1）未接线 → 现只服务 Claude：codex_dir=None、kinds 空
        // （skip → Hello 帧对 Claude 字节不变）。DG1 落地时翻成 Some(codex_dir)+["claude","codex"]。
        codex_dir: None,
        kinds: Vec::new(),
        capabilities: CAPABILITIES.iter().map(|s| s.to_string()).collect(),
        emits: EMITS.iter().map(|s| s.to_string()).collect(),
        commands: inbound::COMMANDS.iter().map(|s| s.to_string()).collect(),
    };
    // U6b-3：写 + flush 一步到位，**并拿到 `HelloFlushed` 见证**。
    // 那个见证是 `inbound::spawn` 的必填参数 ⇒「reader 抢在 Hello 之前起来」
    // 变成编译期不可表示（此前靠一条比较字节位置的机检，被普通函数抽取绕过）。
    let hello_flushed = match wire::write_and_flush_hello(&mut stdout, &hello).await {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("failed to write/flush hello frame: {e}");
            return;
        }
    };

    // (b2) U6b-1：**入方向 reader。位置不可上移。**
    //
    // ★ 必须在上面那个 Hello 已经 flush **之后**才起 —— 客户端要先读到 Hello 才知道
    // 对面是什么版本、有什么能力；在那之前就收命令，等于在能力协商之前执行它。
    // 这是**时序约束**，两边单看都合理、合起来才错（同 U6a 抓到的 PS 握手顺序那一族）。
    // U6b-3 起它**编译期不可表示**：`inbound::spawn` 要一个 `wire::HelloFlushed` 见证，
    // 而那个见证只能由 `write_and_flush_hello` 产出 —— 上一行拿到的就是它。
    // （此前是一条比较 `main.rs` 里两个字符串字节位置的机检，被一次普通的函数抽取绕过，已删。
    //  U8a-2a 顺带订正了本注释与 `doc/IPC-PROTOCOL.md` 里对那条已删机检的指名。）
    //
    // 应答走**独立通道**：出方向丢一帧可恢复（`Overflow` 会说丢了多少，行还在远端 jsonl），
    // 丢一条应答会让客户端永远等下去。混在一个通道里，实时行的洪峰会把应答挤掉。
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(inbound::REPLY_CHANNEL_CAPACITY);
    let inbound_task = inbound::spawn(tokio::io::stdin(), reply_tx.clone(), hello_flushed);

    // (c) Start the watcher reader; it returns the receiving half of the
    // bounded frame channel.
    let (rx, poke) = observe::watcher::spawn(claude_dir, with_bg, tail_only);

    // (c2) **P4：SIGUSR1 = 「tmux 那边有事，赶紧重探一次」。**
    //
    // ★ **这一步必须先于任何 hook 安装落地** —— `SIGUSR1` 的**默认处置是终止进程**。
    // 先装 hook 再装处理器，等于给一个会自杀的 daemon 装了自杀触发器。
    // 本轮（P4 daemon 侧）刻意只做这一半：没有 hook 在发信号，它完全惰性。
    //
    // 为什么是信号而不是别的：原方案让 hook 追加事件日志、daemon inotify 读增量，
    // **撞红线 I7「daemon 只读」**（`readonly_guard` 当场拦下）。信号通路让 daemon 的
    // 文件系统写归零，且会话名根本不经 shell ⇒ 那条引号/注入面直接消失。
    // 代价是信号无载荷且会合并 —— 靠「重探 + 与上一份快照差分」天然免疫。
    // P5：留一份给停机用（下面 select 结束后要显式通知 reader）。
    let poke_for_shutdown = poke.clone();
    #[cfg(unix)]
    let poke_task = tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigusr1 = match signal(SignalKind::user_defined1()) {
            Ok(s) => s,
            Err(e) => {
                // 装不上就退化成「只有 ticker 兜底」，**说出来**而不是静默降级。
                tracing::warn!("装不上 SIGUSR1 处理器（{e}）⇒ tmux hook 通路不可用，退回定时探测");
                return;
            }
        };
        tracing::info!("SIGUSR1 处理器已就位（tmux hook 通路的 daemon 侧）");
        while sigusr1.recv().await.is_some() {
            poke.poke();
        }
    });
    #[cfg(not(unix))]
    let poke_task = {
        let _ = poke;
        tokio::spawn(async {})
    };

    // (d) Run the stdout writer until the channel closes or a signal fires.
    tokio::select! {
        _ = writer_task(stdout, rx, reply_rx) => {
            tracing::info!("writer task ended (channel closed)");
        }
        _ = shutdown_signal() => {
            tracing::info!("shutdown signal received; exiting");
        }
    }
    // P5：**显式告诉 reader 停** —— 删掉 8s ticker 之后，reader 那边的
    // `sink.is_closed()` 复查再没有定期醒来的机会，只会一直阻塞在 `recv()`。
    // 漏这一句不会红任何测试（进程退出时线程随之消亡），所以它与删 ticker 是同一步。
    poke_for_shutdown.shutdown();
    poke_task.abort();
    inbound_task.abort();
    drop(reply_tx);

    // ★ **必须显式 exit，不能让 runtime 自然 drop。**
    //
    // `tokio::io::stdin()` 走的是**阻塞线程池**。`inbound_task.abort()` 只取消那个 async
    // task，**阻塞中的 `read(0)` 不受影响**；而 `#[tokio::main]` 展开出来的 runtime 在 drop
    // 时会等所有 blocking 任务结束 ⇒ 只要对端还开着 stdin，进程就永远停在这一行。
    //
    // D 审计实测（U6b-1 引入入方向之后，相对父提交的**回归**）：
    // stdin 接一条开着但没数据的 FIFO，发 SIGTERM ⇒ 3/3 复现「5s 后仍未退出」，
    // stderr 末行已经打了 "shutdown signal received; exiting" —— 清理跑完了，就是不退。
    // 父提交同一脚本 100ms 内退出。
    //
    // 爆炸半径正是生产形状：monitor 经 SSH exec 连着时 stdin 一直开着。
    // 远端手工 kill 一个卡住的 daemon、部署脚本替换在跑的二进制，今天都会失效。
    //
    // 为什么 exit 是安全的：**流模式 daemon 没有任何待落盘状态** —— 它只读；
    // 唯一的写盘入口 `control/fork_write.rs` 在一次性查询模式，那条路早就 exit 了。
    // stdout 也不欠 flush：`writer_task` 每帧写完即 flush。
    std::process::exit(0);
}

/// The stdout writer half of the §5.4 split: drain frames and write one wire
/// line each, flushing per frame so a connected client sees them promptly.
///
/// Awaiting `recv()` here is what back-pressures the bounded channel when the
/// SSH pipe is slow; that back-pressure never reaches the inotify reader.
/// 排空两条通道写 stdout。
///
/// **`biased` + 应答在前**：出方向的实时行可以有洪峰（`CHANNEL_CAPACITY` 是 10_000），
/// 公平轮询会让应答排在一万行后面，客户端那头看起来就是「命令没反应」。
/// 应答量极小（一条命令一两帧），优先它不会饿死行。
///
/// `reply_rx` 永远不会返回 `None`（`main` 自己留着一个 sender 到进程结束），
/// 所以这个 select 不会退化成 `None` 忙转。
async fn writer_task<W: tokio::io::AsyncWrite + Unpin>(
    mut out: W,
    mut rx: tokio::sync::mpsc::Receiver<Frame>,
    mut reply_rx: tokio::sync::mpsc::Receiver<Frame>,
) {
    // ★ 应答优先，但**有预算**。
    //
    // 第一版是无条件 `biased` + 应答在前，注释写着「应答量极小，优先它不会饿死行」——
    // **那个前提由不可信输入决定，不成立**。D 审计端到端实测（客户端只是往 stdin 灌
    // `{"id":"n","cmd":"n"}`）：500ms 内应答 70 789 条、出方向实时行 **4 条**，队列里一直排着。
    //
    // 机理是闭环的：读循环在应答通道满时阻塞（`send` 是 `.await`），于是通道**恒满**；
    // `biased` 每次都命中 `reply_rx`，`rx` 永远轮不到。生产里出方向是 10 000 容量，
    // 会先堆满再 `Overflow` 丢实时行 —— 一行命令就能让远端会话看起来「卡住」。
    //
    // 现在：连发 `REPLY_BURST` 条应答之后**强制让位一次**给出方向。
    // 仍然偏向应答（正常场景一条命令一两帧，够不到预算），但偏置是**有界**的。
    const REPLY_BURST: u32 = 8;
    let mut burst = 0u32;
    loop {
        let frame = if burst < REPLY_BURST {
            tokio::select! {
                biased;
                Some(f) = reply_rx.recv() => { burst += 1; f }
                f = rx.recv() => match f {
                    Some(f) => { burst = 0; f }
                    None => return, // 出方向通道关了 = 寿终
                },
            }
        } else {
            burst = 0;
            tokio::select! {
                biased;
                f = rx.recv() => match f {
                    Some(f) => f,
                    None => return,
                },
                Some(f) = reply_rx.recv() => f,
            }
        };
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

/// U6b-2：**argv 三分表的完备性与互斥**。
///
/// 这是 [`SUBCOMMANDS`] 那套判据能成立的**唯一**理由：判据改成「`args[0]` ∈ SUBCOMMANDS
/// ⇒ 查询模式」之后，漏登记一条子命令的后果**不再是 exit 2**（吵，但看得见），
/// 而是那条子命令**静默变成「起了个流」** —— 调用方拿到一堆 jsonl 行而不是查询结果。
/// v3.4.0 `--account-trust-zero` 漏登记那次事故的加强版。
#[cfg(test)]
mod argv_table_guard {
    use super::{STREAM_FLAGS, SUBCOMMANDS, SUBCOMMAND_OPTIONS};

    /// 4 个分派文件里出现的每个 `--token`。
    ///
    /// **直接复用 U6a 的抽取**（`protocol_doc_guard` 的 `DISPATCH_FILES` +
    /// `dispatched_subcommands`），而不是在这里重抄一份文件名单 —— 两份名单必然漂移，
    /// 而 U6a 那份已经有 `dispatch_registry_is_complete` 反向核对它没漏文件。
    fn dispatched() -> Vec<String> {
        crate::protocol_doc_guard::dispatched_subcommands()
    }

    /// ★★ **每条调度臂都必须真的调到一个实现**〔G1，Phase G 变异抽样 A2 的产物〕。
    ///
    /// # 为什么需要这条：隔壁那条名字里写着它该抓这个，但它没验
    ///
    /// [`every_listed_subcommand_is_actually_dispatched`] 靠
    /// [`crate::protocol_doc_guard::dispatched_subcommands`] 抽 **臂上的 token**
    /// （`"--resolve"` 这个字面量）。变异 A2 把
    /// `Some("--resolve") => control::resolve_query::run(…)` 换成 `Some("--resolve") => 2,`
    /// —— **token 还在，那条判据全绿**，而 `--resolve` 已经彻底不工作了。
    /// ⇒ 名字里的 **"actually dispatched"** 它没验；它验的是「有一条臂」。
    ///
    /// ★ 一般化（判据覆盖面**第④格·性质面**）：**判据比的必须是它声称的那个性质本身。**
    /// 「臂上有 token」与「臂真的调了实现」是两件事，正如「同一个值」≠「同一个来源」。
    ///
    /// # 本条钉什么
    ///
    /// 逐条取调度块里每一条臂 `=>` 右边的**臂体**，断言它是**一次调用**
    /// （含 `::` 与 `(`），而不是常量/裸表达式。
    ///
    /// ⚠ 抽取器自检：臂数必须 ≥ 7（**按实测写**，今天恰好 7 —— 6 条具名 + 1 条 `_`）。
    /// 少于它就是块界找错或剥测试剥过头，本条会零命中地绿。
    #[test]
    fn every_dispatch_arm_actually_calls_an_implementation() {
        let src = crate::guard_support::production_code(include_str!("main.rs"));
        let beg = src
            .find("let code = match args.first()")
            .expect("找不到一次性查询的调度块 —— 块界锚点变了");
        let end = src[beg..]
            .find("std::process::exit(code);")
            .expect("找不到调度块的结尾锚点")
            + beg;
        let block = &src[beg..end];
        let arms: Vec<&str> = block
            .lines()
            .filter_map(|l| l.split_once("=>"))
            .map(|(_, body)| body.trim())
            .collect();
        assert!(
            arms.len() >= 7,
            "只抽到 {} 条调度臂（应 ≥7：6 条具名 + 1 条 `_`）—— 块界找错或剥过头，本条在空转：{arms:?}",
            arms.len()
        );
        for body in &arms {
            let b = body.trim_end_matches(',').trim();
            assert!(
                b.contains("::") && b.contains('('),
                "调度臂的臂体不是一次调用：{b:?}\n\
                 ⇒ 这条子命令的 token 还挂在那儿，但它已经不调任何实现了 —— \n\
                 v3.4.0 出过同型事故（`--account-trust-zero` 漏列 ⇒ 落进 `_` 臂）。\n\
                 ⚠ 隔壁 `every_listed_subcommand_is_actually_dispatched` **看不见这种错**：\n\
                 它抽的是臂上的 token，不是臂体。"
            );
        }
    }

    /// ★ 每个被分派的 token 都必须在三分表里。
    #[test]
    fn every_dispatched_token_is_classified() {
        let tokens = dispatched();
        assert!(
            tokens.len() >= 14,
            "只抽到 {} 个 token —— 抽取坏了，本断言在空转：{tokens:?}",
            tokens.len()
        );
        let unclassified: Vec<&String> = tokens
            .iter()
            .filter(|t| {
                let t = t.as_str();
                !STREAM_FLAGS.contains(&t)
                    && !SUBCOMMANDS.contains(&t)
                    && !SUBCOMMAND_OPTIONS.contains(&t)
            })
            .collect();
        assert!(
            unclassified.is_empty(),
            "这些 token 被分派了但不在 argv 三分表里：{unclassified:?}\n\
             ⚠ 后果**不是**报错退出，而是：`args[0]` 认不出来 ⇒ 当成流模式 ⇒ \n\
             那条子命令**静默变成起了个流**，调用方拿到 jsonl 行而不是查询结果。\n\
             把它加进 STREAM_FLAGS / SUBCOMMANDS / SUBCOMMAND_OPTIONS 之一。"
        );
    }

    /// ★ 三类**两两不交**。同一个 token 分两类 = 判据自相矛盾。
    #[test]
    fn the_three_classes_do_not_overlap() {
        for (an, a) in [
            ("STREAM_FLAGS", STREAM_FLAGS),
            ("SUBCOMMANDS", SUBCOMMANDS),
            ("SUBCOMMAND_OPTIONS", SUBCOMMAND_OPTIONS),
        ] {
            for (bn, b) in [
                ("STREAM_FLAGS", STREAM_FLAGS),
                ("SUBCOMMANDS", SUBCOMMANDS),
                ("SUBCOMMAND_OPTIONS", SUBCOMMAND_OPTIONS),
            ] {
                if an == bn {
                    continue;
                }
                let both: Vec<&&str> = a.iter().filter(|t| b.contains(t)).collect();
                assert!(both.is_empty(), "{an} 与 {bn} 同时含有：{both:?}");
            }
        }
    }

    /// ★ 表里登记的子命令必须**真的被分派**（防表里堆死条目，让上面那条越来越松）。
    #[test]
    fn every_listed_subcommand_is_actually_dispatched() {
        let tokens = dispatched();
        let ghosts: Vec<&&str> = SUBCOMMANDS
            .iter()
            .filter(|t| !tokens.iter().any(|d| d == *t))
            .collect();
        assert!(
            ghosts.is_empty(),
            "SUBCOMMANDS 里这些 token 没有任何分派点：{ghosts:?}（删掉，别让表虚胖）"
        );
    }

    /// ★ 未知 `--flag` 不许把 daemon 踢出流模式。
    ///
    /// 变异回旧行为（「非空即查询」）⇒ 本测试红。
    #[test]
    fn an_unknown_flag_does_not_kick_the_daemon_out_of_stream_mode() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(
            !super::is_query_mode(&v(&["--some-future-flag"])),
            "未知 flag 把 daemon 踢进了查询模式 ⇒ exit 2、无 hello ⇒ monitor 重连死循环\n\
             （2026-07-09 事故的形状；实测过 `--some-future-flag` 会 rc=2）"
        );
        assert!(
            !super::is_query_mode(&v(&["--a", "--b"])),
            "多个未知 flag 同理"
        );
    }

    /// ★ 但未知**裸参数**仍要报错 —— 那是明确的调用错误，静默吞掉只会让人查半天。
    #[test]
    fn an_unknown_bare_argument_still_goes_to_the_error_path() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(super::is_query_mode(&v(&["/some/path"])));
        assert!(super::is_query_mode(&v(&["--future", "bare"])));
    }

    /// 已登记的子命令照旧进查询模式（回归）。
    #[test]
    fn listed_subcommands_still_enter_query_mode() {
        for c in SUBCOMMANDS {
            assert!(
                super::is_query_mode(&[c.to_string()]),
                "{c} 不再进查询模式了 —— 它会静默变成起了个流"
            );
        }
    }
}
