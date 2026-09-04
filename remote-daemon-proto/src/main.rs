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

mod agent_boundary_guard; // S1：通用层不许知道任何 agent 的名字与文件格式（整体 #[cfg(test)]）
mod agent_locality_guard; // S2：codex 的格式知识只许住 agents/codex/ + kind 派发点逐条登记（整体 #[cfg(test)]）
mod agents; // S2/S3：agent 适配层——每个 agent 一份，装它专属的知识（codex + claudecode）
#[cfg(test)]
mod alloc_probe; // U-2：线程级内存量具（F22：`VmHWM` 是进程级的，会把邻居测试算进来）
mod build_id_guard; // E77：加了子命令必须 bump BUILD_ID（内部整体 #[cfg(test)]，生产构建为空）
mod cc_bus_boundary_guard; // P4f-Y2：daemon 不许碰 cc-bus 的数据布局（整体 #[cfg(test)]）
mod common; // U2：两边都要、又不含平台原语的纯工具（§0.5-6 打掉了「三分够用」那个判断）
mod control; // U3：控制面 —— 会改变世界（写盘 / 改 tmux server / 发信号），或产出改变世界的计划
#[cfg(test)]
mod guard_support; // U-1：各条源码扫描型守卫共用的「只留生产段」剥法（仅测试构建）
mod inbound; // U6b-1：流连接上的入方向（信封 / 分派 / 取消）
mod layering_guard; // U3：§1.1 第二条解耦线的机器判据（observe↔control 方向与条数）
mod listen; // K-P1：常驻监听口 —— 脱离宿主之后还能被找到 / 被问到 / 被接上（纯判定住这里，接受循环住 main.rs）
mod no_timer_guard; // P6：零定时器护栏（内部整体 #[cfg(test)]，生产构建为空）
mod observe; // U3：观测面 —— 读，不改变世界
mod platform; // U2：唯一允许平台原语与平台 cfg 的层（§1.1 第一条解耦线）
mod plugin; // K-W1A：插件通用调用口 —— 找它 / 传 argv 起它 / 问它会什么（方向由 layering_guard 钉）
mod protocol_doc_guard; // U6a：IPC-PROTOCOL.md 与真实协议面的对拍
mod ratchet_guard; // K-P1 KPY7：本件动过的那几张登记表，**断言那几行**逐字没动（整体 #[cfg(test)]）
mod readonly_guard; // F08a：daemon 只读机器护栏（内部整体 #[cfg(test)]，生产构建为空）
mod relay; // K-H1：HTTP 中转（搬字节那半）——只听回环、按路径前缀分流、逐块透传 + tee
mod single_stream_guard; // K-P1 KPY8：「多客户端的流」明确不做 —— 三处「恰好一个客户端」的触发器（整体 #[cfg(test)]）
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
/// - p1w-inbound-in-fingerprint = **audit-0805 F02**：**结清一笔从 08-02 起就欠着的 bump**。
///   `inbound::COMMANDS` 从零条长到 5 条（`cancel`/`ping`/`resolve` 08-02、`kill` 08-04），
///   而 `build_id_guard` 的指纹只看 `main.rs` 的 `Some("--`（一次性子命令那一面）
///   ⇒ **加了整整一个命令面，一次 bump 都没被逼出来**。
///   ⚠ 后果不是纸面的：`sftp.rs::deploy_decision` 的唯一判据是 build_id 字符串，
///   报同一个 id ⇒ 判 `Skip` ⇒ 已部署的旧 daemon **整个控制面静默不可用**。
///   本轮把通道面纳入指纹并 bump；**本条 bump 本身就是那笔欠账的偿付** ——
///   报 `p1v` 的远端从此会被判 stale 并重装。CLI 那一面**一字未改**。
/// - p1x-overflow-identity = **audit-0805 F03**：`Overflow` additive 加 `lost` / `lost_truncated`。
///   出方向那条通道此前对 11 种帧一视同仁地 `try_send` 丢弃，而「丢一帧可恢复」**只对内容帧成立** ——
///   `session_added`/`session_removed`/`tmux_session_closed` 是一次差分的结果、别处不存在，
///   客户端拿着「丢了 N 条」没法重同步。现在不可恢复的那些会带 `kind`+`subject` 出来（有界 64）。
///   ⚠ wire additive、**不 bump `PROTO_VERSION`**；但二进制行为变了 ⇒ 照 p1v 的先例 bump build_id。
///   `session_kind` 此前把两件事压在一个轴上 —— ①「该不该在 UI 出现」②「attach 进去对人有没有
///   意义」。SDK / 脚本驱动的会话正好「①要②不要」：它**有** tmux、`@ccm_sid` 也对，但
///   `stdin=DEVNULL`，用户敲的字会被脚本吃掉。省略 = true（存量零迁移）。
///   **必须 bump**：monitor 要靠新 daemon 才拿得到这个字段；不 bump 就不判 stale、不重装。
///   （wire 是 additive、旧 monitor 忽略未知字段 ⇒ **不 bump PROTO_VERSION**。）
///
/// - p2a-rewatch-sessions〔`P0b-Y2` 08-13〕：**盯着的 `sessions/` 被换掉/还没出现时会重挂**。
///   wire 一个字节没变（不 bump `PROTO_VERSION`），但**二进制行为变了** ⇒ 照上面的先例 bump。
///   ★ **必须 bump**：旧 daemon 在这条路上是**静默失效**的（活着、不吭声、不发 `session_added`），
///   报同一个 id 就不会被判 stale、不会自动重装 —— 用户会带着一个永远不宣告会话的 daemon 过日子。
const BUILD_ID: &str = "p2d-relay";

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

/// `K-P4`（09-04）：命令级 code —— **「这台机器上没有 tmux」**。
///
/// ⚠ 这是这个字面量在仓里的第 N 份，但它**不是第 N 个真相源**：真相是
/// `inbound::REGISTRY` 里各条命令自己登记的 `codes`，本常量只拿它去**查那张表**。
/// 查不到就红（`the_declared_code_is_one_the_registry_already_declares`）⇒
/// 谁把那边的拼写改了，这边不会静默跟丢。
const NO_TMUX: &str = "no_tmux";

/// `K-P4`：这台机器上**做不到**的命令 —— **纯判定那一半**（不碰世界 ⇒ 可拿合成读数驱动）。
///
/// # 这里为什么没有一张手写的「命令 → 它依赖什么」表
///
/// 那件事 `inbound::REGISTRY` 已经说过了：**谁会回 `no_tmux`，谁就依赖 tmux**。
/// 再手写一张 = 第二份真相，而两份真相里迟早有一份是旧的（本工作区最贵的那一类病）。
/// ⇒ 直接从 `codes` 派生。副作用正是要的：以后**新加**一条会回 `no_tmux` 的命令，
/// 它自动进这张表，没有人需要记得来改这里。
///
/// # `tmux` 的三态各自落在哪（🔴 这一格是本函数唯一容易假绿的地方）
///
/// · `Some(false)`（**确证没有**）⇒ 列进表；
/// · `Some(true)` ⇒ 不列；
/// · `None`（**判不出来**）⇒ **不列**。
///
/// 「不知道」有两条压法，两条都坏、但坏得不一样：压成**做不到** ⇒ 客户端灰掉按钮 ⇒
/// **能用的功能从界面上消失，而这种消失没有任何回音**（用户只会以为它不支持）；
/// 压成**做得到** ⇒ 客户端照今天的样子办（照发、点了看 `no_tmux`）⇒
/// **一个字节都没退化**，只是这一格没买到。⇒ 后者是唯一安全的那一侧。
/// 一句话：**这张表只在有把握时才开口，没把握时它退回今天的行为。**
#[allow(dead_code)] // 同 `unavailable_here`：唯一的生产调用点是它，而它今天不接线。
fn unavailable_from(tmux: Option<bool>) -> Vec<wire::Unavailable> {
    let mut out = Vec::new();
    if tmux == Some(false) {
        for spec in inbound::REGISTRY {
            if spec.codes.contains(&NO_TMUX) {
                out.push(wire::Unavailable {
                    command: spec.name.to_string(),
                    code: NO_TMUX.to_string(),
                });
            }
        }
    }
    out
}

/// `K-P4`：与世界打交道的那一半 —— 给定 `PATH` 的值，`tmux` 在不在它上面。
///
/// # 判准为什么正好是「`PATH` 上有没有一个可执行的 `tmux`」
///
/// 因为**真调用那一刻就是这么找的**：`control/kill.rs` `control/gate.rs` `control/launch.rs`
/// 三处都走 `Command::new("tmux")`，unix 上它是 `execvp` ⇒ 逐字就是 `PATH` 查找。
/// 事前那句话与事后那句话用**同一个判准**，两者才不会各说各话。
///
/// # `None`（判不出来）今天只剩**一个**来源
///
/// ① `PATH` 没设、或切不出任何一个非空目录 ⇒ **无处可查**，「没找到」这句话说不出口。
///
/// # 🔴 首行那个 `cfg!(unix)` 今天只说一件事（`K-P4` 下一拍拆开的就是它）
///
/// 上一版它**同时**说了两句话：「**我这个探针在非 unix 上不工作**」（**真的** ——
/// Windows 的 `CreateProcess` 还会看进程自身目录、当前目录、`System32`，并按 `PATHEXT`
/// 补后缀，真装了的那个叫 `tmux.exe`，而本扫描找的是无后缀的 `tmux`）
/// 与「**所以答案未知**」（**假的**）。后一句让这张表**在 Windows 上恒空**，
/// 而 Windows 正是这一件的动机平台 —— 原始问题在那儿一格都没被治。
///
/// 现在**平台那一维搬去了 [`TmuxPlatform`]**：非 unix 上压根走不到这个函数
/// （`tmux_present` 在 windows 那一档调的是 [`tmux_exe_in`]）。
/// ⇒ 这一行留着，只表达**它自己那一句**：「扫 `PATH` 找无后缀 `tmux`」这个判准
/// 只在 unix 上与 `execvp` 等价，别处不等价，所以它在别处**不开口**。
#[allow(dead_code)] // 同上。
fn tmux_in(path: Option<&std::ffi::OsStr>) -> Option<bool> {
    if !cfg!(unix) {
        return None;
    }
    let dirs: Vec<PathBuf> = std::env::split_paths(path?)
        .filter(|d| !d.as_os_str().is_empty())
        .collect();
    if dirs.is_empty() {
        return None;
    }
    Some(
        dirs.iter()
            .any(|d| plugin::discover::is_executable(&d.join("tmux"))),
    )
}

/// `K-P4`（下一拍）：**平台**这一维 —— 与「探针」那一维分开的第二个值。
///
/// # 它治的是「一个值装了两件事」
///
/// 上一拍 `tmux_in` 的首行把「**探针在这个平台上不工作**」（真）与「**答案未知**」（假）
/// 压成了同一个 `None`。于是 `unavailable_from(None)` 不列任何命令 ⇒
/// **这张表在 Windows 上恒空** ⇒「没有 tmux 却宣称我认 `kill`/`launch`」这个原始问题，
/// 在**动机平台**上一格都没治。⇒ 本枚举把那两件事拆成两个值。
///
/// # 三档，🔴 不许再合并
///
/// ⚠ 拆开的是**平台**这一维，**不是**三态处置那条规则 —— 那条（「不知道」不许压成
/// 「做不到」）在 [`unavailable_from`] 的头注里，本拍一个字没动，也不该动：
/// 它论证过「不知道」压成「做不到」会让**能用的功能从界面上无声消失**。
/// 上一拍的病不在那条规则，在**把 Windows 归进了「不知道」这一档**。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)] // 同 `unavailable_here`：生产今天不接线。
enum TmuxPlatform {
    /// **unix** —— 平台这一维**没有结论**，整件事交给探针（[`tmux_in`] 扫 `PATH`）。
    /// 探针的三档原样保留：找到 = `Some(true)` · 没找到 = `Some(false)` ·
    /// `PATH` 无处可查 = `None`。
    AskThePath,
    /// **windows** —— 那儿**没有原生 tmux**（它要 `fork` / pty / unix domain socket）
    /// ⇒ 默认答案是**确证的「没有」**，这一句在**编译期**就成立，不需要探针去证。
    ///
    /// 探针在这一档里只有**一个方向**的作用：把答案**抬成「有」**（有人把 MSYS2 /
    /// Cygwin 的 `tmux.exe` 放上了 `PATH` —— 那台机器上 `Command::new("tmux")` 真的能起来）；
    /// 它**永远不会**把答案压回「不知道」。
    ///
    /// ⇒ 漏看的方向也是安全的：`tmux_exe_in` 看不见的地方（进程自身目录 / 当前目录 /
    /// `System32`），我们说「没有」，而同一台机器真调用时 `CreateProcess` **也搜 `PATH`**、
    /// 一样失败、一样回 `no_tmux` ⇒ **事前那句话与事后那句话仍然是同一句**。
    AbsentUnlessExeOnPath,
    /// **既不是 unix 也不是 windows** —— 探针不适用，平台这一维**也没有结论**
    /// ⇒ 老实说「不知道」，按三态处置那条不列进表。
    /// 🔴 **这一档不许再拿来装 Windows**：那正是上一拍的病。
    NoOpinion,
}

/// `K-P4`：Windows 形状的探针 —— `PATH` 上有没有一个叫 `tmux.exe` 的普通文件。
///
/// # 为什么不复用 [`tmux_in`]
///
/// 那个找的是**无后缀**的 `tmux`，还要执行位；而 Windows 上真装了的叫 `tmux.exe`，
/// 且那儿没有执行位这个概念（`plugin::discover::is_executable` 在非 unix 上恒真）。
/// 两条都不是那边的形状 ⇒ 各写各的判准，别让一个函数装两个平台的语义。
///
/// # 它证不到什么（如实写）
///
/// `CreateProcess` 的查找面比 `PATH` 大（进程自身目录 · 当前目录 · `System32` · `Windows`），
/// 还会按 `PATHEXT` 补后缀 ⇒ **它可能漏看**。漏看之后答案是「没有」，
/// 而那正是 [`TmuxPlatform::AbsentUnlessExeOnPath`] 头注里论证过的安全方向。
#[allow(dead_code)] // 同上。
fn tmux_exe_in(path: Option<&std::ffi::OsStr>) -> bool {
    let Some(path) = path else {
        return false;
    };
    std::env::split_paths(path)
        .filter(|d| !d.as_os_str().is_empty())
        .any(|d| d.join("tmux.exe").is_file())
}

/// `K-P4`：两维合起来 —— **平台**（编译期定死）× **探针**（去世界上看）。
///
/// 三个入参组合的答案由 [`TmuxPlatform`] 各档头注给；这个函数本身**不碰 `cfg!`**，
/// 平台是**入参**。⇒ 本机是 Linux 也能把「Windows 那台机器」当成一个入参跑出来，
/// 而那正是 `the_windows_answer_is_confirmed_absent_not_unknown` 拿到读数的方式。
#[allow(dead_code)] // 同上。
fn tmux_present(platform: TmuxPlatform, path: Option<&std::ffi::OsStr>) -> Option<bool> {
    match platform {
        TmuxPlatform::AskThePath => tmux_in(path),
        TmuxPlatform::AbsentUnlessExeOnPath => Some(tmux_exe_in(path)),
        TmuxPlatform::NoOpinion => None,
    }
}

/// 这份二进制编译到哪个平台 —— **全文件唯一**一处把平台翻成值的地方。
///
/// 「唯一一处」与「windows 那一支给的是哪一档」由
/// `the_windows_arm_is_wired_into_the_source` 钉住（它读的是**磁盘上的源码文本**）。
#[allow(dead_code)] // 同上。
const TMUX_PLATFORM: TmuxPlatform = if cfg!(windows) {
    TmuxPlatform::AbsentUnlessExeOnPath
} else if cfg!(unix) {
    TmuxPlatform::AskThePath
} else {
    TmuxPlatform::NoOpinion
};

/// ★ 一条**只在 Windows 编译时存在**的编译期断言。
///
/// 🔴 **它在本仓门禁上给不出任何读数** —— 本机是 Linux，这个 item 在这儿
/// **编译期就不存在**。它开口的时刻是任何一次 **Windows 编译**
/// （daemon「必须在 Windows 上编得过」这条纪律见 `plugin/discover.rs::is_executable` 头注）。
/// 写它的理由：Linux 那侧只有源码文本判据（读的是「那一支写在那儿」），
/// 而这一条读的是「**那一支真的被编进去了**」—— 两者证的不是同一件事。
#[cfg(windows)]
const _: () = assert!(
    matches!(TMUX_PLATFORM, TmuxPlatform::AbsentUnlessExeOnPath),
    "Windows 上平台这一维必须是「确证没有」而不是「不知道」——\
     改回 NoOpinion 会让握手帧第四条面在 Windows 上恒空，而 Windows 正是这一件的动机平台。"
);

/// `K-P4`：生产入口 —— 这一帧**真填的话**该填什么。
///
/// ⚠ 今天生产**不调它**（`build_hello` 硬写 `Vec::new()`）——与 `agents::visible_homes()`
/// 同一个口径：**能填不真填**。摘掉下面这个 `allow` 的那天，就是把 `build_hello` 那一行
/// 换成本函数的那天；要**同轮**做的三件事写在 `wire.rs` 那个字段的头注里。
///
/// 🔴 **真填那天连着要想清楚的一件事：这一趟探测的结果会被用很久。**
/// `build_hello` 在分档**之前**只调一次，那一帧随后交给两条载体；常驻那条（`listen.rs`）
/// 服务**不限次**的「只读 hello 就走」⇒ **同一帧被这个进程后续的所有连接共用**。
/// ⇒ 探测本身必须**便宜且挂不住**（所以 `tmux_in` 是纯 `stat` 扫 `PATH`，不是真 exec 一次），
/// 而消费侧必须把它当**提示**（`wire.rs` 那个字段头注的口径③）。
#[allow(dead_code)] // `K-P4`：能填不真填 —— 接线是一次纯发布决策，不是忘了。
fn unavailable_here() -> Vec<wire::Unavailable> {
    unavailable_from(tmux_present(
        TMUX_PLATFORM,
        std::env::var_os("PATH").as_deref(),
    ))
}

// 本测块紧邻被测的第四条面（就近可读）、不挪文件尾；显式 allow 让 clippy --all-targets 净
//（同上面 `stream_flag_tests` 那一块的理由）。
#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod fourth_face_tests {
    use super::{
        tmux_exe_in, tmux_in, tmux_present, unavailable_from, unavailable_here, TmuxPlatform,
        NO_TMUX,
    };

    /// ★ `K-P4` 红线之一：**生产路径今天恒空** ⇒ hello 帧的线上字节逐字节不变。
    ///
    /// 它与 `wire.rs::hello_unavailable_is_additive_present_and_absent` 是**两半**：
    /// 那条证「给空表就得到旧字节」，本条证「**生产确实给的是空表**」。
    /// 缺任一条，「今天线上字节没变」这句话都不成立 —— 与 `homes` 那两条同一个分工。
    /// ⚠ 真填那天本条会**故意变红**：那是提醒（去 bump `BUILD_ID`、去更新 fixture），不是障碍。
    #[test]
    fn production_hello_leaves_unavailable_empty_so_the_wire_bytes_stay_frozen() {
        let prod = crate::guard_support::production_code(include_str!("main.rs"));
        let sites: Vec<&str> = prod
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("unavailable:"))
            .collect();
        assert_eq!(
            sites.len(),
            1,
            "`main.rs` 生产段里给 `unavailable` 赋值的地方有 {} 处（应当恰好 1 处）——\n\
             0 处 ⇒ 抽取坏了（本断言此刻在空转）；≥2 处 ⇒ 有第二条路，红线只守住一条。\n\
             实得：{sites:?}",
            sites.len()
        );
        assert_eq!(
            sites[0], "unavailable: Vec::new(),",
            "`main.rs` 开始往 `unavailable` 里填东西了 ⇒ hello 帧的线上字节**变了**。\n\
             那是一次**跨仓契约变更**（仓外 aterm 按精确字节读这一帧，契约冻结 2026-07-18）。\n\
             要真填就同轮做三件事（见 `wire.rs` 那个字段的头注）：换这一行 · 更新 fixture 期望串 ·\n\
             **bump `BUILD_ID`**（否则已部署的远端不判 stale、不重装，整轮改动在那边休眠）。"
        );
    }

    /// ★★ `K-P4` 红线之二，也是本拍的核心交付：**这个字段不是编译期常量。**
    ///
    /// # 少了本条会怎样
    ///
    /// 只有上一条的话，「`unavailable` 恒空」与「daemon 根本答不出这个问题」在判据眼里
    /// **一模一样** —— 那样这一拍就只是在 wire 上多挂了一个永远为空的字段，
    /// 也就是握手帧上多了一句谁都不会读的话。**「事前协商」一格都没买到，而没有任何东西会说。**
    ///
    /// # 它证的到底是什么
    ///
    /// 同一份二进制，**换一台机器就换一个答案**。所以两半都要证：
    /// ① **判定**那一半（`unavailable_from`）—— 三种世界，两种答案，且「判不出来」不倒向「做不到」；
    /// ② **读世界**那一半（`tmux_in`）—— 真去文件系统上看，看得见和看不见给不同的答案。
    /// 只证 ① 的话，一个 `fn tmux_in(_) -> Option<bool> { Some(true) }` 的退化实现照样绿。
    #[test]
    fn the_answer_is_a_function_of_the_machine_not_of_the_build() {
        // ── ① 判定那一半：三种"世界"，两种答案 ──────────────────────────────
        assert!(
            unavailable_from(Some(true)).is_empty(),
            "有 tmux 还报做不到 ⇒ 界面会灰掉一个能用的按钮"
        );
        assert!(
            unavailable_from(None).is_empty(),
            "🔴 **「判不出来」被压成了「做不到」** —— 这是本字段最贵的那个错：\n\
             能用的功能会从界面上消失，而这种消失没有任何回音（用户只会以为它不支持）。\n\
             没把握时必须退回今天的行为（照发、点了看命令级 code），那一侧是安全的。"
        );
        let missing = unavailable_from(Some(false));
        let names: Vec<&str> = missing.iter().map(|u| u.command.as_str()).collect();
        assert_eq!(
            names,
            vec!["kill", "launch"],
            "没有 tmux 的那台机器上，做不到的恰好是 `REGISTRY` 里登记了 `{NO_TMUX}` 的那几条。\n\
             ⚠ 本条红**未必是错**：你要是新加了一条会回 `{NO_TMUX}` 的命令，它已经自动进表了\n\
             （这张表是从 `codes` 派生的，不是手写的）—— 那就把这里的期望值补上。\n\
             实得：{names:?}"
        );
        assert!(
            missing.iter().all(|u| u.code == NO_TMUX),
            "表里出现了不是 `{NO_TMUX}` 的原因：{missing:?}"
        );

        // ── ② 读世界那一半：真去文件系统上看 ────────────────────────────────
        //
        // 夹具**不依赖这台机器上装没装 tmux**（那是世界的事实，不是代码的），
        // 所以两个答案都能精确断言。同 `wire.rs` 那条 `homes` 夹具的纪律。
        assert_eq!(
            tmux_in(None),
            None,
            "`PATH` 没设 ⇒ **无处可查** ⇒ 「没找到」这句话说不出口，只能是「判不出来」"
        );

        let root = std::env::temp_dir().join(format!("ccm-kp4-tmux-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let with = root.join("with");
        let without = root.join("without");
        std::fs::create_dir_all(&with).expect("建夹具目录");
        std::fs::create_dir_all(&without).expect("建夹具目录");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let fake = with.join("tmux");
            std::fs::write(&fake, b"#!/bin/sh\nexit 0\n").expect("写合成 tmux");
            let mut perm = std::fs::metadata(&fake).expect("读夹具权限").permissions();
            perm.set_mode(0o755);
            std::fs::set_permissions(&fake, perm).expect("给合成 tmux 上执行位");

            assert_eq!(
                tmux_in(Some(with.as_os_str())),
                Some(true),
                "`PATH` 上摆着一个可执行的 `tmux` 却没看见 ⇒ 读世界那一半是瞎的"
            );
            assert_eq!(
                tmux_in(Some(without.as_os_str())),
                Some(false),
                "空目录当 `PATH` 却报「有」⇒ 读世界那一半在撒谎（退化成常量了）"
            );

            // ★ 合起来：**同一份代码，两台不同的机器，两个不同的答案。**
            //   这一步才是「不是编译期常量」的正面证据 —— 上面两组各自都只证了一半。
            assert!(
                unavailable_from(tmux_in(Some(without.as_os_str()))).len() == 2
                    && unavailable_from(tmux_in(Some(with.as_os_str()))).is_empty(),
                "端到端：没有 tmux 的机器上要报出两条做不到，有 tmux 的机器上一条都不报"
            );
        }
        #[cfg(not(unix))]
        {
            // 🔴 这一格断的是**探针**，不是**答案**（下一拍把两者拆开了）：
            //    「扫 `PATH` 找无后缀 `tmux`」这个判准在非 unix 上不等价于 `execvp`
            //    ⇒ 这个**函数**必须不开口。而那台机器上的**答案**由 `TmuxPlatform` 给，
            //    走 `tmux_exe_in`，见 `the_windows_answer_is_confirmed_absent_not_unknown`。
            assert_eq!(
                tmux_in(Some(with.as_os_str())),
                None,
                "非 unix 上这个探针必须不开口 ——`Command::new(\"tmux\")` 在那儿还会看进程自身\
                 目录与当前目录，而且真装了也叫 `tmux.exe`，这个扫描对不上它"
            );
        }
        let _ = std::fs::remove_dir_all(&root);

        // ── ③ 生产入口跑得通（真填那天换过去的就是它）────────────────────────
        //   只断言**与世界无关**的性质：不断言条数 —— 那会变成「跑测试这台机器上装没装 tmux」。
        for u in &unavailable_here() {
            assert!(
                super::inbound::COMMANDS.contains(&u.command.as_str()),
                "声明做不到的 `{}` 根本不在 `commands` 里 —— 本字段说的是「接得下但做不到」，\
                 「根本不接」那一格由不在 `commands` 里表达",
                u.command
            );
        }
    }

    /// ★★ `K-P4` 下一拍的正题：**Windows 上那张表不许是空的。**
    ///
    /// # 上一拍在这一格上明确没买到，而它恰好是动机平台
    ///
    /// 上一版 `tmux_in` 首行 `if !cfg!(unix) { return None; }` ⇒ Windows 上恒「判不出来」
    /// ⇒ `unavailable_from(None)` 不列 ⇒ **表恒空** ⇒「没有 tmux 却宣称我认 `kill`/`launch`」
    /// 在 Windows 上一格没治。病灶是**一个值装了两件事**：探针不工作（真）＋答案未知（假）。
    ///
    /// # 🔴 这个读数是怎么取的，以及它**证不到什么**（本机没有 Windows）
    ///
    /// 走的是**纯函数入参**：`tmux_present` 自己**不碰 `cfg!`**，平台是它的第一个参数。
    /// ⇒ 下面每一条都是**在这台 Linux 上真跑出来的**，不是「合成样本上大概会这样」。
    /// **它守得住**：那两维怎么合成答案（包括「Windows 那一档必须给确证的 `Some(false)`」）。
    /// **它守不住**：Windows 上编出来的二进制**真的选了**那一档 —— 那是 `TMUX_PLATFORM`
    /// 那一行的事，由 `the_windows_arm_is_wired_into_the_source`（源码文本）与
    /// `#[cfg(windows)] const _`（只在 Windows 编译时开口）各守一半。
    /// ⚠ **别把本条读成「Windows 上成立」** —— 本条成立的是「给定平台入参时成立」。
    #[test]
    fn the_windows_answer_is_confirmed_absent_not_unknown() {
        let root = std::env::temp_dir().join(format!("ccm-kp4-win-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bare = root.join("bare"); // 什么都没有的一格 PATH
        let msys = root.join("msys"); // 有人把 MSYS2 的 tmux.exe 放上来了
        std::fs::create_dir_all(&bare).expect("建夹具目录");
        std::fs::create_dir_all(&msys).expect("建夹具目录");
        std::fs::write(msys.join("tmux.exe"), b"pretend this is msys2 tmux").expect("写合成 exe");

        // ── ① 正题：没有 tmux.exe 的那台 Windows ⇒ **确证的「没有」**，不是「不知道」──
        assert_eq!(
            tmux_present(TmuxPlatform::AbsentUnlessExeOnPath, Some(bare.as_os_str())),
            Some(false),
            "Windows 那一档又变回「不知道」了 —— 那正是上一拍的病：\n\
             tmux 结构上不存在于 Windows，「没有」这句话在编译期就成立，不需要探针去证。"
        );
        let names: Vec<String> =
            unavailable_from(tmux_present(TmuxPlatform::AbsentUnlessExeOnPath, Some(bare.as_os_str())))
                .iter()
                .map(|u| u.command.clone())
                .collect();
        assert_eq!(
            names,
            vec!["kill".to_string(), "launch".to_string()],
            "🔴 **Windows 上这张表又空了** —— 这一格就是本拍的正题。\n\
             握手帧第四条面在动机平台上不说话 = 这一拍什么都没买到。\n\
             ⚠ 本条红未必是错：新加了一条会回 `no_tmux` 的命令，它会自动进表 —— 那就补期望值。\n\
             实得：{names:?}"
        );

        // ── ② `PATH` 读不到，Windows 上**仍然**是确证的「没有」──────────────────
        //    这一档的默认值来自**平台**，不来自探针 ⇒ 探针无话可说不影响它。
        //    （unix 那一档正相反：没有默认值，探针不开口就只能是 `None`。）
        assert_eq!(
            tmux_present(TmuxPlatform::AbsentUnlessExeOnPath, None),
            Some(false),
            "Windows 上「PATH 读不到」被读成了「答案未知」—— 又把两件事压回一个值了"
        );

        // ── ③ 探针只能把它**抬成「有」**，不会把它压回「不知道」────────────────
        assert_eq!(
            tmux_present(TmuxPlatform::AbsentUnlessExeOnPath, Some(msys.as_os_str())),
            Some(true),
            "`PATH` 上摆着 `tmux.exe` 却仍报「没有」⇒ 会把一个真能用的按钮灰掉\n\
             （MSYS2 / Cygwin 那台机器上 `Command::new(\"tmux\")` 是真能起来的）"
        );
        assert!(
            unavailable_from(tmux_present(
                TmuxPlatform::AbsentUnlessExeOnPath,
                Some(msys.as_os_str())
            ))
            .is_empty(),
            "有 tmux.exe 还报做不到 ⇒ 界面会灰掉一个能用的按钮"
        );

        // ── ④ 🔴 Linux 那一侧一格都没破：三档原样 ──────────────────────────────
        assert_eq!(
            tmux_present(TmuxPlatform::AskThePath, None),
            None,
            "unix 上 `PATH` 无处可查仍然必须是「判不出来」—— 这一档不许被 Windows 那一档带跑"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let with = root.join("with");
            std::fs::create_dir_all(&with).expect("建夹具目录");
            let fake = with.join("tmux");
            std::fs::write(&fake, b"#!/bin/sh\nexit 0\n").expect("写合成 tmux");
            let mut perm = std::fs::metadata(&fake).expect("读夹具权限").permissions();
            perm.set_mode(0o755);
            std::fs::set_permissions(&fake, perm).expect("给合成 tmux 上执行位");
            assert_eq!(
                tmux_present(TmuxPlatform::AskThePath, Some(with.as_os_str())),
                Some(true),
                "unix 那一档必须仍然走探针 —— 摆着一个可执行的 `tmux` 却说没有"
            );
            assert_eq!(
                tmux_present(TmuxPlatform::AskThePath, Some(bare.as_os_str())),
                Some(false),
                "unix 上 `tmux` 不在 `PATH` 上仍然必须是**确证没有**（不是「不知道」）"
            );
            // ★ 两个平台形状的判准**不许互串**：`tmux.exe` 不是 unix 上的 tmux，
            //   无后缀 `tmux`（且没有执行位）也不是 Windows 认的那个。
            assert_eq!(
                tmux_present(TmuxPlatform::AskThePath, Some(msys.as_os_str())),
                Some(false),
                "unix 那一档把 `tmux.exe` 当成 tmux 了 —— 两个平台的判准串了线"
            );
            assert!(
                !tmux_exe_in(Some(with.as_os_str())),
                "Windows 那个判准把无后缀的 `tmux` 当成 `tmux.exe` 了"
            );
        }

        // ── ⑤ 第三档还在：既不是 unix 也不是 windows ⇒ 仍然老实说「不知道」──────
        assert_eq!(
            tmux_present(TmuxPlatform::NoOpinion, Some(msys.as_os_str())),
            None,
            "「真不知道」这一档被合并掉了 —— 拆的是平台那一维，不是三态处置那条规则"
        );
        assert!(
            unavailable_from(tmux_present(TmuxPlatform::NoOpinion, Some(bare.as_os_str())))
                .is_empty(),
            "「不知道」被压成了「做不到」—— 能用的功能会从界面上无声消失"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// ★ `K-P4` 下一拍第二条：**windows 那一支真的写在生产源码里，且只写了一处。**
    ///
    /// # 为什么单有上面那条不够
    ///
    /// 上面那条把平台当**入参**，于是它在 Linux 上跑得出真读数；
    /// 代价是：**没有任何东西说生产那一行怎么选这个入参**。
    /// 把 `TMUX_PLATFORM` 的 windows 那一支改回 `NoOpinion`，上面那条**照样全绿** ——
    /// 表在 Windows 上重新恒空，而没有人会说话。本条钉的就是那一行。
    ///
    /// # 🔴 它守不住什么（本机没有 Windows，这一句必须写在这儿）
    ///
    /// 它读的是**磁盘上的源码文本**，证的是「那一支写在那儿、而且只有一处」；
    /// 它**证不了**「Windows 上编出来的二进制真的走了那一支」——
    /// 那由 `#[cfg(windows)] const _` 那条编译期断言守，而**那一条在本仓门禁上不存在**。
    #[test]
    fn the_windows_arm_is_wired_into_the_source() {
        let prod = crate::guard_support::production_code(include_str!("main.rs"));
        let decl = "const TMUX_PLATFORM: TmuxPlatform = if cfg!(windows) {";
        let at = guard_core::pin_line(&prod, decl)
            .unwrap_or_else(|why| panic!("生产段里钉不住那一行：{why}\n（找的是 `{decl}`）"));
        let body: Vec<&str> = prod
            .lines()
            .skip(at)
            .take_while(|l| l.trim() != "};")
            .collect();
        // 反空真：抽取跑飞了（没收住尾）时下面几条会在整份文件上恒真。
        assert!(
            (2..=12).contains(&body.len()),
            "抽出来 {} 行 —— 那不是一个三档选择器，抽取器跑飞了",
            body.len()
        );
        let body = body.join("\n");
        let win = body
            .find("AbsentUnlessExeOnPath")
            .expect("windows 那一支不见了 —— 表会在 Windows 上重新恒空");
        let unix = body
            .find("cfg!(unix)")
            .expect("unix 那一支不见了 —— Linux 上会不再走探针");
        assert!(
            win < unix,
            "`cfg!(windows)` 那一支给的不是「确证没有」——两支的次序被换过了：\n{body}"
        );
        assert!(
            body.contains("NoOpinion"),
            "第三档没了 —— 「真不知道」被合并进别的档里了：\n{body}"
        );
        // 「只许一处」：全生产段里把平台翻成值的地方**恰好一个**。
        assert_eq!(
            prod.matches("cfg!(windows)").count(),
            1,
            "生产段里有 {} 处 `cfg!(windows)` —— 平台这一维只许在 `TMUX_PLATFORM` 一处成值，\n\
             第二处就是第二份真相（本工作区最贵的那一类病）。",
            prod.matches("cfg!(windows)").count()
        );
        // windows 那一档给出的必须是一个**确定的布尔**，不是 `None`。
        guard_core::pin_line(
            &prod,
            "TmuxPlatform::AbsentUnlessExeOnPath => Some(tmux_exe_in(path)),",
        )
        .unwrap_or_else(|why| {
            panic!("windows 那一档不再给确定答案了：{why}\n它一旦回 `None`，表在 Windows 上就又空了。")
        });
    }

    /// ★ `K-P4` 红线之三：**声明用的 code，必须是那条命令自己登记过的 code。**
    ///
    /// 事前那句话与事后那句话要是各说各的词，客户端就得维护**两张**「这句话怎么翻成人话」
    /// 的表，而 monitor 侧那张已经写好了（`daemon_launch.rs` 等三处逐字「远端未安装 tmux」）。
    ///
    /// # 它真正逮的是什么（不是同义反复）
    ///
    /// 这张表从 `REGISTRY.codes` 派生，但**常量 `NO_TMUX` 那个字面量是第二份拷贝**。
    /// 有人把 `REGISTRY` 里的 `no_tmux` 改名（比如收窄成 `tmux_missing`），
    /// 派生出来的表会**静默变空** —— 所有测试照绿，而第四条面从此永远不说话。
    /// 本条把那次改名变成一次红。
    #[test]
    fn the_declared_code_is_one_the_registry_already_declares() {
        let owners: Vec<&str> = crate::inbound::REGISTRY
            .iter()
            .filter(|s| s.codes.contains(&NO_TMUX))
            .map(|s| s.name)
            .collect();
        assert!(
            !owners.is_empty(),
            "`REGISTRY` 里没有任何一条命令登记 `{NO_TMUX}` —— 要么那个 code 被改名了、\n\
             要么依赖 tmux 的命令都没了。无论哪种，握手帧第四条面此刻**永远为空**，\n\
             而它自己不会喊疼。（本常量只是拿去查表，`REGISTRY` 才是真相源。）"
        );
        for u in unavailable_from(Some(false)) {
            let spec = crate::inbound::REGISTRY
                .iter()
                .find(|s| s.name == u.command)
                .unwrap_or_else(|| panic!("声明了一条 `REGISTRY` 里没有的命令：{}", u.command));
            assert!(
                spec.codes.contains(&u.code.as_str()),
                "给 `{}` 声明的原因 `{}` 不在它自己登记的 codes {:?} 里 ——\n\
                 事前说的和事后回的不是同一句话，客户端得为此维护第二张翻译表。",
                u.command,
                u.code,
                spec.codes
            );
        }
    }
}

// `K-P4`：拉窗构件的**落点判据**（PM 09-04 裁的那一条，见 `K-P4-PM.md §五㈠`）。
//
// # 它管的是哪一维 —— 与已有三道护栏**不重叠**
//
// 摸底那一拍现打过：Win32 那几个构件在三张针表里**一个都没有**
//（`readonly_guard` 的 11 条写盘针 · `no_timer_guard` 的 5 条周期唤醒针 + 8 条调用形态）
// ⇒ 一道拦写盘、一道拦「自己醒来」，**拉窗那一段没有任何后端判据看着它**。
// 而 `platform/fallback_guard` **只扫 `src/platform/`**，且它自陈「人群比性质小」、
// 真守住的是那一层里**内联写法**的 ⇒ 它本来就不是「管平台原语」的那把尺子。
// 🔴 PM 因此裁：**拉窗构件落 `platform/` 之外，并同拍补一条判据管那一维** —— 就是本模块。
//
// # 今天它扫到的真实命中是 **0**（这一句必须写在前面）
//
// daemon crate 里今天一个 Win32 拉窗构件都没有（`Cargo.toml` 里 `windows`/`winapi` 命中 0）。
// ⇒ 上面两条正题断言今天**都在空转**，真正有读数的是**空转自检**那两条合成样本。
// 本模块是**在搬家之前**先把闸门立起来：等 `control/focus.rs` 那一段真落进来的那天，
// 它是第一个开口的人。
#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod window_raise_guard {
    /// 拉窗那一族构件（Win32）。**只认名字，不认它从哪个 crate 来** ——
    /// `windows` 与 `winapi` 两条路都盖得住，换 crate 不会让它掉出人群。
    const WINDOW_RAISE: &[&str] = &[
        "SetForegroundWindow",
        "AllowSetForegroundWindow",
        "AttachThreadInput",
        "BringWindowToTop",
        "SwitchToThisWindow",
        "ShowWindow",
        "SetWindowPos",
        "EnumWindows",
        "GetForegroundWindow",
        "keybd_event",
        "WindowsAndMessaging",
        "winapi::um::winuser",
    ];

    /// 「带 `#[cfg(windows)]`」只认这两种写法。
    ///
    /// ⚠ 刻意**不认** `#[cfg(all(windows, …))]` 之类：那种写法要么是加了第二个条件
    ///（于是这段代码在某些 Windows 上会不存在，那是另一件事、要另外说清楚），
    /// 要么是把平台条件藏进了一个更长的表达式里。**要放宽就来改这里，别在别处绕。**
    const WINDOWS_CFGS: &[&str] = &["#[cfg(windows)]", "#[cfg(target_os = \"windows\")]"];

    /// 一份**生产段**文本里，拉窗构件的落法合不合规。`Ok(n)` = 命中 n 处且每处都在门后。
    ///
    /// # 判法：往上找**最近的一条** `#[cfg(`
    ///
    /// 它必须是 `WINDOWS_CFGS` 里那两种之一。
    /// **守得住**：完全不带 cfg 就写了一段拉窗 · 带的是别的平台 cfg（`unix`/`linux`）·
    /// 带的是更长的 cfg 表达式。
    /// **守不住**（如实写）：属性归属是**文本近似**，不是语法树 ——
    /// 一个 `#[cfg(windows)]` 挂在 A 上、拉窗写在它下面**另一个**没有 cfg 的 item 里，
    /// 本条会误判为合规。要堵这一格得解析语法树，本拍不假装能做。
    fn raise_sites_are_gated(prod: &str) -> Result<usize, String> {
        // 🔴 注释里提到名字**不算落点** —— 一段注释调不动 Win32，而且这一族名字
        //    正是要在注释里被讨论的。剥法用共享那份（块注释也剥、行号不变，`K-R9`），
        //    别在这里内联第二份。
        let text = guard_core::strip_comment_lines(prod);
        let lines: Vec<&str> = text.lines().collect();
        let mut sites = 0usize;
        for (i, line) in lines.iter().enumerate() {
            let Some(hit) = WINDOW_RAISE.iter().find(|n| line.contains(**n)) else {
                continue;
            };
            sites += 1;
            let gate = lines[..i]
                .iter()
                .rev()
                .find(|l| l.trim_start().starts_with("#[cfg("));
            match gate {
                Some(g) if WINDOWS_CFGS.contains(&g.trim()) => {}
                Some(g) => {
                    return Err(format!(
                        "第 {} 行的 `{hit}` 落在 `{}` 之下，而不是 `#[cfg(windows)]` —— \
                         拉窗构件只许在 Windows 那一支里存在。",
                        i + 1,
                        g.trim()
                    ))
                }
                None => {
                    return Err(format!(
                        "第 {} 行的 `{hit}` **一条 `#[cfg]` 门都没有** —— 它会被编进每个平台，\
                         而拉窗这件事只在 Windows 上成立。",
                        i + 1
                    ))
                }
            }
        }
        Ok(sites)
    }

    /// ★ 正题：**拉窗构件只许出现在一处，且只许在 `#[cfg(windows)]` 之下。**
    ///
    /// # 「一处」为什么按**文件**算
    ///
    /// 这一件真正怕的是「Windows 那条路在后端里长出第二份」：一份在 `control/focus.rs`、
    /// 一份在别人顺手写的地方，两份各自演化 ⇒ 就是本工作区最贵的那类病（第二份真相）。
    /// 文件是今天唯一能机检的「一处」；更细的粒度要语法树。
    #[test]
    fn window_raising_lives_in_one_file_and_only_behind_cfg_windows() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        // ⚠ 走 `scan_tree!` 而不是自己 `read_dir`：`scanning_guard_registry` 那条棘轮要求如此，
        //   而它按构造摘掉**调用者自己那份**（`main.rs`）⇒ 下面必须把 `main.rs` 补回来，
        //   否则「拉窗写进 main.rs」这一格逃得掉，而逃掉之后看起来和守住一模一样。
        let mut files: Vec<(String, String)> = guard_core::scan_tree!(&root, &["rs"])
            .into_iter()
            .map(|(p, src)| {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, crate::guard_support::production_code(&src))
            })
            .collect();
        assert!(
            !files.iter().any(|(rel, _)| rel == "main.rs"),
            "`scan_tree!` 的自摘那一刀没落下 —— 下面补回来的那份会变成第二份"
        );
        files.push((
            "main.rs".to_string(),
            crate::guard_support::production_code(include_str!("main.rs")),
        ));
        // 反空真：扫描面塌了的话，下面两条会在空集上恒绿。地板同 `guard_support` 那条（37）。
        assert!(
            files.len() >= 37,
            "只扫到 {} 个 `.rs`（地板 37）—— 扫描面塌了，本护栏正在空转",
            files.len()
        );

        let mut hosts: Vec<(String, usize)> = Vec::new();
        for (rel, prod) in &files {
            match raise_sites_are_gated(prod) {
                Ok(0) => {}
                Ok(n) => hosts.push((rel.clone(), n)),
                Err(why) => panic!("{rel}：{why}"),
            }
        }
        assert!(
            hosts.len() <= 1,
            "拉窗构件散在 {} 个文件里（只许一处）：{hosts:?}\n\
             ⇒ 第二处就是第二份真相；要挪就整段挪，别两边各留一半。",
            hosts.len()
        );

        // ── 空转自检：今天真实命中是 0 ⇒ 上面两条都在空转，判定本体的读数只能从这儿来 ──
        assert_eq!(
            raise_sites_are_gated("#[cfg(windows)]\nfn f() { unsafe { SetForegroundWindow(h) } }")
                .expect("带门的合规样本被判违规了"),
            1,
            "带 `#[cfg(windows)]` 的拉窗必须放行，否则这条判据会逼人把它藏起来"
        );
        assert!(
            raise_sites_are_gated("fn f() { unsafe { SetForegroundWindow(h) } }").is_err(),
            "🔴 不带任何 cfg 的拉窗**没被逮住** —— 本护栏此刻是个摆设"
        );
        assert!(
            raise_sites_are_gated("#[cfg(unix)]\nfn f() { unsafe { AttachThreadInput(a, b) } }")
                .is_err(),
            "带着**别的平台** cfg 的拉窗没被逮住 —— 那比不带 cfg 更坏（它看起来是合规的）"
        );
        assert_eq!(
            raise_sites_are_gated("fn f() { /* 这里提一句 SetForegroundWindow */ }")
                .expect("注释里提一句不该算违规"),
            0,
            "注释里提到名字被算成了落点 —— 那会让人不敢在注释里讨论它"
        );
    }
}

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
    // ── P4d：控制面的 CLI 面。**它们不在这里各写一条实现** ——
    // 分派臂按 `cli_control::spec_for` 派生（见下面那条臂），实现落在 `inbound::REGISTRY`。
    // 登记在这张表里是因为 `is_query_mode` 与 `argv_table_guard` 都读它，
    // 而且 `build_id_guard` 的指纹也取自它 ⇒ 加在这里会**逼出一次 BUILD_ID bump**，那正是要的。
    // P4f：cc-bus 的两条基础命令。⚠ **不加这两行的后果是静默的** ——
    // 分派臂是派生的（认得出来），但 `is_query_mode` 这道**闸门**读的是本表：
    // 不在表里 ⇒ 当成未知 flag ⇒ 打一行 warn 之后**照常进流模式**，
    // CLI 面看上去"存在"却永远调不到（08-13 实测到了这个形状）。
    // ⇒ 现由 `cli_control::tests::every_cli_exposed_command_is_in_the_query_mode_gate` 钉住。
    "--bus-kill",
    "--bus-list",
    "--bus-send",
    "--daemon-probe",
    "--fork-session",
    "--kill",
    "--launch",
    "--list-accounts",
    "--list-subagents",
    "--list-projects",
    "--list-sessions",
    "--ping",
    "--read-session",
    "--read-session-from-offset",
    "--read-session-tail",
    // K-H1：起 HTTP 中转（常驻，不是一次性查询 —— 它住在这张表里是因为
    // `is_query_mode` 那道闸门读的是本表；不登记就会被当成未知 flag 静默进流模式）。
    "--relay",
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

    let agent_home = resolve_agent_home();

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
            Some("--search") => observe::search_query::run(&agent_home, &args),
            // P7c-1：列一个父会话的 subagent 候选。**只列不挑**（匹配与排序留在 monitor）。
            Some("--list-subagents") => observe::history_query::list_subagents(&agent_home, &args),
            Some("--usage") => observe::usage_query::run(&agent_home, &args),
            Some("--resolve") => control::resolve_query::run(&agent_home, &args),
            // G2（branch-anywhere）：从指定消息处分叉出一个新会话文件。
            // **daemon 唯一的写盘入口**，护栏白名单层单独盯着它（readonly_guard）。
            Some("--fork-session") => control::fork_write::run(&agent_home, &args),
            // K-H1：HTTP 中转。**常驻**，起来就不返回；配置面只有环境变量。
            // K-H2a：多传一个 `agent_home` —— 中转要从 `<home>/claudecode-frontend/` 下
            // 读那份凭据文件。**不新开子命令、不动 `SUBCOMMANDS`** ⇒ 不逼出 BUILD_ID bump。
            Some("--relay") => relay::run(&agent_home, &args),
            // ★ 这几个字面量必须与 `observe::accounts_query::run` 自己认的子命令**完全一致**。
            // v3.4.0 出过一次事故：`--account-trust-zero` 在 accounts_query 里实现完整，
            // 但这里漏列 ⇒ 落进下面的 `_` 臂走历史查询 ⇒ `unknown argument` + exit 2，
            // 而 monitor 的账号 0 路径**真的在发这条命令**。
            // 测试当时抓不到，是因为它们直接调 `observe::accounts_query::run`、**绕过了本处调度**。
            // 现由 `observe::accounts_query::tests::main_dispatches_every_subcommand_we_handle` 钉住。
            Some("--list-accounts")
            | Some("--session-accounts")
            | Some("--account-trust")
            | Some("--account-trust-zero") => observe::accounts_query::run(&agent_home, &args),
            // ★ P4d：控制面的 CLI 入口。**这条臂刻意不写命令字面量** ——
            // 认哪些 flag 由 `cli_control::spec_for` 从 `inbound::REGISTRY` 派生，
            // 于是「帧面加一条命令」不需要回来改这里。上面 `--resolve` 那条臂**故意留在前面**：
            // 它的信封与仓外 aterm 冻结在 2026-07-18，走原路一个字节都不动
            // （两条路的输出实为同一个 `CommandPlan`，但冻结的契约不拿「实际上一样」去赌）。
            // ⚠ **必须写成单行臂**：`argv_table_guard::every_dispatch_arm_actually_calls_an_implementation`
            // 按**行**取 `=>` 右边的臂体，块体臂会被抽成 `""` 当场红（实测）。
            // 那条约束是保守的（宁可假红），照它写就是了 —— 单行形式下它钉的
            // 「臂体是一次真调用」也确实成立。
            Some(f) if control::cli_control::handles(f) => control::cli_control::run(&args).await,
            _ => observe::history_query::run(&agent_home, &args),
        };
        std::process::exit(code);
    }

    // 一次性查询已 exit；到此必是流模式。agent_home 日志放此（审计 correctness-重要①：
    // --resolve/一次性查询模式 stderr 只承载结构化错误/查询结果，不掺 info——兑现协议 v1 §3
    // 「错误 exit2 + stderr 纯 {code,message} JSON」，客户端可整段 JSON-parse stderr）。
    tracing::info!("agent_home = {}", agent_home.display());

    // ★★ `K-P1`：**同一个流模式，两种载体**。
    //
    // 「脱离宿主」本身不难（`launch.rs` 里三份现成的范例）；难的是**脱离之后还怎么跟它对话**
    // —— 今天讲协议走的就是那对 stdio 管道，一脱离管道就没了。
    // ⇒ 换载体**不换协议**：`wire.rs` 头注那句「exactly one UTF-8 JSON object per line」
    // 在两条载体上逐字成立；`inbound::spawn<R: AsyncRead>` 与 `writer_task<W: AsyncWrite>`
    // 本来就是泛型的，喂 socket 的两半与喂 stdin/stdout **在类型上无差别**。
    //
    // ⚠ **由环境决定，不由 argv 决定**：`SUBCOMMANDS` 那张表一动就要 bump `BUILD_ID`
    // 并进 `IPC-PROTOCOL.md` 的对拍面（`build_id_guard` / `protocol_doc_guard` 各钉一半），
    // 而本件**一条子命令都没加** —— 它换的是同一个流模式的载体。
    // 判定住 [`listen::mode_from`]（**纯函数**，所以「只写了一半」那两条错误支都测得到）。
    let mode = match listen::mode_from(&|k| std::env::var(k).ok()) {
        Ok(m) => m,
        Err(e) => {
            // **fail closed**：宁可不起，也不要起一个不设防的口 —— 回环 TCP 没有权限位。
            tracing::error!("监听口配置不成立 ⇒ 拒绝起：{e}");
            std::process::exit(listen::EXIT_BAD_LISTEN_CONFIG);
        }
    };

    // (b) Emit the Hello handshake FIRST, flushed, before anything else.
    let hello = build_hello(&agent_home);

    match mode {
        listen::Mode::Stdio => run_over_stdio(hello, agent_home, with_bg, tail_only).await,
        listen::Mode::Listen { port, token } => {
            // 停机信号只挂**一次**（不在 accept 循环里每轮重装一个 SIGTERM 处理器）。
            tokio::select! {
                _ = serve_listening(port, token, hello, agent_home, with_bg, tail_only) => {}
                _ = shutdown_signal() => {
                    tracing::info!("shutdown signal received; exiting");
                }
            }
        }
    }

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

/// 造那一帧 hello。**抽出来是因为两条载体都要发它**，而它必须只有一份 ——
/// 两份 hello 会各自漂，而这一帧是仓外 aterm 按精确字节在读的东西。
fn build_hello(agent_home: &std::path::Path) -> Frame {
    Frame::Hello {
        v: PROTO_VERSION,
        build_id: BUILD_ID.to_string(),
        host_arch: std::env::consts::ARCH.to_string(),
        // ⚠ **左边的字段名与右边的变量名刻意不一致**〔`S4b`〕，这不是笔误：
        // 左边 `claude_dir` 是 **wire 字段**，冻结兼容（真在线上、仓外 aterm 在读），
        // 登记在 `agent_boundary_guard::FROZEN_COMPAT`，带解锁条件，**不许改名**；
        // 右边 `agent_home` 是**仓内的参数名**，`S4b` 已把它从 `claude_dir` 改过来
        //（通用层不该在标识符里叫得出某个 agent 的名字 —— `D3` 的同一条道理往仓内推）。
        // ⇒ 这一行正是两条纪律的交界处：**字段名归契约，变量名归架构**。
        claude_dir: agent_home.to_string_lossy().into_owned(),
        // `S4`（`D3`）：wire 面已换成通用的 `homes`（`[{agent_kind, path}]`）。
        //
        // ★★〔`S5` 08-14〕**这一行是空表，但已经不是因为"做不到"了。**
        // `agents::visible_homes()` 今天就能答出这台机器看得见哪些 agent
        //（判准：home 目录存在；理由与被排除的另两条候选写在那个函数的头注里）。
        // 换过去只要改这一行 —— `S5` 的口径是 **能填不真填**：
        //   填 = 一次**跨仓契约变更**（仓外 aterm 的 hello fixture 按精确字节对），
        //   而本机没有 aterm 仓、验不了它的运行时（前提 `P3`）
        //   ⇒ 把"何时真填"留成一次**纯发布决策**，而不是顺手改过去。
        // 真填那天要同轮做的三件事：① 换这一行；② 更新
        //   `dg3_codex_fields_skipped_when_absent_claude_byte_equivalent` 的期望串；
        //   ③ **bump `BUILD_ID`**（那天线上字节真的变了，已部署的远端得被判 stale）。
        // ⚠ 无论如何**不要再加第二个目录字段** —— 那正是 `D3` 排除掉的路。
        // 这一行由 `production_hello_leaves_homes_empty_so_claude_bytes_stay_frozen` 钉住
        //（它会在那天**故意变红**：那是提醒，不是障碍）；旁边那条
        // `the_daemon_can_already_discover_homes_it_just_does_not_send_them`
        // 钉的是另一半 —— 空表不等于没能力。
        homes: Vec::new(),
        capabilities: CAPABILITIES.iter().map(|s| s.to_string()).collect(),
        emits: EMITS.iter().map(|s| s.to_string()).collect(),
        commands: inbound::COMMANDS.iter().map(|s| s.to_string()).collect(),
        // ★★〔`K-P4` 09-04〕**握手帧第四条面：「我做得到什么」。这一行也是空表，
        // 而它同样已经不是因为"做不到"了。** `unavailable_here()` 今天就能答出这台机器上
        // 哪几条命令做不到（判准 = `tmux` 在不在 `PATH` 上，与真调用那一刻同一个判准；
        // 表本身从 `inbound::REGISTRY` 的 `codes` **派生**，不是手写的第二份真相）。
        //
        // 换过去只要改这一行 —— 口径与 `homes` 那一行逐字相同：**能填不真填**。
        //   填 = 一次**跨仓契约变更**（仓外 aterm 的 hello fixture 按精确字节对，
        //   契约冻结 2026-07-18），而本机没有 aterm 仓、验不了它的运行时
        //   ⇒ 把「何时真填」留成一次**纯发布决策**。
        // 真填那天要同轮做的三件事写在 `wire.rs` 那个字段的头注里（第三件是 **bump `BUILD_ID`**）。
        //
        // 🔴 **真填之前，这一格买到的不是「事前协商」本身，是它的形状 + 一条能验的填法。**
        // 别把「字段加上了」读成「界面已经不会画死按钮了」——那要等消费侧接线。
        // 这一行由 `production_hello_leaves_unavailable_empty_so_the_wire_bytes_stay_frozen`
        // 钉住（它会在那天**故意变红**：那是提醒，不是障碍）；旁边那条
        // `the_answer_is_a_function_of_the_machine_not_of_the_build` 钉的是另一半 ——
        // **空表不等于这个字段是个编译期常量**。
        unavailable: Vec::new(),
    }
}

/// 今天那条路：**stdin/stdout 一对管道**。宿主一退读端就断，daemon 153ms 内自己走。
///
/// ⚠ 本函数体是 `K-P1` 之前 `main()` 的那一段**原样搬过来的**，一行行为都没改 ——
/// 常驻是**加一条载体**，不是把这条改掉。改这一段之前先问：另一条载体要不要跟着改？
async fn run_over_stdio(hello: Frame, agent_home: PathBuf, with_bg: bool, tail_only: bool) {
    let mut stdout = BufWriter::new(tokio::io::stdout());
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
    // 应答走**独立通道**：出方向丢一条**内容帧**可恢复（行还在远端 jsonl 里），
    // ⚠ 而**状态增量帧**丢了别处没有 —— 那半靠 `Overflow.lost` 带身份让客户端重同步（audit-0805 F03），
    // 丢一条应答会让客户端永远等下去。混在一个通道里，实时行的洪峰会把应答挤掉。
    let (reply_tx, reply_rx) = tokio::sync::mpsc::channel::<Frame>(inbound::REPLY_CHANNEL_CAPACITY);
    let inbound_task = inbound::spawn(tokio::io::stdin(), reply_tx.clone(), hello_flushed);

    // (c) Start the watcher reader; it returns the receiving half of the
    // bounded frame channel.
    let (rx, poke) = observe::watcher::spawn(agent_home, with_bg, tail_only);

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
    // `K-P1`：处理器认的是一个**槽**而不是句柄（另一条载体上 watcher 会换人）。
    // 这条路上槽里永远只装这一个 —— 形状统一，实现只有一份。
    let slot: PokeSlot = std::sync::Arc::new(std::sync::Mutex::new(Some(poke)));
    let poke_task = spawn_sigusr1_task(slot);

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
}

/// SIGUSR1 处理器要 poke 的那个 watcher 住的**槽**。
///
/// ★ **为什么是槽而不是句柄**〔`K-P1`〕：常驻那条载体上 watcher 会**换人** ——
/// 每接上一个客户端换一份新的（理由见 [`serve_listening`]），而处理器活得比任何一个 watcher 都长。
/// 拿句柄的话，第二个客户端连上之后 SIGUSR1 会去 poke 一个**已经退掉的** watcher：
/// 那不会报错，它只是**再也不响应 tmux hook 了** —— 又一个「假信号不报错，它只是一直说是」。
type PokeSlot = std::sync::Arc<std::sync::Mutex<Option<observe::watcher::WatcherPoke>>>;

/// **P4：SIGUSR1 = 「tmux 那边有事，赶紧重探一次」。**
///
/// ★ **这一步必须先于任何 hook 安装落地** —— `SIGUSR1` 的**默认处置是终止进程**。
/// 先装 hook 再装处理器，等于给一个会自杀的 daemon 装了自杀触发器。
///
/// 为什么是信号而不是别的：原方案让 hook 追加事件日志、daemon inotify 读增量，
/// **撞红线 I7「daemon 只读」**（`readonly_guard` 当场拦下）。信号通路让 daemon 的
/// 文件系统写归零，且会话名根本不经 shell ⇒ 那条引号/注入面直接消失。
/// 代价是信号无载荷且会合并 —— 靠「重探 + 与上一份快照差分」天然免疫。
#[cfg(unix)]
fn spawn_sigusr1_task(slot: PokeSlot) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigusr1 = match signal(SignalKind::user_defined1()) {
            Ok(s) => s,
            Err(e) => {
                // ★★ 说出来，而且**说真话**〔用@08-27 现打逮到，`K-P1` 回修第 7 处〕。
                //
                // 这句话原先逐字是「⇒ tmux hook 通路不可用，退回定时探测」——
                // 而**那条退路今天不存在**：`P5` 删掉 8s ticker 之后本进程零定时器
                //（`observe/watcher.rs` 那条 `P0b-Y2` 头注逐字：「`P5` 删掉 8s ticker 之后
                // daemon **零定时器**，之后每一拍都靠事件」；`no_timer_guard` 钉着它）。
                //
                // ★ 病根不是打错字：**`P5` 删掉了一个构件，而替那个构件说话的散文散在别处，
                //   没人回去改。**然后它继续以权威口吻骗下一个读者 ——
                //   「不可用但有兜底」与「不可用而且没兜底」是两件完全不同的事。
                // ⇒ 说清**什么事会变得发现不了**（`control/tmux_hook.rs` 头注逐字：
                //   「多个 tmux 会话里杀掉其中一个」是**唯一**没有内核事件源的场景）
                //   与**下一步能做什么**。
                tracing::warn!(
                    "装不上 SIGUSR1 处理器（{e}）⇒ tmux hook 通路整条不可用，而且没有兜底：\
                     「多个 tmux 会话里杀掉其中一个」是唯一没有内核事件源的场景\
                     （pidfd 只看 server 进程、socket inotify 只看 server 生死），\
                     它会一直显示成还在，直到 tmux server 自己起停或别的事件把本进程推醒；\
                     P5 删掉 8s ticker 之后本进程零定时器，没有任何东西会自己醒过来。\
                     下一步：把本机后端停掉再起一次（monitor 的「停」「起」），仍装不上就重开 monitor"
                );
                return;
            }
        };
        tracing::info!("SIGUSR1 处理器已就位（tmux hook 通路的 daemon 侧）");
        while sigusr1.recv().await.is_some() {
            // 锁毒化不该让 tmux 通路整条哑掉 ⇒ `into_inner` 取回内容再用。
            if let Some(p) = slot.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                p.poke();
            }
        }
    })
}

#[cfg(not(unix))]
fn spawn_sigusr1_task(slot: PokeSlot) -> tokio::task::JoinHandle<()> {
    let _ = slot;
    tokio::spawn(async {})
}

/// 一条**已经认证通过**的连接，等着被接成流。
///
/// 三样一起交出去不是为了打包好看：`HelloFlushed` 是**类型级见证**，
/// 它只能由 `write_and_flush_hello` 产出，而握手那一步就发生在这条连接自己身上
/// ⇒ 「hello 先于 reader」这条时序在换了载体之后**逐字保留**，仍然编译期不可表示。
struct Attached {
    reader: tokio::io::BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: BufWriter<tokio::net::tcp::OwnedWriteHalf>,
    hello_flushed: wire::HelloFlushed,
}

/// 一条连接的**握手**：先写 hello，再读一行 attach 请求，然后分档。
///
/// # 为什么 hello 写在分档**之前**
///
/// 否则「这台机上有没有一个长驻 daemon」只能从「`connect()` 成没成」推 ——
/// 而 TCP 的 backlog 会让**没人 accept 的口照样连得上** ⇒ 那是个「一直说是」的假信号。
/// 写在前面之后，那一问的答案是**读一行**，协议一个字节都不用加
/// （`shared/ccm:1182-1185` 自陈「后者今天没有便宜的问法」，说的就是这一格）。
///
/// # `candidate` 是什么
///
/// accept 那一刻用一次 `swap(true)` 决出来的：**赢的那条**才有资格要流，
/// 输的那条最多只能读 hello。这样「谁占着流」不靠事后检查，
/// 而是**一次原子操作**决定的 —— 两条连接同时握手也不会都拿到流。
/// 赢了却没能接成流（对端只想读 hello / token 不对 / 写不出去）⇒ **必须把牌还回去**。
async fn handshake_one(
    sock: tokio::net::TcpStream,
    hello: Frame,
    token: String,
    candidate: bool,
    busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    attached: tokio::sync::mpsc::Sender<Attached>,
) {
    use std::sync::atomic::Ordering;
    let release = || {
        if candidate {
            busy.store(false, Ordering::SeqCst);
        }
    };
    let (r, w) = sock.into_split();
    let mut w = BufWriter::new(w);
    let hello_flushed = match wire::write_and_flush_hello(&mut w, &hello).await {
        Ok(x) => x,
        Err(e) => {
            tracing::warn!("往一条新连接写 hello 失败（{e}）；关掉它");
            release();
            return;
        }
    };
    let mut r = tokio::io::BufReader::new(r);
    // ⚠ **有上限地读** —— 对端是同机任何进程，它完全可以一直发字节不发换行，
    // 而无界读就是无界堆分配（daemon 侧为同一形栽过一次实测，见 `inbound.rs` 头注）。
    let line = match listen::read_capped_line(&mut r, listen::ATTACH_LINE_CAP).await {
        Ok(listen::HandshakeLine::Line(l)) => l,
        Ok(listen::HandshakeLine::Eof) => {
            // 「只读 hello 就走」那一档：对端读完就关，是**正常**结局，不出声。
            release();
            return;
        }
        Ok(listen::HandshakeLine::TooLong(bytes)) => {
            tracing::warn!("一条 attach 请求 {bytes} 字节还没换行 ⇒ 整行丢弃并关连接");
            release();
            return;
        }
        Err(e) => {
            tracing::warn!("读 attach 请求失败（{e}）；关掉这条连接");
            release();
            return;
        }
    };
    match listen::admit(listen::attach_verdict(&line, &token), !candidate) {
        listen::Admit::Refuse(reason) => {
            // **出声地拒**（照 `relay::serve` 那条 503 的形状：宁可拒绝，也不静默 FIN）。
            // 静默 FIN 会让对端只能靠「等了很久没动静」去猜，而那是猜不出原因的。
            let _ = listen::write_line(&mut w, &listen::refusal_line(reason)).await;
            tracing::warn!("拒绝一条 attach 请求：{reason}");
            release();
        }
        listen::Admit::Stream => {
            if listen::write_line(&mut w, listen::ATTACH_OK_LINE)
                .await
                .is_err()
            {
                release();
                return;
            }
            if attached
                .send(Attached {
                    reader: r,
                    writer: w,
                    hello_flushed,
                })
                .await
                .is_err()
            {
                release();
            }
        }
    }
}

/// `K-P1`：**常驻形态的接受循环** —— 一条流 + 不限次的「只读 hello 就走」。
///
/// # 空转期为什么还留着一个 watcher
///
/// 常驻真正买到的是两样东西，第二样就在这里：
/// ① `ccm` 那一问有答案了（连一次 + 读一行 hello）；
/// ② **`@ccm_sid` 打标那段时间窗关掉了** —— 写 `@ccm_sid` 的是**正在跑的** daemon
///   （`observe/watcher.rs` inotify `sessions/` → `identity_tag::tag`），
///   而 `shared/ccm` 那条每会话每秒的身份 poller 已经被 `U-NP④` **整条删掉、不留轮询退路**。
///   monitor 没开着的时候若这里不看 `sessions/`，②就一格都没买到。
/// ⇒ 空转期照样起一个 watcher，帧**读出来就丢**（没人要），打标那一半照常发生。
///
/// # 接上客户端时为什么**换一份新的** watcher，而不是把空转那份的帧转给他
///
/// `watch_loop` 的 Phase 1 是一次**同步初扫**（`WalkDir` 走一遍 `sessions/` 逐个
/// `process_session_added`），之后靠 `ReaderState` 去重。⇒ 半路接进来的客户端
/// **拿不到那次初扫**，屏上就是空的。换一份新的 = 他拿到一次完整快照，
/// 而这不需要动 `watcher.rs` 一个字节。
/// ⚠ 代价如实记：换人期间 inotify 有一个**极短的重装窗口**，那段时间的文件事件不会补发；
/// 而 Phase 1 的初扫恰好覆盖「换人之前已经存在的会话」⇒ 漏的只有「正好落在那一瞬的新会话」。
///
/// # 它**不做**什么
///
/// **不重起自己**（`K14` 裁定：第一档，自愈单独立成 `K-P3`）。宿主不在时**没有监护**，
/// 这是**如实登记的降级**，而且那句话要在 UI 上说出来（`KPY4` 钉它），不许只写在这条注释里。
async fn serve_listening(
    port: u16,
    token: String,
    hello: Frame,
    agent_home: PathBuf,
    with_bg: bool,
    tail_only: bool,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let addr = std::net::SocketAddr::new(listen::LOOPBACK, port);
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            let in_use = e.kind() == std::io::ErrorKind::AddrInUse;
            // ★★ **绑不上就退出，绝不自己换端口。**
            // 换端口 = 每台机 N 个 daemon，各自往 tmux server 装 `[50]` 槽位的全局 hook
            // 互相盖（`control/tmux_hook.rs::install_hooks`，**没有关掉它的开关**，
            // 载荷里烤着那一个 daemon 的 pid+starttime）⇒ 比今天更糟。
            tracing::error!(
                "绑不上 {addr}（{e}）⇒ 退出。\n\
                 这个口上已经有东西了：宿主该**连上去读一行 hello 比对**，\n\
                 对不上就出声并拒绝，**不许静默复用**，更不许换个口再起一个。"
            );
            std::process::exit(if in_use {
                listen::EXIT_ADDR_IN_USE
            } else {
                listen::EXIT_BAD_LISTEN_CONFIG
            });
        }
    };
    tracing::info!("常驻监听口已就位：{addr}（一条流 + 不限次「只读 hello 就走」）");

    let busy = Arc::new(AtomicBool::new(false));
    let poke_slot: PokeSlot = Arc::new(std::sync::Mutex::new(None));
    let _poke_task = spawn_sigusr1_task(Arc::clone(&poke_slot));
    let set_slot = |p: Option<observe::watcher::WatcherPoke>| {
        *poke_slot.lock().unwrap_or_else(|e| e.into_inner()) = p;
    };

    let (mut idle_rx, mut idle_poke) = {
        let (rx, poke) = observe::watcher::spawn(agent_home.clone(), with_bg, tail_only);
        set_slot(Some(poke.clone()));
        (Some(rx), Some(poke))
    };

    // 容量 1：同一时刻最多只有一条连接能通过认证（`busy` 那次 `swap` 保证的）。
    let (attached_tx, mut attached_rx) = tokio::sync::mpsc::channel::<Attached>(1);
    let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(1);

    loop {
        tokio::select! {
            // ① 有连接进来。**每条连接一个 task** —— 握手不许挡住 accept，
            //    否则一条只想读 hello 的连接会把整条监听线堵住。
            accepted = listener.accept() => {
                match accepted {
                    Ok((sock, _peer)) => {
                        let candidate = !busy.swap(true, Ordering::SeqCst);
                        tokio::spawn(handshake_one(
                            sock,
                            hello.clone(),
                            token.clone(),
                            candidate,
                            Arc::clone(&busy),
                            attached_tx.clone(),
                        ));
                    }
                    Err(e) => tracing::warn!("accept 失败（{e}）；继续听"),
                }
            }
            // ② 有人认证通过 ⇒ 退掉空转那份 watcher，换一份新的给他，起流。
            Some(att) = attached_rx.recv() => {
                if let Some(p) = idle_poke.take() {
                    p.shutdown();
                }
                idle_rx = None;
                let Attached { reader, writer, hello_flushed } = att;
                let (rx, poke) = observe::watcher::spawn(agent_home.clone(), with_bg, tail_only);
                set_slot(Some(poke.clone()));
                // 应答走**独立通道**：出方向丢一条内容帧可恢复，丢一条应答会让客户端永远等下去。
                let (reply_tx, reply_rx) =
                    tokio::sync::mpsc::channel::<Frame>(inbound::REPLY_CHANNEL_CAPACITY);
                let mut inbound_task = inbound::spawn(reader, reply_tx.clone(), hello_flushed);
                let done = done_tx.clone();
                tracing::info!("一条流已接上（认证通过）");
                tokio::spawn(async move {
                    // ★★ **两个事件都算「客户端走了」，缺一个就会把那一档永久占住。**
                    //
                    // ⚠ 这一格是 `K-P1` 的 e2e **实测**逼出来的，不是设计出来的：
                    // 只等 `writer_task`（它靠**写**拿到错误才结束）时，
                    // 一个**空闲**的 daemon 根本没有东西可写 ⇒ 上一个 monitor 退了之后
                    // 那张牌**永远不还回来** ⇒ 下一个 monitor 拿到 `stream-busy`
                    // ⇒ 「换个 monitor 重开就没有本机后端了」。实测：等满 50×20ms 仍是 busy。
                    //
                    // ⇒ 再认一个事件：**入方向读到 EOF**（客户端关了它的写半边 / 进程没了）。
                    // 那与 stdio 那条载体上「stdin EOF」是同一个事实，只是这条载体上它**必须**被当真：
                    // socket 的对端关了就是走了，而 stdio 那边刻意对写端关闭不敏感
                    //（那是为了不让一次误关掉整个 daemon —— 两条载体的取舍不同，写清楚）。
                    tokio::select! {
                        _ = writer_task(writer, rx, reply_rx) => {
                            tracing::info!("流结束：写不出去了（客户端走了）");
                        }
                        _ = &mut inbound_task => {
                            tracing::info!("流结束：入方向读到 EOF（客户端关了它的写半边）");
                        }
                    }
                    // P5：**显式告诉 reader 停** —— 没有 ticker 之后它只会一直阻塞在 `recv()`。
                    poke.shutdown();
                    inbound_task.abort();
                    drop(reply_tx);
                    let _ = done.send(()).await;
                });
            }
            // ③ 流结束（客户端走了）⇒ 把牌还回去，回到空转。
            Some(()) = done_rx.recv() => {
                tracing::info!("流结束 ⇒ 回到空转：口仍在听，sessions/ 仍在看");
                busy.store(false, Ordering::SeqCst);
                let (rx, poke) = observe::watcher::spawn(agent_home.clone(), with_bg, tail_only);
                set_slot(Some(poke.clone()));
                idle_rx = Some(rx);
                idle_poke = Some(poke);
            }
            // ④ 空转期把 watcher 的帧读出来丢掉。**没人要它们**，
            //    但不读的话 10_000 容量的通道会填满并开始记 `Overflow`，
            //    而那本账是给「有客户端在听」那一档记的 —— 空转期记它没有意义。
            f = async { idle_rx.as_mut().expect("上面刚判过 is_some").recv().await }, if idle_rx.is_some() => {
                if f.is_none() {
                    tracing::warn!("空转期的 watcher 自己结束了 ⇒ 这台机的 @ccm_sid 打标停了");
                    idle_rx = None;
                    idle_poke = None;
                    set_slot(None);
                }
            }
        }
    }
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

/// 解析会话数据根。
///
/// ⚠ `S3` 把**怎么解析**搬进了 `agents/claudecode/paths.rs`（环境变量名与目录名是
/// Claude 的知识）。这里只剩"去问适配层" —— 今天 daemon 只服务一种 agent，所以是写死的一句。
///
/// ⚠〔`S5` 08-14 订正〕原注这里写着「`S5` 落地时它会变成按 kind 取」——**`S5` 没有那么做，
/// 而且这条订正比原话更要紧**：本函数要的是**恒定**答得出的那个 home（流式 watcher 与
/// 所有一次性子命令都拿它当根），而 `agents::visible_homes()` 只报**看得见**的那些
///（home 目录不存在就一条都不报）。两者语义不同 ——
/// 把这里换成"按 kind 取"会让 `~/.claude` 还没建出来的新机器上 daemon 直接失根，
/// 而它原本是能正常起来、等 inotify 等到第一个会话的。
/// ⇒ 真正会变的是**别处**：`main` 里 `homes:` 那一行（见上）。归 `S6`/`L2` 的接口那轮再看。
fn resolve_agent_home() -> PathBuf {
    agents::claudecode::paths::resolve_home()
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
