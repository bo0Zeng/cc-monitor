//! Phase-0 daemon watcher: tails `<claude_dir>/projects/**.jsonl` plus the
//! `<claude_dir>/sessions/<PID>.json` files and turns filesystem activity into
//! [`Frame`]s on a bounded channel.
//!
//! # Architecture (the §5.4 slow-consumer guard)
//!
//! This module is split into two halves joined by a **bounded** `mpsc` channel
//! (see [`spawn`]):
//!
//! - the **reader** ([`watch_loop`]) owns the `notify-debouncer-mini` watcher,
//!   does the incremental per-file offset reads, assigns `seq`s, and *sends*
//!   [`Frame`]s into the channel via [`FrameSink`]. It uses `try_send`, so a full
//!   channel drops the frame rather than ever blocking the notify callback — but
//!   it **counts** the dropped frames and emits a [`Frame::Overflow`] signal once
//!   （audit-0805 F03：**不可恢复**的那些还会带上身份 `lost`，见 `LOST_IDENTITY_CAP`）
//!   the channel drains (#32), so the client can warn that live lines were lost.
//! - the **writer** ([`crate::main`]'s stdout task) drains the channel and
//!   writes one wire line per frame. A slow SSH pipe back-pressures the channel
//!   (the writer awaits on a full pipe), and the bound on the channel means
//!   that back-pressure stops at the channel — it never reaches the inotify
//!   reader, so the kernel inotify queue is the only thing that can overflow.
//!
//! The reader runs on a dedicated blocking thread (`notify-debouncer-mini` is a
//! synchronous, `std::sync::mpsc`-based API) and talks to the async writer
//! through `tokio::sync::mpsc`.
//!
//! # Parity with `../src-tauri/src/watcher.rs`
//!
//! The incremental read mirrors `process_file`: a per-file [`ReadCursor`],
//! read from `cursor.consumed` up to the **last `\n`** in the new region — a
//! torn tail without a trailing `\n` is deferred to the next event, never
//! emitted half-way (Batch4-F14). BOM strip via
//! `trim_start_matches('\u{feff}')`, skip blank lines, and `is_subagent_path`
//! excludes any path containing a `subagents` segment. Truncation is detected
//! against `cursor.seen_len` (the observed EOF high-water mark, which covers a
//! deferred torn tail); on truncation the cursor resets to byte 0 **but the
//! per-file seq keeps climbing** (the seq comes from [`SeqCounter`], which is
//! never reset) — see [`read_new_lines`].
//!
//! Known non-parity (accepted): on a mid-read I/O error the monitor keeps the
//! complete lines it already consumed and advances the cursor past them, while
//! this daemon gives up the whole pass (cursor untouched). Both are
//! at-least-once-safe.
//!
//! ⚠ 这段话此前写的是 "reads via one `fs::read` snapshot" —— 那**曾经是真的**，
//! 而它只评了**错误语义**那一面，对**内存与 IO 后果一个字没记**（audit-0805 B-4）：
//! 每个 debounce 事件把整份 jsonl 读进内存，257 MB 的活跃会话 ⇒ 每次读 257 MB，
//! 只为提取约 500 字节的新行；而本地 monitor 同一件事一直是 `seek` + `read_until`
//! ⇒ **强机器流式、弱机器整读，正好反了**（daemon 住树莓派那类小机器）。
//! F04 已改成只读新字节（`read_tail_from` + `read_new_lines_at`），
//! 由 `the_two_tail_readers_do_not_slurp_the_whole_session_file` 钉住不许改回去。
//! ★ 留这段话是因为**「未登记的缺陷」比「登记过的取舍」更难发现** ——
//! 当时那条头注读起来完全像一条深思熟虑的取舍。

use crate::wire::{Frame, LostFrame, RemovalCause, SeqCounter};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

// U2：平台原语搬进 `platform/`（§1.1 第一条解耦线）。这里 re-import 回来，
// 调用点一处未改 —— 纯重构，行为逐字不变。
use crate::platform::paths::path_key;
use crate::platform::proc::{pid_alive, proc_cmdline, proc_starttime, start_epoch_from_ticks};
// 只在测试段用到的几个（生产段的调用点随函数一起搬走了）。分开写而不是给整条 `use`
// 加 `#[cfg(test)]`：上面那四个生产段真在用，混在一起会让「谁是生产依赖」看不出来。
// U4a：`pidfd_open` 只在 Linux 上存在（`platform/pidwatch/linux.rs`）。
// 少这个 cfg 会让 `cargo check --all-targets --target …windows-msvc` 红 —— 而那正是
// §1.1 第一条解耦线的**唯一真判据**（计划自审 §0.5-3：cfg 位置扫描抓不到 pidfd）。
#[cfg(test)]
use crate::platform::liveness::is_same_live_process;
#[cfg(all(test, target_os = "linux"))]
use crate::platform::pidwatch::pidfd_open;
#[cfg(test)]
use crate::platform::proc::{parse_btime, parse_starttime_from_stat, session_alive};
use std::time::Duration;
use tokio::sync::mpsc;
use walkdir::WalkDir;

// ============ P2（zero-poll-liveness）：统一事件 channel + pidfd 判活 ============
//
// **账本第 1 行的最终形态在这里建立**（`.claude/planned-build/zero-poll-liveness/MASTERPLAN.md` §3）：
// `watch_loop` 阻塞在**无超时 `recv()`** 上、消费**单一** `mpsc<WatchEvent>`。
// 所有事件源（notify / pidfd / tmux 探测）都往这一个 channel 发。
//
// **给 P3/P4 的硬约束**：往里**加事件源**，**不许**各自再挂一条独立线程 + 定时器
// ——那正是"补丁叠补丁"。加一个 `WatchEvent` 变体 + 一个发送方即可。
//
// P2 之前这里有两条轮询：
// - **轮询 A**（本功能消掉）：循环 tick 2s（`recv_timeout` 的超时值本身）驱动的判活扫描，
//   遍历 `state.sessions` 调 `session_alive` 检「pidfile 还在但 PID 已死」+ PID 复用。
// - ~~**轮询 B**：`TMUX_EMIT_INTERVAL` = 8s 的 `tmux ls`~~ **P5 已删**。P2 曾把它从
//   "主循环里的节流判断"搬进一条独立 ticker 线程（见 `spawn_tmux_ticker`），
//   使主循环**现在**就是最终形态；P5 删 ticker 线程即可，不必再动循环结构。

/// P2：`watch_loop` 消费的统一事件。
enum WatchEvent {
    /// 文件系统事件（`notify-debouncer-mini` 经 [`DebouncerSink`] 转投进来）。
    Notify(DebounceEventResult),
    /// **P3：tmux server 的 pidfd 醒了** —— 那个 server 进程实例已退出 ⇒ 等价于零会话。
    ///
    /// 带 `pid` 同样是为了挡陈旧唤醒（server 复活后是**新** pid，旧看守迟到的事件要被忽略）。
    TmuxServerGone { pid: u32 },
    /// **pidfd 醒了**：这个 pidfile 当时追踪的那个进程实例已退出。
    ///
    /// 带 `pid` 是为了挡**陈旧唤醒**：同一 pidfile 路径可能已换成别的 pid
    /// （`/clear` 原地换 sid、PID 复用写同路径），或已被移除。消费侧比对
    /// `state.sessions[key].pid == pid` 才退休 ⇒ 天然幂等。
    PidDied { key: PathBuf, pid: u32 },
    /// 一次性 `tmux ls` 探测线程的结果。
    TmuxObserved(TmuxProbe),
    /// tmux 探测节拍——**本 crate 剩下的唯一定时器**，住在独立 ticker 线程里。**P5 删。**
    TmuxProbeDue,
    /// **P4：外部戳一下** —— 语义与 [`WatchEvent::TmuxProbeDue`] **完全相同**（立刻重探
    /// tmux 并走同一条差分/发帧路径），区别只在**来源**：这个是事件驱动的（tmux hook
    /// → `--tmux-notify` → `SIGUSR1` → 这里），那个是定时器。
    ///
    /// **为什么不复用 `TmuxProbeDue` 就完事**：两者共用一条处理路径是对的，但**来源要能
    /// 分辨** —— P5 删掉 ticker 之后，「还有没有人在戳」是判断 hook 通路是否活着的唯一线索；
    /// 混成一个变体就再也分不出「hook 没装上」与「ticker 还在兜底」。
    Poke,
    /// 预留给 P5：删掉 ticker 之后，主循环需要一条**显式**的停机信号才能及时
    /// 发现 stdout 写端已关（`sink.is_closed()` 现在靠 ticker 每 8s 醒一次来复查）。
    ///
    /// **P5 已接上**：`main` 在 writer 结束 / 收到停机信号后经 [`WatcherPoke::shutdown`]
    /// 发它。没有它的话，删掉 ticker 之后 reader 线程会一直阻塞在 `recv()`
    ///（进程退出时才随之消亡——不是泄漏，但「没人听就停读」这条性质会丢）。
    Shutdown,
}

/// P3：主循环对 tmux server 的认知。**三值**——`Unknown` 与 `Gone` 必须分开：
/// 只有在 `Alive` 时才敢把「`tmux ls` rc=1」判成真异常（见 `classify_with_server_state`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerState {
    /// 还没探到过（daemon 刚起 / 探测失败）。
    Unknown,
    /// 已探到并挂了 pidfd 看守。
    Alive(u32),
    /// pidfd 报过死，或探测明确说没有 server。
    Gone,
}

/// P3：**收紧 P1 那处刻意的保守**。
///
/// P1 把 `tmux ls` rc=1 一律判成"确证零会话"，并留了一条注释说
/// 「P3 持有 pidfd 后可把『server 活着但 rc=1』归 `Unobservable`」。这里落地。
///
/// **实现上刻意不依赖"pidfd 是否已经醒过"**——那会有个危险的失效模式：若 pidfd 路
/// 因任何原因没醒，状态永远停在 `Alive`，rc=1 就被永久压成 `Unobservable` ⇒ 永不 retire。
/// 改成**直接查 `/proc` 里那个 server pid 还在不在**：
/// - 不在了 ⇒ 就是真的没 server（`NoServer` 原样通过，顺带把状态推到 `Gone`）
/// - 还在 ⇒ 「server 明明活着、`tmux ls` 却连不上」= 真异常（socket 权限/被换掉之类）
///   ⇒ `Unobservable`（保守跳过，不误 retire）
///
/// 一次 `/proc` 存在性读，无定时器、无状态耦合、无挂死风险。
fn classify_with_server_state(obs: TmuxObservation, server: ServerState) -> TmuxObservation {
    match (&obs, server) {
        (TmuxObservation::NoServer, ServerState::Alive(pid)) if pid_alive(pid) => {
            tracing::warn!(
                "tmux ls 报 rc=1 但记着的 server pid {pid} 仍在 /proc ⇒ 观测无效（不当零会话）"
            );
            TmuxObservation::Unobservable
        }
        _ => obs,
    }
}

/// P2：把 debouncer 的事件转投进统一 channel（零额外线程——`notify` 本来就在自己的
/// 线程里回调 handler，这里只是换个投递目标）。
struct DebouncerSink(std::sync::mpsc::Sender<WatchEvent>);

impl notify_debouncer_mini::DebounceEventHandler for DebouncerSink {
    fn handle_event(&mut self, event: DebounceEventResult) {
        // 接收端已走（reader 退出）⇒ 丢弃即可，不 panic。
        let _ = self.0.send(WatchEvent::Notify(event));
    }
}

/// P3：pidfd 看守的目标——决定醒了之后发哪个事件。**让 `spawn_pid_watcher` 一份实现服务
/// 两种目标 ⇒ 全 crate 只有一处 pidfd 的 `unsafe`。**
#[derive(Debug, Clone, PartialEq, Eq)]
enum PidWatchTarget {
    /// 会话进程（P2）：醒了发 `PidDied { key, pid }`。
    Session { key: PathBuf },
    /// tmux server（P3）：醒了发 `TmuxServerGone { pid }`。
    TmuxServer,
}

impl PidWatchTarget {
    fn death_event(&self, pid: u32) -> WatchEvent {
        match self {
            PidWatchTarget::Session { key } => WatchEvent::PidDied {
                key: key.clone(),
                pid,
            },
            PidWatchTarget::TmuxServer => WatchEvent::TmuxServerGone { pid },
        }
    }
}

/// **薄包装**：把 `platform::pidwatch::watch_pid_until_exit` 的「判死」回调
/// 落成本模块的域事件。U2 切分后本函数**只剩这一件事** —— 平台那半（pidfd_open /
/// 身份复核 / poll / 起线程）在 `platform/pidwatch.rs`，它不知道 `WatchEvent` 是什么。
fn spawn_pid_watcher(
    target: PidWatchTarget,
    pid: u32,
    expected_start: Option<u64>,
    tx: std::sync::mpsc::Sender<WatchEvent>,
) {
    crate::platform::pidwatch::watch_pid_until_exit(pid, expected_start, move || {
        let _ = tx.send(target.death_event(pid));
    });
}

/// P4b：给当前 tmux server 装通知 hook。**尽力而为**：拿不到自己的 exe 路径 /
/// starttime 就跳过（那说明 `/proc` 不可用，整条 pidfd 路本来也不成立）。
///
/// 装的是**我们自己进程**的 pid + starttime —— hook 子进程据此校验身份再发 SIGUSR1，
/// 挡的是「daemon 退出后 pid 被复用，hook 误伤无关进程」。
fn install_tmux_hooks_best_effort() {
    let me = std::process::id();
    let (Ok(exe), Some(start)) = (std::env::current_exe(), proc_starttime(me)) else {
        tracing::warn!("拿不到自身 exe/starttime ⇒ 跳过装 tmux hook（退回定时探测）");
        return;
    };
    let n = crate::control::tmux_hook::install_hooks(&exe, me, start);
    tracing::info!("tmux hook 已装 {n}/3（会话生/死/改名 → SIGUSR1 → 立刻重探）");
}

/// P2：挂 pidfd 看守的幂等入口。`events_tx` 为 `None`（单元测试）时什么都不做。
fn arm_pid_watcher(key: &Path, pid: u32, expected_start: Option<u64>, state: &mut ReaderState) {
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    // F11：键含 `expected_start` ⇒ **同 pid 但换了进程实例（PID 复用）也会重新挂**。
    if !state
        .pid_watched
        .insert((key.to_path_buf(), pid, expected_start))
    {
        return; // 这个 (pidfile, pid, starttime) 已经挂过了
    }
    // ★★ `P0b-Y2` 第十五拍〔08-13〕：**这一步原先一行日志都不打。**
    //
    // 第十四拍在可信台架上复现了 `#60`：全链里杀掉 claude **一帧 removed 都没有**，
    // 而**同一个二进制离线三种旗标组合全都发得出来** ⇒ 差别在全链那条路上。
    // 但「pidfd 看守到底有没有挂上」从外面**看不见** —— 只能推断，不能读数。
    // ⇒ 与第十一拍「removal 那跳不可观测」同一族：先让它可观测，再谈根因。
    // ⚠ 量级：每个会话一次（`pid_watched` 挡住重复），不会淹日志。
    tracing::info!(
        "pidfd 看守已挂: pid={pid} start={expected_start:?} key={}",
        key.display()
    );
    spawn_pid_watcher(
        PidWatchTarget::Session {
            key: key.to_path_buf(),
        },
        pid,
        expected_start,
        tx,
    );
}

/// **P5：一次性初探**（取代 P2 那个 8s ticker 线程）。
///
/// **删 ticker 时差点顺手删掉的东西**：它除了打节拍，还承担「**首轮立即发一拍**」——
/// monitor 一连上就该拿到 tmux 状态。全删的话，daemon 要等到第一个 hook 触发才会探，
/// 空闲机器上可能是**永远**。所以节拍没了，但这一拍要留下。
///
/// 之后的每一拍都由事件驱动：tmux hook → `--tmux-notify` → SIGUSR1 → `WatchEvent::Poke`
/// （P4），server 生死由 pidfd / socket inotify 管（P3）。**零定时器。**
fn initial_tmux_probe(tx: &std::sync::mpsc::Sender<WatchEvent>) {
    // send 失败 = reader 已经走了 ⇒ 无所谓。
    let _ = tx.send(WatchEvent::TmuxProbeDue);
}

/// Bounded channel capacity between the reader and the stdout writer.
///
/// Large enough to absorb a `/resume` history burst without dropping, small
/// enough that a wedged writer cannot grow memory without bound. A full channel
/// drops frames with a warning (Phase-0 gap, see module docs).
pub const CHANNEL_CAPACITY: usize = 10_000;

/// notify-debouncer-mini debounce window, matching `../src-tauri/src/watcher.rs`.
const DEBOUNCE_MS: u64 = 100;

/// B2：`tmux ls -F` 格式串——**与 monitor `tmux::TMUX_LS_FMT` 逐字对齐**（真 TAB 分列，monitor
/// `parse_tmux_ls` 靠它解析）。name⇥path⇥cmd⇥attached⇥windows⇥@ccm_sid。**改此须同步 monitor（双写点）。**
const TMUX_LS_FMT: &str = "#{session_name}\t#{pane_current_path}\t#{pane_current_command}\t#{?session_attached,1,0}\t#{session_windows}\t#{@ccm_sid}";

// ---------- P1（zero-poll-liveness）：`TmuxSessions.observation` 的取值 ----------
//
// **双写点**：与 monitor `src-tauri/src/tmux.rs` 的同名 const 逐字节一致，由 monitor 侧
// `observation_tokens_double_write_point_stays_in_sync` 测试钉住（`include_str!` 读本文件 +
// 锚定 const 定义行）。**改本处必须同步 monitor**，同 `TMUX_LS_FMT` 的纪律。
/// daemon 确证零会话（rc=0 但 stdout 空 = `exit-empty off`；或 rc=1 = server 不在）。
const OBS_ZERO_SESSIONS: &str = "zero_sessions";
/// 远端没装 tmux——与既有 `NO_TMUX` 哨兵同义，显式化。
const OBS_NO_TMUX: &str = "no_tmux";
/// 观测无效（`tmux ls` 以非 0/1 退出、或 exec 本身失败）⇒ monitor 必须跳过，绝不当零会话。
const OBS_UNOBSERVABLE: &str = "unobservable";

/// P1（zero-poll-liveness）：探测脚本里「PATH 中没有 tmux」的约定退出码。
///
/// **为什么要一个专用 rc 而不是让脚本 `printf 'NO_TMUX'`**：P1 之前脚本用
/// `tmux ls … || true` 把 tmux 自己的 rc **吞掉了**，于是「零会话」「`tmux ls` 出错」
/// 「exec 失败」三种语义全压成同一个空串，monitor 只能一律保守跳过 ⇒ 就是
/// `doc/INVARIANTS.md` §24bis 那条残留 bug 的根。改成 `exec tmux …` 让 tmux 的 rc
/// 原样成为 `sh` 的 rc，无 tmux 那格才需要一个不与 tmux 冲突的自定义值。
///
/// 97 是任意选的哨兵值（tmux 只用 0/1）。
const TMUX_PROBE_NO_TMUX_RC: i32 = 97;

/// P1：`tmux ls` 一次观测的四态分类（**P0 实测定死**，见
/// `.claude/planned-build/zero-poll-liveness/features/P0-machine-facts.md` §3 ④）。
///
/// P0 实测的状态空间（隔离 socket）：
/// - rc=0 + stdout 非空 → 有会话
/// - rc=0 + stdout 空 → **server 活着但零会话**（只在 `exit-empty off` 下出现；
///   默认 `exit-empty on` 时 server 随最后一个会话一起退出，走下一格）
/// - rc=1 → server 不在（socket 存在但无 server / socket 根本不存在，两种 stderr 措辞）
/// - 其他 rc → 观测无效
///
/// **前三格里后两格对 retire 决策完全等价**（都是"零会话"），区别只对 P3 的复活监视有意义
/// ⇒ 折成一个 `ZeroSessions`，P3 加细分时**不必改帧契约**。
#[derive(Debug, Clone, PartialEq, Eq)]
enum TmuxObservation {
    /// rc=0 + stdout 非空：`tmux ls -F` 原文。
    Sessions(String),
    /// **P3 细分**：rc=0 但 stdout 空 = **server 活着、零会话**（只在 `exit-empty off` 下出现）。
    ServerEmpty,
    /// **P3 细分**：rc=1 = **server 不在**（socket 存在但无 server / socket 根本不存在）。
    ///
    /// 与 `ServerEmpty` **在 wire 上是同一个取值**（`zero_sessions`）——两者对 retire 决策
    /// 完全等价，区别只对 P3 的复活监视与"真异常"判定有意义。**这正是 P0/P1 预判的
    /// "P3 加细分时不必改帧契约"**：`observation_to_frame` 把两者映射到同一格。
    NoServer,
    /// PATH 里没有 tmux。
    NoTmux,
    /// 观测无效（非 0/1/97 的 rc、被信号杀、exec 失败）⇒ monitor 必须跳过。
    Unobservable,
}

/// P1：`sh -c` 探测脚本。`command -v` 门控解析 PATH（同 monitor `list_remote_tmux`）；
/// **`exec` 让 tmux 的 rc 原样成为 sh 的 rc**（这是 P1 的关键改动——原先 `|| true` 吞了 rc）。
///
/// **提成独立函数是为了可测**：真机 tmux 的四种 rc 由 P0 实测过，但脚本本身（`command -v`
/// 门控 + `exec` 的 rc 透传）要能在 CI 上用**假 tmux** 验证，不能只信字符串断言。
/// ★★ **探测必须有上界**〔audit-0805 F09 / 报告 I-2〕。
///
/// # 它此前会永久卡死整条 tmux 观测
///
/// [`run_tmux_ls`] 的 `output()` 是**无超时**阻塞调用（它自己的头注就这么写着：
/// 远端 tmux 卡死时「会永不返回」）。它跑在一次性后台线程里，所以不会冻住 reader ——
/// **但 `watch_loop` 的 `tmux_inflight` 去重标志置位三处、只在收到 `TmuxObserved` 时清一处**。
/// 线程永不返回 ⇒ 那一帧永不到达 ⇒ 标志**永远为真** ⇒ **此后一次 tmux 探测都不会再发起**，
/// 而且**不发任何理由帧**（撞定框 **E4**：静默失败一律给身份）。
///
/// # 为什么把上界放在 shell 里而不是 Rust 里
///
/// Rust 侧加超时要引入一个 `Duration::from_*`，而 daemon 有**零定时器**硬门禁
/// （`no_timer_guard`：生产段 `Duration::from_*` 的处数必须**恰好等于**登记表条数，
/// 「多一处就红」「登记表不是豁免清单」）。承接 **C8/C12** 与定框 **E6**。
/// ⇒ 用 `timeout(1)`：**没有 Rust 定时器、没有新线程、不改线程模型**，
/// 卡死变成一次有界失败 ⇒ `TmuxObserved` 照常到达 ⇒ 标志被清 ⇒ 观测能自愈。
///
/// # `timeout` 不在时怎么办
///
/// 用 `command -v` 门控（同这段脚本对 `tmux` 本身的做法），拿不到就**退回原样**。
/// 这是**诚实降级**（承接 **C7**）：在没有 `timeout` 的系统上行为与从前一字不差，
/// 而不是假装有上界。⚠ 代价要说清：那些系统上 I-2 **仍然存在**。
fn tmux_probe_script() -> String {
    format!(
        "if command -v tmux >/dev/null 2>&1; then \
           if command -v timeout >/dev/null 2>&1; then \
             exec timeout -s KILL {TMUX_PROBE_TIMEOUT_SECS} tmux ls -F '{TMUX_LS_FMT}' 2>/dev/null; \
           else \
             exec tmux ls -F '{TMUX_LS_FMT}' 2>/dev/null; \
           fi; \
         else exit {TMUX_PROBE_NO_TMUX_RC}; fi"
    )
}

/// 探测的墙钟上界（秒）〔audit-0805 F09〕。
///
/// `tmux ls` 在健康机器上是毫秒级；给到 5 秒是为了容忍一次慢盘/高负载，
/// 又远短于「用户会注意到 tmux 面板不更新」的时间尺度。
/// ⚠ **超时后的 rc 会落进 [`classify_tmux_probe`] 的 `Unobservable`**（`-s KILL` ⇒ `code == None`
/// 或 124）—— 那正是「观测无效」该有的语义，**不是**「零会话」（后者会误 retire 活会话）。
const TMUX_PROBE_TIMEOUT_SECS: u32 = 5;

/// P1：把探测的 (rc, stdout) 折成四态。**纯函数、可单测**（判据只有 rc + stdout 空否，
/// 刻意**不看 stderr**——P0 实测 stderr 有两种措辞，且拿英文消息当判据本身就是错的）。
///
/// `code == None` = 被信号杀（如 tmux 卡死后探测线程连带被清）⇒ 观测无效。
fn classify_tmux_probe(code: Option<i32>, stdout: &str) -> TmuxObservation {
    match code {
        Some(0) if stdout.trim().is_empty() => TmuxObservation::ServerEmpty,
        Some(0) => TmuxObservation::Sessions(stdout.to_string()),
        // rc=1 = server 不在。**一处刻意的保守**：socket 权限异常这类罕见情形也会落这里
        // ⇒ 理论上可能误 retire。缓解：socket 路径 uid 隔离（`/tmp/tmux-<uid>/`），同 uid 下
        // 权限异常几乎不可能。**P3 落地后有更强判据**：那时 daemon 持有 server 的 pidfd，
        // 「pidfd 说 server 活着但 tmux ls rc=1」= 真异常 ⇒ 归 `Unobservable`。
        // 该升级**不改帧契约**（`ZeroSessions` 语义不变），所以 P1 现在就能安全落地。
        Some(1) => TmuxObservation::NoServer,
        Some(TMUX_PROBE_NO_TMUX_RC) => TmuxObservation::NoTmux,
        _ => TmuxObservation::Unobservable,
    }
}

/// B2：在**本机**（daemon 就在远端主机）跑 `tmux ls` 取观测。`sh -c` + `command -v` 门控
/// （同 monitor `list_remote_tmux` 命令）解析 PATH。**只读**（tmux ls 不改任何状态）。
///
/// P1 起返回四态分类而非裸 `String`——见 [`TmuxObservation`]。
///
/// **无超时**：`output()` 是无超时阻塞调用，远端 tmux 卡死（D-state/socket 卡住/NFS home）时会永不返回。
/// 故**只能在一次性后台线程里调用**（见 `watch_loop` 的 `tmux_inflight`），**绝不可**直接跑在 watch_loop
/// 线程上——否则会冻结整个 reader（Line/notify/判活全停）。
fn run_tmux_ls() -> TmuxObservation {
    match std::process::Command::new("sh")
        .arg("-c")
        .arg(tmux_probe_script())
        .output()
    {
        Ok(out) => classify_tmux_probe(out.status.code(), &String::from_utf8_lossy(&out.stdout)),
        Err(e) => {
            tracing::warn!("tmux ls 本地执行失败: {e}");
            TmuxObservation::Unobservable
        }
    }
}

/// P3：一次 tmux 探测的完整结果——观测分类 + （能拿到时）server 的 pid 与 socket 路径。
///
/// **为什么和 `tmux ls` 同一趟拿**：`display-message` 是另一个 subprocess，而
/// `run_tmux_ls` 头注那条约束（无超时 ⇒ 只能在一次性后台线程里跑）对它同样适用。
/// 放同一个探测线程里 ⇒ 不新增线程、不阻塞主循环（P2 建立的硬约束）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct TmuxProbe {
    obs: TmuxObservation,
    /// tmux server 进程的 pid（`#{pid}`）——给 pidfd 看守用。拿不到 = 没有 server。
    server_pid: Option<u32>,
    /// 这个 server 的 socket 绝对路径（`#{socket_path}`）——给"复活"inotify 用。
    socket_path: Option<PathBuf>,
}

/// P3：问 tmux server 要 pid 与 socket 路径。**只读**、无副作用；
/// **死 socket 上调用不会把 server 拉活**（P0 实测）。
///
/// 与 `run_tmux_ls` 同一套 `sh -c` + `command -v` 门控；rc≠0（没有 server）⇒ 全 None。
fn query_tmux_server() -> (Option<u32>, Option<PathBuf>) {
    // 一行两列（TAB 分隔），避免两次 subprocess。
    let script = "if command -v tmux >/dev/null 2>&1; then exec tmux display-message -p '#{pid}\t#{socket_path}' 2>/dev/null; else exit 97; fi";
    let out = match std::process::Command::new("sh")
        .arg("-c")
        .arg(script)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("tmux display-message 执行失败: {e}");
            return (None, None);
        }
    };
    if out.status.code() != Some(0) {
        return (None, None); // 没有 server / 没有 tmux
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next().unwrap_or("");
    let mut it = line.split('\t');
    let pid = it.next().and_then(|x| x.trim().parse::<u32>().ok());
    let sock = it
        .next()
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(PathBuf::from);
    (pid, sock)
}

/// 把 watch 挂到某个目录上，**可重入**：目录没了就翻记账，在就先 `unwatch` 再 `watch`。
///
/// # 为什么每个挂点都要这一套
///
/// inotify 的 watch 绑在 **inode** 上。本仓 08-13 一天之内在**三处**踩到同一个形状：
/// `sessions/`（第十拍）· tmux socket 目录（第二十二拍）· `projects/`（本拍）——
/// 症状都是「目录被删掉再重建之后，那一路的帧永远不来，而且没有任何错误」。
/// ⇒ 与其每处各写一遍，不如**只有一份**：改一处漏两处正是本仓一路在收的那族。
///
/// ⚠ 不做重扫：`sessions/` 那侧需要「挂上顺带把已有 pidfile 过一遍」，那是它**特有**的
/// （见 `rewatch_sessions`）；`projects/` 的历史由 `process_jsonl` 按 sid 决定要不要读，
/// 在这里重扫会把**整棵历史**拉一遍 —— 那正是 `P0` 立件时要消灭的行为。
fn rewatch_dir(
    debouncer: &mut notify_debouncer_mini::Debouncer<impl notify::Watcher>,
    dir: &Path,
    watched: &mut bool,
    mode: RecursiveMode,
) {
    if !dir.is_dir() {
        if *watched {
            tracing::info!("目录消失了 {} —— 解除记账，等它回来再挂", dir.display());
            let _ = debouncer.watcher().unwatch(dir);
            *watched = false;
        }
        return;
    }
    let _ = debouncer.watcher().unwatch(dir);
    match debouncer.watcher().watch(dir, mode) {
        Ok(()) => {
            if !*watched {
                tracing::info!("已（重新）挂上 watch: {}", dir.display());
            }
            *watched = true;
        }
        Err(e) => {
            *watched = false;
            tracing::warn!("挂 watch 失败 {}: {e}（下次事件再试）", dir.display());
        }
    }
}

/// ★★ `P0b-Y2` 第十六拍〔08-13〕：**tmux socket 目录的推算规则**（零 server 时唯一的耳朵）。
///
/// # 病（`#60` 的根因）
///
/// `P5` 删掉 8s ticker 之后 daemon **零定时器**，之后每一拍都靠事件。而两条唤醒路
/// **都以「已经见过 server」为前提**：tmux hook 是**观测到 server 那一刻**才装的；
/// socket 目录的 inotify 路径是从 `probe.socket_path` 里拿的 —— 没 server 就没有路径。
///
/// ⇒ **daemon 起得比 tmux server 早 = 永远不再探**。08-13 全链实测：`tmux_sessions` 帧
/// 只有一帧且内容是 `zero_sessions`，`已给 tmux server … 挂 pidfd 看守` 与 `tmux hook 已装`
/// 一行都没有；后来建的会话它一无所知 ⇒ `@ccm_sid` 到不了 monitor ⇒ 死亡一律判归档，
/// **灰灯永不出现**（`#60` 现象 1）。
///
/// # 规则按 tmux 自己的来
///
/// tmux 的 socket 落在 `${TMUX_TMPDIR:-/tmp}/tmux-<uid>/`。**不硬编码 `/tmp`** ——
/// 那会在设了 `TMUX_TMPDIR` 的机器上监视错目录，而且失败是静默的。
/// ⚠ 这里只**读**（挂 inotify），不发任何 tmux 命令 ⇒ 与 `C7i` 无关
/// （那条红线管的是我们自己发 tmux 命令时必须带 socket 选择器）。
/// 把 watch 挂到 socket 目录**本身**（目录在才挂，幂等）。见调用点的两段头注。
fn watch_sock_dir_if_present(
    debouncer: &mut notify_debouncer_mini::Debouncer<impl notify::Watcher>,
    sock_dir: &Path,
    watched: &mut bool,
) {
    // ★★ **目录没了要把记账翻回去**〔08-13 实测补〕：socket 目录**整个被删掉再重建**
    //    之后是**另一个 inode**，而我们的 watch 还挂在已删的那个上 ——
    //    与 `sessions/` 那个 inode bug **同型，只是高一层**。
    //    实测：删掉目录、再起一个新 tmux server ⇒ daemon **一帧都收不到**（`sid-two` 命中 0）。
    //    ⇒ `*watched` 必须跟着盘上的事实走，否则下面那个 `if *watched` 会永远短路。
    if !sock_dir.is_dir() {
        if *watched {
            tracing::info!("tmux socket 目录消失了 {} —— 解除记账，等它回来再挂", sock_dir.display());
            let _ = debouncer.watcher().unwatch(sock_dir);
            *watched = false;
        }
        return;
    }
    // 目录在。**无条件先 unwatch 再 watch**（分辨不出「同 inode 的普通事件」与「换了 inode」，
    // 而重挂同一个 inode 无害、漏挂新 inode 致命）——与 `rewatch_sessions` 同一条纪律。
    let _ = debouncer.watcher().unwatch(sock_dir);
    match debouncer
        .watcher()
        .watch(sock_dir, RecursiveMode::NonRecursive)
    {
        Ok(()) => {
            let first = !*watched;
            *watched = true;
            let _ = first;
            tracing::info!(
                "已监视 tmux socket 目录 {}（零 server 时的唯一耳朵）",
                sock_dir.display()
            );
        }
        Err(e) => tracing::warn!("监视 {} 失败: {e}", sock_dir.display()),
    }
}

fn tmux_socket_dir() -> PathBuf {
    let base = std::env::var_os("TMUX_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    // ⚠⚠ **`libc::getuid` 只在 unix 存在** —— daemon 是**跨平台编**的（`scripts/verify-committed-state.sh`
    //   会对 `x86_64-pc-windows-msvc` 跑一次 `cargo check --all-targets`）。
    //   首版直接 `unsafe { libc::getuid() }`，**本机 cargo test 全绿、Windows 侧编不过**
    //   —— 而那道门量的是**提交状态**，本会话所有工作树读数都看不见它。
    // ⚠ Windows 上没有 tmux，这条路本来就走不到；给个哑值只为**让它编得过**，
    //   而不是假装那里有个 socket 目录。
    #[cfg(unix)]
    // SAFETY: getuid 无副作用、不会失败。
    let uid = unsafe { libc::getuid() };
    #[cfg(not(unix))]
    let uid: u32 = 0;
    base.join(format!("tmux-{uid}"))
}
/// P3：一次完整探测（跑在一次性后台线程里）。
///
/// 只在"可能有 server"时才问 pid/socket——`NoTmux`/`Unobservable` 下问了也是白问。
/// 注意 **`ServerEmpty` 也要问**：`exit-empty off` 下 server 活着但零会话。
fn run_tmux_probe() -> TmuxProbe {
    let obs = run_tmux_ls();
    let (server_pid, socket_path) = match &obs {
        TmuxObservation::Sessions(_) | TmuxObservation::ServerEmpty | TmuxObservation::NoServer => {
            query_tmux_server()
        }
        TmuxObservation::NoTmux | TmuxObservation::Unobservable => (None, None),
    };
    TmuxProbe {
        obs,
        server_pid,
        socket_path,
    }
}

/// P1：四态 → wire。**`raw` 载荷刻意与 P1 之前逐字节一致**，新信息全部走 additive 的
/// `observation` 字段 ⇒ **旧 monitor 行为零变化**（有会话时它照旧解析 raw；零会话/出错时
/// 它看到空 raw、照旧保守跳过 = 今天的行为，无回归）。新 monitor 读 `observation` 才能
/// 区分"确证零会话"与"观测失败"，从而修掉灰灯卡死。
/// P5：从 `tmux ls` 的 `raw` 里取**会话名集合**。
///
/// 只依赖「名字在第一列」这一点 —— 列的构成由 `TMUX_LS_FMT` 定（**红线：不改它**），
/// 这里刻意不解析其余列，免得再造一处对格式的依赖。
fn session_names(raw: &str) -> std::collections::BTreeSet<String> {
    raw.lines()
        .map(|l| l.split('\t').next().unwrap_or(""))
        .map(str::trim)
        .filter(|n| !n.is_empty() && *n != "NO_TMUX")
        .map(str::to_string)
        .collect()
}

/// P5：与上一份快照差分，返回**这一轮消失了的会话名**（升序，`BTreeSet` 保证稳定）。
///
/// **差分而不是逐事件**，因为 SIGUSR1 会合并：一串 hook 同时打进来可能只醒一次，
/// 逐事件必漏；差分则一次报全。
///
/// **三态语义（最要紧的是第三条）**：
/// - `Sessions(raw)` ⇒ 现集 = raw 里的名字；消失 = 旧集 − 现集
/// - `ServerEmpty` / `NoServer` ⇒ 现集为空 ⇒ 旧集里**全部**算消失（server 没了，会话自然都没了）
/// - `NoTmux` / `Unobservable` ⇒ **「不知道」，绝不当作「都没了」** —— 观测失败时报一堆
///   死亡帧，会把活着的会话全部误 retire。此时**旧快照原样保留**，等下一次成功观测。
fn diff_closed(
    prev: &mut Option<std::collections::BTreeSet<String>>,
    obs: &TmuxObservation,
) -> Vec<String> {
    let now = match obs {
        TmuxObservation::Sessions(raw) => session_names(raw),
        TmuxObservation::ServerEmpty | TmuxObservation::NoServer => Default::default(),
        // 观测无效 ⇒ 什么都不结论，快照不动。
        TmuxObservation::NoTmux | TmuxObservation::Unobservable => return Vec::new(),
    };
    let closed = match prev.as_ref() {
        Some(old) => old.difference(&now).cloned().collect(),
        // 第一次观测没有「上一份」可比 ⇒ 不报任何死亡（否则 daemon 一启动就诬告一批）。
        None => Vec::new(),
    };
    *prev = Some(now);
    closed
}

fn observation_to_frame(obs: TmuxObservation) -> Frame {
    match obs {
        TmuxObservation::Sessions(raw) => Frame::TmuxSessions {
            raw,
            // 有会话时**刻意省略**：raw 非空本身就说明是有会话，省略保持热路径字节不变。
            observation: None,
        },
        // P3：两个细分映射到**同一个** wire 取值 ⇒ 帧契约与 P1 逐字节一致。
        TmuxObservation::ServerEmpty | TmuxObservation::NoServer => Frame::TmuxSessions {
            raw: String::new(),
            observation: Some(OBS_ZERO_SESSIONS.to_string()),
        },
        TmuxObservation::NoTmux => Frame::TmuxSessions {
            // 保留 `NO_TMUX` 哨兵：旧 monitor 认它（`raw.trim() != "NO_TMUX"` 那道门）。
            raw: "NO_TMUX\n".to_string(),
            observation: Some(OBS_NO_TMUX.to_string()),
        },
        TmuxObservation::Unobservable => Frame::TmuxSessions {
            raw: String::new(),
            observation: Some(OBS_UNOBSERVABLE.to_string()),
        },
    }
}

/// Spawn the watcher reader on a dedicated blocking thread and return the
/// receiving half of the bounded frame channel for the stdout writer to drain.
///
/// `agent_home` is the resolved `~/.claude` (or `$CLAUDE_CONFIG_DIR`). The
/// reader watches `<claude_dir>/projects/` recursively and
/// `<claude_dir>/sessions/`.
/// **P4：戳一下 watcher 的句柄** —— 故意是个**窄类型**而不是把
/// `Sender<WatchEvent>` 整个交出去：外部（`main` 的 SIGUSR1 流）只该有「催一次重探」
/// 这一个权力，不该能伪造 `PidDied` / `TmuxObserved` 这类带载荷的内部事件。
///
/// 这是账本第 1 行那条「只加事件源、不加定时器」的自然延伸：新来源经**同一个** channel。
#[derive(Clone)]
pub struct WatcherPoke(std::sync::mpsc::Sender<WatchEvent>);

impl WatcherPoke {
    /// 催一次 tmux 重探。watcher 已停（channel 关）时静默无操作 —— 那说明没人在听，
    /// 不是错误。
    pub fn poke(&self) {
        let _ = self.0.send(WatchEvent::Poke);
    }

    /// **P5：显式停机。** 删掉 8s ticker 之后，`sink.is_closed()` 那道复查再没有定期
    /// 醒来的机会 ⇒ 必须由 `main` 在 writer 结束时主动说一声，reader 线程才会退出
    /// `recv()`。**这件事漏做不会红任何测试**（进程退出时线程随之消亡），
    /// 所以它和删 ticker 是同一步、不许拆开。
    pub fn shutdown(&self) {
        let _ = self.0.send(WatchEvent::Shutdown);
    }
}

pub fn spawn(
    agent_home: PathBuf,
    with_bg: bool,
    tail_only: bool,
) -> (mpsc::Receiver<Frame>, WatcherPoke) {
    let (tx, rx) = mpsc::channel::<Frame>(CHANNEL_CAPACITY);
    // P4：事件 channel 从 `watch_loop` 内部**上提到这里**造 —— 因为 poke 句柄必须在线程
    // 起来之前就能交给 `main`。**仍然只有这一条 channel**（账本第 1 行不许再开第二条）：
    // 交出去的是同一个 sender 的 clone，`watch_loop` 收的是同一条的 receiver。
    let (events_tx, events_rx) = std::sync::mpsc::channel::<WatchEvent>();
    let poke = WatcherPoke(events_tx.clone());
    std::thread::Builder::new()
        .name("jsonl-watcher".into())
        .spawn(move || watch_loop(agent_home, tx, with_bg, tail_only, events_tx, events_rx))
        .expect("spawn jsonl-watcher thread");
    (rx, poke)
}

/// The reader half: initial walkdir scan, then the live debouncer loop.
///
/// Runs on its own OS thread. `tx` is the bounded sender; it is wrapped in a
/// [`FrameSink`] whose [`FrameSink::send`] never blocks the notify callback and
/// turns dropped frames into an [`Frame::Overflow`] signal (#32).
fn watch_loop(
    agent_home: PathBuf,
    tx: mpsc::Sender<Frame>,
    with_bg: bool,
    tail_only: bool,
    events_tx: std::sync::mpsc::Sender<WatchEvent>,
    events_rx: std::sync::mpsc::Receiver<WatchEvent>,
) {
    // U2 Phase D 审计 重要-2：`projects` 这个目录名原本有**五**处，不是 `agents/claudecode/paths.rs`
    // 注释里写的四处 —— 这是第五处（内联的，grep `fn projects_root` 找不到它）。
    // 不收的话「合并去重」承诺的性质（改布局只改一处）根本没拿到。
    let projects = crate::agents::claudecode::paths::projects_root(&agent_home);
    let sessions = crate::agents::claudecode::paths::sessions_root(&agent_home);

    let mut state = ReaderState::new(projects.clone(), with_bg, tail_only);
    // All frames go out through a FrameSink: a bounded-channel sender that counts
    // frames dropped on a full channel and emits a single `Overflow` signal once
    // the channel drains enough to accept it (#32). Never blocks this reader.
    let mut sink = FrameSink::new(tx);

    // P2：**唯一**的事件 channel（账本第 1 行最终形态）。notify / pidfd / tmux 全走它。
    //
    // **刻意建在 Phase 1 之前**：`process_session_added` 会顺手挂 pidfd 看守，而 Phase 1 的
    // 初始扫描就在调它。若把 channel 建在 Phase 2（本功能初版就是这么写的、被 clippy 的
    // 「field `start` is never read」间接暴露），**daemon 启动时就活着的会话会一个看守都没有**
    // ——而原先那条 2s 判活轮询是覆盖它们的 ⇒ 那是回归。Phase 1 期间发出的 `PidDied` 只是
    // 在 channel 里排队，主循环起来后照常消费。
    // P4：channel 已由 `spawn` 造好传进来（poke 句柄要在线程起来前就交出去）。
    // **顺序仍然关键**（P2 那个静默回归的教训）：`state.events_tx` 必须在 Phase 1 之前设好，
    // 否则 daemon 启动时就活着的会话一个 pidfd 看守都拿不到。
    state.events_tx = Some(events_tx.clone());

    // --- Phase 1: synchronous initial scan. ---
    // Mirror the LOCAL watcher's `active_filter` (`session_map.is_session_active`):
    // only stream sessions whose PID is alive (sessions/<PID>.json + /proc/<pid>).
    // We scan sessions/ FIRST to build the active set; process_session_added marks
    // the sid active and rescans its jsonl so an already-running session snapshots
    // on startup. We deliberately do NOT walk projects/ unconditionally — pulling
    // every historical jsonl as a Tab is the bug this fixes; browsing history is
    // the Ctrl+H history browser's job (Phase 1 for remote).
    if sessions.is_dir() {
        for entry in WalkDir::new(&sessions).into_iter().filter_map(Result::ok) {
            let p = entry.path();
            if is_session_json(p) {
                process_session_added(p, &mut state, &mut sink);
            }
        }
    }

    // --- Phase 2: live watch. ---
    let mut debouncer = match new_debouncer(
        Duration::from_millis(DEBOUNCE_MS),
        DebouncerSink(events_tx.clone()),
    ) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("debouncer init failed: {e}");
            return;
        }
    };
    // ★★ `P0b-Y2`〔08-13〕：**监视 `agent_home` 本身** —— 这是「子目录出现/被换掉」的唯一耳朵。
    //
    // inotify 的 watch 绑在 **inode** 上，不是路径上。`sessions/` 被 `rm -rf` 再 `mkdir`
    // 之后是**另一个 inode**，旧 watch 还挂在那个已删的 inode 上 ⇒ 新目录里发生什么都听不见，
    // **而且不会有任何错误**（daemon 活着、不吭声）。
    // 监视父目录之后，`sessions` 的创建/删除会作为**父目录里的一个事件**送到，我们据此重挂。
    if agent_home.is_dir() {
        if let Err(e) = debouncer
            .watcher()
            .watch(&agent_home, RecursiveMode::NonRecursive)
        {
            // 挂不上不致命（退回「起来时是什么样就什么样」），但**要说出来**。
            tracing::warn!(
                "watch failed for {}: {e} —— 子目录若被重建，本 daemon 将听不见",
                agent_home.display()
            );
        }
    }
    // Watch projects recursively; watch sessions (flat) for PID.json add/remove.
    // ★★ `projects/` 也走**可重入**挂法〔08-13〕：它是 jsonl 的来源，
    //    被换 inode 之后**行帧再也不来**（实测：删掉重建后写入，line 帧停在 1）。
    //    这是 `sessions/`（第十拍）与 socket 目录（第二十二拍）之后的**同族第三个**。
    let mut projects_watched = false;
    rewatch_dir(&mut debouncer, &projects, &mut projects_watched, RecursiveMode::Recursive);
    // ★★ `P0b-Y2` 第十六拍：**零 server 时也要有耳朵** —— 监视 tmux socket 目录本身。
    // 目录不在（本机从没起过 tmux）⇒ 退一层监视它的父，等目录被创建出来。
    // 两种情况都只当「该重新探一次」的触发器，绝不拿文件存在性判活（沿用 P3 的既定纪律）。
    let sock_dir = tmux_socket_dir();
    // `sock_dir_watched` = 目录**本身**挂上了没有。没挂上时退一层监视它的父
    // （目录还不存在 —— 本机从没起过 tmux 就是这样），等它被创建出来再升级。
    let mut sock_dir_watched = false;
    if let Some(parent) = sock_dir.parent() {
        if parent.is_dir() {
            if let Err(e) = debouncer.watcher().watch(parent, RecursiveMode::NonRecursive) {
                tracing::warn!("监视 {} 失败: {e}", parent.display());
            }
        }
    }
    watch_sock_dir_if_present(&mut debouncer, &sock_dir, &mut sock_dir_watched);

    // `sessions_watched` = 「**当前这个 inode** 我挂上了没有」。事件循环里靠它决定要不要重挂。
    let mut sessions_watched = false;
    if sessions.is_dir() {
        match debouncer
            .watcher()
            .watch(&sessions, RecursiveMode::NonRecursive)
        {
            Ok(()) => sessions_watched = true,
            Err(e) => tracing::error!("watch failed for {}: {e}", sessions.display()),
        }
    } else {
        // ⚠⚠ **这一支是个真缺陷，08-13 实测复现过**〔`P0b` 查 `#60` 时逮到〕：
        // 目录不存在 ⇒ 只打这一行 `warn!`，**然后再也不重试**。
        // 而 `<claude_dir>/sessions/` 正是**用户第一次跑 claude 时才被创建**的
        // ⇒ daemon 起得比它早，就**永远看不到 pidfile、永远不宣告会话**。
        //
        // 复现（帧的 `kind` 直方图）：
        // · fixture 目录里**有** `sessions/` ⇒ `hello · line · session_added · tmux_sessions`
        // · **没有** `sessions/`（其余一模一样）⇒ **只有** `hello · tmux_sessions`
        //   —— 即便它随后被 fake-claude 建出来也不补发。
        //
        // ⚠ **不能靠「把目录建出来」修**：`<claude_dir>` 对我们是**只读**的（`INVARIANTS` 铁律）。
        // 正确形状是**监视父目录**（`agent_home` 本身）等它出现再挂上去，或按需重试 ——
        // 那是一次行为改动，要 bump `BUILD_ID` + 重编内嵌，**没在发现它的那一拍顺手做**。
        //
        // ⚠ 射程：这是 `#60`（灰灯不出现）的**候选机制**，**不是**已证实的根因 ——
        // 那次全链复现用的 fixture 里 `sessions/` 是**在**的，所以它解释不了那三跑。
        tracing::warn!(
            "sessions dir does not exist: {} —— **本进程不会再重试挂它**（见上方注释：\
             它若稍后才被创建，本 daemon 将永远不宣告会话）",
            sessions.display()
        );
    }

    // B2 审计（`run_tmux_ls` 无超时 → 阻塞会冻结整个 reader）：`tmux ls` 一律跑在**一次性后台
    // 线程**里，主循环只收结果。gate 在「无在途探测」上 → 最多同时一个探测线程；即便远端 tmux
    // 卡死（D-state/socket 卡住/NFS home），也只泄漏这一个后台线程，reader 永不冻结。
    let mut tmux_inflight = false;
    // S0：本批 notify 事件里是否发生过「pidfile 绑的 sid 变了」。见下方使用处。
    let mut sid_drifted = false;
    // P5：上一份 `tmux ls` 的会话名集合。`None` = 还没成功观测过（第一次不报死亡）。
    // 纯 reader 线程内部状态，不进 `ReaderState`（那是 per-file 记账，语义不同）。
    let mut last_names: Option<std::collections::BTreeSet<String>> = None;
    // P3：对 tmux server 的认知 + 已挂过的 socket 目录 watch（只加一次）。
    let mut server = ServerState::Unknown;
    let mut watched_socket: Option<PathBuf> = None;
    // P2：轮询 B（8s）从"主循环里的节流判断"搬进独立 ticker 线程 ⇒ 主循环变成最终形态。
    // **P5 删掉这一行 + `spawn_tmux_ticker` 即可**（并按 `WatchEvent::Shutdown` 的注释接停机信号）。
    initial_tmux_probe(&events_tx);

    // P2：**无超时** `recv()`——本循环再没有任何定时器（轮询 A 已由 pidfd 取代）。
    // 事件源：notify（经 DebouncerSink）· pidfd 看守线程 · tmux 探测/节拍线程。
    // **P3/P4 只往 `WatchEvent` 加变体 + 加发送方，不许再挂独立定时器。**
    // `while let Ok(..)` = 所有发送端都掉了就结束（等价于原来的 Disconnected 分支）。
    while let Ok(event) = events_rx.recv() {
        match event {
            WatchEvent::Notify(Ok(events)) => {
                for ev in events {
                    let p = ev.path.as_path();
                    // ★★ `P0b-Y2`：**`sessions/` 换了 inode 或刚出现 ⇒ 重挂 + 重扫。**
                    //
                    // 触发面刻意宽：`agent_home` 里任何与 `sessions` 有关的动静都来这儿判一次
                    //（判的是**盘上此刻的样子**，不是事件类型 —— notify 会合并事件，
                    // 「删了又建」很可能只到一个事件，靠 kind 去分辨是猜）。
                    // ★ `projects/` 换 inode 时也要重挂（同族第三个，见 `rewatch_dir` 头注）。
                    if p == projects.as_path() {
                        rewatch_dir(
                            &mut debouncer,
                            &projects,
                            &mut projects_watched,
                            RecursiveMode::Recursive,
                        );
                    }
                    if p == sessions.as_path() {
                        rewatch_sessions(
                            &mut debouncer,
                            &sessions,
                            &mut sessions_watched,
                            &mut state,
                            &mut sink,
                        );
                    }
                    // P3：**tmux socket 复活**——我们监视的正是它所在目录，socket 被
                    // unlink+create 时（P0 实测 inode 会变）这里就会命中。
                    // 只当"该重新探一次"的触发器：立刻起一次探测（若无在途），
                    // 由探测结果去重挂 pidfd / 重同步。**绝不拿文件存在性判活。**
                    // ★ 第十六拍：**已知 socket 文件**或 **socket 目录里的任何动静**都触发重探。
                    //   后者覆盖「零 server 起步、server 后来才出现」那一族 —— 那时我们还
                    //   不知道 socket 叫什么，只知道它会出现在这个目录里。
                    let in_sock_dir =
                        p.parent() == Some(sock_dir.as_path()) || p == sock_dir.as_path();
                    // ★ **目录后来才出现**（本机第一次起 tmux）⇒ 从「监视父」升级成「监视目录」。
                    //   不升级的话：目录创建那一下能探到一次，但**那一瞬 server 往往还没就绪**，
                    //   而随后 socket 文件落在一个**没被监视的目录**里 ⇒ 再无事件、永远漏掉。
                    //   （e2e 第 3 格 `TMUX_TMPDIR` 实测就是这么红的。）
                    // ⚠ 去掉 `&& !sock_dir_watched`〔08-13〕：目录**被删掉再重建**时
                    //   `sock_dir_watched` 仍是 true ⇒ 那个条件会把重挂整个短路掉，
                    //   于是新 inode 永远没人听（实测：新起的 server 一帧都收不到）。
                    if p == sock_dir.as_path() || p.parent() == Some(sock_dir.as_path()) {
                        watch_sock_dir_if_present(&mut debouncer, &sock_dir, &mut sock_dir_watched);
                    }
                    if watched_socket.as_deref() == Some(p) || in_sock_dir {
                        if !tmux_inflight {
                            tmux_inflight = true;
                            let tx = events_tx.clone();
                            std::thread::spawn(move || {
                                let _ = tx.send(WatchEvent::TmuxObserved(run_tmux_probe()));
                            });
                        }
                        continue;
                    }
                    if is_jsonl(p) && !is_subagent_path(p) {
                        // process_jsonl skips sids not in active_sids.
                        process_jsonl(p, &mut state, &mut sink);
                    } else if is_session_json(p) {
                        // notify coalesces to "something happened to this path";
                        // decide add vs remove by current existence on disk.
                        //
                        // ★ S0：顺带记「这个 pidfile 绑的 sid 有没有真的变」。变了就该重探
                        // tmux —— 因为 `tmux ls` 快照里的 `@ccm_sid` 此刻已经过期。
                        // **只认 sid 变化，不认「有 .json 事件」**：CC 状态转换时会重写
                        // pidfile（红绿灯就靠它），拿那个当触发器等于把探测变成变相轮询。
                        let key = path_key(p);
                        let sid_before = state.sessions.get(&key).map(|e| e.sid.clone());
                        if p.exists() {
                            process_session_added(p, &mut state, &mut sink);
                        } else {
                            process_session_removed(p, &mut state, &mut sink);
                        }
                        if state.sessions.get(&key).map(|e| e.sid.clone()) != sid_before {
                            sid_drifted = true;
                        }
                    }
                }
                // ★ S0：一批事件处理完再决定探不探（批内多次漂移只探一次；`tmux_inflight`
                // 再兜一层去重）。**注意这只让快照更新鲜，不是本 bug 的修复** —— `@ccm_sid`
                // 由 `shared/ccm` 的 1 秒 poller 回填，这次探测很可能仍抓到旧 tag。
                // 真正的修复是 `RemovalCause::Superseded`（让 monitor 不必依赖这份快照）。
                if sid_drifted {
                    // 只在**真的起了**探测时才清标志。在途时清掉会丢信号——那次在途的探测是
                    // 在漂移**之前**发起的，它带回来的快照照样是旧的。留着标志，下一个事件
                    // 会补上一次（代价只是晚一拍；这本来就只是"更新鲜"，不是本 bug 的修复）。
                    if !tmux_inflight {
                        sid_drifted = false;
                        tmux_inflight = true;
                        let tx = events_tx.clone();
                        std::thread::spawn(move || {
                            let _ = tx.send(WatchEvent::TmuxObserved(run_tmux_probe()));
                        });
                    }
                }
            }
            WatchEvent::Notify(Err(errs)) => tracing::warn!("debouncer error: {errs:?}"),
            // P2：pidfd 醒了 = 该 pidfile 当时追踪的**那个进程实例**已退出。取代原先
            // 每 2s 遍历 `state.sessions` 调 `session_alive` 的判活扫描。
            // **pid 比对挡陈旧唤醒**（同路径已换 pid / 已被移除）⇒ 幂等。
            // Batch6-F22-② 的引用计数语义原样保留：经 `retire_sid_if_unreferenced`，
            // 同 sid 多 pidfile（resume 时原进程未死）任一 PID 死亡不误杀整个 sid。
            WatchEvent::PidDied { key, pid } => {
                // ★ 同上：醒没醒、醒了之后那道 `sessions` 对账过没过，都要看得见。
                //   两者分开报 —— 「没醒」与「醒了但被对账挡掉」是两个完全不同的诊断。
                tracing::info!(
                    "PidDied 醒了: pid={pid} key={} 账上是 {:?}",
                    key.display(),
                    state.sessions.get(&key).map(|e| e.pid)
                );
                if state.sessions.get(&key).map(|e| e.pid) == Some(pid) {
                    if let Some(e) = state.sessions.remove(&key) {
                        // pidfd 醒了 = 那个进程实例真的退了。
                        retire_sid_if_unreferenced(
                            &e.sid,
                            RemovalCause::Gone,
                            &mut state,
                            &mut sink,
                        );
                    }
                }
            }
            // P4：`Poke` 与 `TmuxProbeDue` 走**同一段**——`tmux_inflight` 那道去重顺带
            // 免疫了「信号合并 / 一串 hook 同时打进来」：多次戳只会落一次探测。
            WatchEvent::TmuxProbeDue | WatchEvent::Poke => {
                if !tmux_inflight {
                    tmux_inflight = true;
                    let tx = events_tx.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(WatchEvent::TmuxObserved(run_tmux_probe()));
                    });
                }
            }
            WatchEvent::TmuxObserved(probe) => {
                tmux_inflight = false;
                // P3：先按「我们对 server 的认知」收紧判据，再发帧。
                let obs = classify_with_server_state(probe.obs, server);
                // P3：探到了 server ⇒ 挂 pidfd 看守（换了 pid 也要重挂：server 复活是新进程）。
                match probe.server_pid {
                    Some(pid) if server != ServerState::Alive(pid) => {
                        server = ServerState::Alive(pid);
                        // server 的 procStart 基线我们没有（不是从 pidfile 读的）⇒ 传 None，
                        // 判据 2 退化成存在性（`is_same_live_process` 的 `_ => true` 臂）。
                        spawn_pid_watcher(PidWatchTarget::TmuxServer, pid, None, events_tx.clone());
                        tracing::info!("已给 tmux server pid {pid} 挂 pidfd 看守（零轮询判死）");
                        // P4b：**这里就是「该（重）装 hook」的时机** —— hook 活在 server
                        // 内存里，server 每次起来都要重装，而 P3 把「server 起来了」变成了事件。
                        // 装不上不致命（退回定时探测），但会 warn ⇒ 不静默降级。
                        install_tmux_hooks_best_effort();
                    }
                    None if matches!(obs, TmuxObservation::NoServer) => {
                        server = ServerState::Gone;
                    }
                    _ => {}
                }
                // P3：第一次知道 socket 路径就给它**所在目录**加 inotify——server 复活时
                // tmux 会 unlink+create 那个 socket 文件（P0 实测 inode 会变）⇒ `IN_CREATE`
                // 能感知。**注意「socket 文件存在」≠「server 活着」**（P0 实测：server 死后
                // 文件仍留着）⇒ 只把它当"该重新探一次"的触发器，绝不用它判活。
                if let Some(sock) = probe.socket_path.as_ref() {
                    if watched_socket.as_ref() != Some(sock) {
                        if let Some(dir) = sock.parent() {
                            match debouncer.watcher().watch(dir, RecursiveMode::NonRecursive) {
                                Ok(()) => {
                                    tracing::info!("已监视 tmux socket 目录 {}", dir.display());
                                    watched_socket = Some(sock.clone());
                                }
                                Err(e) => tracing::warn!(
                                    "监视 tmux socket 目录失败 {}: {e}（复活仍由 ticker 兜）",
                                    dir.display()
                                ),
                            }
                        }
                    }
                }
                // P5：**先**差分发死亡帧、**再**发快照帧。顺序有意：monitor 那边死亡帧是
                // 「直接 retire」，快照帧是「对账」；先 retire 再对账，两者对同一 sid 的结论
                // 一致（retire 幂等）。反过来则会出现「快照说它不在 → miss+1」这种多余一跳。
                for name in diff_closed(&mut last_names, &obs) {
                    tracing::info!("tmux 会话 {name} 已关闭（快照差分）⇒ 发正向死亡帧");
                    sink.send(Frame::TmuxSessionClosed { name });
                }
                // P1：四态 → wire（`raw` 载荷不变、新信息走 additive `observation`）。
                sink.send(observation_to_frame(obs));
            }
            // P3：tmux server 的 pidfd 醒了 ⇒ 立刻等价于零会话，**不等下一个 8s 节拍**。
            // pid 比对挡陈旧唤醒（复活后是新 pid，旧看守迟到的事件要忽略）。
            WatchEvent::TmuxServerGone { pid } => {
                if server == ServerState::Alive(pid) {
                    server = ServerState::Gone;
                    tracing::info!("tmux server pid {pid} 已退出（pidfd）⇒ 发零会话帧");
                    // P5：server 没了 = 它上面的会话全没了 ⇒ 逐个报死亡，别只发一个零会话帧
                    // 就指望 monitor 自己推断（那又回到「靠快照 + miss 计数」那条慢路）。
                    for name in diff_closed(&mut last_names, &TmuxObservation::NoServer) {
                        sink.send(Frame::TmuxSessionClosed { name });
                    }
                    sink.send(observation_to_frame(TmuxObservation::NoServer));
                }
            }
            WatchEvent::Shutdown => break,
        }

        if sink.is_closed() {
            break;
        }
    }
}

/// Reader-side bookkeeping shared across `process_*` calls.
///
/// Not behind a lock: the reader is single-threaded (one OS thread), so all
/// access is serialized by construction.
struct ReaderState {
    /// `<claude_dir>/projects` — used to rescan a session's jsonl when it becomes
    /// active (so its existing lines stream, mirroring the local watcher's
    /// force-rescan on session-added).
    projects: PathBuf,
    /// Per-file consumed byte offset, keyed by [`path_key`]. Reset to 0 on
    /// truncation; the climbing seq lives separately in [`Self::seqs`] so a
    /// truncation never rolls the seq back.
    offsets: HashMap<PathBuf, ReadCursor>,
    /// Per-file monotonic seq source. `SeqCounter` only ever climbs for a given
    /// path (it is never reset), so truncation resetting `offsets` cannot pull
    /// the seq back — exactly the `watcher.rs:243-247` invariant.
    seqs: SeqCounter,
    /// PID-file path → [`SessionEntry`] for sessions currently considered ACTIVE
    /// (announced via `SessionAdded`). The pid + captured procStart let the
    /// liveness poll detect both a dead process AND a **reused PID** (#34); the
    /// cached sid lets a file-delete still emit the right `SessionRemoved`.
    sessions: HashMap<PathBuf, SessionEntry>,
    /// P2：统一事件 channel 的发送端，由 `watch_loop` 注入。
    /// **测试里为 `None`** ⇒ 不起 pidfd 看守线程（11 处 `ReaderState::new` 因此无需改签名；
    /// pidfd 本身由专门的双向验收测试覆盖，见 `pidfd_*` 那几条）。
    events_tx: Option<std::sync::mpsc::Sender<WatchEvent>>,
    /// P2：已挂过 pidfd 看守的 **(pidfile, pid) 对**——防同一进程重复起线程。
    /// **按对而不是按路径**：同路径换了 pid（`/clear` 原地换 sid、PID 复用写同路径）
    /// 要能重新挂；而按对存就不必在任何移除路径上做清理（陈旧条目至多一个/对，
    /// 且 daemon 生命周期 ⊆ 一次 SSH 连接）。
    /// 已挂过 pidfd 看守的 **(pidfile 路径, pid, 进程启动时刻)**〔audit-0805 F11 / 报告 I-7〕。
    ///
    /// ⚠ 第三元 `start` 是 F11 补的。此前键是 `(路径, pid)` 两元 ——
    /// 「同路径换 pid」能重新挂（有测试钉着），**而「同路径、同 pid、不同进程实例」落进去重、
    /// 不再挂**。那恰好就是 **PID 复用**本身，也正是 `spawn_pid_watcher` 头注声称要处理的那格。
    /// 区分进程实例的东西（`starttime`）本来就在 `arm_pid_watcher` 的参数里，只是没进键。
    pid_watched: HashSet<(PathBuf, u32, Option<u64>)>,
    /// Fast membership for the active-session filter: sids currently streaming.
    /// Mirrors the local watcher's `active_filter` — only sessions whose PID is
    /// alive on this host stream; historical jsonl is NOT pulled (that is the
    /// Ctrl+H history browser's job).
    active_sids: HashSet<String>,
    /// Batch7-F24：`--with-bg` 时放行 kind:"bg" 会话（宣告+流行，帧带元信息）；
    /// 默认 false = Batch6-F21 行为（bg 不算会话）。
    with_bg: bool,
    /// Batch8-F25：`--tail-only` 时连接不重放历史——初扫/宣告只推进 cursor 与
    /// seq 计数器到当前完整行数 L（行号语义，之后新行 seq 从 L 起），零行帧；
    /// 历史由 monitor 经 `--read-session` 旁路快照拉取（0..L'-1 由 monitor 编号，
    /// 重叠区被 (sid,seq) 去重吸收）。默认 false = 全量重放（旧 monitor 兼容）。
    tail_only: bool,
}

impl ReaderState {
    fn new(projects: PathBuf, with_bg: bool, tail_only: bool) -> Self {
        ReaderState {
            projects,
            offsets: HashMap::new(),
            seqs: SeqCounter::new(),
            sessions: HashMap::new(),
            events_tx: None,
            pid_watched: HashSet::new(),
            active_sids: HashSet::new(),
            with_bg,
            tail_only,
        }
    }
}

/// An ACTIVE session tracked by the reader, keyed in [`ReaderState::sessions`]
/// by its `sessions/<PID>.json` path.
///
/// `start` is the PID's procStart captured at session-add time (#34): on Linux
/// the `/proc/<pid>/stat` starttime (jiffies since boot). The liveness poll
/// compares the *current* procStart against this captured value so a PID that
/// the OS reused for an unrelated process is detected as dead (the original
/// session ended) rather than masquerading as still-live. `None` = procStart
/// unavailable (non-Linux smoke / read failure) → liveness degrades to plain
/// `/proc/<pid>` existence, matching the Phase-0 behaviour.
///
/// **Residual limitation (#34 §5, by design)**: `start` is captured at add-time
/// and never persisted. A daemon **restart** re-baselines `start` from the
/// *current* `/proc` on the next scan, so a PID that was reused *before* the
/// restart is indistinguishable from the original session. Probability is low
/// (restart ∧ PID-reuse ∧ reused-proc-still-alive) and this matches the local
/// watcher's identical non-persisted `proc_start`.
struct SessionEntry {
    pid: u32,
    sid: String,
    // P2：原先这里有个 `start: Option<u64>`——**那是 2s 判活轮询的 procStart 基线**
    // （每轮拿它跟 /proc 现值比对判 PID 复用）。pidfd 取代轮询后基线只在**挂看守那一刻**
    // 用一次（`arm_pid_watcher` 的实参），之后身份由内核保证 ⇒ 不必再存在 entry 里。
    // 删它是"轮询消失 ⇒ 它的状态也消失"，不是丢信息。
    /// Batch9-F27：pidfile 的官方 status（busy/idle/shell/waiting）与 waitingFor
    /// ——modify 事件 diff，变了发 session_status 帧（远端红绿灯）。
    status: Option<String>,
    waiting_for: Option<String>,
}

/// One line read out of a JSONL file, with its assigned per-file seq.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadLine {
    pub seq: u64,
    pub raw: String,
    /// daemon-01（gap#2）：本行末尾（含 `\n`）的累计**原始字节** offset，逐字节对齐 aterm `LineFramer`
    /// （计 `\r`、含 `\n`、残行不计）。**在原始字节上算**（非解码后串），故非法 UTF-8/CRLF 不错。
    pub byte_offset: u64,
}

/// Per-file read cursor, mirroring the monitor watcher's `FileCursor`
/// (Batch4-F14 audit fix).
///
/// - `consumed`: bytes of **complete lines** already emitted — the next
///   incremental read starts here. A deferred torn tail is not included.
/// - `seen_len`: high-water mark of the observed file length. Truncation must
///   be judged against this, not `consumed`: while a torn tail is pending,
///   `consumed < real EOF`, so a non-append rewrite whose new length lands in
///   `[consumed, seen_len)` would slip past a `len < consumed` check and read
///   garbage from a stale offset — silently, bypassing the truncation warn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadCursor {
    pub consumed: u64,
    pub seen_len: u64,
}

/// Pure bookkeeping core, factored out so it is unit-testable without a real
/// filesystem watcher.
///
/// Given the file's *full current bytes*, the prior [`ReadCursor`], the file's
/// `key` and the shared [`SeqCounter`], return the newly-appeared lines (with
/// seqs assigned) and the updated cursor. The seq for each kept line comes
/// from `seqs.next(key)`, so it is per-path monotonic and **never reset**.
///
/// Mirrors `../src-tauri/src/watcher.rs` `process_file`:
///
/// - read from `cursor.consumed`, but only consume **complete lines** — bytes
///   up to and including the last `\n` in the new region. A torn tail without
///   a trailing `\n` (the CLI caught mid-write) stays in the file: it is
///   neither emitted nor skipped over, and `consumed` stops right before it,
///   so the next event re-reads it once completed (Batch4-F14; the old
///   behaviour emitted the half line — the record was then lost for good after
///   the JSON parse failure — and a torn multibyte tail decayed into U+FFFD).
///   Accepted trade-off: a final line that is complete JSON but never gets its
///   `\n` (writer killed between the two writes) is never emitted if the file
///   never grows again — real jsonl ends with `\n` (8/8 sampled);
/// - **truncation**: judged against the high-water mark
///   (`len < cursor.seen_len`), so a rewrite landing inside a pending torn-tail
///   window `[consumed, seen_len)` is still caught → start over from byte 0;
/// - on truncation the byte cursor resets but the seq keeps climbing (it comes
///   from `SeqCounter`, which never resets), so a client that already placed
///   the old seqs still sorts the new lines after them;
/// - strip a leading UTF-8 BOM (`\u{feff}`) and skip blank lines;
/// - the returned `raw` is the original (untrimmed) line, exactly as
///   `watcher.rs` pushes `line` (not `trimmed`) into the batch.
///
/// F04 之后这是 **test-only** 的：两个生产调用点都改走 [`read_new_lines_at`] 了。
///
/// 留着它不是为了兼容，而是因为它是那条等价性判据
/// (`the_chunked_entry_agrees_with_the_whole_file_entry_byte_for_byte`) 的一半：
/// 「分块读」与「整读」得出同样结果，这件事只有两条路都在才证得了。
/// 它今天的身份是参照实现；删了它，那条判据就只剩一条路、无从对拍。
#[cfg(test)]
pub fn read_new_lines(
    bytes: &[u8],
    cursor: ReadCursor,
    key: &str,
    seqs: &mut SeqCounter,
) -> (Vec<ReadLine>, ReadCursor) {
    let len = bytes.len() as u64;
    read_new_lines_at(bytes, 0, len, cursor, key, seqs)
}

/// 同上，但**文件长度是显式入参**，`chunk` 只需覆盖 `[chunk_start, file_len)`
/// 〔audit-0805 F04 第 1 步〕。
///
/// # 为什么必须先有这个入口
///
/// 上面那个入口的契约是「给我**整个文件**的字节」，而它把 `bytes.len()` **当成文件长度**用 ——
/// 截断高水位判定 `len < cursor.seen_len` 整个压在这一点上。
/// ⇒ **直接给它喂一段「只有新字节」的切片会当场把它骗坏**：`len` 变成增量长度、必然小于
/// `seen_len`、于是每次都判「文件被截断」、`start` 归零、按 `INVARIANTS §25` 重发全部 seq。
/// **那不是慢，是行为错，比整读更糟。** 所以「改成 seek 只读新字节」这件事的第一步
/// 不是改调用点，是把「文件长度」从切片长度里摘出来。
///
/// # 调用方的义务（**违反即 panic，不静默**）
///
/// `chunk` 必须一直覆盖到 **EOF**：`chunk_start + chunk.len() == file_len`。
/// 少一截会让 torn-tail 判定把「还没读到的字节」误当成「写了一半」，
/// 于是游标停在半路且**再也不前进**（下一次又从同一处读起、又差同一截）。
/// ⚠ 这类错**无声且自洽** —— 所以这里用 `assert!` 而不是 `debug_assert!`。
pub fn read_new_lines_at(
    chunk: &[u8],
    chunk_start: u64,
    file_len: u64,
    cursor: ReadCursor,
    key: &str,
    seqs: &mut SeqCounter,
) -> (Vec<ReadLine>, ReadCursor) {
    let (lines, _, cursor) = scan_new_lines(chunk, chunk_start, file_len, cursor, key, seqs, true);
    (lines, cursor)
}

/// 同上，但**只数不建**〔audit-0805 F04 第 3 步〕。
///
/// `prime_file_cursor` 要的只有「有多少行」——它把每行单独 `to_string` 建成 `Vec<ReadLine>`，
/// 然后**只用了 `.len()`**（一条 `tracing::debug!`）。首次 prime 时「新增那一段」就是整份文件，
/// 于是一份 257 MB 的会话会被物化成另一份 257 MB 的 `String` 堆，用完即扔。
///
/// ⚠ 它与 [`read_new_lines_at`] **共用同一段扫描逻辑**（`scan_new_lines`），不是抄一份 ——
/// torn-tail 延后、`seen_len` 高水位、seq 推进这些语义抄一份迟早漂。
pub fn count_new_lines_at(
    chunk: &[u8],
    chunk_start: u64,
    file_len: u64,
    cursor: ReadCursor,
    key: &str,
    seqs: &mut SeqCounter,
) -> (usize, ReadCursor) {
    let (_, n, cursor) = scan_new_lines(chunk, chunk_start, file_len, cursor, key, seqs, false);
    (n, cursor)
}

#[allow(clippy::too_many_arguments)]
fn scan_new_lines(
    chunk: &[u8],
    chunk_start: u64,
    file_len: u64,
    cursor: ReadCursor,
    key: &str,
    seqs: &mut SeqCounter,
    collect: bool,
) -> (Vec<ReadLine>, usize, ReadCursor) {
    assert_eq!(
        chunk_start + chunk.len() as u64,
        file_len,
        "read_new_lines_at: chunk 必须覆盖到 EOF（chunk_start {chunk_start} + len {} != file_len {file_len}）\n\
         少一截会让 torn-tail 判定把「还没读到的字节」误当成「写了一半」，游标就此永远停在半路。",
        chunk.len()
    );
    let len = file_len;
    // Truncation guard against the high-water mark (see ReadCursor docs).
    let truncated = len < cursor.seen_len;
    let start = if truncated { 0 } else { cursor.consumed };
    assert!(
        !truncated || chunk_start == 0,
        "read_new_lines_at: 判定为截断（file_len {len} < seen_len {}）时要从头重读，\n\
         但 chunk 从 {chunk_start} 起 —— 调用方必须在这种情况下整读。",
        cursor.seen_len
    );
    if truncated && len > 0 {
        // Parity with the monitor's truncation warn (INVARIANTS §25: re-reads
        // hand out new seqs — must leave a trace; silence made an old
        // mis-folding bug near-impossible to diagnose). len == 0 re-reads
        // nothing, so stay quiet like the monitor.
        tracing::warn!(
            "jsonl truncated (len {len} < seen_len {}), full re-read with new seqs: {key}",
            cursor.seen_len
        );
    }

    let mut out = Vec::new();
    let mut n_lines: usize = 0;
    let mut consumed: u64 = 0;
    if start < len {
        let slice = &chunk[(start - chunk_start) as usize..];
        // Only the region ending at the last '\n' is complete; a torn tail
        // (mid-write, possibly mid-multibyte) is deferred to the next event.
        let complete_end = slice.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        consumed = complete_end as u64;
        // daemon-01（gap#2）：**在原始字节上逐行切**（非先解码整段再 `.lines()`）——因为 `byte_offset` 必须是
        // 累计原始字节（对齐 aterm `LineFramer`：计 `\r`、含 `\n`），而解码后串的字节位在非法 UTF-8（U+FFFD 替换
        // 3 字节换 1 字节）会漂。每行的原始内容单独 lossy 解码（残行已在 tail 外，故整行 multibyte 完整、安全）。
        let mut pos = 0usize; // 相对 slice 的原始字节游标
        while pos < complete_end {
            // 完整区内必有 '\n'（complete_end 到最后一个 '\n' 之后）。
            let nl = slice[pos..complete_end]
                .iter()
                .position(|&b| b == b'\n')
                .expect("complete region ends at a '\\n'");
            let content = &slice[pos..pos + nl]; // 行内容原始字节（不含 '\n'，可能尾随 '\r'）
            let line_end = pos + nl + 1; // 本行末尾（含 '\n'）在 slice 内的原始字节位
                                         // raw = 内容解码 + 剥尾随 '\r'（对齐 aterm：raw 无 CRLF/无尾 \n；但 offset **计** `\r`）。
            let text = String::from_utf8_lossy(content);
            let raw = text.strip_suffix('\r').unwrap_or(&text);
            let is_blank = raw.trim_start_matches('\u{feff}').trim().is_empty();
            if !is_blank {
                // Seq from the never-reset per-path counter. Blank lines do not
                // call `next`, so they do not consume a seq.
                // seq **必须照常推进**（哪怕不收集）——它是 per-path 单调且永不重置的，
                // 少推一次会让后续所有行的 seq 与整读那条路错开。
                let seq = seqs.next(key);
                n_lines += 1;
                if collect {
                    out.push(ReadLine {
                        seq,
                        raw: raw.to_string(),
                        byte_offset: start + line_end as u64,
                    });
                }
            }
            pos = line_end;
        }
    }
    // `consumed` advances only past complete lines (from the possibly
    // truncation-reset `start`), never past a deferred torn tail; `seen_len`
    // records the full observed length so a later rewrite inside the torn-tail
    // window is still detected as truncation.
    (
        out,
        n_lines,
        ReadCursor {
            consumed: start + consumed,
            seen_len: len,
        },
    )
}

/// 只读新字节：从游标处 `seek` 到 EOF〔audit-0805 F04 第 2 步〕。
///
/// 返回 `(chunk, chunk_start, file_len)`，直接喂 [`read_new_lines_at`]。
///
/// # 两件必须想清楚的事
///
/// 1. **截断时退回整读。** `read_new_lines_at` 的断言②要求判定为截断时 `chunk_start == 0`，
///    因为截断的语义就是「从头重来、重发 seq」。这里先用 `metadata` 的长度判一次。
/// 2. ★ **`file_len` 用「真读到多少」算，不用 `metadata` 那个数。**
///    `metadata()` 与 `read_to_end()` 之间文件还在被 CC 追加 —— 拿元数据那个长度当 `file_len`
///    会让断言①（chunk 必须覆盖到 EOF）在**完全正常的并发追加**下炸。
///    改用 `start + chunk.len()`：多读到的字节这一轮就一起处理掉，天然自洽。
///    ⚠ 反过来（读的时候文件缩了）也自洽：`file_len` 变小 ⇒ 下一次事件 `file_len < seen_len`
///    ⇒ 判截断 ⇒ 整读重来。**两个方向都不需要额外分支。**
fn read_tail_from(path: &Path, cursor: ReadCursor) -> Option<(Vec<u8>, u64, u64)> {
    use std::io::{Read, Seek, SeekFrom};
    let meta_len = std::fs::metadata(path).ok()?.len();
    let truncated = meta_len < cursor.seen_len;
    let start = if truncated {
        0
    } else {
        cursor.consumed.min(meta_len)
    };
    let mut f = std::fs::File::open(path).ok()?;
    if start > 0 {
        f.seek(SeekFrom::Start(start)).ok()?;
    }
    let mut chunk = Vec::new();
    f.read_to_end(&mut chunk).ok()?;
    let file_len = start + chunk.len() as u64;
    Some((chunk, start, file_len))
}

/// Read a JSONL file incrementally and send a [`Frame::Line`] per new line.
fn process_jsonl(path: &Path, state: &mut ReaderState, sink: &mut FrameSink) {
    let Some(session_id) = file_stem_str(path) else {
        return;
    };
    // Active-session filter (mirrors the local watcher's `active_filter`): only
    // stream sessions whose PID is alive. Historical jsonl is never pulled.
    if !state.active_sids.contains(&session_id) {
        return;
    }
    let key = path_key(path);
    let key_str = key.to_string_lossy().into_owned();
    let prev_cursor = state.offsets.get(&key).copied().unwrap_or_default();
    // F04：只读新字节（截断时 `read_tail_from` 自己退回整读）。此前是 `fs::read` 整读 ——
    // 257 MB 会话的每一次文件事件都要把整份读进内存，只为提取约 500 字节的新行。
    let Some((chunk, chunk_start, file_len)) = read_tail_from(path, prev_cursor) else {
        return;
    };
    let (lines, new_cursor) = read_new_lines_at(
        &chunk,
        chunk_start,
        file_len,
        prev_cursor,
        &key_str,
        &mut state.seqs,
    );
    state.offsets.insert(key, new_cursor);
    let path_str = path.to_string_lossy().into_owned();
    for line in lines {
        // daemon-09（phase②）：turn-end 边沿在 raw **之外**额外算——先解析（畸形→None、不影响 Line）。
        // 在 raw move 进 Line 帧前抽出（避免 clone raw）。§2.1 不变量并存：Line 逐行照发**每一条**。
        let turn_uuid: Option<String> = serde_json::from_str::<serde_json::Value>(&line.raw)
            .ok()
            .and_then(|v| crate::observe::turn_detect::turn_end_uuid(&v).map(str::to_string));
        sink.send(Frame::Line {
            session_id: session_id.clone(),
            path: path_str.clone(),
            seq: line.seq,
            raw: line.raw,
            byte_offset: line.byte_offset, // daemon-01 gap#2：累计原始字节（对齐 aterm LineFramer）
        });
        // **先 Line 后 TurnEnd**：对齐 aterm β 的按行序处理——TurnEnd 结算时 currentOffset 已含本行。
        // 方案 C raw-per-record、daemon 不 dedup（aterm rolling-latest+debounce baselineByPath 塌合，
        // #daemon 2026-07-18 定）。TurnEnd 不带 byte_offset（只 Line 带）。
        if let Some(uuid) = turn_uuid {
            sink.send(Frame::TurnEnd {
                session_id: session_id.clone(),
                uuid,
            });
        }
    }
}

/// ★★ `P0b-Y2`〔08-13〕：`<claude_dir>/sessions/` **换了 inode 或刚出现**，重新挂上并重扫。
///
/// # 为什么必须有这一步（九拍排除链的终点）
///
/// inotify 的 watch 绑在 **inode** 上。`rm -rf sessions && mkdir sessions` 之后是另一个
/// inode，旧 watch 还挂在已删的那个上 ⇒ **新目录里发生什么都听不见，且没有任何错误**。
/// 症状是 daemon「活着、不吭声」—— 08-13 离线对照实测：
///
/// | 组 | 帧 |
/// |---|---|
/// | 控制（不动 `sessions/`） | `hello · line · session_added · session_removed · tmux_sessions×2` |
/// | **删掉再建** | **`hello · tmux_sessions`** |
///
/// 下面那一行**与 `#60` 全链台架量到的 app 路径签名逐字相同**，而台架自己在 daemon
/// 已经开始盯之后跑了 `rm -rf -- "$CLAUDE_DIR/sessions"`（`graylight-suite.sh`）。
///
/// # 为什么重挂之后**必须重扫**
///
/// 「重建目录」与「往里写第一个文件」之间有窗口期：那次写发生在我们挂上之前的话，
/// 事件永远不会来。⇒ 挂上就把当下的 pidfile 全过一遍（`process_session_added` 幂等：
/// 同 sid 重复宣告会被 `active_sids` 挡掉）。
///
/// # 射程（如实写）
///
/// 这修的是**「盯着的目录被换掉/还没出现」**这一族。它是 `#60` 台架九拍拿不到读数的原因，
/// **但「它是否也是 `#60` 本身的根因」要等台架跑出第一份可信读数才能说** —— 见 `P0b §1c`：
/// 不拿到可信读数不写根因。
fn rewatch_sessions(
    debouncer: &mut notify_debouncer_mini::Debouncer<impl notify::Watcher>,
    sessions: &Path,
    watched: &mut bool,
    state: &mut ReaderState,
    sink: &mut FrameSink,
) {
    if !sessions.is_dir() {
        // 目录没了：把记账翻回去，等它回来时再挂（**不再是「永不重试」**）。
        if *watched {
            tracing::info!(
                "sessions 目录消失了 {} —— 解除记账，等它回来再挂",
                sessions.display()
            );
            let _ = debouncer.watcher().unwatch(sessions);
            *watched = false;
        }
        return;
    }
    // 目录在。**无条件先 unwatch 再 watch**：我们分辨不出「同一个 inode 的普通事件」与
    // 「换了 inode」——而重挂同一个 inode 是无害的（notify 幂等），漏挂新 inode 是致命的。
    // ⇒ 宁可多挂一次。`unwatch` 在没挂过时会报错，忽略它。
    let _ = debouncer.watcher().unwatch(sessions);
    match debouncer
        .watcher()
        .watch(sessions, RecursiveMode::NonRecursive)
    {
        Ok(()) => {
            if !*watched {
                tracing::info!("sessions 目录已（重新）挂上 watch: {}", sessions.display());
            }
            *watched = true;
            // 重扫：补上「重建 → 挂上」之间那段窗口期里写进去的东西。
            for entry in WalkDir::new(sessions).into_iter().filter_map(Result::ok) {
                let p = entry.path();
                if is_session_json(p) {
                    process_session_added(p, state, sink);
                }
            }
        }
        Err(e) => {
            *watched = false;
            tracing::warn!("重挂 sessions watch 失败 {}: {e}（下次事件再试）", sessions.display());
        }
    }
}

/// A `sessions/<PID>.json` appeared (or was already present): read it, extract
/// `sessionId`, cache PID→sid, emit [`Frame::SessionAdded`].
///
/// Idempotent: if we already cached the same sid for this path, skip the emit
/// so a debounced modify event does not re-announce an existing session.
fn process_session_added(path: &Path, state: &mut ReaderState, sink: &mut FrameSink) {
    let key = path_key(path);
    // PID is the sessions/<PID>.json filename stem.
    let Some(pid) = file_stem_str(path).and_then(|s| s.parse::<u32>().ok()) else {
        return;
    };
    let Some(bytes) = std::fs::read(path).ok() else {
        return;
    };
    let Some(sid) = parse_session_id(&bytes) else {
        return;
    };
    // Only ACTIVE if the process is actually alive (mirrors local STILL_ACTIVE).
    // A stale pidfile for a dead process is NOT an active session.
    if !pid_alive(pid) {
        return;
    }
    // Batch6-F21: interactivity gate. CC 2.1.x 的 daemon 后台任务
    // (--fork-session --resume) **会**写 sessions/<PID>.json（kind:"bg" +
    // jobId）——"子会话不注册 pidfile"的旧假设已过期。bg 进程是自己 pidfile
    // 的真作者（F20 身份证据对它们正确地放行），但不是交互会话、不该成 tab。
    // 保守规则（与本地 session_map 一字一致）：kind 字段存在且非 "interactive"
    // 才排除；旧 CC 不写该字段 → 放行。
    if let Some(kind) = parse_kind(&bytes) {
        if kind != "interactive" && !state.with_bg {
            // 审计 S1：若该 key 此前以 interactive 身份被 track（原地翻 kind /
            // PID 复用写同路径），对称走退休路径——与 F22-① 一致，免掉 poll 的
            // 2s 窗口，并补齐"同进程翻 kind"这条本地有、远端缺的清理。
            if let Some(old) = state.sessions.remove(&key) {
                // 原地翻成非交互 kind：交互会话确实没了（进程还在，但不该是 tab）。
                retire_sid_if_unreferenced(&old.sid, RemovalCause::Gone, state, sink);
            }
            tracing::debug!(
                "sessions json skipped (kind={kind}): {} pid {pid} is a non-interactive claude (bg task)",
                path.display()
            );
            return;
        }
    }
    // Batch9-F27：帧元信息一次解析（status diff 与后面的宣告帧共用）
    let meta: Option<serde_json::Value> = serde_json::from_slice(&bytes).ok();
    let meta_str = |k: &str| {
        meta.as_ref()
            .and_then(|v| v.get(k))
            .and_then(|x| x.as_str())
            .map(str::to_string)
    };
    // Idempotent: a debounced modify of an already-tracked session re-announces
    // nothing —— Batch9-F27：但 status/waitingFor 变了要发 session_status 帧
    // （远端红绿灯的唯一数据源；CC 仅在状态转换时重写 pidfile，天然稀疏）。
    if state.sessions.get(&key).map(|e| e.sid.as_str()) == Some(sid.as_str()) {
        let new_status = meta_str("status");
        let new_waiting = meta_str("waitingFor");
        let entry = state.sessions.get_mut(&key).expect("just checked");
        if entry.status != new_status || entry.waiting_for != new_waiting {
            entry.status = new_status.clone();
            entry.waiting_for = new_waiting.clone();
            sink.send(Frame::SessionStatus {
                sid: sid.clone(),
                status: new_status,
                waiting_for: new_waiting,
                // Claude pidfile 路 → 判活权威、省略 liveness_confidence（缺=authoritative）。DG2 判活/DG1
                // Codex 会话时才发 heuristic。
                liveness_confidence: None,
            });
        }
        return;
    }
    // Batch6-F22-①：同 pidfile 原地换 sid（/clear 等重写 sessionId）——旧 sid
    // 必须走 removed 路径：旧实现 insert 直接覆盖 entry，旧 sid 既不清
    // active_sids 也永不发 SessionRemoved、还被挤出活性 poll 遍历 → 假 live
    // 到断连（跨机审计实锤，本地 diff_sessions 按 sid 集合 diff 无此病）。
    // 引用计数感知：其它 pidfile 仍持旧 sid 时只解绑本 entry、不发帧。
    if let Some(old) = state.sessions.remove(&key) {
        // ★ S0：**同 pidfile 原地换 sid** —— 旧 sid 不是死了，是被顶替了。
        // monitor 若不知道这点，就会去查自己那份会陈旧的 tmux 快照、把它判成灰点。
        retire_sid_if_unreferenced(&old.sid, RemovalCause::Superseded, state, sink);
    }
    // Batch5-F20: add-time imposter check. `/proc/<pid>` existing is NOT enough:
    // a stale pidfile (CC force-killed, tmux server killed, power loss — nothing
    // ever cleans sessions/ up) plus PID reuse by any long-lived process (tmux
    // server, pane shell, sshd …) used to sail through and stream the whole dead
    // session's history as a live zombie tab, un-healable because the #34
    // procStart baseline below was captured FROM the imposter itself.
    //
    // Primary evidence: the pidfile's own `procStart` field — on Linux CC writes
    // the process's /proc starttime ticks verbatim (audit-verified bit-identical
    // on live sessions), so equality with the CURRENT occupant's starttime is
    // exact process identity (the same PID+starttime pair #34 uses), immune to
    // every wall-clock concern. Fallback heuristics (field absent, or mismatch
    // that could be CC format drift rather than reuse): the real claude wrote
    // this pidfile while alive, so its start must not be later than the file's
    // mtime; and its cmdline must look like claude. Missing data degrades to
    // allow (same philosophy as the local procStart-absent fallback).
    let current_ticks = proc_starttime(pid);
    match add_time_verdict(
        parse_procstart_ticks(&bytes),
        current_ticks,
        start_epoch_from_ticks(current_ticks),
        file_mtime_epoch(path),
        proc_cmdline(pid).as_deref(),
    ) {
        AddTimeVerdict::Imposter(reason) => {
            tracing::warn!(
                "stale sessions json ignored ({reason}): {} pid {pid} is not the claude that wrote it",
                path.display()
            );
            return;
        }
        AddTimeVerdict::Alive => {}
    }
    // #34: the poll baseline. Reuse the very ticks the verdict just examined —
    // no second /proc read, so no verdict-to-baseline TOCTOU window.
    let start = current_ticks;
    // P2：`key` 下面被 insert 消耗掉，先留一份给 pidfd 看守用。
    let key_for_watch = key.clone();
    state.sessions.insert(
        key,
        SessionEntry {
            pid,
            sid: sid.clone(),
            status: meta_str("status"),
            waiting_for: meta_str("waitingFor"),
        },
    );
    state.active_sids.insert(sid.clone());
    // `U-NP④`：身份打标（`@ccm_sid`）—— 接 `shared/ccm` 那条每秒轮询的班，见
    // `control::identity_tag`（跨层边已登记进 `layering_guard`）。放在冒名检查**之后**。
    crate::control::identity_tag::tag(pid, &sid);
    // P2：给这个进程实例挂 pidfd 看守（取代原先每 2s 一遍的判活扫描）。
    // `start` 就是上面 verdict 用过的那次 /proc 读，不再多读一次。
    arm_pid_watcher(&key_for_watch, pid, start, state);
    // Batch8-F25：先定位该 sid 的 jsonl（帧要带 path 供 monitor 旁路快照；
    // mtime 降序，first=当前活跃文件。会话刚起还没写首行时为空 → path=None，
    // 此时无历史可拉，后续行天然从 tail 全量到达）。
    let projects = state.projects.clone();
    let jsonls = find_sid_jsonls(&projects, &sid);
    // 历史处理按模式分流（Batch8-F25）：
    // - tail-only：**先 prime**（推进 cursor/seq 到当前完整行数 L，零行帧）——
    //   帧要带 first 文件的 L 供 monitor 校验快照完整性（审计 D-I2），prime
    //   无行帧故"帧先于行"契约不受影响；
    // - 全量（默认，旧 monitor 兼容）：帧先行，再照旧全量推流（镜像本地
    //   session-added 触发的 force-rescan）。
    let mut first_lines: Option<u64> = None;
    if state.tail_only {
        for (i, p) in jsonls.iter().enumerate() {
            let n = prime_file_cursor(p, state);
            if i == 0 {
                first_lines = Some(n);
            }
        }
    }
    sink.send(Frame::SessionAdded {
        sid: sid.clone(),
        // 本 producer = Claude pidfile 发现路 → agent_kind/liveness_confidence 省略（缺=claude/authoritative）。
        // DG1 Codex 发现路才发 agent_kind="codex"+liveness_confidence="heuristic"。
        agent_kind: None,
        liveness_confidence: None,
        session_kind: meta_str("kind"),
        // E73：pidfile 的 `attachable`。**只认真正的布尔** —— 字符串 "false" 之类当没写
        //（缺席 = true = 照旧），宁可少一次门控也不要把一个拼错的值当成"不可 attach"。
        attachable: meta
            .as_ref()
            .and_then(|v| v.get("attachable"))
            .and_then(|x| x.as_bool()),
        cwd: meta_str("cwd"),
        name: meta_str("name"),
        path: jsonls.first().map(|p| p.to_string_lossy().into_owned()),
        lines: first_lines,
        status: meta_str("status"),
        waiting_for: meta_str("waitingFor"),
    });
    if !state.tail_only {
        for p in &jsonls {
            process_jsonl(p, state, sink);
        }
    }
}

/// A `sessions/<PID>.json` was deleted: look up the cached sid (the file is
/// gone, so we cannot read it now) and retire the sid if unreferenced.
fn process_session_removed(path: &Path, state: &mut ReaderState, sink: &mut FrameSink) {
    let key = path_key(path);
    if let Some(e) = state.sessions.remove(&key) {
        // pidfile 从盘上没了 = 真死。
        retire_sid_if_unreferenced(&e.sid, RemovalCause::Gone, state, sink);
    }
}

/// Batch6-F22：sid 退休的**唯一**出口——`sessions` 表中已无任何存活 entry 持有
/// 该 sid 时才清 active_sids + 发 [`Frame::SessionRemoved`]。同 sid 多 pidfile
/// （resume 时原进程未死）场景下，先死的那个只解绑、不误杀整个 tab。
/// 调用方约定：先从 `state.sessions` remove 掉当事 entry 再调本函数。
fn retire_sid_if_unreferenced(
    sid: &str,
    cause: RemovalCause,
    state: &mut ReaderState,
    sink: &mut FrameSink,
) {
    let still_referenced = state.sessions.values().any(|e| e.sid == sid);
    if still_referenced {
        tracing::debug!("sid {sid} still referenced by another pidfile; not retiring");
        return;
    }
    state.active_sids.remove(sid);
    sink.send(Frame::SessionRemoved {
        sid: sid.to_string(),
        cause,
    });
}

/// Walk `projects/` for this session's jsonl (`<sid>.jsonl`, non-subagent) and
/// stream its already-present lines. Called when a session becomes active so an
/// already-running session snapshots on session-added (mirrors local force-rescan).
fn find_sid_jsonls(projects: &Path, sid: &str) -> Vec<std::path::PathBuf> {
    if !projects.is_dir() {
        return Vec::new();
    }
    let mut v: Vec<std::path::PathBuf> = WalkDir::new(projects)
        .into_iter()
        .filter_map(Result::ok)
        .map(|e| e.into_path())
        .filter(|p| {
            is_jsonl(p)
                && !is_subagent_path(p)
                && p.file_stem().and_then(|s| s.to_str()) == Some(sid)
        })
        .collect();
    // Batch8 审计（缝合-R4）：同 sid 多 jsonl（项目目录改名后 resume）时
    // WalkDir 顺序未定义——按 mtime 降序让 first = 当前活跃文件（帧的 path/
    // lines 取 first，快照拉错陈文件 = 当前历史全缺）。
    v.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
    v
}

/// Batch8-F25：tail-only 的初扫/宣告路径——把 cursor 与 seq 计数器推进到当前
/// **最后一个完整行**（F14 torn-line 语义：残行不计数、留给 tail 阶段），
/// 不发任何行帧。之后 notify 到来的新行 seq == 此刻完整行数 L（行号语义），
/// 与 monitor 快照侧的 0..L'-1 编号同处一个行号空间，重叠区被 (sid,seq)
/// 去重精确吸收（MASTERPLAN-batch8 §2）。
fn prime_file_cursor(path: &Path, state: &mut ReaderState) -> u64 {
    let Some(session_id) = file_stem_str(path) else {
        return 0;
    };
    if !state.active_sids.contains(&session_id) {
        return 0;
    }
    let key = path_key(path);
    let key_str = key.to_string_lossy().into_owned();
    let prev = state.offsets.get(&key).copied().unwrap_or_default();
    // F04：同 `process_jsonl`，只读新字节。
    let Some((chunk, chunk_start, file_len)) = read_tail_from(path, prev) else {
        return 0;
    };
    // F04 第 3 步：**只数不建**。此前这里把每行 `to_string` 成 `Vec<ReadLine>`，
    // 而下面只用了 `.len()`（一条 debug 日志）—— 首次 prime 时那一段就是整份文件。
    let (n_lines, cursor) = count_new_lines_at(
        &chunk,
        chunk_start,
        file_len,
        prev,
        &key_str,
        &mut state.seqs,
    );
    state.offsets.insert(key, cursor);
    tracing::debug!(
        "primed {key_str}: cursor→{} (+{} lines suppressed, tail seq starts here)",
        cursor.consumed,
        n_lines
    );
    // Batch8 审计 D-I2：返回 prime 后的行号计数器现值（= 完整行总数 L），
    // session_added 帧带给 monitor 做快照完整性校验（拉到的行数 < L = 快照
    // 中途断/daemon 报错——exit status 拿不到，行数校验更强）。
    state.seqs.peek(&key_str)
}

/// Add-time verdict on whether the current occupant of a PID is plausibly the
/// claude process that wrote the `sessions/<PID>.json` pidfile (Batch5-F20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddTimeVerdict {
    Alive,
    Imposter(&'static str),
}

/// A process that started noticeably later than the pidfile's last write cannot
/// be its author. 60s absorbs clock fuzz (mtime granularity, btime rounding,
/// NTP slew) — real reuse gaps are hours-to-weeks, so the tolerance is safe.
const ADD_TIME_TOLERANCE_SECS: u64 = 60;

/// Pure decision core (unit-tested on every platform):
///
/// - **identity evidence（primary）**: the pidfile's `procStart` field equals
///   the current occupant's `/proc/<pid>/stat` starttime ticks → the occupant
///   IS the author (PID + starttime is exact process identity, the same pair
///   #34 relies on) — Alive, no further checks, immune to every wall-clock
///   concern (NTP steps, NFS mtime, btime drift). A **mismatch** is NOT
///   immediately fatal: it is either PID reuse (imposter) or a CC version
///   writing a different format into `procStart` (a hard reject on format
///   drift would black out every real session) — fall through, the heuristics
///   below catch the stale-pidfile case either way.
/// - **time evidence**: `proc_start_epoch > file_mtime_epoch + tolerance` →
///   imposter. CC rewrites its pidfile on every state transition, so the file's
///   mtime is a lower bound on "the real claude was alive at this instant"; a
///   later-started process is a PID-reuse squatter. Both sides are wall-clock
///   seconds from the same host clock (btime + starttime/USER_HZ vs mtime), so
///   there is no timezone concern. This also subsumes the reboot case: after a
///   reboot every process starts after btime > old mtime.
/// - **cmdline evidence**: a readable, non-empty cmdline that mentions neither
///   `claude` nor `node` is not a claude CLI (tmux, bash, sshd …).
/// - Missing data (absent procStart, unreadable stat/mtime/cmdline) skips that
///   check — degrade to allow, mirroring the local procStart-absent fallback.
fn add_time_verdict(
    pidfile_procstart_ticks: Option<u64>,
    current_starttime_ticks: Option<u64>,
    proc_start_epoch: Option<u64>,
    file_mtime_epoch: Option<u64>,
    cmdline: Option<&str>,
) -> AddTimeVerdict {
    // F74b(#43「父会话恒绿」总闸)：bg-spare = 守护池停泊的备用进程（cmdline 含 "bg-spare"）。
    // 它是真 claude 进程、会写合规 pidfile、procStart 自洽——**必须在 exact-identity 之前拦**，
    // 否则下面的 `recorded == current` 会把它判 Alive 而恒绿。语义上它不是一个运行中的会话。
    if let Some(cmd) = cmdline {
        if cmd.to_lowercase().contains("bg-spare") {
            return AddTimeVerdict::Imposter("bg-spare");
        }
    }
    if let (Some(recorded), Some(current)) = (pidfile_procstart_ticks, current_starttime_ticks) {
        if recorded == current {
            return AddTimeVerdict::Alive; // exact identity: author confirmed
        }
        // mismatch: fall through to the heuristics (see doc comment)
    }
    if let (Some(start), Some(mtime)) = (proc_start_epoch, file_mtime_epoch) {
        if start > mtime + ADD_TIME_TOLERANCE_SECS {
            return AddTimeVerdict::Imposter("started-after-pidfile");
        }
    }
    if let Some(cmd) = cmdline {
        let lower = cmd.to_lowercase();
        if !crate::agents::claudecode::liveness::cmdline_may_be_agent(&lower) {
            return AddTimeVerdict::Imposter("cmdline");
        }
    }
    AddTimeVerdict::Alive
}

/// Parse the pidfile's `kind` field ("interactive" / "bg" …，Batch6-F21)。
/// None = 字段缺失（旧 CC）或不可读 → 调用方放行。
fn parse_kind(bytes: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    v.get("kind")?.as_str().map(str::to_string)
}

/// Parse the pidfile's `procStart` field as starttime ticks. CC writes it as a
/// decimal string on Linux（audit-verified verbatim /proc starttime ticks）；
/// accept a bare number too. Anything else → None（fallback heuristics apply）.
fn parse_procstart_ticks(bytes: &[u8]) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let field = v.get("procStart")?;
    if let Some(s) = field.as_str() {
        return s.trim().parse::<u64>().ok();
    }
    field.as_u64()
}

/// The pidfile's mtime as epoch seconds (None on any error → check skipped).
fn file_mtime_epoch(path: &Path) -> Option<u64> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    mtime
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Pure parse of the `sessionId` field out of a sessions JSON blob.
fn parse_session_id(bytes: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    v.get("sessionId")?.as_str().map(str::to_string)
}

/// The reader's send half: a bounded-channel sender that turns a wedged pipe
/// into an explicit **overflow signal** (#32) instead of silently losing data.
///
/// A full channel means the writer/SSH pipe is wedged. We still `try_send` (never
/// blocking the notify reader), but now we **count** the frames we had to drop
/// and, once the channel drains enough to accept it, emit a single
/// [`Frame::Overflow`] carrying that count. The client warns the user that live
/// lines were lost. One signal per congestion burst — naturally throttled.
struct FrameSink {
    tx: mpsc::Sender<Frame>,
    /// Frames dropped since the last successfully-sent `Overflow` signal.
    dropped: u64,
    /// 〔audit-0805 F03〕那批丢帧里**不可恢复**的那些的身份。
    ///
    /// 只有计数的 `Overflow` 对内容帧够用（行还在远端 jsonl 里），对状态增量帧不够：
    /// 它是一次差分的结果、别处不存在，客户端拿着「丢了 N 条」没法重同步。
    lost: Vec<LostFrame>,
    /// 身份表触顶过（超出 [`LOST_IDENTITY_CAP`] 的那些只计数、不留身份）。
    lost_truncated: bool,
}

/// 丢帧身份表的上限〔audit-0805 F03，定框 **E5**：上限与超限语义成对定义〕。
///
/// **超限语义**：超出的那些**仍然计入 `dropped`**，只是不再留身份，并置 `lost_truncated`
/// 让客户端知道「这份清单不全」——**不是**静默截断（那正是本区在治的病）。
///
/// ⚠ **为什么必须有界**：通道卡死时 `dropped` 会一直涨，如果身份表跟着无界增长，
/// 就等于把 `CHANNEL_CAPACITY` 想防的内存增长从帧**挪到了 `Overflow` 自己身上**。
/// 64 的取法：一次拥塞里真正值得逐个重同步的会话数量级是「几个到几十个」；
/// 再多时客户端理性的做法本来就是整体重取快照，而 `lost_truncated` 恰好告诉它该这么做。
const LOST_IDENTITY_CAP: usize = 64;

impl FrameSink {
    fn new(tx: mpsc::Sender<Frame>) -> Self {
        FrameSink {
            tx,
            dropped: 0,
            lost: Vec::new(),
            lost_truncated: false,
        }
    }

    /// Send `frame`, first flushing any owed overflow signal.
    ///
    /// Order matters: we try to emit the pending `Overflow` *before* the real
    /// frame so the client learns "you lost N frames" no later than the next
    /// frame it receives. If the channel is still full, we keep owing the count
    /// (it only ever grows until a send succeeds); a closed channel is a quiet
    /// shutdown (the loop checks `is_closed`).
    fn send(&mut self, frame: Frame) {
        if self.dropped > 0 {
            match self.tx.try_send(Frame::Overflow {
                dropped: self.dropped,
                lost: self.lost.clone(),
                lost_truncated: self.lost_truncated,
            }) {
                Ok(()) => {
                    tracing::warn!(
                        "recovered from frame-channel overflow; signalled {} dropped frame(s), \
                         {} unrecoverable identities{}",
                        self.dropped,
                        self.lost.len(),
                        if self.lost_truncated {
                            " (identity list truncated)"
                        } else {
                            ""
                        }
                    );
                    self.dropped = 0;
                    self.lost.clear();
                    self.lost_truncated = false;
                }
                // Still wedged: keep owing the count, retry on the next send.
                Err(mpsc::error::TrySendError::Full(_)) => {}
                Err(mpsc::error::TrySendError::Closed(_)) => return,
            }
        }
        match self.tx.try_send(frame) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(frame)) => {
                self.dropped += 1;
                // 〔audit-0805 F03〕**不可恢复的那些要留下身份**，否则客户端只知道
                // 「丢了 N 条」，而状态增量帧丢了别处没有、它无从重同步。
                // 有界：超出 `LOST_IDENTITY_CAP` 的仍计入 `dropped`，只是不再留身份并置位标志。
                if !frame.loss_is_recoverable() {
                    if self.lost.len() < LOST_IDENTITY_CAP {
                        self.lost.push(frame.loss_identity());
                    } else {
                        self.lost_truncated = true;
                    }
                }
                tracing::warn!(
                    "frame channel full (cap {CHANNEL_CAPACITY}); dropping frame \
                     ({} dropped since last overflow signal, {} unrecoverable identities kept)",
                    self.dropped,
                    self.lost.len()
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Writer gone (shutdown). Nothing to do; the loop checks is_closed.
            }
        }
    }

    fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

/// `true` for a regular `*.jsonl` file.
fn is_jsonl(p: &Path) -> bool {
    crate::agents::claudecode::records::is_session_file(p)
}

/// `true` for a `sessions/<PID>.json` file. We only ever feed this paths under
/// the sessions dir, so an extension check suffices.
fn is_session_json(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "json")
}

/// subagent JSONL is excluded: any path containing a `subagents` segment.
/// Mirrors `../src-tauri/src/watcher.rs::is_subagent_path`.
fn is_subagent_path(p: &Path) -> bool {
    p.components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case("subagents"))
}

fn file_stem_str(p: &Path) -> Option<String> {
    p.file_stem().and_then(|s| s.to_str()).map(str::to_string)
}

#[cfg(test)]
mod tests {

    /// ★ **事件分派不许有兜底臂**〔audit-0805 08-06〕。
    ///
    /// # 它钉的是一个「没人盯的前提」，不是一个缺陷
    ///
    /// 本文件 87 条判据里，七个 `WatchEvent` 变体**每一个都被构造过** ——
    /// 抽样时逐个数过（`Notify` 3 · `TmuxServerGone` 4 · `PidDied` 6 ·
    /// `TmuxObserved` 4 · `TmuxProbeDue` 5 · `Poke` 5 · `Shutdown` 4）。
    /// 而「加第八个变体时会不会被漏掉」靠的**不是**这些判据，
    /// 是**编译器**：顶层分派是一条没有兜底臂的 `match`，少一个变体就编不过。
    ///
    /// ⚠ 那是一个**前提**，而且今天没人盯着它：谁在那条 `match` 上加一条兜底臂，
    /// 穷尽性当场消失，新变体从此被静默吞掉 —— 而**所有既有判据仍然全绿**
    ///（它们各测各的变体，没有一条会因为「多了一个没人处理的变体」而红）。
    /// 本会话反复量到的正是这个形状：**一条纪律的成立依赖另一条，而那条依赖没人盯。**
    ///
    /// ⇒ 本条只做一件事：钉住那条 `match` 里**没有兜底臂**。
    #[test]
    fn the_event_dispatch_has_no_catch_all_arm() {
        let prod = crate::guard_support::production_code(include_str!("watcher.rs"));
        let anchor = "WatchEvent::Notify(Ok(events)) =>";
        let at = prod
            .find(anchor)
            .expect("找不到事件分派的锚点臂 —— 分派改形了，本条要跟着改");
        // 从锚点臂往后取到分派块结束：按缩进找同级臂，遇到缩进更浅的行即出块。
        let indent = prod[..at].rfind('\n').map_or(0, |k| at - k - 1);
        let mut arms: Vec<String> = Vec::new();
        for line in prod[at - indent..].lines() {
            let cur = line.len() - line.trim_start().len();
            if line.trim().is_empty() {
                continue;
            }
            if cur < indent {
                break;
            }
            if cur == indent {
                let head: String = line.trim_start().chars().take(24).collect();
                if head.starts_with("WatchEvent::") || head.starts_with('_') {
                    arms.push(head);
                }
            }
        }
        // 抽取器自检：抠不到臂 ⇒ 下面那条会零命中地绿。
        assert!(
            arms.len() >= 6,
            "只抠到 {} 条分派臂（08-06 实测 7）—— 抽取坏了，本条此刻是空转的：{arms:?}",
            arms.len()
        );
        assert!(
            !arms.iter().any(|a| a.starts_with('_')),
            "事件分派里出现了兜底臂：{arms:?}\n\
             ⚠ 它一加上，`match` 的穷尽性就没了 —— 新增的 `WatchEvent` 变体会被**静默吞掉**，\n\
             而本文件既有的判据**不会有一条因此变红**（它们各测各的变体）。\n\
             daemon 的判活全靠这七路信号；被吞掉的那一路不会报错，只会「什么都不发生」。\n\
             ⇒ 要新增变体就在这里显式处理它；确实无事可做也请写成具名臂加一句注释。"
        );
    }
    use super::*;

    // ---------- P2（zero-poll-liveness）：pidfd 判活 ----------

    /// 起一个**假**进程当靶子。**绝不起真实已认证的 claude/codex**——这里只要一个
    /// 「活着、能被杀、pid 可拿」的进程，`sleep` 足够。
    fn spawn_target() -> std::process::Child {
        std::process::Command::new("sleep")
            .arg("30")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn sleep")
    }

    /// `pidfd_open` 的基本性质：自己开得开；不存在的 pid 开不开。
    ///
    /// 不存在的 pid 取一个刚退出并已回收的子进程 pid——比硬编码一个大数可靠
    /// （大数也可能恰好被占）。
    // U4a：本测试直接调 `pidfd_open`，那是 Linux-only 原语。
    #[cfg(target_os = "linux")]
    #[test]
    fn pidfd_open_works_for_self_and_fails_for_dead_pid() {
        assert!(
            pidfd_open(std::process::id()).is_ok(),
            "自己的 pid 必须开得开"
        );

        let mut child = spawn_target();
        let dead_pid = child.id();
        child.kill().expect("kill");
        child.wait().expect("reap"); // 回收，pid 彻底消失
        let err = pidfd_open(dead_pid).expect_err("已回收的 pid 不该开得开");
        assert_eq!(
            err.raw_os_error(),
            Some(libc::ESRCH),
            "应是 ESRCH，实得 {err:?}"
        );
    }

    /// ★ **双向验收（本功能 DoD 的硬项）**：杀 → 事件真的到；不杀 → 事件不到。
    ///
    /// 只测"杀了会到"是不够的——一个恒发 `PidDied` 的实现也能让那半边绿。
    /// 反方向那半边才是钉住"事件由**目标进程退出**驱动"的那条。
    ///
    /// 测试里用 `recv_timeout` 是可以的：**要求零定时器的是生产循环**，不是测试。
    #[test]
    fn pidfd_watcher_fires_on_death_and_stays_silent_while_alive() {
        let key = PathBuf::from("/tmp/ccm-p2-fixture/1234.json");
        let mut child = spawn_target();
        let pid = child.id();
        let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
        // expected_start=None ⇒ 判据 2 退化成存在性（`is_same_live_process` 的 `_ => true` 臂）。
        spawn_pid_watcher(PidWatchTarget::Session { key: key.clone() }, pid, None, tx);

        // —— 反方向：目标还活着 ⇒ 不该有任何事件 ——
        match rx.recv_timeout(Duration::from_millis(400)) {
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            other => panic!("目标活着时不该有事件，实得 {:?}", other.is_ok()),
        }

        // —— 正方向：杀掉 ⇒ 内核唤醒 poll ⇒ 事件到 ——
        child.kill().expect("kill");
        let got = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("杀掉后必须收到 PidDied（超时 = pidfd 没被内核唤醒）");
        match got {
            WatchEvent::PidDied { key: k, pid: p } => {
                assert_eq!(k, key, "带回的 key 必须是挂看守时那个");
                assert_eq!(
                    p, pid,
                    "带回的 pid 必须是挂看守时那个（消费侧靠它挡陈旧唤醒）"
                );
            }
            _ => panic!("期望 PidDied"),
        }
        child.wait().expect("reap");
    }

    /// 判据 1：`pidfd_open` 失败（目标已不在）⇒ **立刻**发 `PidDied`，不静默丢。
    #[test]
    fn pidfd_watcher_reports_dead_when_open_fails() {
        let mut child = spawn_target();
        let pid = child.id();
        child.kill().expect("kill");
        child.wait().expect("reap");

        let key = PathBuf::from("/tmp/ccm-p2-fixture/dead.json");
        let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
        spawn_pid_watcher(PidWatchTarget::Session { key: key.clone() }, pid, None, tx);
        match rx
            .recv_timeout(Duration::from_secs(2))
            .expect("open 失败必须立刻报死")
        {
            WatchEvent::PidDied { key: k, pid: p } => {
                assert_eq!((k, p), (key, pid));
            }
            _ => panic!("期望 PidDied"),
        }
    }

    /// ★ 判据 2：**PID 复用**（open 之后身份复核不符）⇒ 当死。
    ///
    /// 造法：给一个**活着**的进程配一个**对不上**的 procStart 基线。真实场景里这等价于
    /// 「读 pidfile 拿到 (pid, start) → 那个进程死了 → 别人占了同一个 pid → 我们开到了冒名者」。
    /// 这一格是原先那套 procStart 启发式的全部去处：从"每 2s 复查"降成"挂看守时校验一次"。
    #[test]
    fn pidfd_watcher_rejects_reused_pid_via_start_mismatch() {
        let mut child = spawn_target();
        let pid = child.id();
        let real = proc_starttime(pid);
        assert!(real.is_some(), "本机应能读到 /proc/<pid>/stat 的 starttime");
        // 刻意错开：真值 + 1 ⇒ `is_same_live_process(true, Some(a), Some(b))` 的 a != b 臂。
        let bogus = real.map(|t| t + 1);

        let key = PathBuf::from("/tmp/ccm-p2-fixture/reused.json");
        let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
        spawn_pid_watcher(PidWatchTarget::Session { key: key.clone() }, pid, bogus, tx);
        match rx
            .recv_timeout(Duration::from_secs(2))
            .expect("基线不符必须立刻报死（否则会把冒名者当成原会话一直判活）")
        {
            WatchEvent::PidDied { key: k, pid: p } => {
                assert_eq!((k, p), (key, pid));
            }
            _ => panic!("期望 PidDied"),
        }
        child.kill().expect("kill");
        child.wait().expect("reap");
    }

    /// `arm_pid_watcher` 幂等：同一 `(pidfile, pid)` 挂两次只起一条看守。
    /// 按**对**而不是按路径存，所以同路径换了 pid 要能重新挂——两条都测。
    #[test]
    fn arm_pid_watcher_is_idempotent_per_pidfile_and_pid() {
        let mut st = ReaderState::new(PathBuf::from("/tmp/ccm-p2-proj"), false, false);
        let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
        st.events_tx = Some(tx);
        let key = PathBuf::from("/tmp/ccm-p2-fixture/idem.json");

        // 用一个已死的 pid：每次成功挂载都会立刻投一条 PidDied ⇒ 收到几条 = 挂了几次。
        let mut child = spawn_target();
        let dead = child.id();
        child.kill().expect("kill");
        child.wait().expect("reap");

        arm_pid_watcher(&key, dead, None, &mut st);
        arm_pid_watcher(&key, dead, None, &mut st); // 同对 ⇒ 应被跳过
        assert!(
            rx.recv_timeout(Duration::from_secs(2)).is_ok(),
            "第一次必须挂上"
        );
        match rx.recv_timeout(Duration::from_millis(400)) {
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            _ => panic!("同一 (pidfile, pid) 不该挂第二条看守"),
        }

        // 同路径换 pid ⇒ 必须重新挂（/clear 原地换 sid、PID 复用写同路径）
        let mut child2 = spawn_target();
        let dead2 = child2.id();
        child2.kill().expect("kill");
        child2.wait().expect("reap");
        arm_pid_watcher(&key, dead2, None, &mut st);
        assert!(
            rx.recv_timeout(Duration::from_secs(2)).is_ok(),
            "同路径换 pid 必须重新挂看守"
        );
    }

    /// ★★ **PID 复用：同路径、同 pid、不同进程实例，必须重新挂**〔audit-0805 F11 / 报告 I-7〕。
    ///
    /// # 上面那条测的是「换 pid」，而这一格是「**没换 pid**」
    ///
    /// `spawn_pid_watcher` 的头注声称要处理「PID 复用写同路径」，而去重键此前是
    /// `(路径, pid)` 两元 —— PID 被复用时**这两元都没变**，于是落进去重、**不再挂看守**。
    /// **头注声称能处理的那格，恰是它处理不了的那格。**
    ///
    /// 区分进程实例的东西（`starttime`）本来就在参数里，只是没进键。F11 把它加进去了。
    #[test]
    fn a_recycled_pid_at_the_same_path_gets_a_fresh_watcher() {
        let mut st = ReaderState::new(PathBuf::from("/tmp/ccm-f11-proj"), false, false);
        let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
        st.events_tx = Some(tx);
        let key = PathBuf::from("/tmp/ccm-f11-fixture/reuse.json");

        let mut child = spawn_target();
        let dead = child.id();
        child.kill().expect("kill");
        child.wait().expect("reap");

        // 同一个 pid、同一个路径，但**两个不同的 starttime** = PID 被复用了。
        arm_pid_watcher(&key, dead, Some(1_000), &mut st);
        assert!(
            rx.recv_timeout(Duration::from_secs(2)).is_ok(),
            "抽取器自检：第一次都没挂上，本条后面的判定无意义"
        );
        arm_pid_watcher(&key, dead, Some(2_000), &mut st);
        assert!(
            rx.recv_timeout(Duration::from_secs(2)).is_ok(),
            "★ 同路径 + 同 pid + **不同 starttime** 没有重新挂看守。\n\
             那正是 PID 复用：pid 数字被回收给了另一个进程，而 (路径, pid) 两元键看不出区别\n\
             ⇒ 新进程死了没人报，会话永远停在「活着」。\n\
             `spawn_pid_watcher` 的头注声称处理这一格 —— 别让它继续说假话。"
        );

        // 反向：**同 starttime** 仍然要去重（防把上面写成「每次都挂」）。
        arm_pid_watcher(&key, dead, Some(2_000), &mut st);
        match rx.recv_timeout(Duration::from_millis(400)) {
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            _ => panic!("同 (路径, pid, starttime) 三元不该挂第二条看守 —— 去重被写坏了"),
        }
    }

    /// `events_tx` 为 `None`（单元测试默认）时 `arm_pid_watcher` 什么都不做——
    /// 11 处 `ReaderState::new` 因此不必改签名。
    #[test]
    fn arm_pid_watcher_is_a_noop_without_sender() {
        let mut st = ReaderState::new(PathBuf::from("/tmp/ccm-p2-proj"), false, false);
        arm_pid_watcher(&PathBuf::from("/x/1.json"), 1, None, &mut st);
        assert!(
            st.pid_watched.is_empty(),
            "没有发送端时不该登记，也不该起线程"
        );
    }

    /// ★ 守卫：**事件 channel 必须建在 Phase 1 初始扫描之前**。
    ///
    /// 为什么需要一条扫源码的守卫而不是一条行为测试：`watch_loop` 要真文件系统 + notify +
    /// 多线程，单测碰不到；而这个顺序错了的后果**极其安静**——`process_session_added` 在
    /// Phase 1 里被调用时 `events_tx` 还是 `None` ⇒ `arm_pid_watcher` 直接 return ⇒
    /// **daemon 启动时就活着的会话一个 pidfd 看守都没有**，永远判不出死。而 P2 之前那条
    /// 2s 判活轮询是覆盖它们的 ⇒ 是回归。
    ///
    /// **P2 初版真犯了这个错**（channel 建在 "Phase 2: live watch" 处），是被 clippy 的
    /// 「field `start` is never read」间接暴露出来的——不是被任何测试抓到的。所以补这条。
    /// ⚠⚠ **08-08 订正：这条原来比的是一行注释的位置。**
    ///
    /// 原实现拿 `// --- Phase 1: synchronous initial scan. ---` 当「初始扫描」的锚点。
    /// 实测：把**真扫描那一块**（`if sessions.is_dir()` 那段）搬到 `events_tx` 注入之前、
    /// **注释原地不动**，本条照样绿 —— 而那正是它自陈要挡的那个静默回归。
    /// ⇒ 「文本顺序 ≠ 执行顺序」这一族里还有更基础的一层：**判据得先比对代码，
    /// 而不是比对描述代码的那句话**（本会话第四次撞上同一形状）。
    ///
    /// 另一半也一起修：注释若被重排/改写，原实现会红在一个**与语义无关**的位置上
    /// （把注入挪到注释之后、真扫描之前，语义完全正确却会红）。
    /// 现在锚在 `WalkDir::new(&sessions)`（生产段唯一一处，扫描真正开始的地方）。
    #[test]
    fn events_channel_is_created_before_the_initial_scan() {
        let src = crate::guard_support::production_code(include_str!("watcher.rs"));
        let tx_at = src
            .find("state.events_tx = Some(events_tx.clone());")
            .expect("找不到 events_tx 注入点——守卫锚点漂了，先修锚点别改断言");
        let scan_at = src
            .find("WalkDir::new(&sessions)")
            .expect("找不到初始扫描的锚点（`WalkDir::new(&sessions)`）——扫描改写了就把本条一起改");
        // 锚点唯一性：两个都必须**恰好一处**，否则「谁在前」比的可能是别处那一份。
        assert_eq!(
            src.matches("WalkDir::new(&sessions)").count(),
            1,
            "初始扫描的锚点在生产段里不止一处 —— 本条会比到别的那一份上去"
        );
        assert!(
            tx_at < scan_at,
            "events_tx 必须在 Phase 1 初始扫描**之前**注入，否则启动时已在跑的会话拿不到 \
             pidfd 看守（静默回归：那些会话永远判不出死）。实测 tx@{tx_at} scan@{scan_at}"
        );
        // 反向自检：断言的是"两个锚点都找到了 + 源码真读进来了"，
        // 不是"命中数 < N"——阈值不能挂在被检查的量上。
        assert!(
            src.len() > 1000,
            "include_str! 没读到源码，上面的断言是空转"
        );
    }

    /// **P5：ticker 已删，但「首轮立即发一拍」这条行为必须留着** ——
    /// 这正是删 ticker 时差点顺手删掉的东西：monitor 一连上就该拿到 tmux 状态，
    /// 否则空闲机器上要等到第一个 hook 触发才探（可能是**永远**）。
    /// 本测试从「ticker 首拍」改判为「一次性初探真的发了一拍」，**性质没放松**。
    #[test]
    fn initial_probe_fires_once() {
        let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
        initial_tmux_probe(&tx);
        match rx.try_recv().expect("初探必须立刻发一拍") {
            WatchEvent::TmuxProbeDue => {}
            _ => panic!("期望 TmuxProbeDue"),
        }
        // 且**只发一拍** —— 它不是节拍器。
        assert!(
            rx.try_recv().is_err(),
            "初探不该发第二拍（那就又成定时器了）"
        );
    }

    /// P5：`WatcherPoke::shutdown()` 发的是 `Shutdown` 而不是别的。
    /// 这条漏了不会红任何别的测试（进程退出时 reader 线程随之消亡），所以单独钉。
    #[test]
    fn poke_shutdown_sends_shutdown_variant() {
        let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
        WatcherPoke(tx).shutdown();
        assert!(matches!(rx.try_recv(), Ok(WatchEvent::Shutdown)));
    }

    // ---------- P5（zero-poll-liveness）：快照差分 → 正向死亡帧 ----------

    use std::collections::BTreeSet;

    fn names(v: &[&str]) -> Option<BTreeSet<String>> {
        Some(v.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn session_names_takes_first_column_only() {
        let raw = "s1\t/p\tclaude\t1\t2\tsid-a\ns2\t/q\tbash\t0\t1\t\n";
        assert_eq!(
            session_names(raw),
            ["s1", "s2"].iter().map(|s| s.to_string()).collect()
        );
    }

    #[test]
    fn session_names_ignores_blank_lines_and_no_tmux_sentinel() {
        assert!(session_names("\n\n").is_empty());
        assert!(session_names("NO_TMUX\n").is_empty());
    }

    #[test]
    fn diff_reports_only_the_disappeared_one() {
        let mut prev = names(&["a", "b", "c"]);
        let closed = diff_closed(
            &mut prev,
            &TmuxObservation::Sessions("a\t/p\tsh\t0\t1\t\nc\t/p\tsh\t0\t1\t\n".into()),
        );
        assert_eq!(closed, vec!["b".to_string()]);
        assert_eq!(prev, names(&["a", "c"]));
    }

    /// 信号会合并 ⇒ 一次差分要能报出**所有**消失的（逐事件必漏）。
    #[test]
    fn diff_reports_all_disappeared_at_once() {
        let mut prev = names(&["a", "b", "c", "d"]);
        let closed = diff_closed(
            &mut prev,
            &TmuxObservation::Sessions("b\t/p\tsh\t0\t1\t\n".into()),
        );
        assert_eq!(
            closed,
            vec!["a".to_string(), "c".to_string(), "d".to_string()]
        );
    }

    #[test]
    fn server_gone_closes_everything() {
        let mut prev = names(&["a", "b"]);
        let closed = diff_closed(&mut prev, &TmuxObservation::NoServer);
        assert_eq!(closed, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(prev, Some(BTreeSet::new()));
    }

    /// ★ 最要紧的一条：**观测失败 ≠ 都没了**。
    /// 报一堆死亡帧会把活着的会话全部误 retire —— 这正是 P1 当年那条
    /// 「空 `raw` 同时意味着零会话和出错」的教训在死亡帧这条路上的复发点。
    #[test]
    fn unobservable_never_reports_deaths_and_keeps_snapshot() {
        for obs in [TmuxObservation::Unobservable, TmuxObservation::NoTmux] {
            let mut prev = names(&["a", "b"]);
            assert!(
                diff_closed(&mut prev, &obs).is_empty(),
                "{obs:?} 不该报死亡"
            );
            assert_eq!(prev, names(&["a", "b"]), "{obs:?} 不该动快照");
        }
    }

    /// 第一次观测没有「上一份」可比 ⇒ 不报任何死亡（否则 daemon 一启动就诬告一批）。
    #[test]
    fn first_observation_reports_nothing() {
        let mut prev = None;
        let closed = diff_closed(
            &mut prev,
            &TmuxObservation::Sessions("a\t/p\tsh\t0\t1\t\n".into()),
        );
        assert!(closed.is_empty());
        assert_eq!(prev, names(&["a"]));
    }

    /// 幂等：同一份观测再来一次，不该重复报死亡。
    #[test]
    fn repeated_identical_observation_reports_nothing() {
        let mut prev = names(&["a"]);
        let obs = TmuxObservation::Sessions("a\t/p\tsh\t0\t1\t\n".into());
        assert!(diff_closed(&mut prev, &obs).is_empty());
        assert!(diff_closed(&mut prev, &obs).is_empty());
    }

    /// 新会话出现不该被当成死亡（差分方向别搞反）。
    #[test]
    fn new_session_is_not_a_death() {
        let mut prev = names(&["a"]);
        let closed = diff_closed(
            &mut prev,
            &TmuxObservation::Sessions("a\t/p\tsh\t0\t1\t\nb\t/p\tsh\t0\t1\t\n".into()),
        );
        assert!(closed.is_empty());
        assert_eq!(prev, names(&["a", "b"]));
    }

    // ---------- P4（zero-poll-liveness）：外部 poke ⇒ 立刻重探 ----------

    /// P4：`Poke` 与 `TmuxProbeDue` 必须走**同一条**处理路径 —— 事件驱动的那一拍和
    /// 定时器那一拍，除了来源不同，后续行为应当逐字相同。
    ///
    /// 这条扫源码而不是跑循环：`watch_loop` 要真 spawn 线程 + 真跑 `tmux ls`，
    /// 在单测里不可控。**范围窄于性质，如实记**：它钉的是「两个变体在同一个 match 臂上」，
    /// 钉不住「那个臂里的代码是对的」——后者由 P4 的真机验收（hook 装上之后）覆盖。
    #[test]
    fn poke_shares_the_probe_arm_with_the_ticker() {
        // ★★ 判据**运行时拼**且**只扫生产段** —— 初版两样都没做，于是变异验收时
        // 「把 Poke 拆成独立臂」**没有变红**：断言那串字面量就在本测试自己的源码里，
        // `contains` 恒真 ⇒ 这条守卫是**安慰剂**。是变异（而不是审读）把它揪出来的。
        let me = include_str!("watcher.rs");
        let marker = "\n#[cfg(test)]\nmod tests";
        let prod = match me.find(marker) {
            Some(i) => &me[..i],
            None => me,
        };
        let arm = format!("WatchEvent::TmuxProbeDue {} WatchEvent::Poke =>", "|");
        assert!(
            prod.contains(&arm),
            "P4：Poke 必须与 TmuxProbeDue 共用同一个 match 臂（否则两条路会各自漂）"
        );
        assert!(
            prod.len() < me.len() && prod.len() > 1000,
            "剥 cfg(test) 没生效，这条断言在空转"
        );
    }

    /// ★ S0：pidfile **绑的 sid 变了**才重探 tmux —— 不是「有 .json 事件就探」。
    ///
    /// 为什么这条区分是必须的：CC 在**每次状态转换**时都会重写 `sessions/<PID>.json`
    /// （远端红绿灯就靠它，见 `process_session_added` 里 F27 那段）。拿「有事件」当触发器，
    /// 等于让 tmux 探测跟着对话节奏跑 —— 那是**变相轮询**，把 P5 好不容易拆掉的定时器
    /// 用另一种形式装回来。
    ///
    /// 同 `poke_shares_the_probe_arm_with_the_ticker`：扫**生产段**源码（`watch_loop` 要真
    /// spawn 线程 + 真跑 `tmux ls`，单测里跑不动）。**范围窄于性质，如实记**：它钉的是
    /// 「触发条件写的是 sid 前后比对」，钉不住「比对结果被正确用了」。
    #[test]
    fn tmux_reprobe_triggers_on_sid_drift_not_on_every_json_event() {
        let me = include_str!("watcher.rs");
        let marker = "\n#[cfg(test)]\nmod tests";
        let prod = match me.find(marker) {
            Some(i) => &me[..i],
            None => me,
        };
        assert!(
            prod.len() < me.len() && prod.len() > 1000,
            "剥 cfg(test) 没生效，这条断言在空转"
        );
        // 触发器 = 处理前后拿同一个 key 的 sid 比一次。判据运行时拼，避免本测试自己的
        // 源码把 `contains` 喂成恒真（那正是 P4 那条守卫初版栽的跟头）。
        let before = format!(
            "let sid_before = state.sessions.get(&key){}",
            ".map(|e| e.sid.clone());"
        );
        assert!(
            prod.contains(&before),
            "S0：处理 session json 之前必须先记下该 key 当前绑的 sid"
        );
        let cmp = format!(
            "if state.sessions.get(&key).map(|e| e.sid.clone()) {} sid_before {{",
            "!="
        );
        assert!(
            prod.contains(&cmp),
            "S0：触发条件必须是「sid 前后不同」，不是「收到了 .json 事件」"
        );
    }

    /// ★ P4：`WatcherPoke` 是**窄**句柄 —— 外部只能催重探，不能伪造带载荷的内部事件。
    ///
    /// 为什么要钉：`main` 的 SIGUSR1 流拿到的若是 `Sender<WatchEvent>` 本身，它就能发
    /// `PidDied{key,pid}` / `TmuxObserved(..)` —— 那等于把「谁能宣布一个会话死了」这件事
    /// 从 watcher 内部漏到了进程边界上（signal 处理器是最不该有这个权力的地方）。
    #[test]
    fn watcher_poke_is_a_narrow_handle() {
        let me = include_str!("watcher.rs");
        // 结构性判据：poke 句柄只暴露一个无参方法，且它只发 Poke 这一个变体。
        assert!(
            me.contains("pub fn poke(&self)"),
            "WatcherPoke 应当只暴露 poke()"
        );
        assert!(
            me.contains("self.0.send(WatchEvent::Poke)"),
            "poke() 只许发 Poke 变体"
        );
        // 反向：句柄字段不许是 pub（否则外部直接拿 sender，窄类型就白设了）。
        //
        // ★ 判据**在运行时拼出来**，绝不把它当字面量写进源码 —— 否则这条 `!contains`
        // 会被**本测试自己的那行字面量**命中而恒红（初版就是这么栽的：源码里字段并不是
        // pub，测试却红了，因为 `me` 里找到了断言自己那串）。
        let forbidden = format!("pub struct WatcherPoke({} ", "pub");
        assert!(
            !me.contains(&forbidden),
            "WatcherPoke 的字段不许 pub —— 那就绕开了窄接口"
        );
        assert!(me.len() > 1000, "include_str! 没读到源码，上面的断言是空转");
    }

    /// P4：**channel 仍然只有一条**（账本第 1 行：不许再开第二条）。
    /// `spawn` 把 channel 上提到自己这里造，是为了能在线程起来前交出 poke 句柄；
    /// 上提**不等于**新增 —— `watch_loop` 收的是同一条的 receiver。
    #[test]
    fn still_exactly_one_event_channel() {
        // ★ 只数**生产代码**：测试自己造了 6 条同型 channel（各自的夹具），把它们算进来
        // 这条断言就恒红（初版实测 7 处）。做法与 `readonly_guard::tests::strip_cfg_test`
        // 同源，这里用够用的简化版：本文件只有**一个**测试模块，且在文件末尾。
        //
        // ★★ **锚点必须避开自指**，这个坑本功能连踩三次：
        //   ① 用 `rfind("#[cfg(test)]")` —— 找到的是**本测试源码里那行字面量**（在模块标记
        //      之后）⇒ 几乎什么都没剥掉，实测仍数出 6。
        //   ② 下面那条「字段不许 pub」的 `!contains` 判据写成字面量 —— 被自己命中而恒红。
        //      （连**解释这个坑的注释**逐字引用那串时也会再次触发，实测第四次才收干净。）
        // 现在锚 `\n#[cfg(test)]\nmod tests`：源码里这串是**转义写法**（反斜杠 + n 两个字符），
        // 与真正的换行不相等 ⇒ 不会匹配到本行自己。
        let me = include_str!("watcher.rs");
        let marker = "\n#[cfg(test)]\nmod tests";
        let prod = match me.find(marker) {
            Some(i) => &me[..i],
            None => me,
        };
        let n = prod
            .matches("std::sync::mpsc::channel::<WatchEvent>()")
            .count();
        assert_eq!(
            n, 1,
            "生产代码里事件 channel 的创建点应当恰好 1 处（实得 {n}）——账本第 1 行不许开第二条"
        );
        // 反向自检：真剥掉了测试段（否则上面数的是全文）。
        assert!(
            prod.len() < me.len() && prod.len() > 1000,
            "剥 cfg(test) 没生效，这条断言在空转"
        );
    }

    // ---------- P3（zero-poll-liveness）：tmux server 生 / 死 / 复活 ----------

    /// ★ P3 收紧判据：`tmux ls` rc=1 时，**只有在我们记着的 server pid 确实已经不在**
    /// 才认"零会话"；pid 还在 = 真异常 ⇒ `Unobservable`（保守跳过，不误 retire）。
    ///
    /// **刻意不依赖"pidfd 是否已经醒过"**——那会有个危险失效模式：pidfd 路万一没醒，
    /// 状态永停 `Alive`，rc=1 被永久压成 `Unobservable` ⇒ 永不 retire。改成直接查 `/proc`。
    #[test]
    fn no_server_is_tightened_only_when_the_pid_is_really_still_alive() {
        // ① 记着的 server 是**自己**（铁定活着）+ rc=1 ⇒ 真异常 ⇒ Unobservable
        let me = std::process::id();
        assert_eq!(
            classify_with_server_state(TmuxObservation::NoServer, ServerState::Alive(me)),
            TmuxObservation::Unobservable,
            "server 明明活着而 tmux ls 连不上 = 真异常，不该当成零会话"
        );

        // ② 记着的 server 已死 + rc=1 ⇒ 原样通过（真的没 server）
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn");
        let dead = child.id();
        child.kill().expect("kill");
        child.wait().expect("reap");
        assert_eq!(
            classify_with_server_state(TmuxObservation::NoServer, ServerState::Alive(dead)),
            TmuxObservation::NoServer,
            "server 真没了就该照常判零会话"
        );

        // ③ Unknown / Gone 一律不收紧（还没探过、或已知没了）
        for st in [ServerState::Unknown, ServerState::Gone] {
            assert_eq!(
                classify_with_server_state(TmuxObservation::NoServer, st),
                TmuxObservation::NoServer,
                "{st:?} 下不该收紧"
            );
        }

        // ④ **收紧只作用于 NoServer**（守卫范围必须等于性质范围）：别的观测原样穿过，
        //    尤其 `ServerEmpty`（exit-empty off 下 server 活着 + 零会话，是合法观测）。
        for obs in [
            TmuxObservation::Sessions("x".into()),
            TmuxObservation::ServerEmpty,
            TmuxObservation::NoTmux,
            TmuxObservation::Unobservable,
        ] {
            assert_eq!(
                classify_with_server_state(obs.clone(), ServerState::Alive(me)),
                obs,
                "{obs:?} 不该被 server 状态改写"
            );
        }
    }

    /// pidfd 看守的**目标**决定醒了发哪个事件——一份实现服务两种目标
    /// （全 crate 只有一处 pidfd 的 unsafe）。
    #[test]
    fn pid_watch_target_maps_to_the_right_death_event() {
        let key = PathBuf::from("/x/7.json");
        match (PidWatchTarget::Session { key: key.clone() }).death_event(7) {
            WatchEvent::PidDied { key: k, pid } => assert_eq!((k, pid), (key, 7)),
            _ => panic!("Session 目标必须发 PidDied"),
        }
        match PidWatchTarget::TmuxServer.death_event(9) {
            WatchEvent::TmuxServerGone { pid } => assert_eq!(pid, 9),
            _ => panic!("TmuxServer 目标必须发 TmuxServerGone"),
        }
    }

    /// ★ pidfd 用在**真 tmux server 进程**上（不只是 `sleep`）：双向验收。
    ///
    /// 隔离 socket（无 `-L` 一律不跑——本测试自己带 `-L`）。这一格 P2 没覆盖：
    /// P2 只测了会话进程，P0-③ 只测了 cgroup 拓扑。
    ///
    /// **无 tmux 就硬失败而不是静默跳过**——静默 SKIP 是 gate-integrity 在治的那个病。
    /// 本 crate 的 CI job 跑在 ubuntu-latest，tmux 是标配；真缺了应当看见红。
    // U4a：本测试清理 Linux 的 `/tmp/tmux-<uid>/` socket 目录（`libc::getuid`），
    // 是 Linux-only 的夹具 —— 少这个门会让跨 target check 红。
    #[cfg(target_os = "linux")]
    #[test]
    fn pidfd_watches_a_real_tmux_server_and_stays_silent_while_it_lives() {
        let sock = format!("ccmP3-{}", std::process::id());
        let tmux = |args: &[&str]| -> std::process::Output {
            std::process::Command::new("tmux")
                .args(["-L", &sock])
                .args(args)
                .output()
                .expect("tmux 不可执行——本测试要求环境有 tmux（刻意不静默跳过）")
        };
        // -f /dev/null：不读用户的 ~/.tmux.conf（隔离）
        let out = std::process::Command::new("tmux")
            .args([
                "-f",
                "/dev/null",
                "-L",
                &sock,
                "new-session",
                "-d",
                "-s",
                "p3",
                "sh",
            ])
            .output()
            .expect("tmux 不可执行——本测试要求环境有 tmux");
        assert!(
            out.status.success(),
            "隔离 socket 上建会话失败: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        let pid_out = tmux(&["display-message", "-p", "#{pid}"]);
        let pid: u32 = String::from_utf8_lossy(&pid_out.stdout)
            .trim()
            .parse()
            .expect("拿 server pid");

        let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
        spawn_pid_watcher(PidWatchTarget::TmuxServer, pid, None, tx);

        // 反方向：server 还活着 ⇒ 不该有事件
        match rx.recv_timeout(Duration::from_millis(400)) {
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            other => panic!("server 活着时不该有事件（ok={}）", other.is_ok()),
        }

        // 正方向：杀掉**这个隔离 socket 上的** server ⇒ pidfd 醒
        let _ = tmux(&["kill-server"]);
        match rx
            .recv_timeout(Duration::from_secs(5))
            .expect("server 退出后必须收到 TmuxServerGone")
        {
            WatchEvent::TmuxServerGone { pid: p } => assert_eq!(p, pid),
            _ => panic!("期望 TmuxServerGone"),
        }
        let _ = std::fs::remove_file(format!("/tmp/tmux-{}/{sock}", unsafe { libc::getuid() }));
    }

    /// `query_tmux_server`：**死 socket 上不该把 server 拉活**、且拿不到 pid/socket。
    ///
    /// 这里不能直接调 `query_tmux_server()`（它走默认 socket = 用户实况），所以只钉住
    /// 「探测脚本对没有 server 的情形返回全 None」这条语义——用 PATH 前置一个 rc=1 的假 tmux。
    #[test]
    fn tmux_server_query_yields_nothing_without_a_server() {
        let dir = std::env::temp_dir().join(format!("ccm-p3-q-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let fake = dir.join("tmux");
        std::fs::write(&fake, "#!/bin/sh\necho 'error connecting' >&2\nexit 1\n").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(&fake).expect("stat").permissions();
            perm.set_mode(0o755);
            std::fs::set_permissions(&fake, perm).expect("chmod");
        }
        // 跑与生产同一段脚本，只把 PATH 指向假 tmux。
        let script = "if command -v tmux >/dev/null 2>&1; then exec tmux display-message -p '#{pid}\t#{socket_path}' 2>/dev/null; else exit 97; fi";
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .env("PATH", dir.display().to_string())
            .output()
            .expect("spawn");
        assert_ne!(out.status.code(), Some(0), "没有 server 时脚本不该 rc=0");
        assert!(
            String::from_utf8_lossy(&out.stdout).trim().is_empty(),
            "没有 server 时不该有 stdout"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---------- P1（zero-poll-liveness）：tmux 观测四态 ----------

    /// 纯分类：四态各自的判据。**P0 实测的状态空间**（见
    /// `.claude/planned-build/zero-poll-liveness/features/P0-machine-facts.md` §3 ④）。
    #[test]
    fn tmux_probe_classifies_four_states() {
        // rc=0 + 非空 → 有会话
        assert_eq!(
            classify_tmux_probe(Some(0), "s1\t/p\tclaude\t1\t1\tsid-a\n"),
            TmuxObservation::Sessions("s1\t/p\tclaude\t1\t1\tsid-a\n".to_string())
        );
        // rc=0 + 空 → **server 活但零会话**（exit-empty off）。P3 起与 rc=1 分开。
        assert_eq!(
            classify_tmux_probe(Some(0), ""),
            TmuxObservation::ServerEmpty
        );
        assert_eq!(
            classify_tmux_probe(Some(0), "  \n"),
            TmuxObservation::ServerEmpty,
            "只有空白也算空"
        );
        // rc=1 → **server 不在**（两种 stderr 措辞都走这里，刻意不看 stderr）
        assert_eq!(classify_tmux_probe(Some(1), ""), TmuxObservation::NoServer);
        // 约定 rc → 无 tmux
        assert_eq!(
            classify_tmux_probe(Some(TMUX_PROBE_NO_TMUX_RC), ""),
            TmuxObservation::NoTmux
        );
        // 其他 rc / 被信号杀 → 观测无效（**绝不当零会话**）
        assert_eq!(
            classify_tmux_probe(Some(2), ""),
            TmuxObservation::Unobservable
        );
        assert_eq!(
            classify_tmux_probe(Some(127), ""),
            TmuxObservation::Unobservable
        );
        assert_eq!(classify_tmux_probe(None, ""), TmuxObservation::Unobservable);
    }

    /// **`raw` 载荷与 P1 之前逐字节一致**——旧 monitor 行为零变化的那条保证。
    /// 有会话时 `observation` 必须**省略**（热路径不加字节）。
    #[test]
    fn observation_frame_keeps_raw_payload_backward_compatible() {
        match observation_to_frame(TmuxObservation::Sessions("s1\t/p\tclaude\t1\t1\tx".into())) {
            Frame::TmuxSessions { raw, observation } => {
                assert_eq!(raw, "s1\t/p\tclaude\t1\t1\tx");
                assert_eq!(observation, None, "有会话时必须省略，否则热路径白涨字节");
            }
            f => panic!("期望 TmuxSessions，实得 {f:?}"),
        }
        // 无 tmux：保留 NO_TMUX 哨兵（旧 monitor 那道门认它）
        match observation_to_frame(TmuxObservation::NoTmux) {
            Frame::TmuxSessions { raw, observation } => {
                assert_eq!(raw.trim(), "NO_TMUX");
                assert_eq!(observation.as_deref(), Some(OBS_NO_TMUX));
            }
            f => panic!("期望 TmuxSessions，实得 {f:?}"),
        }
        // 零会话 / 观测无效：raw 都是空串（旧 monitor 一律保守跳过 = 今天的行为），
        // 区别只在 observation ⇒ 只有新 monitor 分得开。
        // P3：`ServerEmpty` 与 `NoServer` 两个细分**必须映射到同一个 wire 取值**
        // ——这就是"P3 加细分不改帧契约"那条承诺的机器化。
        for (obs, token) in [
            (TmuxObservation::ServerEmpty, OBS_ZERO_SESSIONS),
            (TmuxObservation::NoServer, OBS_ZERO_SESSIONS),
            (TmuxObservation::Unobservable, OBS_UNOBSERVABLE),
        ] {
            match observation_to_frame(obs) {
                Frame::TmuxSessions { raw, observation } => {
                    assert_eq!(raw, "", "旧 monitor 必须看到与今天相同的空 raw");
                    assert_eq!(observation.as_deref(), Some(token));
                }
                f => panic!("期望 TmuxSessions，实得 {f:?}"),
            }
        }
    }

    /// ★ **真跑那段 shell 脚本**（拿假 tmux 喂各种 rc），不只做字符串断言。
    ///
    /// 为什么必须这样测：P1 的关键改动是把 `tmux ls … || true` 换成 `exec tmux …` 让 rc
    /// 透出。`|| true` 与 `exec` 的差别**在字符串断言里看不出来**——只有真执行才知道 rc
    /// 有没有传出来。（同 `tmux.rs::emit_guarded_commands_for_e2e` 的教训：门禁只锁字符串
    /// 形状不锁行为。）
    #[test]
    fn probe_script_propagates_rc_with_fake_tmux() {
        let dir = std::env::temp_dir().join(format!("ccm-p1-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let fake = dir.join("tmux");

        let run = |path_value: &str| -> TmuxObservation {
            let out = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(tmux_probe_script())
                .env("PATH", path_value)
                .output()
                .expect("spawn /bin/sh");
            classify_tmux_probe(out.status.code(), &String::from_utf8_lossy(&out.stdout))
        };
        let write_fake = |body: &str| {
            std::fs::write(&fake, body).expect("write fake tmux");
            let mut perm = std::fs::metadata(&fake).expect("stat").permissions();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                perm.set_mode(0o755);
            }
            std::fs::set_permissions(&fake, perm).expect("chmod");
        };
        let path_with_fake = format!("{}:/usr/bin:/bin", dir.display());

        // ① 假 tmux 打印一行会话、rc=0 → Sessions
        write_fake("#!/bin/sh\nprintf 's1\\t/p\\tclaude\\t1\\t1\\tsid-a\\n'\nexit 0\n");
        assert!(matches!(
            run(&path_with_fake),
            TmuxObservation::Sessions(ref s) if s.contains("sid-a")
        ));

        // ② rc=0 但不输出 → ServerEmpty（exit-empty off 那格）
        write_fake("#!/bin/sh\nexit 0\n");
        assert_eq!(run(&path_with_fake), TmuxObservation::ServerEmpty);

        // ③ rc=1（真 tmux 在 server 不在时就是这个）→ ZeroSessions
        //    **这一格是 P1 的核心**：改回 `|| true` 会让它变成 rc=0+空 ⇒ 仍是 ZeroSessions，
        //    所以本格单独看不出回归；真正钉住 `exec` 的是 ④。
        write_fake("#!/bin/sh\necho 'no server running on /tmp/x' >&2\nexit 1\n");
        assert_eq!(run(&path_with_fake), TmuxObservation::NoServer);

        // ④ ★ rc=2（观测无效）→ 必须是 Unobservable，**绝不能被折成零会话**。
        //    这一格就是 `|| true` 的变异检测点：加回 `|| true` 会把 rc=2 吞成 rc=0+空
        //    ⇒ 误判成 ZeroSessions ⇒ 本断言红。
        write_fake("#!/bin/sh\necho boom >&2\nexit 2\n");
        assert_eq!(
            run(&path_with_fake),
            TmuxObservation::Unobservable,
            "观测失败被折成零会话会批量误灰——这里红说明 rc 没有真的透出来"
        );

        // ⑤ PATH 里没有 tmux → NoTmux（command -v 门控）。
        //    **必须用一个确实没有 tmux 的空目录**：初版这里写的是 `/usr/bin:/bin`，而真 tmux
        //    就在 `/usr/bin` ⇒ 测试真跑了**默认 socket** 上的 `tmux ls`（只读、无损，但违反
        //    "tmux 一律走隔离 socket"的纪律，且在别人机器上结果不可预测）。断言当场红是因为
        //    它列出了真实会话而不是 NoTmux —— 算这条测试自己抓到的第一个问题。
        let empty = dir.join("no-tmux-here");
        std::fs::create_dir_all(&empty).expect("mkdir empty");
        assert_eq!(
            run(&empty.display().to_string()),
            TmuxObservation::NoTmux,
            "PATH 里没有 tmux 时必须走 command -v 门控那支"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Build a byte buffer from JSONL lines joined with `\n` and a trailing one.
    fn jsonl(lines: &[&str]) -> Vec<u8> {
        let mut s = String::new();
        for l in lines {
            s.push_str(l);
            s.push('\n');
        }
        s.into_bytes()
    }

    const KEY: &str = "/some/session.jsonl";

    #[test]
    fn appending_lines_advances_offset_and_seq_monotonically() {
        let mut seqs = SeqCounter::new();

        let first = jsonl(&[r#"{"a":1}"#, r#"{"a":2}"#]);
        let (out, cur) = read_new_lines(&first, ReadCursor::default(), KEY, &mut seqs);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].seq, 0);
        assert_eq!(out[1].seq, 1);
        assert_eq!(cur.consumed, first.len() as u64);
        assert_eq!(cur.seen_len, first.len() as u64);

        // Append two more lines (same prefix bytes, longer file).
        let mut second = first.clone();
        second.extend_from_slice(jsonl(&[r#"{"a":3}"#, r#"{"a":4}"#]).as_slice());
        let (out2, cur2) = read_new_lines(&second, cur, KEY, &mut seqs);
        assert_eq!(out2.len(), 2, "only the newly-appended lines come back");
        assert_eq!(out2[0].seq, 2);
        assert_eq!(out2[1].seq, 3);
        assert_eq!(cur2.consumed, second.len() as u64);
        assert_eq!(out2[0].raw, r#"{"a":3}"#);
    }

    #[test]
    fn byte_offset_matches_aterm_lineframer() {
        // daemon-01（gap#2）：Line.byte_offset **逐字节对齐 aterm `LineFramer.endOffset`**——计 CRLF 的 `\r`、
        // 含 `\n`、残行不计、在**原始字节**上算（非解码后串）。移植自 aterm LineFramerTest 的关键语料。
        let mut seqs = SeqCounter::new();
        // aterm feedFramedCountsCrlfAndMultibyteRawBytes: "你\r\nx\n" → endOffset [5,7]
        // 你=3B + \r + \n = 5；x + \n = 2 → 累计 7。raw 剥 \r/\n。
        let (out, cur) = read_new_lines(
            "你\r\nx\n".as_bytes(),
            ReadCursor::default(),
            KEY,
            &mut seqs,
        );
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].raw.as_str(), out[0].byte_offset), ("你", 5));
        assert_eq!((out[1].raw.as_str(), out[1].byte_offset), ("x", 7));
        assert_eq!(cur.consumed, 7);

        // 无 CRLF 累计：jsonl(["ab","cde"]) = "ab\ncde\n" → [3, 7]。
        let mut s2 = SeqCounter::new();
        let (o2, _) = read_new_lines(&jsonl(&["ab", "cde"]), ReadCursor::default(), KEY, &mut s2);
        assert_eq!((o2[0].byte_offset, o2[1].byte_offset), (3, 7));

        // 增量续读用**绝对**文件 offset（start + line_end），非本次 slice 相对：
        let mut s3 = SeqCounter::new();
        let first = jsonl(&["x"]); // "x\n" = 2B
        let (_, cur3) = read_new_lines(&first, ReadCursor::default(), KEY, &mut s3);
        let mut second = first.clone();
        second.extend_from_slice(&jsonl(&["yy"])); // + "yy\n"
        let (o3, _) = read_new_lines(&second, cur3, KEY, &mut s3);
        assert_eq!(o3.len(), 1);
        assert_eq!(o3[0].byte_offset, 5, "绝对 offset = 2(x\\n) + 3(yy\\n)");

        // 残行（torn tail）不计入 byte_offset：
        let mut s4 = SeqCounter::new();
        let (o4, cur4) = read_new_lines(
            b"done\nhalf-no-newline",
            ReadCursor::default(),
            KEY,
            &mut s4,
        );
        assert_eq!(o4.len(), 1);
        assert_eq!((o4[0].byte_offset, cur4.consumed), (5, 5)); // done\n=5；残行不计

        // 空行跳过、不占 byte_offset 连续性（offset 仍按原始字节累计）：
        let mut s5 = SeqCounter::new();
        let (o5, _) = read_new_lines(b"a\n\nb\n", ReadCursor::default(), KEY, &mut s5);
        assert_eq!(o5.len(), 2); // 空行跳过
        assert_eq!((o5[0].byte_offset, o5[1].byte_offset), (2, 5)); // a\n=2；空\n 占 1B（→3，跳过）；b\n 到 5
    }

    #[test]
    fn no_new_bytes_yields_nothing_and_does_not_bump_seq() {
        let mut seqs = SeqCounter::new();
        let buf = jsonl(&[r#"{"x":1}"#]);
        let (_, cur) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
        // Re-process identical bytes: consumed == len, start >= len, nothing new.
        let (again, cur2) = read_new_lines(&buf, cur, KEY, &mut seqs);
        assert!(again.is_empty());
        // A fresh read of the same key still hands out seq 1 only if a line was
        // produced; here nothing new, so the next live line would be seq 1.
        assert_eq!(seqs.next(KEY), 1, "seq must not have advanced past 1");
        assert_eq!(cur2.consumed, buf.len() as u64);
    }

    #[test]
    fn truncation_resets_offset_but_seq_keeps_climbing() {
        let mut seqs = SeqCounter::new();

        let big = jsonl(&[r#"{"n":1}"#, r#"{"n":2}"#, r#"{"n":3}"#]);
        let (out, big_cur) = read_new_lines(&big, ReadCursor::default(), KEY, &mut seqs);
        assert_eq!(out.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![0, 1, 2]);

        // Simulated truncation: file is now SHORTER than the recorded cursor.
        let small = jsonl(&[r#"{"n":99}"#]);
        assert!((small.len() as u64) < big_cur.seen_len, "test precondition");
        let (out2, small_cur) = read_new_lines(&small, big_cur, KEY, &mut seqs);

        // Cursor reset to 0 then re-advanced to the new (smaller) length.
        assert_eq!(small_cur.consumed, small.len() as u64);
        // The whole truncated file is re-read from byte 0 ...
        assert_eq!(out2.len(), 1);
        // ... but seq KEEPS CLIMBING (3, not back to 0): the climbing invariant.
        assert_eq!(out2[0].seq, 3, "seq must never reset on truncation");
    }

    /// F14 audit fix: a rewrite whose new length lands inside the pending
    /// torn-tail window [consumed, seen_len) must still be detected as
    /// truncation — no garbage line from a stale offset.
    #[test]
    fn rewrite_within_torn_window_detected_as_truncation() {
        let mut seqs = SeqCounter::new();
        // 19 bytes: complete line (8) + torn tail (11). consumed=8, seen_len=19.
        let torn = b"{\"a\":1}\n{\"a\":2,\"tor".to_vec();
        let (out, cur) = read_new_lines(&torn, ReadCursor::default(), KEY, &mut seqs);
        assert_eq!(out.len(), 1);
        assert_eq!(
            cur,
            ReadCursor {
                consumed: 8,
                seen_len: 19
            }
        );

        // Whole-file rewrite to 18 bytes: len >= consumed(8) but < seen_len(19).
        let rewritten = b"{\"b\":111}\n{\"b\":2}\n".to_vec();
        let (out2, cur2) = read_new_lines(&rewritten, cur, KEY, &mut seqs);
        assert_eq!(out2.len(), 2, "rewrite must be detected and re-read fully");
        assert_eq!(
            out2[0].raw, r#"{"b":111}"#,
            "no garbage from a stale offset"
        );
        assert_eq!(out2[0].seq, 1, "seq keeps climbing across truncation");
        assert_eq!(cur2.consumed, rewritten.len() as u64);
    }

    /// Truncate-to-empty must reset the cursor so a regrown file (even one
    /// longer than the old consumed offset) is read from byte 0.
    #[test]
    fn truncate_to_empty_then_regrow_reads_from_zero() {
        let mut seqs = SeqCounter::new();
        let old = jsonl(&[r#"{"n":1}"#, r#"{"n":2}"#]); // 16 bytes
        let (_, cur) = read_new_lines(&old, ReadCursor::default(), KEY, &mut seqs);

        let (empty_out, cur2) = read_new_lines(&[], cur, KEY, &mut seqs);
        assert!(empty_out.is_empty());
        assert_eq!(
            cur2,
            ReadCursor {
                consumed: 0,
                seen_len: 0
            }
        );

        let regrown = jsonl(&[r#"{"m":1}"#, r#"{"m":2}"#, r#"{"m":3}"#]); // 24 > 16
        let (out, cur3) = read_new_lines(&regrown, cur2, KEY, &mut seqs);
        assert_eq!(out.len(), 3, "must re-read from byte 0, no lost prefix");
        assert_eq!(out[0].raw, r#"{"m":1}"#);
        assert_eq!(out[0].seq, 2, "seq never resets");
        assert_eq!(cur3.consumed, regrown.len() as u64);
    }

    /// \r\n endings: raw must match str::lines() semantics (strip \n plus one
    /// adjacent \r); consumed advances by the byte count including \r\n.
    #[test]
    fn crlf_line_endings_are_stripped_like_lines() {
        let mut seqs = SeqCounter::new();
        let buf = b"{\"a\":1}\r\n{\"a\":2}\n".to_vec();
        let (out, cur) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].raw, r#"{"a":1}"#, "\\r must be stripped");
        assert_eq!(out[1].raw, r#"{"a":2}"#);
        assert_eq!(cur.consumed, 17);
    }

    /// Old-bug regression: invalid UTF-8 inside a COMPLETE line is lossy-decoded
    /// for that line only — it must not abort the rest of the batch.
    #[test]
    fn invalid_utf8_in_complete_line_does_not_abort_batch() {
        let mut seqs = SeqCounter::new();
        let mut buf = b"{\"a\":1}\n".to_vec();
        buf.extend_from_slice(b"\xFF\xFEgarbage\n");
        buf.extend_from_slice(b"{\"a\":3}\n");
        let (out, _) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
        assert_eq!(out.len(), 3, "batch must not be silently aborted");
        assert_eq!(out[2].raw, r#"{"a":3}"#, "lines after the bad one survive");
        assert!(out[1].raw.contains('\u{FFFD}'), "bad line delivered lossy");
    }

    #[test]
    fn leading_bom_is_stripped_for_the_empty_check_and_line_is_kept() {
        let mut seqs = SeqCounter::new();
        // A line that is ONLY a BOM + whitespace must be treated as empty.
        let only_bom = "\u{feff}   \n".as_bytes().to_vec();
        let (out, _) = read_new_lines(&only_bom, ReadCursor::default(), KEY, &mut seqs);
        assert!(out.is_empty(), "BOM-only/blank line is skipped");
        assert_eq!(seqs.next(KEY), 0, "skipped line must not consume a seq");

        // A BOM-prefixed real line is kept (and not double counted).
        let mut seqs2 = SeqCounter::new();
        let bom_line = "\u{feff}{\"k\":1}\n".as_bytes().to_vec();
        let (out2, _) = read_new_lines(&bom_line, ReadCursor::default(), KEY, &mut seqs2);
        assert_eq!(out2.len(), 1);
        assert_eq!(out2[0].seq, 0);
    }

    #[test]
    fn empty_lines_are_skipped_and_do_not_consume_seq() {
        let mut seqs = SeqCounter::new();
        let buf = jsonl(&[r#"{"a":1}"#, "", "   ", r#"{"a":2}"#, ""]);
        let (out, _) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
        assert_eq!(out.len(), 2, "two blank/whitespace lines dropped");
        assert_eq!(out[0].seq, 0);
        assert_eq!(out[1].seq, 1);
        // Only two seqs were consumed; the next one is 2.
        assert_eq!(seqs.next(KEY), 2);
    }

    #[test]
    fn subagents_path_is_excluded() {
        // A path containing a `subagents` segment must be filtered.
        let p = Path::new("/home/u/.claude/projects/foo/subagents/bar.jsonl");
        assert!(is_subagent_path(p));
        // Case-insensitive, mirrors watcher.rs.
        let p2 = Path::new("/home/u/.claude/projects/foo/SubAgents/bar.jsonl");
        assert!(is_subagent_path(p2));
        // A normal session file is not excluded.
        let p3 = Path::new("/home/u/.claude/projects/foo/abc-123.jsonl");
        assert!(!is_subagent_path(p3));
    }

    #[test]
    fn is_jsonl_and_is_session_json_classify_correctly() {
        assert!(is_jsonl(Path::new("/x/abc.jsonl")));
        assert!(!is_jsonl(Path::new("/x/abc.json")));
        assert!(is_session_json(Path::new("/x/1234.json")));
        assert!(!is_session_json(Path::new("/x/1234.jsonl")));
    }

    #[test]
    fn parse_session_id_extracts_the_field() {
        let blob = br#"{"sessionId":"abc-123","pid":4242}"#;
        assert_eq!(parse_session_id(blob), Some("abc-123".to_string()));
        // Missing field / wrong type / garbage → None.
        assert_eq!(parse_session_id(br#"{"pid":1}"#), None);
        assert_eq!(parse_session_id(br#"{"sessionId":5}"#), None);
        assert_eq!(parse_session_id(b"not json"), None);
    }

    #[test]
    fn torn_line_without_trailing_newline_is_deferred() {
        let mut seqs = SeqCounter::new();
        // Complete line + torn tail (no trailing \n).
        let buf = b"{\"a\":1}\n{\"a\":2,\"tex".to_vec();
        let (out, cur) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
        assert_eq!(out.len(), 1, "torn tail must not be emitted");
        assert_eq!(out[0].raw, r#"{"a":1}"#);
        assert_eq!(
            cur.consumed, 8,
            "consumed stops after the complete line, not at EOF"
        );
        assert_eq!(cur.seen_len, buf.len() as u64, "seen_len covers the tail");

        // The tail completes (plus one more full line) — emitted exactly once,
        // seq continuous across the deferral.
        let mut healed = buf.clone();
        healed.extend_from_slice(b"t\":\"x\"}\n{\"a\":3}\n");
        let (out2, cur2) = read_new_lines(&healed, cur, KEY, &mut seqs);
        assert_eq!(out2.len(), 2);
        assert_eq!(out2[0].raw, r#"{"a":2,"text":"x"}"#);
        assert_eq!(out2[0].seq, 1);
        assert_eq!(out2[1].seq, 2);
        assert_eq!(cur2.consumed, healed.len() as u64);
    }

    #[test]
    fn torn_multibyte_tail_does_not_decay_into_replacement_char() {
        let mut seqs = SeqCounter::new();
        let full = "{\"t\":\"文\"}\n".as_bytes(); // 文 = E6 96 87
        let torn = &full[..7]; // cut inside the multibyte sequence
        let (out, cur) = read_new_lines(torn, ReadCursor::default(), KEY, &mut seqs);
        assert!(out.is_empty(), "mid-multibyte torn tail must be deferred");
        assert_eq!(cur.consumed, 0);

        let (out2, cur2) = read_new_lines(full, cur, KEY, &mut seqs);
        assert_eq!(out2.len(), 1);
        assert_eq!(out2[0].raw, "{\"t\":\"文\"}", "no U+FFFD after healing");
        assert_eq!(cur2.consumed, full.len() as u64);
    }

    #[test]
    fn fully_unterminated_single_line_is_deferred() {
        // A file whose only content is a line still being written: nothing is
        // complete yet, so nothing is emitted and the cursor stays put.
        let mut seqs = SeqCounter::new();
        let buf = br#"{"only":1}"#.to_vec();
        let (out, cur) = read_new_lines(&buf, ReadCursor::default(), KEY, &mut seqs);
        assert!(out.is_empty(), "unterminated line is deferred, not emitted");
        assert_eq!(cur.consumed, 0, "consumed must not advance past the tail");
        assert_eq!(cur.seen_len, buf.len() as u64);
        assert_eq!(seqs.next(KEY), 0, "deferral must not consume a seq");
    }

    // === Batch5-F20 add-time imposter check ===

    #[test]
    fn imposter_when_proc_started_after_pidfile() {
        // pidfile last written at t=1000, process started at t=2000 (> 1000+60).
        let v = add_time_verdict(None, None, Some(2000), Some(1000), None);
        assert_eq!(v, AddTimeVerdict::Imposter("started-after-pidfile"));
        // Reboot case is the same shape: old mtime, post-boot start.
        let v2 = add_time_verdict(
            None,
            None,
            Some(1_700_000_000),
            Some(1_600_000_000),
            Some("claude"),
        );
        assert_eq!(
            v2,
            AddTimeVerdict::Imposter("started-after-pidfile"),
            "time evidence must win even with a claude-looking cmdline (a NEW claude did not write the OLD pidfile)"
        );
    }

    #[test]
    fn alive_within_tolerance() {
        // Started slightly after mtime but inside the 60s fuzz window.
        assert_eq!(
            add_time_verdict(None, None, Some(1030), Some(1000), None),
            AddTimeVerdict::Alive
        );
        // Started before mtime (the normal case: claude starts, then writes).
        assert_eq!(
            add_time_verdict(None, None, Some(900), Some(1000), None),
            AddTimeVerdict::Alive
        );
    }

    #[test]
    fn imposter_by_cmdline() {
        assert_eq!(
            add_time_verdict(None, None, None, None, Some("tmux new-session -d")),
            AddTimeVerdict::Imposter("cmdline")
        );
        assert_eq!(
            add_time_verdict(None, None, Some(900), Some(1000), Some("-bash")),
            AddTimeVerdict::Imposter("cmdline"),
            "time check passing must not mask a non-claude cmdline"
        );
    }

    #[test]
    fn imposter_by_bg_spare_before_exact_identity() {
        // F74b(#43)：bg-spare 优先于 exact-identity——即便 procStart 自洽（recorded==current）
        // 也判 Imposter（否则守护池停泊备用进程恒绿）。
        assert_eq!(
            add_time_verdict(Some(555), Some(555), None, None, Some("claude bg-spare")),
            AddTimeVerdict::Imposter("bg-spare"),
            "bg-spare 必须在 exact-identity Alive 之前拦下"
        );
        assert_eq!(
            add_time_verdict(
                None,
                None,
                None,
                None,
                Some("/usr/bin/claude bg-spare --foo")
            ),
            AddTimeVerdict::Imposter("bg-spare")
        );
        // 普通 claude 会话不受影响（procStart 自洽仍 Alive）。
        assert_eq!(
            add_time_verdict(Some(555), Some(555), None, None, Some("claude --resume x")),
            AddTimeVerdict::Alive
        );
    }

    #[test]
    fn claude_like_cmdlines_pass() {
        for cmd in [
            "claude --resume abc",
            "/usr/bin/node /home/u/.local/bin/claude",
            "NODE_OPTIONS=x node cli.js",
            "Claude", // case-insensitive
        ] {
            assert_eq!(
                add_time_verdict(None, None, Some(900), Some(1000), Some(cmd)),
                AddTimeVerdict::Alive,
                "{cmd}"
            );
        }
    }

    #[test]
    fn missing_data_degrades_to_allow() {
        assert_eq!(
            add_time_verdict(None, None, None, None, None),
            AddTimeVerdict::Alive
        );
        assert_eq!(
            add_time_verdict(None, None, Some(2000), None, None),
            AddTimeVerdict::Alive
        );
        assert_eq!(
            add_time_verdict(None, None, None, Some(1000), None),
            AddTimeVerdict::Alive
        );
        // Empty cmdline (kernel threads read as empty) is not evidence.
        assert_eq!(
            add_time_verdict(None, None, None, None, Some("")),
            AddTimeVerdict::Alive
        );
        assert_eq!(
            add_time_verdict(None, None, None, None, Some("   ")),
            AddTimeVerdict::Alive
        );
    }

    // === Batch6-F22：远端会话生命周期 ===

    /// ★★ `P0b-Y2`〔08-13〕：**「盯着的目录被换掉」这条路必须有人重挂 + 重扫。**
    ///
    /// # 这条判据够得到什么、够不到什么（先说清）
    ///
    /// 够不到：**它真的听得见新 inode 吗** —— 那是 inotify 的运行期事实，
    /// 只有真起 daemon、真删目录才验得出来（`e2e/daemon-sessions-rewatch.sh` 四组对照）。
    /// 够得到：**那两件事还在不在代码里**。删掉任一件，e2e 会红 —— 但 e2e 不在 `cargo test` 里，
    /// 有人只跑单测就会以为没事。⇒ 这条是给「改到这附近的人」的第一道提醒。
    ///
    /// ⚠ 判据自己的失效方式：钉字符串会被注释喂绿 ⇒ 剥掉注释再钉（本仓第 N 次防它）。
    #[test]
    fn the_rewatch_path_still_exists_with_its_rescan() {
        let src = include_str!("watcher.rs");
        let prod: String = src
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//") && !t.starts_with("///")
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            prod.contains("fn rewatch_sessions("),
            "`rewatch_sessions` 没了 —— `sessions/` 被 rm+mkdir 之后 daemon 会**活着、不吭声**\n             \
             （inotify 的 watch 绑在 inode 上）。那正是 `#60` 全链台架九拍拿不到读数的原因。"
        );
        // 调用点在（只有函数、没人调 = 死代码，e2e 会红但单测看不见）。
        assert!(
            prod.matches("rewatch_sessions(").count() >= 2,
            "`rewatch_sessions` 只有定义、没有调用点。"
        );
        // 重扫在：只重挂不重扫的话，「重建 → 挂上」之间写进去的文件永远捞不回来
        //（D 阶段变异 M23：只删这一步，前三组 e2e **全绿**，是第四组逼出来的）。
        let body_start = prod
            .find("fn rewatch_sessions(")
            .expect("上面已确认存在");
        let body = &prod[body_start..(body_start + 2000).min(prod.len())];
        assert!(
            body.contains("WalkDir::new(sessions)"),
            "重挂之后没有重扫 —— 「重建 → 挂上」之间那段窗口期里写进去的 pidfile 会永远丢。"
        );
        // 父目录的耳朵在（听不见子目录出现/消失，重挂就永远不会被触发）。
        assert!(
            prod.contains("watch(&agent_home, RecursiveMode::NonRecursive)"),
            "没有监视 `agent_home` 本身 —— 那 `sessions/` 出现或被换掉时没有任何事件会来。"
        );
    }

    /// ★★★ **每一处 `.watch(` 都要回答「目录被换 inode 了怎么办」** —— 登记表〔08-13〕。
    ///
    /// # 为什么立这张表
    ///
    /// 08-13 一天之内**同一个形状踩了三次**：`sessions/`（第十拍）· tmux socket 目录
    ///（第二十二拍）· `projects/`（第二十三拍）。前两次是**撞出来的**，第三次是
    /// 「把挂点全列一遍、逐个对照有没有重挂路径」**查出来的**。
    ///
    /// ⇒ 与其等第四次，不如把那次清点**固化**：本表锁住生产段 `.watch(` 的**处数与归属**。
    /// 加一处就会红 —— 红了不是坏事，是让加的人**先回答那个问题**再往下写。
    ///
    /// ⚠ 症状为什么值得这么防：inotify 的 watch 绑在 **inode** 上，目录被删掉重建之后
    /// 那一路的事件**永远不来，且没有任何错误**。三次的表现分别是「永不宣告会话」
    ///「看不见新 tmux server」「会话还在但内容不动了」——**每一个都不报错**。
    ///
    /// # 今天的 7 处
    ///
    /// | 处 | 归属 | 换 inode 怎么办 |
    /// |---|---|---|
    /// | `rewatch_dir` | **可重入挂法本体** | 就是它负责 |
    /// | `watch_sock_dir_if_present` | socket 目录专用（多一条「目录没了翻记账」） | 同上 |
    /// | `rewatch_sessions` | `sessions/` 专用（多一件事：挂上顺带重扫 pidfile） | 同上 |
    /// | `watch_loop` 里 `agent_home` | **父目录的耳朵**（子目录出现/消失的唯一信号源） | 父目录被换掉 = 整个 agent_home 没了，那时没有任何路可走，**不在这一族** |
    /// | `watch_loop` 里 socket 目录的**父** | 同上（等 socket 目录出现） | 同上 |
    /// | `watch_loop` 里 `sessions` 起步那次 | 起步挂一次，之后归 `rewatch_sessions` | 已有 |
    /// | `watch_loop` 里 tmux socket **所在目录**（P3 复活探测） | 一次性触发器，socket 换 inode 由上面那条目录耳朵覆盖 | 已有 |
    #[test]
    fn every_watch_site_answers_the_inode_swap_question() {
        let src = include_str!("watcher.rs");
        // 生产段 = 测试模块之前（`guard_core::production_code` 在这里不能用：本条就住在测试模块里）。
        let cut = src.find("\n#[cfg(test)]").map(|i| {
            src[i..].find("\nmod ").map(|j| i + j).unwrap_or(i)
        });
        let prod = match cut {
            Some(i) => &src[..i],
            None => src,
        };
        let sites = prod.matches(".watch(").count();
        assert_eq!(
            sites, 7,
            "生产段 `.watch(` 有 {sites} 处（登记表记着 7 处）。\n             \
             ⇒ **加了一处就来回答这个问题**：那个目录被删掉重建（换 inode）之后，\n             \
             它还收得到事件吗？收不到就走 `rewatch_dir`；确实不需要就把理由写进本条头注的表里。\n             \
             ⚠ 08-13 同一个形状踩了三次，三次的症状都是**不报任何错**：\n             \
             「永不宣告会话」「看不见新 tmux server」「会话还在但内容不动了」。"
        );
        // 三个可重入挂法必须都在（删掉任一个，上面的计数会跟着变，但报错要说得准）。
        for f in ["fn rewatch_dir(", "fn rewatch_sessions(", "fn watch_sock_dir_if_present("] {
            assert!(prod.contains(f), "可重入挂法 {f} 不见了 —— 那一路的重挂就没人做了");
        }
    }

    /// ★★ socket 目录**被删掉再重建**时，watch 必须跟着换到新 inode〔08-13〕。
    ///
    /// # 为什么这条是结构判据而不是 e2e
    ///
    /// 要真跑它得有一个**私有 socket 目录**（不然就得删用户真实的 `/tmp/tmux-<uid>`），
    /// 而私有 socket 目录只能靠 `TMUX_TMPDIR` —— 那正是 `C7i` **零例外**禁止的东西
    /// （`e2e_gate_registry::no_e2e_suite_isolates_with_tmux_tmpdir` 拦下了 e2e 那一版）。
    /// ⇒ 红线与覆盖面冲突时本仓选红线，**换判据落在哪一层**。
    ///
    /// ⚠ **如实记损失**：运行期行为 08-13 手工验过一次（删目录 → 起新 server ⇒
    /// 修前新 server 一帧收不到、修后收得到），**没有进 CI**。这里钉的是那两处形状。
    ///
    /// # 钉哪两处
    ///
    /// ① 事件条件里**不许**再有 `&& !sock_dir_watched` —— 目录被换掉时那个标志仍是 true，
    ///    加上它等于把重挂整个短路（这正是首版的 bug）；
    /// ② `watch_sock_dir_if_present` 必须**先 `unwatch` 再 `watch`** ——
    ///    分辨不出「同 inode 的普通事件」与「换了 inode」，重挂同一个无害、漏挂新的致命。
    #[test]
    fn the_socket_dir_watch_survives_an_inode_swap() {
        let src = include_str!("watcher.rs");
        let prod: String = src
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//") && !t.starts_with("///")
            })
            .collect::<Vec<_>>()
            .join("\n");
        // ⚠⚠ 针**运行时拼**：写成字面量的话，**本条自己的诊断文案**里那个串也会进语料
        //   ⇒ 判据恒红（08-13 实测栽了一次）。隔壁 `wire.rs` 的同族判据逐字记着这条：
        //   「判据扫自己所在的文件时，它写下的每一个例子都会变成语料」。
        //   ⚠ 而 `guard_core::production_code` 在这里**救不了**：它剥的是 `#[cfg(test)] mod`，
        //   而本条**就住在那个 mod 里** —— 剥完连要找的生产代码一起没了（首版又栽在这）。
        let short_circuit = format!("p == sock_dir.as_path() {} !sock_dir_watched", "&&");
        assert!(
            !prod.contains(short_circuit.as_str()),
            "事件条件里又出现了 `&& !sock_dir_watched` —— 目录被删掉再重建时那个标志仍是 true，\n             \
             这会把重挂整个短路，于是**新起的 tmux server 一帧都收不到**（08-13 实测过）。"
        );
        let at = prod
            .find("fn watch_sock_dir_if_present(")
            .expect("`watch_sock_dir_if_present` 不在了 —— 那是 socket 目录耳朵的唯一挂点");
        let body = &prod[at..(at + 1200).min(prod.len())];
        // ⚠ **数两处、不找第一处**：函数里有**两个** `unwatch(sock_dir)` ——
        //   一个在「目录没了」那支（翻记账），一个在重挂之前。首版用 `find` 取第一处，
        //   于是删掉重挂那处、变异**照样绿**（第一处顶了包）。这是本会话反复撞的
        //   「针在窗口内不唯一」那一族。
        let n_unwatch = body.matches("unwatch(sock_dir)").count();
        assert_eq!(
            n_unwatch, 2,
            "`watch_sock_dir_if_present` 里 `unwatch(sock_dir)` 应恰好 2 处\n             \
             （① 目录没了 ⇒ 翻记账；② 重挂之前 ⇒ 换 inode 时挂得上新的），实得 {n_unwatch}"
        );
        let watch_at = body.find("watch(sock_dir,").expect("没有 `watch` —— 那它什么都没挂");
        let last_unwatch = body.rfind("unwatch(sock_dir)").expect("上面已确认有两处");
        assert!(
            last_unwatch < watch_at,
            "重挂前那次 `unwatch` 必须排在 `watch` **之前** —— 反了等于先挂再解，白挂一次"
        );
    }

    /// ★ `P0b-Y2` 第十六拍：**socket 目录按 `TMUX_TMPDIR` 推，不许硬编码 `/tmp`。**
    ///
    /// # 为什么这条是单测而不是 e2e
    ///
    /// 它是个**纯函数**（env → 路径），单测才是对的 acceptor。
    /// ⚠ 第一版把它写进 e2e 的第三格，逼得那个套件自己设 `TMUX_TMPDIR` ——
    /// 而 `C7i` **零例外**禁止 e2e 靠它做隔离，`e2e_gate_registry` 那条守卫当场拦下。
    /// **它报得对**：判据要钉的性质与套件要用的隔离手段撞在同一个变量上时，
    /// 该换的是**判据落在哪一层**，不是给红线开例外。
    ///
    /// ⚠ 硬编码 `/tmp` 的后果是**静默失效**：在设了 `TMUX_TMPDIR` 的机器上，
    /// daemon 会去监视一个永远不会有动静的目录 —— 与修之前一模一样，且没有任何错误。
    #[test]
    fn tmux_socket_dir_follows_tmux_tmpdir() {
        // ⚠ env 是进程全局的：设完必须还原，否则会污染同进程里别的测试。
        let saved = std::env::var_os("TMUX_TMPDIR");
        // SAFETY: 单线程内设/取环境变量；本测试跑完立即还原。
        unsafe { std::env::set_var("TMUX_TMPDIR", "/x/y") };
        let d = tmux_socket_dir();
        unsafe {
            match &saved {
                Some(v) => std::env::set_var("TMUX_TMPDIR", v),
                None => std::env::remove_var("TMUX_TMPDIR"),
            }
        }
        let s = d.to_string_lossy();
        assert!(
            s.starts_with("/x/y/tmux-"),
            "socket 目录没跟着 `TMUX_TMPDIR` 走（实得 {s}）—— \
             硬编码 `/tmp` 会让 daemon 监视一个永远没动静的目录，且**没有任何错误**。"
        );
        assert!(
            !s.starts_with("/tmp/"),
            "socket 目录仍落在 `/tmp` 下（实得 {s}）"
        );
    }

    /// 同 pidfile 原地换 sid（/clear）：旧 sid 立即 Removed、新 sid Added，
    /// active_sids 恰含新 sid（跨机审计实锤的假 live 泄漏回归测试）。
    #[cfg(target_os = "linux")]
    #[test]
    fn sid_change_in_place_retires_old_sid() {
        let dir = std::env::temp_dir().join(format!("ccm-sidchange-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, false);
        let path = dir.join(format!("{pid}.json"));

        let write = |sid: &str| {
            std::fs::write(
                &path,
                format!(r#"{{"pid":{pid},"sessionId":"{sid}","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#),
            )
            .unwrap();
        };
        write("sid-1");
        process_session_added(&path, &mut state, &mut sink);
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "sid-1"));

        write("sid-2"); // /clear：同文件重写 sessionId
        process_session_added(&path, &mut state, &mut sink);
        assert!(
            // ★ S0：原地换 sid 的 removed 必须带 `Superseded`——monitor 靠它区分
            // 「死了（tmux 还在 ⇒ 灰点）」和「被顶替了（⇒ 直接归档）」。
            matches!(rx.try_recv(), Ok(Frame::SessionRemoved { sid, cause })
                if sid == "sid-1" && cause == RemovalCause::Superseded),
            "old sid must be retired BEFORE the new announcement"
        );
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "sid-2"));
        assert!(!state.active_sids.contains("sid-1"));
        assert!(state.active_sids.contains("sid-2"));
        assert_eq!(state.sessions.len(), 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// daemon-09：`process_jsonl` 对 turn-end 记录发 **Line 后紧跟 TurnEnd**；非 turn-end 只发 Line；
    /// **畸形行照发 Line、不 panic、无 TurnEnd**（§2.1 逐行转发 + turn-end 是 raw 之外额外边沿）。
    #[test]
    fn process_jsonl_emits_turn_end_after_line_raw_per_record() {
        let dir = std::env::temp_dir().join(format!("ccm-turnend-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, false);
        let path = dir.join("sess-1.jsonl");
        state.active_sids.insert("sess-1".to_string()); // process_jsonl 门控
                                                        // 三行：非 turn-end user / turn-end assistant / 畸形。
        let content = concat!(
            r#"{"type":"user","message":{}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"u-2","message":{"stop_reason":"end_turn"}}"#,
            "\n",
            "not json at all",
            "\n",
        );
        std::fs::write(&path, content).unwrap();
        process_jsonl(&path, &mut state, &mut sink);
        // 帧序：Line(user,seq0) / Line(end_turn,seq1) → TurnEnd(u-2) / Line(畸形,seq2)。
        assert!(
            matches!(rx.try_recv(), Ok(Frame::Line { seq: 0, .. })),
            "user 行 Line"
        );
        assert!(
            matches!(rx.try_recv(), Ok(Frame::Line { seq: 1, .. })),
            "end_turn 行 Line **先**发"
        );
        assert!(
            matches!(rx.try_recv(), Ok(Frame::TurnEnd { session_id, uuid }) if session_id == "sess-1" && uuid == "u-2"),
            "Line 后紧跟 TurnEnd(u-2)"
        );
        assert!(
            matches!(rx.try_recv(), Ok(Frame::Line { seq: 2, .. })),
            "畸形行照发 Line、不 panic"
        );
        assert!(
            rx.try_recv().is_err(),
            "无多余帧（畸形行不产 TurnEnd、user 行不产 TurnEnd）"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 同 sid 多 pidfile（resume 原进程未死）：删一个不发 Removed（引用计数），
    /// 删第二个才 Removed 恰一次。
    #[cfg(target_os = "linux")]
    #[test]
    fn same_sid_two_pidfiles_refcount() {
        let dir = std::env::temp_dir().join(format!("ccm-refcount-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, false);

        // 两个 pidfile 同 sid（借同一真实存活 pid；path key 不同即两个 entry）
        let p1 = dir.join(format!("{pid}.json"));
        // 第二个 pidfile 放子目录（path key 不同、file_stem 仍是 pid 数字）
        let sub = dir.join("dup");
        std::fs::create_dir_all(&sub).unwrap();
        let p2 = sub.join(format!("{pid}.json"));
        let body = format!(
            r#"{{"pid":{pid},"sessionId":"shared-sid","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#
        );
        std::fs::write(&p1, &body).unwrap();
        std::fs::write(&p2, &body).unwrap();
        process_session_added(&p1, &mut state, &mut sink);
        assert!(
            matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "shared-sid")
        );
        process_session_added(&p2, &mut state, &mut sink);
        // 第二个 pidfile：幂等检查是 per-key 的 → 恰好再发一条 Added（前端
        // ensureTab 幂等）。断言帧序（审计 S3：吞帧会掩盖"先 Removed 再 Added
        // 闪烁"类回归）。
        assert!(
            matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "shared-sid"),
            "second pidfile re-announces exactly once"
        );
        assert!(
            rx.try_recv().is_err(),
            "and nothing else (no spurious Removed)"
        );
        assert_eq!(state.sessions.len(), 2);

        // 删第一个 → 仍被 p2 引用 → 不发 Removed
        process_session_removed(&p1, &mut state, &mut sink);
        assert!(
            rx.try_recv().is_err(),
            "no Removed while another pidfile holds the sid"
        );
        assert!(state.active_sids.contains("shared-sid"));

        // 删第二个 → 归零 → Removed 恰一次
        process_session_removed(&p2, &mut state, &mut sink);
        assert!(
            matches!(rx.try_recv(), Ok(Frame::SessionRemoved { sid, cause })
            if sid == "shared-sid" && cause == RemovalCause::Gone)
        );
        assert!(!state.active_sids.contains("shared-sid"));
        assert!(rx.try_recv().is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 常规 added/removed 回归：单 pidfile 生命周期行为与 F22 前一致。
    #[cfg(target_os = "linux")]
    #[test]
    fn plain_lifecycle_regression() {
        let dir = std::env::temp_dir().join(format!("ccm-plainlife-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, false);
        let path = dir.join(format!("{pid}.json"));
        std::fs::write(
            &path,
            format!(r#"{{"pid":{pid},"sessionId":"solo","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        process_session_added(&path, &mut state, &mut sink);
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { sid, .. }) if sid == "solo"));
        process_session_removed(&path, &mut state, &mut sink);
        assert!(
            matches!(rx.try_recv(), Ok(Frame::SessionRemoved { sid, cause })
            if sid == "solo" && cause == RemovalCause::Gone)
        );
        assert!(state.sessions.is_empty() && state.active_sids.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    // === Batch9-F27：status 透传 ===

    /// 宣告帧带初始 status；同 pidfile modify：status 变 → session_status 帧、
    /// 不变 → 静默（幂等早退保留）。
    #[cfg(target_os = "linux")]
    #[test]
    fn status_diff_emits_session_status_frame() {
        let dir = std::env::temp_dir().join(format!("ccm-status-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("projects")).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, true);
        let pidfile = dir.join(format!("{pid}.json"));
        let write = |status: &str, waiting: Option<&str>| {
            let w = waiting
                .map(|x| format!(r#","waitingFor":"{x}""#))
                .unwrap_or_default();
            std::fs::write(
                &pidfile,
                format!(
                    r#"{{"pid":{pid},"sessionId":"st-sid","cwd":"/p","procStart":"{ticks}","status":"{status}"{w}}}"#
                ),
            )
            .unwrap();
        };
        write("busy", None);
        process_session_added(&pidfile, &mut state, &mut sink);
        match rx.try_recv() {
            Ok(Frame::SessionAdded { sid, status, .. }) => {
                assert_eq!(sid, "st-sid");
                assert_eq!(status.as_deref(), Some("busy"), "宣告带初始 status");
            }
            other => panic!("expected SessionAdded, got {other:?}"),
        }
        // 同内容 modify → 静默
        process_session_added(&pidfile, &mut state, &mut sink);
        assert!(rx.try_recv().is_err(), "status 未变不发帧");
        // status 变 → session_status 帧
        write("waiting", Some("permission prompt"));
        process_session_added(&pidfile, &mut state, &mut sink);
        match rx.try_recv() {
            Ok(Frame::SessionStatus {
                sid,
                status,
                waiting_for,
                ..
            }) => {
                assert_eq!(sid, "st-sid");
                assert_eq!(status.as_deref(), Some("waiting"));
                assert_eq!(waiting_for.as_deref(), Some("permission prompt"));
            }
            other => panic!("expected SessionStatus, got {other:?}"),
        }
        // 再变回 → 再发
        write("idle", None);
        process_session_added(&pidfile, &mut state, &mut sink);
        assert!(matches!(
            rx.try_recv(),
            Ok(Frame::SessionStatus { status: Some(s), waiting_for: None, .. }) if s == "idle"
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    // === Batch8-F25：tail-only 模式 ===

    /// tail-only 初扫：宣告帧带 path、零行帧；随后追加的新行 seq == 初扫时完整
    /// 行数 L（行号语义）；末尾残行不计数（F14 torn-line 语义）。
    #[cfg(target_os = "linux")]
    #[test]
    fn tail_only_primes_cursor_and_new_line_seq_is_line_number() {
        let dir = std::env::temp_dir().join(format!("ccm-tailonly-{}", std::process::id()));
        let proj = dir.join("projects").join("proj-x");
        std::fs::create_dir_all(&proj).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        // 既有历史：3 个完整行 + 1 个残行（残行不计数 → L=3）
        let jsonl = proj.join("tail-sid.jsonl");
        std::fs::write(&jsonl, b"{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n{\"torn").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, true); // --tail-only
        let pidfile = dir.join(format!("{pid}.json"));
        std::fs::write(
            &pidfile,
            format!(r#"{{"pid":{pid},"sessionId":"tail-sid","cwd":"/p","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        process_session_added(&pidfile, &mut state, &mut sink);
        // ① 宣告帧带 path
        match rx.try_recv() {
            Ok(Frame::SessionAdded {
                sid, path, lines, ..
            }) => {
                assert_eq!(sid, "tail-sid");
                assert_eq!(path.as_deref(), Some(jsonl.to_string_lossy().as_ref()));
                assert_eq!(
                    lines,
                    Some(3),
                    "帧应带 prime 时的完整行数 L（快照完整性校验用）"
                );
            }
            other => panic!("expected SessionAdded, got {other:?}"),
        }
        // ② 零行帧（历史被 prime 吸收）
        assert!(rx.try_recv().is_err(), "tail-only 初扫不得发行帧");
        // ③ 补全残行 + 追加新行 → 唯一行帧 seq==3（残行补全后成为第 3 行，0-based）
        std::fs::write(
            &jsonl,
            b"{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n{\"torn\":true}\n{\"new\":1}\n",
        )
        .unwrap();
        process_jsonl(&jsonl, &mut state, &mut sink);
        match rx.try_recv() {
            Ok(Frame::Line { seq, raw, .. }) => {
                assert_eq!(seq, 3, "残行补全行的 seq 应为初扫完整行数 L=3");
                assert_eq!(raw, r#"{"torn":true}"#);
            }
            other => panic!("expected Line, got {other:?}"),
        }
        match rx.try_recv() {
            Ok(Frame::Line { seq, .. }) => assert_eq!(seq, 4),
            other => panic!("expected Line, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 默认（全量）模式行为不变：初扫把既有行全部推流（旧 monitor 兼容锚点）。
    #[cfg(target_os = "linux")]
    #[test]
    fn full_replay_mode_still_streams_history() {
        let dir = std::env::temp_dir().join(format!("ccm-fullmode-{}", std::process::id()));
        let proj = dir.join("projects").join("proj-y");
        std::fs::create_dir_all(&proj).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        std::fs::write(proj.join("full-sid.jsonl"), b"{\"h\":1}\n{\"h\":2}\n").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, false); // 默认全量
        let pidfile = dir.join(format!("{pid}.json"));
        std::fs::write(
            &pidfile,
            format!(r#"{{"pid":{pid},"sessionId":"full-sid","cwd":"/p","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        process_session_added(&pidfile, &mut state, &mut sink);
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { .. })));
        assert!(matches!(rx.try_recv(), Ok(Frame::Line { seq: 0, .. })));
        assert!(matches!(rx.try_recv(), Ok(Frame::Line { seq: 1, .. })));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// F25 DoD ④：(with_bg, tail_only) = (true, true) 组合——bg 会话放行且
    /// tail-only 生效（宣告带元信息+path+lines，历史零行帧）。
    #[cfg(target_os = "linux")]
    #[test]
    fn with_bg_and_tail_only_combined() {
        let dir = std::env::temp_dir().join(format!("ccm-combo-{}", std::process::id()));
        let proj = dir.join("projects").join("proj-c");
        std::fs::create_dir_all(&proj).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        std::fs::write(proj.join("combo-sid.jsonl"), b"{\"h\":1}\n{\"h\":2}\n").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), true, true); // 双开
        let pidfile = dir.join(format!("{pid}.json"));
        std::fs::write(
            &pidfile,
            format!(r#"{{"pid":{pid},"sessionId":"combo-sid","cwd":"/p","kind":"bg","name":"任务","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        process_session_added(&pidfile, &mut state, &mut sink);
        match rx.try_recv() {
            Ok(Frame::SessionAdded {
                sid,
                session_kind,
                lines,
                path,
                ..
            }) => {
                assert_eq!(sid, "combo-sid");
                assert_eq!(session_kind.as_deref(), Some("bg"), "with_bg 放行");
                assert_eq!(lines, Some(2), "tail-only 带 L");
                assert!(path.is_some());
            }
            other => panic!("expected SessionAdded, got {other:?}"),
        }
        assert!(rx.try_recv().is_err(), "tail-only：历史零行帧");
        std::fs::remove_dir_all(&dir).ok();
    }

    // === Batch7-F24：--with-bg 放行 + 帧元信息 ===

    #[cfg(target_os = "linux")]
    #[test]
    fn with_bg_announces_bg_with_metadata() {
        let dir = std::env::temp_dir().join(format!("ccm-withbg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), true, false); // --with-bg
        let path = dir.join(format!("{pid}.json"));
        std::fs::write(
            &path,
            format!(r#"{{"pid":{pid},"sessionId":"bg-sid","cwd":"/proj/x","kind":"bg","jobId":"j","name":"评估任务","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        process_session_added(&path, &mut state, &mut sink);
        match rx.try_recv() {
            Ok(Frame::SessionAdded {
                sid,
                session_kind,
                cwd,
                name,
                ..
            }) => {
                assert_eq!(sid, "bg-sid");
                assert_eq!(session_kind.as_deref(), Some("bg"));
                assert_eq!(cwd.as_deref(), Some("/proj/x"));
                assert_eq!(name.as_deref(), Some("评估任务"));
            }
            other => panic!("expected SessionAdded with metadata, got {other:?}"),
        }
        assert!(state.active_sids.contains("bg-sid"), "bg 行要能流出");
        std::fs::remove_dir_all(&dir).ok();
    }

    // === Batch6-F21：kind 交互性门 ===

    #[test]
    fn parse_kind_variants() {
        assert_eq!(
            parse_kind(br#"{"sessionId":"s","kind":"bg","jobId":"j"}"#).as_deref(),
            Some("bg"),
            "真实 bg 样本形态"
        );
        assert_eq!(
            parse_kind(br#"{"sessionId":"s","kind":"interactive"}"#).as_deref(),
            Some("interactive")
        );
        assert_eq!(parse_kind(br#"{"sessionId":"s"}"#), None, "旧 CC 无 kind");
        assert_eq!(parse_kind(b"not json"), None);
    }

    /// 集成：kind:"bg" 的 pidfile（真实存活进程 = 本进程，身份/时间证据全过）
    /// 在 kind 门被拒——不发 SessionAdded、不进 sessions/active_sids。
    /// 对照组：同进程 interactive pidfile 正常宣告。
    #[cfg(target_os = "linux")]
    #[test]
    fn bg_pidfile_is_gated_even_when_author_is_alive() {
        let dir = std::env::temp_dir().join(format!("ccm-kind-gate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pid = std::process::id();
        let ticks = proc_starttime(pid).expect("own starttime");

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Frame>(64);
        let mut sink = FrameSink::new(tx);
        let mut state = ReaderState::new(dir.join("projects"), false, false);

        // bg pidfile：作者活着、procStart 逐位相等——F20 证据全过，但 kind 门拒
        let bg_path = dir.join(format!("{pid}.json"));
        std::fs::write(
            &bg_path,
            format!(r#"{{"pid":{pid},"sessionId":"bg-sid","cwd":"/x","kind":"bg","jobId":"j","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        process_session_added(&bg_path, &mut state, &mut sink);
        assert!(state.sessions.is_empty(), "bg must not be tracked");
        assert!(!state.active_sids.contains("bg-sid"));
        assert!(rx.try_recv().is_err(), "no SessionAdded frame for bg");

        // 对照：interactive 正常宣告
        std::fs::write(
            &bg_path,
            format!(r#"{{"pid":{pid},"sessionId":"int-sid","cwd":"/x","kind":"interactive","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        process_session_added(&bg_path, &mut state, &mut sink);
        assert!(state.active_sids.contains("int-sid"));
        match rx.try_recv() {
            Ok(Frame::SessionAdded { sid, .. }) => assert_eq!(sid, "int-sid"),
            other => panic!("expected SessionAdded, got {other:?}"),
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn procstart_identity_match_short_circuits_all_heuristics() {
        // Recorded ticks == current ticks → author confirmed, even when the
        // heuristics would individually scream imposter (stale mtime, bad
        // cmdline): identity evidence is strictly stronger.
        assert_eq!(
            add_time_verdict(
                Some(12285972),
                Some(12285972),
                Some(9_999_999),
                Some(1000),
                Some("tmux")
            ),
            AddTimeVerdict::Alive
        );
    }

    #[test]
    fn procstart_mismatch_falls_through_to_heuristics() {
        // Mismatch + stale time evidence → imposter (the tmux reuse case).
        assert_eq!(
            add_time_verdict(Some(12285972), Some(99999999), Some(2000), Some(1000), None),
            AddTimeVerdict::Imposter("started-after-pidfile")
        );
        // Mismatch alone with fresh mtime and claude-like cmdline → allow
        // (defends against CC changing the procStart format: a hard reject
        // would black out every real session).
        assert_eq!(
            add_time_verdict(
                Some(12285972),
                Some(99999999),
                Some(990),
                Some(1000),
                Some("claude")
            ),
            AddTimeVerdict::Alive
        );
    }

    #[test]
    fn tolerance_exact_boundary() {
        // start == mtime + 60 → still inside tolerance (uses >, not >=).
        assert_eq!(
            add_time_verdict(None, None, Some(1060), Some(1000), None),
            AddTimeVerdict::Alive
        );
        // One second past → imposter.
        assert_eq!(
            add_time_verdict(None, None, Some(1061), Some(1000), None),
            AddTimeVerdict::Imposter("started-after-pidfile")
        );
    }

    #[test]
    fn parse_procstart_ticks_variants() {
        assert_eq!(
            parse_procstart_ticks(br#"{"sessionId":"abc","procStart":"12285972"}"#),
            Some(12285972),
            "CC's real format: decimal string"
        );
        assert_eq!(
            parse_procstart_ticks(br#"{"procStart":12285972}"#),
            Some(12285972),
            "bare number tolerated"
        );
        assert_eq!(parse_procstart_ticks(br#"{"sessionId":"abc"}"#), None);
        assert_eq!(
            parse_procstart_ticks(br#"{"procStart":"133849906480000000"}"#),
            Some(133_849_906_480_000_000),
            "Windows FILETIME magnitude still parses (mismatch then falls to heuristics)"
        );
        assert_eq!(parse_procstart_ticks(b"not json"), None);
    }

    /// Integration sanity on the real /proc (Linux only): our own process's
    /// start epoch must be between boot and now — catches a broken btime +
    /// ticks/USER_HZ composition that pure-function tests cannot see.
    #[cfg(target_os = "linux")]
    #[test]
    fn own_process_start_epoch_is_sane() {
        let ticks = proc_starttime(std::process::id());
        assert!(ticks.is_some(), "own starttime must be readable");
        let epoch = start_epoch_from_ticks(ticks).expect("own start epoch");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(
            epoch <= now + 2,
            "start {epoch} must not be in the future (now {now})"
        );
        assert!(
            now - epoch < 24 * 3600,
            "test process started within a day (got {})",
            now - epoch
        );
    }

    #[test]
    fn parse_btime_from_realistic_proc_stat() {
        let stat = "cpu  123 0 456 789 0 0 0 0 0 0\n\
                    cpu0 61 0 228 394 0 0 0 0 0 0\n\
                    intr 12345 0 0\n\
                    ctxt 987654\n\
                    btime 1719900000\n\
                    processes 4321\n\
                    procs_running 2\n";
        assert_eq!(parse_btime(stat), Some(1_719_900_000));
        assert_eq!(parse_btime("cpu 1 2 3\n"), None, "no btime line");
        assert_eq!(parse_btime("btime notanumber\n"), None);
    }

    // === #34 procStart double-check (F04) ===

    /// A normal `/proc/<pid>/stat` line: starttime is field 22. Sample is a real
    /// kernel layout with a simple comm `(bash)`.
    #[test]
    fn parse_starttime_normal_line() {
        // pid=1234 comm=(bash) state=S ... field22(starttime)=9876543 ...
        let stat = "1234 (bash) S 1 1234 1234 0 -1 4194304 1 0 0 0 0 0 0 0 \
                    20 0 1 0 9876543 12345678 100 18446744073709551615 1 1 0 0";
        assert_eq!(parse_starttime_from_stat(stat), Some(9876543));
    }

    /// The comm gotcha: a process named with a space inside the parens must not
    /// derail field counting (splitting the whole line would shift every field).
    #[test]
    fn parse_starttime_comm_with_space() {
        let stat = "4242 (my proc) R 1 4242 4242 0 -1 0 0 0 0 0 0 0 0 0 \
                    20 0 1 0 555000 0 0";
        assert_eq!(parse_starttime_from_stat(stat), Some(555000));
    }

    /// The hard comm gotcha: parentheses *inside* comm. We must key off the LAST
    /// `')'`, not the first, or the offset is wrong.
    #[test]
    fn parse_starttime_comm_with_inner_parens() {
        let stat = "7 ((odd) name)) S 1 7 7 0 -1 0 0 0 0 0 0 0 0 0 \
                    20 0 1 0 424242 0 0";
        assert_eq!(parse_starttime_from_stat(stat), Some(424242));
    }

    /// Malformed / too-few-fields stat → None (never panics, no bad starttime).
    #[test]
    fn parse_starttime_malformed_returns_none() {
        assert_eq!(parse_starttime_from_stat(""), None); // no ')'
        assert_eq!(parse_starttime_from_stat("123 (x) S 1 2 3"), None); // < 22 fields
                                                                        // ')' present but starttime token is non-numeric.
        let bad = "1 (x) S 1 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 notanum 0";
        assert_eq!(parse_starttime_from_stat(bad), None);
    }

    /// `session_alive` truth table around the captured procStart.
    ///
    /// The existence-dependent assertions only hold on Linux. The
    /// reuse-detection logic — the whole point of #34 — is Linux-only, matching
    /// the `/proc` runtime target.
    ///
    /// ⚠ **U4a 起这段的前提变了，本测试因此在 Windows 上会 panic。** 原文写的是
    /// 「on non-Linux `pid_alive` is a hardcoded `true` smoke stub」，并据此**刻意不加 cfg 门**
    /// —— 而 U4a 把那个恒真 stub 换成了 `unimplemented!()`。今天在 Windows 上
    /// `session_alive` → `pid_alive` 会直接 panic。
    ///
    /// **本轮不加门**：`cargo check` 只编不跑，DoD ① 不受影响；而 U4b 一旦在真机上跑
    /// `cargo test`，这会是第一个红 —— 那正是应该有人看一眼的时刻，加门会把它藏起来。
    /// Phase D 审计的问题 9-3 已把它写进 U4b 的清单。
    #[test]
    fn session_alive_self_is_alive_in_existence_only_mode() {
        // Cross-platform: the current process is alive, and with no captured
        // baseline (`None`) liveness degrades to existence — must read alive.
        let me = std::process::id();
        assert!(
            session_alive(me, None),
            "self is alive in existence-only mode"
        );
    }

    /// Full, portable truth table for the pure liveness decision — including the
    /// transient-read-failure arm (`exists=true, expected=Some, current=None`)
    /// that must NOT archive a still-existing PID (the regression #34 audit
    /// flagged). No real `/proc` needed.
    #[test]
    fn is_same_live_process_truth_table() {
        // Process gone → dead regardless of start info.
        assert!(!is_same_live_process(false, Some(5), Some(5)));
        assert!(!is_same_live_process(false, None, None));

        // Exists + baseline + current readable: alive iff equal (reuse = differ).
        assert!(
            is_same_live_process(true, Some(5), Some(5)),
            "same start = alive"
        );
        assert!(
            !is_same_live_process(true, Some(5), Some(6)),
            "different read start = reused PID = dead"
        );

        // Exists but current start unreadable right now → DO NOT false-archive.
        assert!(
            is_same_live_process(true, Some(5), None),
            "transient /proc read failure on a live PID must stay alive"
        );

        // Exists, no baseline captured → existence-only degrade = alive.
        assert!(is_same_live_process(true, None, Some(9)));
        assert!(is_same_live_process(true, None, None));
    }

    // === #32 overflow signal (F05) ===

    /// FrameSink: a full channel drops + counts; once the channel drains, the
    /// next send emits a single `Overflow{dropped}` before the real frame and
    /// resets the counter. tokio's `try_send`/`try_recv` are sync, so no runtime.
    #[test]
    fn frame_sink_counts_drops_then_signals_overflow_on_recovery() {
        let (tx, mut rx) = mpsc::channel::<Frame>(2);
        let mut sink = FrameSink::new(tx);

        // Fill both slots — these go through cleanly, no overflow owed.
        sink.send(Frame::SessionAdded {
            sid: "a".into(),
            agent_kind: None,
            liveness_confidence: None,
            session_kind: None,
            attachable: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        });
        sink.send(Frame::SessionAdded {
            sid: "b".into(),
            agent_kind: None,
            liveness_confidence: None,
            session_kind: None,
            attachable: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        });
        assert_eq!(
            sink.dropped, 0,
            "nothing dropped while the channel had room"
        );

        // Channel is full now: three sends are dropped and counted.
        sink.send(Frame::SessionAdded {
            sid: "c".into(),
            agent_kind: None,
            liveness_confidence: None,
            session_kind: None,
            attachable: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        });
        sink.send(Frame::SessionAdded {
            sid: "d".into(),
            agent_kind: None,
            liveness_confidence: None,
            session_kind: None,
            attachable: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        });
        sink.send(Frame::SessionAdded {
            sid: "e".into(),
            agent_kind: None,
            liveness_confidence: None,
            session_kind: None,
            attachable: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        });
        assert_eq!(sink.dropped, 3);

        // Drain both queued frames (they are the first two, not the dropped ones).
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { .. })));
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { .. })));

        // Next send (channel now empty, cap 2): emits Overflow{3} into slot 1,
        // resets the counter, then the real frame into slot 2.
        sink.send(Frame::SessionRemoved {
            sid: "f".into(),
            cause: RemovalCause::Gone,
        });
        assert_eq!(sink.dropped, 0, "overflow signal flushed, counter reset");
        assert!(
            matches!(rx.try_recv(), Ok(Frame::Overflow { dropped: 3, .. })),
            "overflow signal carries the dropped count and arrives first"
        );
        assert!(
            matches!(rx.try_recv(), Ok(Frame::SessionRemoved { .. })),
            "the real frame follows the overflow signal"
        );

        // Steady state: no spurious Overflow once recovered.
        sink.send(Frame::SessionAdded {
            sid: "g".into(),
            agent_kind: None,
            liveness_confidence: None,
            session_kind: None,
            attachable: None,
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        });
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { .. })));
    }

    /// ★★ **「只数不建」必须与「建了再数」得出同一个数、同一个游标、同一批 seq**
    /// 〔audit-0805 F04 第 3 步〕。
    ///
    /// 最容易错的是 **seq**：不收集时很容易顺手把 `seqs.next(key)` 一起省掉，
    /// 而它是 per-path 单调且**永不重置**的 —— 少推一次，后续所有行的 seq 就与整读那条路
    /// **错开一位**，而**结果仍然像一份合理的输出**（行数对、内容对，只有编号悄悄偏了）。
    #[test]
    fn counting_only_agrees_with_collecting_on_count_cursor_and_seq() {
        let data = b"{\"a\":1}\n{\"b\":2}\r\n\n{\"c\":3}\n{\"torn\":";
        let mut s_collect = SeqCounter::default();
        let (lines, cur_collect) = read_new_lines_at(
            data,
            0,
            data.len() as u64,
            ReadCursor::default(),
            "k",
            &mut s_collect,
        );
        let mut s_count = SeqCounter::default();
        let (n, cur_count) = count_new_lines_at(
            data,
            0,
            data.len() as u64,
            ReadCursor::default(),
            "k",
            &mut s_count,
        );

        assert!(!lines.is_empty(), "夹具没产出行 —— 本条会零命中地绿");
        assert_eq!(
            n,
            lines.len(),
            "行数不一致：只数 {n} vs 建了再数 {}",
            lines.len()
        );
        assert_eq!(cur_collect, cur_count, "游标不一致");

        // ★ seq 推进必须一样。⚠ **必须用同一个 key** —— `SeqCounter` 是 per-path 的，
        //   用两个不同的 key 去比等于两边都从 0 开始，这条判据就恒真了。
        //   （第一版就是这么写错的：变异「不收集时省掉 seqs.next」照样绿。）
        let more = b"{\"d\":4}\n";
        let (l_after_collect, _) = read_new_lines_at(
            more,
            0,
            more.len() as u64,
            ReadCursor::default(),
            "k",
            &mut s_collect,
        );
        let (l_after_count, _) = read_new_lines_at(
            more,
            0,
            more.len() as u64,
            ReadCursor::default(),
            "k",
            &mut s_count,
        );
        assert_eq!(
            l_after_collect[0].seq, l_after_count[0].seq,
            "同一个 key 上两条路推进的 seq 不同步 —— 只数不建时把 `seqs.next` 省掉了。\n\
             它是 per-path 单调且永不重置的：少推一次，后续所有行的编号就与整读那条路错开一位，\n\
             而**结果仍然像一份合理的输出**（行数对、内容对，只有编号悄悄偏了）。"
        );
    }

    /// prime 那条路**不许再走收集入口**〔源码形态钉，防回退〕。
    #[test]
    fn prime_does_not_build_the_line_vector() {
        let src = guard_core::production_code(include_str!("watcher.rs"));
        let begin = src
            .find("fn prime_file_cursor(")
            .expect("找不到 prime_file_cursor —— 抽取器坏了，本条会零命中地绿");
        let end = src[begin..]
            .find("\n}\n")
            .expect("找不到结尾 —— 抽取器坏了");
        let body = &src[begin..begin + end];
        assert!(
            body.contains("count_new_lines_at"),
            "prime_file_cursor 没走「只数不建」那条入口 —— 要么切错范围，要么改回去了"
        );
        assert!(
            !body.contains("read_new_lines_at"),
            "prime_file_cursor 又在走收集入口了。它只要行数（一条 debug 日志），\n\
             而首次 prime 时「新增那一段」就是整份文件 —— 257 MB 会被物化成另一份 String 堆用完即扔。"
        );
    }

    /// ★★ **超时必须落成「观测无效」，绝不能落成「零会话」**〔audit-0805 F09〕。
    ///
    /// 这是本件最要命的一格：`ServerEmpty`（rc=0 且 stdout 空）会让上层认为
    /// **那台机器上一个会话都没有** ⇒ 活着的会话被 retire。
    /// 而超时是「**我没看清**」，不是「**我看清了，是空的**」。
    ///
    /// `timeout -s KILL` 杀掉子进程后 rc 是 137（128+9）；有些实现/路径下是 124；
    /// 被信号直接杀时 `code` 是 `None`。三种都必须落 `Unobservable`。
    #[test]
    fn a_timed_out_probe_is_unobservable_never_zero_sessions() {
        for code in [Some(124), Some(137), None] {
            let got = classify_tmux_probe(code, "");
            assert!(
                matches!(got, TmuxObservation::Unobservable),
                "rc={code:?} 被判成了 {got:?} —— 超时是「我没看清」，不是「我看清了，是空的」。\n\
                 判成 ServerEmpty 会让上层认为那台机器零会话 ⇒ **活着的会话被 retire**。"
            );
        }
        // 对照：真正的「server 在、但零会话」仍然要判 ServerEmpty（防把上面写成恒真）。
        assert!(
            matches!(
                classify_tmux_probe(Some(0), ""),
                TmuxObservation::ServerEmpty
            ),
            "rc=0 且空 stdout 该是 ServerEmpty —— 上面那条不许把它一起吞了"
        );
    }

    /// 探测脚本必须**带上界**，且 `timeout` 缺席时诚实退回〔audit-0805 F09，承接 C7〕。
    #[test]
    fn the_tmux_probe_is_bounded_and_degrades_honestly() {
        let script = tmux_probe_script();
        assert!(
            script.contains("tmux ls"),
            "抽取器自检：脚本里连 `tmux ls` 都没有 —— 拿错东西了：{script}"
        );
        assert!(
            script.contains("timeout"),
            "★ 探测没有上界。`run_tmux_ls` 的 `output()` 无超时，而 `watch_loop` 的 `tmux_inflight`\n\
             只在收到 `TmuxObserved` 时清 —— 探测永不返回 ⇒ 标志永远为真 ⇒ **此后一次 tmux 探测\n\
             都不会再发起，且不发任何理由帧**（报告 I-2）。实得：{script}"
        );
        assert!(
            script.contains("command -v timeout"),
            "★ `timeout` 必须门控。硬用它会在没有 coreutils 的系统上让整条探测直接失败 ——\n\
             那是把一个「偶发卡死」换成「必然不可用」。要诚实降级（C7），不是赌它存在。实得：{script}"
        );
    }

    /// ★★ **两条 tail 读路不许再整读会话 jsonl**〔audit-0805 F04 第 2 步〕。
    ///
    /// # 为什么不能笼统禁 `fs::read`
    ///
    /// 同一个文件里 `process_session_added` **正当地**整读 pidfile
    /// （`sessions/<PID>.json`，几百字节）。一条「本文件不许出现 `fs::read`」的守卫
    /// 会把它一起禁掉，于是下一个人要么绕过守卫、要么把它加进豁免名单 ——
    /// **两条路都会让这条判据失去意义**。⇒ 精确钉**那两个函数的函数体**。
    ///
    /// # 它防的是什么
    ///
    /// 「改回整读」是最容易发生的回退：整读的代码更短、也照样能跑（只是 257 MB 会话的
    /// 每一次文件事件都要把整份读进内存）。**慢不会让任何测试变红** —— 所以只能靠源码形态钉。
    #[test]
    fn the_two_tail_readers_do_not_slurp_the_whole_session_file() {
        let src = guard_core::production_code(include_str!("watcher.rs"));
        for (name, sig) in [
            ("process_jsonl", "fn process_jsonl("),
            ("prime_file_cursor", "fn prime_file_cursor("),
        ] {
            let begin = src
                .find(sig)
                .unwrap_or_else(|| panic!("找不到 {name} —— 抽取器坏了，本条会零命中地绿"));
            let end = src[begin..]
                .find("\n}\n")
                .unwrap_or_else(|| panic!("找不到 {name} 的结尾 —— 抽取器坏了"));
            let body = &src[begin..begin + end];
            assert!(
                body.contains("read_tail_from"),
                "{name} 里没有调 `read_tail_from` —— 要么切错范围（本条会零命中地绿），\n\
                 要么它被改回整读了。"
            );
            for slurp in ["fs::read(", "read_to_string("] {
                assert!(
                    !body.contains(slurp),
                    "{name} 里出现了 `{slurp}` —— 会话 jsonl 又被整读了。\n\
                     257 MB 的活跃会话每来一行就要整份读进内存，而**慢不会让任何测试变红**，\n\
                     所以这条只能靠源码形态钉。要读整份得说明白为什么（pidfile 那种小文件除外，\n\
                     它在 `process_session_added` 里、不在本条管辖内）。"
                );
            }
        }
    }

    /// ★★ **分块入口与整读入口必须逐字节同解**〔audit-0805 F04 第 1 步〕。
    ///
    /// 这条判据存在的全部理由：`read_new_lines` 把 `bytes.len()` 当文件长度用，
    /// 而「改成 seek 只读新字节」的**第一步**就是把长度摘出来。摘的过程里最容易错的
    /// 是那个相对索引（`start - chunk_start`），而错了之后**结果仍然像一份合理的输出** ——
    /// 行还在、seq 还在涨，只是内容错位。⇒ 用同一份数据两条路对拍，逐字段比。
    #[test]
    fn the_chunked_entry_agrees_with_the_whole_file_entry_byte_for_byte() {
        let data = b"{\"a\":1}\n{\"b\":2}\r\n\n{\"c\":3}\n{\"torn\":";
        // 先各自消费前一段，制造一个非零的 consumed。
        let mut seqs_a = SeqCounter::default();
        let (_, cur_a) = read_new_lines(&data[..8], ReadCursor::default(), "k", &mut seqs_a);
        let mut seqs_b = SeqCounter::default();
        let (_, cur_b) = read_new_lines(&data[..8], ReadCursor::default(), "k", &mut seqs_b);
        assert_eq!(cur_a, cur_b, "前置状态本身要一致");

        // 整读入口：喂全文件。
        let (lines_whole, cur_whole) = read_new_lines(data, cur_a, "k", &mut seqs_a);
        // 分块入口：只喂 [consumed, EOF) 那一段。
        let start = cur_b.consumed;
        let (lines_chunk, cur_chunk) = read_new_lines_at(
            &data[start as usize..],
            start,
            data.len() as u64,
            cur_b,
            "k",
            &mut seqs_b,
        );

        assert_eq!(cur_whole, cur_chunk, "游标必须一致（consumed / seen_len）");
        assert_eq!(
            lines_whole.len(),
            lines_chunk.len(),
            "行数必须一致：整读 {lines_whole:?} vs 分块 {lines_chunk:?}"
        );
        for (w, c) in lines_whole.iter().zip(lines_chunk.iter()) {
            assert_eq!(w.raw, c.raw, "内容错位了");
            assert_eq!(
                w.byte_offset, c.byte_offset,
                "byte_offset 错位了（这是相对索引算错时最典型的表现）：{w:?} vs {c:?}"
            );
            assert_eq!(w.seq, c.seq, "seq 不一致");
        }
        assert!(!lines_whole.is_empty(), "夹具没产出行 —— 本条会零命中地绿");
    }

    /// 分块没读到 EOF 时**必须炸**，不许静默〔定框 E4〕。
    ///
    /// 少一截会让 torn-tail 判定把「还没读到的字节」误当成「写了一半」，
    /// 游标停在半路且再也不前进 —— 这类错**无声且自洽**，所以是 `assert!` 不是 `debug_assert!`。
    #[test]
    #[should_panic(expected = "chunk 必须覆盖到 EOF")]
    fn a_chunk_that_stops_short_of_eof_panics_instead_of_silently_wedging() {
        let data = b"{\"a\":1}\n{\"b\":2}\n";
        let mut seqs = SeqCounter::default();
        // 谎报 file_len：说文件更长，但只给了前半段。
        read_new_lines_at(
            &data[..8],
            0,
            data.len() as u64,
            ReadCursor::default(),
            "k",
            &mut seqs,
        );
    }

    /// ★★ **丢的是「别处没有」的帧时，`Overflow` 必须带上身份**〔audit-0805 F03，定框 E4〕。
    ///
    /// 只有计数的 `Overflow` 对**内容帧**够用（行还在远端 jsonl 里），对**状态增量帧**不够：
    /// 它是一次差分的结果、别处不存在，客户端拿着「丢了 N 条」**没法重同步**。
    ///
    /// ⚠ 这条钉的是**行为**，不是「代码里有没有那个字段」——
    /// 塞满通道、真丢一条 `SessionRemoved`，再看排空后那条 `Overflow` 认不认得它。
    #[test]
    fn dropping_an_unrecoverable_frame_puts_its_identity_in_the_overflow() {
        let (tx, mut rx) = mpsc::channel::<Frame>(1);
        let mut sink = FrameSink::new(tx);

        // 占满（cap 1）。
        sink.send(Frame::Line {
            session_id: "occupy".into(),
            path: "/p".into(),
            seq: 0,
            raw: "{}".into(),
            byte_offset: 0,
        });
        // 丢一条内容帧（可恢复 ⇒ 只计数、不留身份）与一条状态增量帧（不可恢复 ⇒ 留身份）。
        sink.send(Frame::Line {
            session_id: "content-lost".into(),
            path: "/p".into(),
            seq: 1,
            raw: "{}".into(),
            byte_offset: 1,
        });
        sink.send(Frame::SessionRemoved {
            sid: "sid-gone".into(),
            cause: RemovalCause::Gone,
        });
        assert_eq!(sink.dropped, 2, "两条都该计入 dropped");

        // 排空后下一次 send 会先补 Overflow。
        assert!(matches!(rx.try_recv(), Ok(Frame::Line { .. })));
        sink.send(Frame::TurnEnd {
            session_id: "x".into(),
            uuid: "u".into(),
        });
        match rx.try_recv() {
            Ok(Frame::Overflow {
                dropped,
                lost,
                lost_truncated,
            }) => {
                assert_eq!(dropped, 2);
                assert!(!lost_truncated, "才两条，远没到上限");
                assert_eq!(
                    lost,
                    vec![crate::wire::LostFrame {
                        kind: "session_removed",
                        subject: Some("sid-gone".into()),
                    }],
                    "只有不可恢复的那条留身份；内容帧丢了别处还有，不该占位"
                );
            }
            other => panic!("期望带身份的 Overflow，实得 {other:?}"),
        }
    }

    /// 身份表**有界**，且超限**不是静默截断**〔定框 E5：上限与超限语义成对定义〕。
    ///
    /// 没有这个界，就等于把 `CHANNEL_CAPACITY` 想防的内存增长从帧挪到了 `Overflow` 自己身上。
    #[test]
    fn the_identity_list_is_bounded_and_says_so_when_it_truncates() {
        let (tx, mut rx) = mpsc::channel::<Frame>(1);
        let mut sink = FrameSink::new(tx);
        sink.send(Frame::Line {
            session_id: "occupy".into(),
            path: "/p".into(),
            seq: 0,
            raw: "{}".into(),
            byte_offset: 0,
        });
        let over = LOST_IDENTITY_CAP + 5;
        for i in 0..over {
            sink.send(Frame::SessionRemoved {
                sid: format!("sid-{i}"),
                cause: RemovalCause::Gone,
            });
        }
        assert_eq!(sink.dropped, over as u64, "超出上限的仍然计入 dropped");
        assert_eq!(sink.lost.len(), LOST_IDENTITY_CAP, "身份表不许越界增长");
        assert!(sink.lost_truncated, "截断了就要说出来，不许静默");

        assert!(matches!(rx.try_recv(), Ok(Frame::Line { .. })));
        sink.send(Frame::TurnEnd {
            session_id: "x".into(),
            uuid: "u".into(),
        });
        match rx.try_recv() {
            Ok(Frame::Overflow {
                lost,
                lost_truncated,
                ..
            }) => {
                assert_eq!(lost.len(), LOST_IDENTITY_CAP);
                assert!(lost_truncated, "标志要真的上线，不能只留在 sink 里");
            }
            other => panic!("期望 Overflow，实得 {other:?}"),
        }
    }

    /// 分类表**不许用 `_ =>` 兜底**〔LEDGER S1 的钉法〕。
    ///
    /// 穷尽 `match` 的全部价值就在于**新增帧种时编译期躲不掉**。
    /// 有人图省事加一条 `_ => true`，编译照过、而新帧种就此默认「丢了没关系」——
    /// 那正是 B-3 的原样复发。⇒ 用零命中守卫钉住源码形态。
    #[test]
    fn the_recoverability_table_has_no_catch_all_arm() {
        let src = guard_core::production_code(include_str!("../wire.rs"));
        let begin = src
            .find("pub fn loss_is_recoverable")
            .expect("找不到 loss_is_recoverable —— 抽取器坏了，本条会零命中地绿");
        let end = src[begin..]
            .find("\n    }\n")
            .expect("找不到函数结尾 —— 抽取器坏了");
        let body = &src[begin..begin + end];
        assert!(
            body.contains("Frame::Line"),
            "抽到的函数体里连 `Frame::Line` 都没有 —— 切错范围了，本条会零命中地绿"
        );
        assert!(
            !body.contains("_ =>"),
            "`loss_is_recoverable` 里出现了 `_ =>` 兜底臂。\n\
             穷尽 match 的全部价值就是**新增帧种时编译期躲不掉**；加了兜底 = 新帧种默认\n\
             「丢了没关系」，而那正是 audit-0805 B-3 的原样复发。逐个列出来，别偷懒。"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn session_alive_decision_table_linux() {
        // A PID that cannot be alive on any sane host → dead regardless of start.
        let dead_pid = u32::MAX;
        assert!(
            !session_alive(dead_pid, Some(123)),
            "absent PID is dead even with an expected start"
        );
        assert!(
            !session_alive(dead_pid, None),
            "absent PID is dead in existence-only mode too"
        );

        // The current process IS alive. Baseline == its real start → alive;
        // a wrong baseline → dead (the PID-reuse signal).
        let me = std::process::id();
        let real = proc_starttime(me).expect("self has a /proc starttime");
        assert!(
            session_alive(me, Some(real)),
            "self is alive when start matches"
        );
        assert!(
            !session_alive(me, Some(real.wrapping_add(1))),
            "a mismatched start means the PID was reused → dead"
        );
    }

    /// `P0b`：**「目录不存在就永远不重试」这条缺陷的登记**〔08-13 实测复现〕。
    ///
    /// 本条**不是**在断言那是对的 —— 它钉的是**那条已知缺陷的说明还在**，
    /// 因为下一个读到那两个 `else` 分支的人，第一反应会是「打个 warn 挺合理」。
    /// 而实测告诉我们：`<claude_dir>/sessions/` 是**用户第一次跑 claude 时才建的**，
    /// daemon 起得早一步，就**永远不宣告会话**。
    ///
    /// 复现（帧的 `kind` 直方图，其余条件一模一样）：
    /// · 有 `sessions/` ⇒ `hello · line · session_added · tmux_sessions`
    /// · 无 `sessions/` ⇒ **只有** `hello · tmux_sessions`
    ///
    /// ⚠ 修掉它之后**请连同这条判据一起改** —— 它守的是「缺陷说明在」，
    /// 缺陷没了这条就该换成守新行为的那一条。
    #[test]
    fn the_missing_dir_branch_still_says_it_never_retries() {
        let src = include_str!("watcher.rs");
        for needle in ["本进程不会再重试挂它", "永远不宣告会话"] {
            assert!(
                src.contains(needle),
                "那条缺陷说明被删了（少了「{needle}」）—— 删它之前请先修掉缺陷本身，\
                 否则下一个人会以为「打个 warn 就够了」"
            );
        }
    }
}
