//! Phase-0 backend watcher: tails every followed agent's session records (`agents::followed`: the record-tree
//! agent's `projects/**.jsonl` with its `sessions/<PID>.json` pidfiles, plus the agents whose sessions are alive
//! while a process holds their record open for writing — 「开着即活」那一路) and turns filesystem activity into
//! [`Frame`]s on a bounded channel. 各家的差别只在适配层（记录住哪 · 怎么切行 · 怎么判一轮 · 靠什么判活）。
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
//! 〔read8a〕**整台一份**：一个进程只起一份 reader（会话账本 · 每份记录的读者 · 运行簿都只一份）；
//! 每条流连接经 [`WatcherPoke::attach`] 拿一条自己的有界通道（[`Sub`]：名单 · 丢帧账按连接），reader 逐条连接投帧。
//! 记录的预读不在 reader 线程上做：宣告先发，预读交线程池（[`PrimePool`]），读完交回再接着出那条会话的行。
//!
//! The reader runs on a dedicated blocking thread (`notify-debouncer-mini` is a
//! synchronous, `std::sync::mpsc`-based API) and talks to the async writer
//! through `tokio::sync::mpsc`.
//!
//! # 增量读的规则（今天全仓只有这一份）
//!
//! 〔CF1 起 monitor 自己那份 jsonl 读者删了：本机会话内容也走本机后端的 `line` 帧。
//!  这一节原先叫「Parity with 那份 monitor 读者」、逐条与它对拍；对拍的另一边没了，下面的规则就是唯一一份。〕
//!
//! The incremental read: a per-file [`ReadCursor`],
//! read from `cursor.consumed` up to the **last `\n`** in the new region — a
//! torn tail without a trailing `\n` is deferred to the next event, never
//! emitted half-way (Batch4-F14). BOM strip via
//! `trim_start_matches('\u{feff}')`, skip blank lines; only active sessions' own record
//! files stream as `line` frames (sub-run records feed the run book instead, see
//! [`crate::observe::runs`]). Truncation is detected
//! against `cursor.seen_len` (the observed EOF high-water mark, which covers a
//! deferred torn tail); on truncation the cursor resets to byte 0 and
//! [`process_jsonl`] restarts the per-file seq after announcing the
//! re-read (`session_file_reread`) — seq is the line number in the file as it is
//! now, never a number past it (see [`process_jsonl`]).
//!
//! 读在块上（8a d，[`NewLines`]）：一趟从起读处按块读到尾，一块一块地出行，不把那一截一次读进内存。
//! 读到一半出错 ⇒ 这一趟停在已经交出去的最后一块之后（交出去的行不再发一遍），其余的下一趟接着读；
//! 读的那一下文件缩了 ⇒ 游标留旧高水位，下一趟按截短从 0 重读并出声。
//!
//! ⚠ 这段话此前写的是 "reads via one `fs::read` snapshot" —— 那**曾经是真的**，
//! 而它只评了**错误语义**那一面，对**内存与 IO 后果一个字没记**（audit-0805 B-4）：
//! 每个 debounce 事件把整份 jsonl 读进内存，257 MB 的活跃会话 ⇒ 每次读 257 MB，
//! 只为提取约 500 字节的新行；而本地 monitor 同一件事一直是 `seek` + `read_until`
//! ⇒ **强机器流式、弱机器整读，正好反了**（backend 住树莓派那类小机器）。
//! F04 已改成只读新字节（`read_tail_from` + `read_new_lines_at`；d 之后按块读，见上），
//! 由 `the_two_tail_readers_do_not_slurp_the_whole_session_file` 钉住不许改回去。
//! ★ 留这段话是因为**「未登记的缺陷」比「登记过的取舍」更难发现** ——
//! 当时那条头注读起来完全像一条深思熟虑的取舍。

use crate::agents::UserTextExt as _;
use crate::platform::fs::{Ro, RoFile};
use crate::stream::wire::{Frame, LostFrame, RemovalCause, RereadWhy, SeqCounter, Topic};
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
use super::tmux_observe::{
    classify_with_server_state, diff_closed, observation_parts, run_tmux_probe, tmux_socket_dir,
    ServerState, TmuxObservation, TmuxProbe,
};
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
// **账本第 1 行的最终形态在这里建立**（§3）：
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
    /// 「开着即活」那几家的记录根下，一批文件被新建 / 打开 / 写完关闭（`platform::writers::watch_opens` 交的一批）。
    Writers(crate::platform::writers::OpenBatch),
    /// 用户按了「重新对齐」：与起步同一套（耳朵重挂 · pidfile 对表 · 重探 tmux · 账号清单），
    /// 做完把差异回给 `done`。`only` = 只对这一个 sid（关卡 2「对齐后重试」：重验 ＋ 重打）。
    Resync {
        only: Option<String>,
        done: std::sync::mpsc::Sender<Reconciled>,
    },
    /// 这条流报了它此刻在看哪几个会话（`stream-watch`，整份换）：回刚进名单的那几个从第几行起上流（[`watch_sessions`]）。
    Watch {
        /// 哪条连接报的（[`Attachment`] 的号）。
        sub: u64,
        ask: crate::stream::wire::WatchAsk,
        done: tokio::sync::oneshot::Sender<Vec<crate::stream::wire::WatchFrom>>,
    },
    /// 一条流连接接上来了：从这一刻起它收这台的帧（先收一份此刻的样子，[`join`]）。
    Join(Sub),
    /// 那条流连接走了（[`Attachment`] 丢了）。
    Leave(u64),
    /// 一条会话的后台预读做完了（线程池交回，[`primed`] 装回）。
    Primed(PrimeDone),
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

/// watcher 看见、控制面去做的那两件事 —— 改世界的不在观测侧，watcher 只在那一刻报一声（起 watcher 时由 `main.rs` 接上控制面那两个函数）。
/// 两件的时机都只有观测侧有：「tmux server 起来了」是 socket 目录 inotify 看到的；`(pid, sid)` 只在 pidfile 出现 / 改写那一刻同时在手。
#[derive(Clone, Copy)]
pub struct Reactions {
    /// tmux server（重新）起来了 ⇒ 给它装通知 hook（hook 活在 server 内存里，server 每次起来都要重装；`control::tmux_hook::install_for_this_process`）。
    pub tmux_server_up: fn(),
    /// pidfile 出现 / 改写：`ccm` 给这个 pid 留过便条 ⇒ 记下 `sid → 号`（`control::launch_account::adopt_seen`；写句柄是起 watcher 时交进来的那一份）。
    pub session_seen: fn(&crate::platform::fs::Own, u32, &str),
}

/// 这一批文件事件落在哪几个会话的任务目录里（`<tasks>/<sid>/…` 的第一段；`<tasks>` 自己不算）。
/// 批内去重、按名排（同一个 sid 动了几次都只报一帧）。**纯函数**（判据直接喂路径）。
fn tasks_touched<'a>(paths: impl Iterator<Item = &'a Path>, tasks: &Path) -> Vec<String> {
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for p in paths {
        if let Some(sid) = p
            .strip_prefix(tasks)
            .ok()
            .and_then(|rel| rel.components().next())
            .and_then(|c| c.as_os_str().to_str())
            .filter(|s| !s.is_empty())
        {
            out.insert(sid.to_string());
        }
    }
    out.into_iter().collect()
}

/// 这一批文件事件里有没有那份账号 manifest。**抽出来是为了判据**：
/// 事件循环本身要起 tmux 探测，单测不许碰用户真实的 tmux server（`C7i`）。
fn manifest_touched<'a>(mut paths: impl Iterator<Item = &'a Path>, manifest: &Path) -> bool {
    paths.any(|p| p == manifest)
}

/// 这一批文件事件里有没有哪个号记「MCP 要登录」的那份文件（`files`：注册表里各家的那个名字）：在那一家的家目录里，
/// 或在账号库里某个号的目录里（都只认直接子项：watcher 盯的就是这两层，不递归）。同 [`manifest_touched`] 抽出来给判据。
fn needs_login_touched<'a>(
    mut paths: impl Iterator<Item = &'a Path>,
    accts_dir: Option<&Path>,
    agent_home: &Path,
    files: &[&str],
) -> bool {
    paths.any(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| files.contains(&n))
            && (p.parent() == Some(agent_home)
                || accts_dir.is_some_and(|a| p.parent().and_then(Path::parent) == Some(a)))
    })
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
    // ★★ `P0b-Y2` 第十五拍：**这一步原先一行日志都不打。**
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
/// monitor 一连上就该拿到 tmux 状态。全删的话，backend 要等到第一个 hook 触发才会探，
/// 空闲机器上可能是**永远**。所以节拍没了，但这一下要留下。
///
/// 之后的每一拍都由事件驱动：tmux hook → `--tmux-notify` → SIGUSR1 → `WatchEvent::Poke`
/// （P4），server 生死由 pidfd / socket inotify 管（P3）。**零定时器。**
fn initial_tmux_probe(tx: &std::sync::mpsc::Sender<WatchEvent>) {
    // send 失败 = reader 已经走了 ⇒ 无所谓。
    let _ = tx.send(WatchEvent::TmuxProbeDue);
}

/// 起一次 tmux 探测（一次性后台线程，结果回 `TmuxObserved`）。已有在途的就不起 —— `inflight` 只在这里置位、
/// 只在收到 `TmuxObserved` 时清。回真 = 这次真起了（会话账本据此知道「摘除之后才起」的那一份在路上了）。
fn start_tmux_probe(
    inflight: &mut bool,
    tx: &std::sync::mpsc::Sender<WatchEvent>,
    sink: &mut FrameSink,
) -> bool {
    if *inflight {
        return false;
    }
    *inflight = true;
    sink.observing();
    let tx = tx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(WatchEvent::TmuxObserved(run_tmux_probe()));
    });
    true
}

/// Bounded channel capacity between the reader and the stdout writer.
///
/// Large enough to absorb a `/resume` history burst without dropping, small
/// enough that a wedged writer cannot grow memory without bound. A full channel
/// drops frames with a warning (Phase-0 gap, see module docs).
pub const CHANNEL_CAPACITY: usize = 10_000;

/// notify-debouncer-mini debounce window.（从前写「与 monitor 那份读者对齐」—— 那份 CF1 删了。）
const DEBOUNCE_MS: u64 = 100;

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

/// 可重入地挂 `agent_home` 本身；不在 ⇒ 退一层挂它的上一层（同 tmux socket 目录）。回 `true` = 这次刚挂上它本身。
/// 上一层挂上不摘：它可能与别的挂点同一目录（`unwatch` 会连带摘掉），留着也听得见 `agent_home` 被删后重建。
/// `what` = 日志里怎么称呼它（账号目录也走这一个函数）。
fn rewatch_agent_home(
    debouncer: &mut notify_debouncer_mini::Debouncer<impl notify::Watcher>,
    agent_home: &Path,
    what: &str,
    home_watched: &mut bool,
    parent_watched: &mut bool,
) -> bool {
    if agent_home.is_dir() {
        let was = *home_watched;
        // 无条件先 unwatch 再 watch：分不清「同 inode 的普通事件」与「换了 inode」（`rewatch_dir` 同一条纪律）。
        let _ = debouncer.watcher().unwatch(agent_home);
        return match debouncer
            .watcher()
            .watch(agent_home, RecursiveMode::NonRecursive)
        {
            Ok(()) => {
                *home_watched = true;
                if !was {
                    tracing::info!("已挂上 {what}: {}", agent_home.display());
                }
                !was
            }
            Err(e) => {
                *home_watched = false;
                // 挂不上不致命（退回「起来时是什么样就什么样」），但**要说出来**。
                tracing::warn!(
                    "watch failed for {}: {e} —— 子目录若被重建，本后端将听不见（下次事件再试）",
                    agent_home.display()
                );
                false
            }
        };
    }
    if *home_watched {
        tracing::info!(
            "{what} 消失了 {} —— 解除记账，等它回来再挂",
            agent_home.display()
        );
        let _ = debouncer.watcher().unwatch(agent_home);
        *home_watched = false;
    }
    if *parent_watched {
        return false;
    }
    match agent_home.parent().filter(|d| d.is_dir()) {
        Some(parent) => match debouncer
            .watcher()
            .watch(parent, RecursiveMode::NonRecursive)
        {
            Ok(()) => {
                *parent_watched = true;
                tracing::warn!(
                    "{what} 还没有：{} —— 先盯着它的上一层 {}，出现时自动挂上",
                    agent_home.display(),
                    parent.display()
                );
            }
            Err(e) => tracing::warn!(
                "{what} 还没有：{}，它的上一层 {} 挂不上 watch（{e}）—— 它之后被建出来，本后端看不见，要重启后端",
                agent_home.display(),
                parent.display()
            ),
        },
        // 设计只退一层（与 tmux socket 目录那条同形）；上一层也不在 ⇒ 说真话，不再往上爬。
        None => tracing::warn!(
            "{what} 还没有：{}，它的上一层也不在 —— 本后端不会看见它，建出来之后要重启后端",
            agent_home.display()
        ),
    }
    false
}

/// 这台的会话注解表（读不了 · 推不出家 ⇒ 空表：当没有改过名；历史清单那一路照旧说为什么）。
fn annotations_table() -> crate::history::history_annotations::Table {
    match crate::history::history_annotations::load() {
        crate::history::history_annotations::Loaded::Read(t) => t,
        _ => Default::default(),
    }
}

/// 配置文件（`~/.cc-monitor/profiles.toml`）那道耳朵：盯它所在的目录（不递归；写它是「写旁名再换名上位」，盯文件本身换名之后就聋了）。
/// 目录起步不在 ⇒ 先挂上一层，出现时重挂（与 `agent_home` 同一个函数）。
pub(crate) struct ProfilesEar {
    file: PathBuf,
    dir: PathBuf,
    watched: bool,
    parent_watched: bool,
}

impl ProfilesEar {
    pub(crate) fn new(file: PathBuf) -> Option<Self> {
        let dir = file.parent()?.to_path_buf();
        Some(ProfilesEar {
            file,
            dir,
            watched: false,
            parent_watched: false,
        })
    }

    pub(crate) fn arm(
        &mut self,
        debouncer: &mut notify_debouncer_mini::Debouncer<impl notify::Watcher>,
    ) {
        rewatch_agent_home(
            debouncer,
            &self.dir,
            "profiles dir",
            &mut self.watched,
            &mut self.parent_watched,
        );
    }

    /// 一批事件里配置文件动没动。目录自己出现 / 消失 ⇒ 重挂，也算动了（里面的配置文件可能跟着来了 / 没了）。
    pub(crate) fn touched<'a>(
        &mut self,
        debouncer: &mut notify_debouncer_mini::Debouncer<impl notify::Watcher>,
        paths: impl Iterator<Item = &'a Path>,
    ) -> bool {
        let mut hit = false;
        let mut dir_moved = false;
        for p in paths {
            hit |= p == self.file;
            dir_moved |= p == self.dir;
        }
        if dir_moved {
            self.arm(debouncer);
        }
        hit || dir_moved
    }
}

/// 账号 manifest 所在目录那道耳朵：起步不在 ⇒ 挂它的上一层；
/// 它出现 / 被删重建（它自己路径上的事件）⇒ 按盘上此刻重挂（与 `agent_home` 同一个函数，VIS2 S3 同法）。
/// 另挂它下面每个号的目录（不递归）：号的凭据文件出现 / 变了 ⇒ 也算「清单可能变了」（登录完成那一刻界面自己变「已登录」）。
struct AccountsEar {
    dir: PathBuf,
    watched: bool,
    parent_watched: bool,
    /// 此刻挂着的各号目录。
    subs: std::collections::BTreeSet<PathBuf>,
}

impl AccountsEar {
    fn new(dir: &Path) -> Self {
        AccountsEar {
            dir: dir.to_path_buf(),
            watched: false,
            parent_watched: false,
            subs: std::collections::BTreeSet::new(),
        }
    }

    fn arm(&mut self, debouncer: &mut notify_debouncer_mini::Debouncer<impl notify::Watcher>) {
        rewatch_agent_home(
            debouncer,
            &self.dir,
            "accounts dir",
            &mut self.watched,
            &mut self.parent_watched,
        );
        self.arm_subs(debouncer);
    }

    /// 按盘上此刻挂各号目录：新出现的挂上，不在了的摘掉（符号链接不跟）。
    fn arm_subs(&mut self, debouncer: &mut notify_debouncer_mini::Debouncer<impl notify::Watcher>) {
        let now: std::collections::BTreeSet<PathBuf> = Ro::read_dir(&self.dir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default();
        for gone in self.subs.difference(&now) {
            let _ = debouncer.watcher().unwatch(gone);
        }
        let fresh: Vec<PathBuf> = now.difference(&self.subs).cloned().collect();
        self.subs.retain(|p| now.contains(p));
        for d in fresh {
            match debouncer.watcher().watch(&d, RecursiveMode::NonRecursive) {
                Ok(()) => {
                    self.subs.insert(d);
                }
                Err(e) => tracing::warn!("监视账号目录 {} 失败: {e}", d.display()),
            }
        }
    }

    /// 一个事件路径 ⇒ 要不要发「清单可能变了」：账号目录自己出现 / 消失（重挂）· 某个号的凭据文件动了 ⇒ `true`；
    /// 账号目录里多了 / 少了一项 ⇒ 重挂各号目录（清单本身动没动由调用方看 manifest）。
    fn on_path(
        &mut self,
        debouncer: &mut notify_debouncer_mini::Debouncer<impl notify::Watcher>,
        p: &Path,
    ) -> bool {
        if p == self.dir.as_path() {
            rewatch_agent_home(
                debouncer,
                &self.dir,
                "accounts dir",
                &mut self.watched,
                &mut self.parent_watched,
            );
            self.arm_subs(debouncer);
            return true;
        }
        if p.parent() == Some(self.dir.as_path()) {
            self.arm_subs(debouncer);
            return false;
        }
        credentials_touched(p, &self.dir)
    }
}

/// `p` 是 `<账号目录>/<某号>/` 下那份凭据文件（文件名与账号清单判「已登录」用的同一个常量）。
fn credentials_touched(p: &Path, accts_dir: &Path) -> bool {
    p.file_name()
        .is_some_and(|n| n == crate::observe::accounts_query::CREDENTIALS_FILE)
        && p.parent().and_then(Path::parent) == Some(accts_dir)
}

/// `agent_home` · `projects/` · `sessions/` 三道耳朵：起步 [`Self::arm`]，之后每个事件路径交 [`Self::on_path`]。
/// 抽出来是为了真 inotify 判据与 `watch_loop` 走同一段代码。
struct HomeEars {
    agent_home: PathBuf,
    projects: PathBuf,
    sessions: PathBuf,
    /// `<agent 家>/tasks/`（递归）：会话的任务清单变了 ⇒ 一帧 `changed {tasks, key: sid}`。
    tasks: PathBuf,
    tasks_watched: bool,
    /// `agent_home` **本身**此刻挂上了没有。
    home_watched: bool,
    /// `agent_home` 不在时退一层挂的那道（它的上一层）挂过没有。挂上之后不摘（[`rewatch_agent_home`] 头注）。
    parent_watched: bool,
    projects_watched: bool,
    /// `sessions/` 的「**当前这个 inode** 我挂上了没有」。事件循环里靠它决定要不要重挂。
    sessions_watched: bool,
}

impl HomeEars {
    fn new(agent_home: &Path, projects: &Path, sessions: &Path) -> Self {
        HomeEars {
            agent_home: agent_home.to_path_buf(),
            projects: projects.to_path_buf(),
            sessions: sessions.to_path_buf(),
            tasks: crate::observe::tasks_query::tasks_root(agent_home),
            tasks_watched: false,
            home_watched: false,
            parent_watched: false,
            projects_watched: false,
            sessions_watched: false,
        }
    }

    /// 起步挂一次：`agent_home`（不在 ⇒ 它的上一层）· `projects/`（可重入挂法）· `sessions/`。
    fn arm(&mut self, debouncer: &mut notify_debouncer_mini::Debouncer<impl notify::Watcher>) {
        // ★★ `P0b-Y2`：**监视 `agent_home` 本身** —— 这是「子目录出现/被换掉」的唯一耳朵。
        //
        // inotify 的 watch 绑在 **inode** 上，不是路径上。`sessions/` 被 `rm -rf` 再 `mkdir`
        // 之后是**另一个 inode**，旧 watch 还挂在那个已删的 inode 上 ⇒ 新目录里发生什么都听不见，
        // **而且不会有任何错误**（backend 活着、不吭声）。
        // 监视父目录之后，`sessions` 的创建/删除会作为**父目录里的一个事件**送到，我们据此重挂。
        rewatch_agent_home(
            debouncer,
            &self.agent_home,
            "agent home",
            &mut self.home_watched,
            &mut self.parent_watched,
        );
        // ★★ `projects/` 也走**可重入**挂法：它是 jsonl 的来源，
        //    被换 inode 之后**行帧再也不来**（实测：删掉重建后写入，line 帧停在 1）。
        //    这是 `sessions/`（第十拍）与 socket 目录（第二十二拍）之后的**同族第三个**。
        rewatch_dir(
            debouncer,
            &self.projects,
            &mut self.projects_watched,
            RecursiveMode::Recursive,
        );
        // `tasks/` 同一套可重入挂法（不在 ⇒ 等它作为 `agent_home` 里的一个事件出现再挂）。
        rewatch_dir(
            debouncer,
            &self.tasks,
            &mut self.tasks_watched,
            RecursiveMode::Recursive,
        );
        // Watch sessions (flat) for PID.json add/remove. 起步挂一次，之后归 `rewatch_sessions`。
        if self.sessions.is_dir() {
            match debouncer
                .watcher()
                .watch(&self.sessions, RecursiveMode::NonRecursive)
            {
                Ok(()) => self.sessions_watched = true,
                Err(e) => tracing::error!("watch failed for {}: {e}", self.sessions.display()),
            }
        } else if self.home_watched {
            // 它出现时是 `agent_home` 里的一个事件 ⇒ `on_path` 重挂 ＋ 重扫（原来那句「不会再重试」是假话）。
            tracing::warn!(
                "sessions 目录还没有：{} —— 出现时自动挂上",
                self.sessions.display()
            );
        }
        // `agent_home` 自己不在那一形，`rewatch_agent_home` 已经说过了（挂没挂上它的上一层各一句）。
    }

    /// 一个事件路径：是这三个目录之一就按**盘上此刻的样子**重挂（不看事件类型 —— notify 会合并事件，
    /// 「删了又建」很可能只到一个事件，靠 kind 去分辨是猜）。
    fn on_path(
        &mut self,
        debouncer: &mut notify_debouncer_mini::Debouncer<impl notify::Watcher>,
        p: &Path,
        state: &mut ReaderState,
        sink: &mut FrameSink,
    ) {
        // `agent_home` 刚出现 ⇒ 挂上它本身，再把它下面此刻已在的两个目录挂上（`sessions/` 带重扫）。
        if p == self.agent_home.as_path()
            && rewatch_agent_home(
                debouncer,
                &self.agent_home,
                "agent home",
                &mut self.home_watched,
                &mut self.parent_watched,
            )
        {
            rewatch_dir(
                debouncer,
                &self.projects,
                &mut self.projects_watched,
                RecursiveMode::Recursive,
            );
            rewatch_dir(
                debouncer,
                &self.tasks,
                &mut self.tasks_watched,
                RecursiveMode::Recursive,
            );
            rewatch_sessions(
                debouncer,
                &self.sessions,
                &mut self.sessions_watched,
                state,
                sink,
            );
        }
        // ★ `projects/` 换 inode 时也要重挂（同族第三个，见 `rewatch_dir` 头注）。
        if p == self.projects.as_path() {
            rewatch_dir(
                debouncer,
                &self.projects,
                &mut self.projects_watched,
                RecursiveMode::Recursive,
            );
        }
        // `tasks/` 刚出现 / 换了 inode ⇒ 重挂（同族第四个）。
        if p == self.tasks.as_path() {
            rewatch_dir(
                debouncer,
                &self.tasks,
                &mut self.tasks_watched,
                RecursiveMode::Recursive,
            );
        }
        // ★★ `P0b-Y2`：**`sessions/` 换了 inode 或刚出现 ⇒ 重挂 + 重扫。**
        if p == self.sessions.as_path() {
            rewatch_sessions(
                debouncer,
                &self.sessions,
                &mut self.sessions_watched,
                state,
                sink,
            );
        }
    }
}

/// ★★ `P0b-Y2` 第十六拍：**tmux socket 目录的推算规则**（零 server 时唯一的耳朵）。
///
/// # 病（`#60` 的根因）
///
/// `P5` 删掉 8s ticker 之后 backend **零定时器**，之后每一拍都靠事件。而两条唤醒路
/// **都以「已经见过 server」为前提**：tmux hook 是**观测到 server 那一刻**才装的；
/// socket 目录的 inotify 路径是从 `probe.socket_path` 里拿的 —— 没 server 就没有路径。
///
/// ⇒ **backend 起得比 tmux server 早 = 永远不再探**。08-13 全链实测：`tmux_sessions` 帧
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
    //    实测：删掉目录、再起一个新 tmux server ⇒ backend **一帧都收不到**（`sid-two` 命中 0）。
    //    ⇒ `*watched` 必须跟着盘上的事实走，否则下面那个 `if *watched` 会永远短路。
    if !sock_dir.is_dir() {
        if *watched {
            tracing::info!(
                "tmux socket 目录消失了 {} —— 解除记账，等它回来再挂",
                sock_dir.display()
            );
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

    /// 接上一条流连接：回它的出方向通道（这台的帧从这里来；先来一份此刻的样子）与它在这台 watcher 上的那一份（丢了 ＝ 走了）。
    /// 这条连接要的时区（`wants.tz`）给按连接现写的那几帧用（任务清单变了那一帧的体）。
    pub fn attach(&self, wants: &crate::StreamWants) -> (mpsc::Receiver<Frame>, Attachment) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = mpsc::channel::<Frame>(CHANNEL_CAPACITY);
        let _ = self
            .0
            .send(WatchEvent::Join(Sub::new(id, tx, wants.tz.clone())));
        (
            rx,
            Attachment(WatchHandle {
                id,
                tx: self.0.clone(),
            }),
        )
    }
}

/// 一条流连接在这台 watcher 上的那一份：丢了 ⇒ watcher 把它摘掉（它的通道 · 名单 · 欠的丢帧账一起走）。
pub struct Attachment(WatchHandle);

impl Attachment {
    /// 这条连接报「在看哪几个」的口（可以有好几份，丢了不算走）。
    pub fn handle(&self) -> WatchHandle {
        self.0.clone()
    }
}

impl Drop for Attachment {
    fn drop(&mut self) {
        let _ = self.0.tx.send(WatchEvent::Leave(self.0.id));
    }
}

/// 一条流连接报名单的口（[`Attachment::handle`]）。
#[derive(Clone)]
pub struct WatchHandle {
    id: u64,
    tx: std::sync::mpsc::Sender<WatchEvent>,
}

impl WatchHandle {
    /// 这条流此刻在看哪几个会话（整份换）；回刚进名单的那几个从第几行起上流。watcher 已停 ⇒ 回的那一头直接断。
    pub fn watch(
        &self,
        ask: crate::stream::wire::WatchAsk,
    ) -> tokio::sync::oneshot::Receiver<Vec<crate::stream::wire::WatchFrom>> {
        let (done, rx) = tokio::sync::oneshot::channel::<Vec<crate::stream::wire::WatchFrom>>();
        let _ = self.tx.send(WatchEvent::Watch {
            sub: self.id,
            ask,
            done,
        });
        rx
    }
}

/// 起这台的 watcher（**一个进程一份**：本机远端、stdio 与常驻都只起这一份；各条流连接经 [`WatcherPoke::attach`] 接上来）。
/// 回戳它的句柄 ＋ 这台的运行簿（整台一本：watcher 写、各连接的流归位读）。
/// `own`：写自家状态的句柄（启动时造的那一份）—— 认起会话便条、记 `sid → 号` 经它写。
pub fn spawn(
    agent_home: PathBuf,
    own: crate::platform::fs::Own,
    reactions: Reactions,
) -> (WatcherPoke, std::sync::Arc<crate::observe::runs::RunBook>) {
    let book = std::sync::Arc::new(crate::observe::runs::RunBook::default());
    // P4：事件 channel 从 `watch_loop` 内部**上提到这里**造 —— 因为 poke 句柄必须在线程
    // 起来之前就能交给 `main`。**仍然只有这一条 channel**（账本第 1 行不许再开第二条）：
    // 交出去的是同一个 sender 的 clone，`watch_loop` 收的是同一条的 receiver。
    let (events_tx, events_rx) = std::sync::mpsc::channel::<WatchEvent>();
    let poke = WatcherPoke(events_tx.clone());
    // 登记在起线程之前：刚 spawn 完的那一刻来的 SIGUSR1 / `resync` 也够得着它。
    let me = live_enter(events_tx.clone());
    let b = book.clone();
    std::thread::Builder::new()
        .name("jsonl-watcher".into())
        .spawn(move || {
            watch_loop(agent_home, b, events_tx, events_rx, own, reactions);
            live_leave(me);
        })
        .expect("spawn jsonl-watcher thread");
    (poke, book)
}

/// 此刻在跑的 watcher（一个进程一份，见 [`spawn`]；判据在同一进程里会起好几份）—— **唯一的名单**：
/// SIGUSR1（[`poke_all`]）与 `resync` 都按它找人。`spawn` 登记、`watch_loop` 返回即摘
/// ⇒ 不会去 poke 一个已经退掉的 watcher（`K-P1`：那不报错，它只是再也不响应 tmux hook）。
static LIVE: std::sync::Mutex<Vec<(u64, std::sync::mpsc::Sender<WatchEvent>)>> =
    std::sync::Mutex::new(Vec::new());

fn live_enter(tx: std::sync::mpsc::Sender<WatchEvent>) -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    LIVE.lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((id, tx));
    id
}

fn live_leave(id: u64) {
    LIVE.lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(k, _)| *k != id);
}

/// **P4：SIGUSR1 = 「tmux 那边有事，赶紧重探一次」** —— 名单上每一份 watcher 都戳一下（语义同 [`WatcherPoke::poke`]）。
pub fn poke_all() {
    for (_, w) in LIVE.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        let _ = w.send(WatchEvent::Poke);
    }
}

/// **手动对齐**：每一份在跑的 watcher 都做一次与起步同一套的对齐，等它们都做完。
/// 回（差异，几份 watcher 答了）。增删取各份最大（每份看的是同一台机器），标签写入相加（只有第一份真写），补读行数相加（各份交的是各自的帧）。
/// 中途退掉的那份丢了 `done` ⇒ 这里不会挂住。
pub(crate) fn resync(only: Option<&str>) -> (Reconciled, usize) {
    let (tx, rx) = std::sync::mpsc::channel::<Reconciled>();
    let live: Vec<_> = LIVE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|(_, w)| w.clone())
        .collect();
    for w in &live {
        let _ = w.send(WatchEvent::Resync {
            only: only.map(str::to_string),
            done: tx.clone(),
        });
    }
    drop(tx);
    let mut n = 0;
    let sum = rx.iter().fold(Reconciled::default(), |acc, r| {
        n += 1;
        Reconciled {
            added: acc.added.max(r.added),
            removed: acc.removed.max(r.removed),
            retagged: acc.retagged + r.retagged,
            caught_up: acc.caught_up + r.caught_up,
        }
    });
    (sum, n)
}

/// 挂全部耳朵：`agent_home` / `projects/` / `sessions/`（[`HomeEars::arm`]）· tmux socket 目录 · 账号目录。
/// 起步一次；「重新对齐」再调同一个（各挂法都先摘再挂，重挂无害、漏挂新 inode 致命）。
fn arm_ears(
    debouncer: &mut notify_debouncer_mini::Debouncer<impl notify::Watcher>,
    ears: &mut HomeEars,
    sock_dir: &Path,
    sock_dir_watched: &mut bool,
    accounts_ear: Option<&mut AccountsEar>,
) {
    ears.arm(debouncer);
    // ★★ `P0b-Y2` 第十六拍：**零 server 时也要有耳朵** —— 监视 tmux socket 目录本身。
    // 目录不在（本机从没起过 tmux）⇒ 退一层监视它的父，等目录被创建出来。
    // 两种情况都只当「该重新探一次」的触发器，绝不拿文件存在性判活（沿用 P3 的既定纪律）。
    if let Some(parent) = sock_dir.parent() {
        if parent.is_dir() {
            if let Err(e) = debouncer
                .watcher()
                .watch(parent, RecursiveMode::NonRecursive)
            {
                tracing::warn!("监视 {} 失败: {e}", parent.display());
            }
        }
    }
    watch_sock_dir_if_present(debouncer, sock_dir, sock_dir_watched);
    // **账号清单变了 ⇒ 一帧 `changed {accounts}`**。监视 manifest 所在目录
    // （NonRecursive；写 manifest 常是「写临时文件再 rename」，盯文件本身会在 rename 之后失聪）。
    // 目录起步不在 / 被删重建 ⇒ 由 `AccountsEar` 挂上一层等它、出现时重挂（原先这里失聪）。
    if let Some(ear) = accounts_ear {
        ear.arm(debouncer);
    }
}

/// The reader half: initial walkdir scan, then the live debouncer loop.
///
/// Runs on its own OS thread. `tx` is the bounded sender; it is wrapped in a
/// [`FrameSink`] whose [`FrameSink::send`] never blocks the notify callback and
/// turns dropped frames into an [`Frame::Overflow`] signal (#32).
fn watch_loop(
    agent_home: PathBuf,
    book: std::sync::Arc<crate::observe::runs::RunBook>,
    events_tx: std::sync::mpsc::Sender<WatchEvent>,
    events_rx: std::sync::mpsc::Receiver<WatchEvent>,
    own: crate::platform::fs::Own,
    reactions: Reactions,
) {
    // 记录树的根与 pidfile 目录问注册表里后端盯着的那一家（`RecordFace.tree` · `LocalFace`）；没有那一家 ⇒ 不盯。
    let (Some(projects), Some(local)) = (
        crate::agents::records_root(&agent_home),
        crate::agents::tree_local_face(),
    ) else {
        tracing::error!("no agent here keeps a record tree with a local layout; nothing to watch");
        return;
    };
    let sessions = (local.pidfile_dir)(&agent_home);
    // 账号 manifest（「账号清单变了」一帧）。
    let accounts_manifest = crate::observe::accounts_query::default_manifest_path();

    let mut state = ReaderState::new(projects.clone());
    // 别的那几家（记录不住记录树、判活认写者）：一并跟上。
    for f in crate::agents::followed(&agent_home) {
        state.follow(&f);
    }
    // 「开着即活」那几家的记录根：行由 debouncer 那道耳朵读（与记录树同一条路），谁在写由 `writers` 那道耳朵报。
    let writer_roots: Vec<PathBuf> = state
        .roots
        .iter()
        .filter(|r| r.by_writer)
        .map(|r| r.root.clone())
        .collect();
    state.skeletons = Some(crate::observe::skeleton::Skeletons::new(own.clone()));
    state.launch = Some((reactions, own));
    // 这台的会话注解（改过的名）：起步读一次，那份文件动了再读（耳朵见下面 `notes_ear`）。读不了 ⇒ 当没有改过名。
    let notes_path = crate::history::history_annotations::path();
    state.annotations = annotations_table();
    // 运行簿：整台一本，与各连接的流归位共用（流按它定归哪个运行）。
    state.set_book(book);
    // 预读交线程池（各会话并行、上限按 CPU 数；同一会话内部照顺序读）：做完经事件通道交回本线程装回（[`primed`]）。
    state.pool = Some(PrimePool::start(events_tx.clone()));
    // All frames go out through a FrameSink: 整台一份会话账本 ＋ 每条连接一份有界通道（各自记丢帧、各自的名单）。
    // 连接经 `Join` / `Leave` 进出；没有连接时帧照算、发不出去（会话账本 · 打标照常）。Never blocks this reader.
    let mut sink = FrameSink::hub();

    // P2：**唯一**的事件 channel（账本第 1 行最终形态）。notify / pidfd / tmux 全走它。
    //
    // **刻意建在 Phase 1 之前**：`process_session_added` 会顺手挂 pidfd 看守，而 Phase 1 的
    // 初始扫描就在调它。若把 channel 建在 Phase 2（本功能初版就是这么写的、被 clippy 的
    // 「field `start` is never read」间接暴露），**backend 启动时就活着的会话会一个看守都没有**
    // ——而原先那条 2s 判活轮询是覆盖它们的 ⇒ 那是回归。Phase 1 期间发出的 `PidDied` 只是
    // 在 channel 里排队，主循环起来后照常消费。
    // P4：channel 已由 `spawn` 造好传进来（poke 句柄要在线程起来前就交出去）。
    // **顺序仍然关键**（P2 那个静默回归的教训）：`state.events_tx` 必须在 Phase 1 之前设好，
    // 否则后端启动时就活着的会话一个 pidfd 看守都拿不到。
    state.events_tx = Some(events_tx.clone());

    // --- 先挂耳朵（Phase 2 的那几道），再做 Phase 1 的初扫 ---
    // 🔴 顺序是承重的（10-07 修）：原先先扫 `sessions/`、再建 debouncer 挂耳朵 ⇒ 扫完到挂上之间落下的
    //   pidfile 两头都看不见，那条会话一直不报（之后的记录文件也被「只认活会话」筛掉），死了也不报。
    //   后端起来那一刻正在起的 claude 就落在这个窗里（e2e `resume-frames` 间歇红的根因）。
    //   ⇒ 先挂：挂上之后落下的由事件接（扫的过程中来的事件在 channel 里排队，初扫完再消费；
    //   `process_session_added` 幂等，`process_session_removed` 只摘在跟的 ⇒ 排队的那几件重复 / 落空都不出帧）；
    //   挂上之前落下的由初扫接。`sessions_replayed` 仍排在初扫的全部 `session_added` 之后。
    //
    // Phase 1 mirrors the LOCAL watcher's `active_filter` (`session_map.is_session_active`):
    // only stream sessions whose PID is alive (sessions/<PID>.json + /proc/<pid>).
    // We scan sessions/ FIRST to build the active set; process_session_added marks
    // the sid active and rescans its jsonl so an already-running session snapshots
    // on startup. We deliberately do NOT walk projects/ unconditionally — pulling
    // every historical jsonl as a Tab is the bug this fixes; browsing history is
    // the Ctrl+H history browser's job (Phase 1 for remote).
    let sock_dir = tmux_socket_dir();
    let armed = arm_then_scan(&sessions, &mut state, &mut sink, || {
        let mut debouncer = match new_debouncer(
            Duration::from_millis(DEBOUNCE_MS),
            DebouncerSink(events_tx.clone()),
        ) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("debouncer init failed: {e}");
                return None;
            }
        };
        // 三道耳朵（`agent_home` 不在 ⇒ 先挂它的上一层）＋ tmux socket 目录 ＋ 账号目录：起步与 `resync` 都经 [`arm_ears`]。
        let mut ears = HomeEars::new(&agent_home, &projects, &sessions);
        // `sock_dir_watched` = 目录**本身**挂上了没有（没挂上时 `arm_ears` 退一层监视它的父）。
        let mut sock_dir_watched = false;
        let mut accounts_ear = accounts_manifest.parent().map(AccountsEar::new);
        let mut profiles_ear = crate::platform::paths::home_dir()
            .and_then(|h| ProfilesEar::new(h.join(relay_route_core::PROFILES_REL)));
        if let Some(ear) = profiles_ear.as_mut() {
            ear.arm(&mut debouncer);
        }
        // 会话注解那份文件（改过的名）：与配置文件同一种耳朵（盯它所在的目录）。
        let mut notes_ear = notes_path.clone().and_then(ProfilesEar::new);
        if let Some(ear) = notes_ear.as_mut() {
            ear.arm(&mut debouncer);
        }
        // 「开着即活」那几家：记录根挂上 debouncer（读行；根还不在 ⇒ 等写者那道耳朵报它出现时再挂）＋ 写者那道耳朵。
        let mut writer_watch: Vec<(PathBuf, bool)> =
            writer_roots.iter().map(|r| (r.clone(), false)).collect();
        for (root, watched) in &mut writer_watch {
            rewatch_dir(&mut debouncer, root, watched, RecursiveMode::Recursive);
        }
        let writer_ears: Vec<crate::platform::writers::OpenEar> = writer_roots
            .iter()
            .filter_map(|r| {
                let tx = events_tx.clone();
                crate::platform::writers::watch_opens(r, move |b| {
                    tx.send(WatchEvent::Writers(b)).is_ok()
                })
            })
            .collect();
        arm_ears(
            &mut debouncer,
            &mut ears,
            &sock_dir,
            &mut sock_dir_watched,
            accounts_ear.as_mut(),
        );
        Some((
            debouncer,
            ears,
            sock_dir_watched,
            accounts_ear,
            (profiles_ear, notes_ear),
            (writer_watch, writer_ears),
        ))
    });
    // --- Phase 2: live watch. ---
    let Some((
        mut debouncer,
        mut ears,
        mut sock_dir_watched,
        mut accounts_ear,
        (mut profiles_ear, mut notes_ear),
        (mut writer_watch, _writer_ears),
    )) = armed
    else {
        return;
    };
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

    // P2：**无超时** `recv()`——本循环没有节拍（轮询 A 已由 pidfd 取代）。唯一的期限：有在跑的子运行时，至多等到
    // 最早那个「久未动静」的时刻（`runs::next_event`），到点回循环头判一次、重算期限；没有在跑的 ⇒ 照旧无超时等。
    // 事件源：notify（经 DebouncerSink）· pidfd 看守线程 · tmux 探测/节拍线程。
    // **P3/P4 只往 `WatchEvent` 加变体 + 加发送方，不许再挂独立定时器。**
    // 所有发送端都掉了就结束（等价于原来的 Disconnected 分支）。
    loop {
        // 每一拍（事件之后 · 到点）都按此刻把全部会话的「久未动静」判一遍，表变了的才发帧。
        for f in state.book.due_frames(crate::common::time::now()) {
            sink.send(f);
        }
        let event = match crate::observe::runs::next_event(
            &events_rx,
            state.book.next_due(),
            crate::common::time::now(),
        ) {
            Ok(Some(e)) => e,
            Ok(None) => continue,
            Err(_) => break,
        };
        match event {
            WatchEvent::Notify(Ok(events)) => {
                // 账号目录自己出现 / 消失 ⇒ 先重挂；它也算「清单可能变了」。
                let mut dir_moved = false;
                if let Some(ear) = accounts_ear.as_mut() {
                    for ev in &events {
                        dir_moved |= ear.on_path(&mut debouncer, &ev.path);
                    }
                }
                // 一批里 manifest 动了几次都只报一帧（批内合并；下一批再动再报）。
                if manifest_touched(
                    events.iter().map(|ev| ev.path.as_path()),
                    &accounts_manifest,
                ) || dir_moved
                {
                    sink.send(Frame::changed(Topic::Accounts, None, None, None));
                    // 账号清单动了 ⇒ 需登录的那几个号可能变了。
                    crate::accounts::sign_in::ring_manual();
                }
                // 哪个号记「MCP 要登录」的那份文件翻了（那一家连它碰到要登录 · 登完自己删掉那一条）⇒ 需手动清单可能变了。
                if needs_login_touched(
                    events.iter().map(|ev| ev.path.as_path()),
                    accounts_manifest.parent(),
                    &agent_home,
                    &crate::agents::mcp_needs_login_files(),
                ) {
                    crate::accounts::sign_in::ring_manual();
                }
                // 配置文件动了 ⇒ 一帧 `changed {profiles}`（批内合并）。
                if let Some(ear) = profiles_ear.as_mut() {
                    if ear.touched(&mut debouncer, events.iter().map(|ev| ev.path.as_path())) {
                        sink.send(Frame::changed(Topic::Profiles, None, None, None));
                    }
                }
                // 注解动了 ⇒ 重读，改了名的活会话各一帧 `session_status`。
                if let Some(ear) = notes_ear.as_mut() {
                    if ear.touched(&mut debouncer, events.iter().map(|ev| ev.path.as_path())) {
                        state.reload_annotations(annotations_table(), &mut sink);
                    }
                }
                // 任务目录里的动静 ⇒ 每个动过的会话一帧 `changed {tasks, key: sid}`（批内合并）。
                for sid in tasks_touched(events.iter().map(|ev| ev.path.as_path()), &ears.tasks) {
                    // 体按各连接看的那一台的时区现写。
                    sink.send_each(|tz| {
                        let body = crate::stream::topic_hook::body(Topic::Tasks, Some(&sid), tz);
                        Frame::changed(Topic::Tasks, Some(sid.clone()), None, body)
                    });
                }
                // 记录文件的表先跟上这一批（同一批里 pidfile 先到、记录文件后到时，宣告也查得到它）。
                for ev in &events {
                    note_jsonl(&mut state, &ev.path);
                }
                // 子运行记录动过的会话（批内合并：一批里同一个会话的运行表只发一帧）。
                let mut runs_touched = std::collections::BTreeSet::new();
                for ev in events {
                    let p = ev.path.as_path();
                    // ★★ `P0b-Y2`：**`sessions/` 换了 inode 或刚出现 ⇒ 重挂 + 重扫**；`projects/` 同族；
                    // `agent_home` 自己刚出现 ⇒ 挂上它、再挂它下面那两个。
                    // 触发面刻意宽：这三个路径上任何动静都来这儿判一次（判的是盘上此刻的样子）。
                    ears.on_path(&mut debouncer, p, &mut state, &mut sink);
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
                    // ⚠ 去掉 `&& !sock_dir_watched`：目录**被删掉再重建**时
                    //   `sock_dir_watched` 仍是 true ⇒ 那个条件会把重挂整个短路掉，
                    //   于是新 inode 永远没人听（实测：新起的 server 一帧都收不到）。
                    if p == sock_dir.as_path() || p.parent() == Some(sock_dir.as_path()) {
                        watch_sock_dir_if_present(&mut debouncer, &sock_dir, &mut sock_dir_watched);
                    }
                    if watched_socket.as_deref() == Some(p) || in_sock_dir {
                        start_tmux_probe(&mut tmux_inflight, &events_tx, &mut sink);
                        continue;
                    }
                    // 子运行的记录（适配层说住哪）：读新行进运行簿、表变了发一帧；不发 `line`（它们不属于主时间线）。
                    let child = state
                        .roots
                        .iter_mut()
                        .find(|r| p.starts_with(&r.root))
                        .and_then(|r| r.runs.on_path(p));
                    if let Some((sid, changed)) = child {
                        if changed {
                            runs_touched.insert(sid);
                        }
                    } else if state.root_of(p).is_some() {
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
                for sid in runs_touched {
                    sink.send(state.runs_frame(&sid));
                }
                // ★ S0：一批事件处理完再决定探不探（批内多次漂移只探一次；`tmux_inflight`
                // 再兜一层去重）。**注意这只让快照更新鲜，不是本 bug 的修复** ——
                // 真正的修复是 `RemovalCause::Superseded`（让 monitor 不必依赖这份快照）。
                //
                // 🔴 上面这段原先还有一句：「`@ccm_sid` 由 `shared/ccm` 的
                // 1 秒 poller 回填，这次探测很可能仍抓到**旧** tag」。**今天两半都不成立**：
                //   · 那个 poller 08-14 被整条删掉（`0085d0d`），事实键改由后端自己写
                //     （`control::identity_tag::tag`，就在本文件 `process_session_added` 里）；
                //   · 而 `tag()` 是**同步**跑完的、在重探线程 spawn 之前 ⇒ 这次探测
                //     **必然**抓到**新** tag，不是「很可能仍是旧的」。
                // ⚠ 留这条订正是因为那句话不是无害的陈账：`tests/e2e/graylight-backend-frames.sh`
                // 就是照着它写的（「本 fixture 没有那个 poller ⇒ 标签恒为最初 sid」），
                // 于是那套夹具从 08-14 起一格红一格恒真，而云端从 08-05 起没跑到过这一步
                // ⇒ 09-09 才被逮到。**一句过期的注释，教出了一套错的判据。**
                if sid_drifted {
                    // 只在**真的起了**探测时才清标志。在途时清掉会丢信号——那次在途的探测是
                    // 在漂移**之前**发起的，它带回来的快照照样是旧的。留着标志，下一个事件
                    // 会补上一次（代价只是晚一拍；这本来就只是"更新鲜"，不是本 bug 的修复）。
                    if start_tmux_probe(&mut tmux_inflight, &events_tx, &mut sink) {
                        sid_drifted = false;
                    }
                }
            }
            WatchEvent::Notify(Err(errs)) => tracing::warn!("debouncer failed: {errs:?}"),
            // 「开着即活」那几家：根刚出现 ⇒ 挂上读行那道耳朵；这一批候选扫一趟进程表对上账。
            WatchEvent::Writers(batch) => {
                for (root, watched) in writer_watch.iter_mut().filter(|(_, w)| !*w) {
                    rewatch_dir(&mut debouncer, root, watched, RecursiveMode::Recursive);
                }
                on_writers(batch, &mut state, &mut sink);
            }
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
                start_tmux_probe(&mut tmux_inflight, &events_tx, &mut sink);
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
                        // 装不上不致命（退回定时探测），但会 warn ⇒ 不静默降级。装的那一半在控制面（[`Reactions::tmux_server_up`]）。
                        (reactions.tmux_server_up)();
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
                    tracing::info!("tmux 会话 {name} 已关闭（快照差分）");
                }
                // 四态观测只喂这条流的会话账本（成品帧由它发），快照本身不上线。
                sink.tmux(obs);
                // 探测结果到达 ⇒ 顺手对账身份标签（hook 不报选项变化）。
                // 真改了 ⇒ 刚发的那份快照里的 `@ccm_sid` 已过期 ⇒ 再探一次。标签打在各自的窗格上（一个会话里几个 claude
                // 互不覆盖）⇒ 下一次全是 AlreadyCurrent，不会连环。
                if retag_tracked(&state, None) > 0 {
                    start_tmux_probe(&mut tmux_inflight, &events_tx, &mut sink);
                }
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
                        tracing::info!("tmux 会话 {name} 随 server 退出关闭");
                    }
                    // server 死在此刻：这一份比任何在等裁的摘除都新。
                    sink.observing();
                    sink.tmux(TmuxObservation::NoServer);
                }
            }
            // 与起步同一套：耳朵重挂 · 账号清单重读（整机时）· pidfile 对表（顺手对账标签）· 重探 tmux。
            WatchEvent::Watch { sub, ask, done } => {
                let _ = done.send(watch_sessions(sub, ask, &mut state, &mut sink));
            }
            WatchEvent::Join(sub) => join(sub, &mut state, &mut sink),
            WatchEvent::Leave(id) => sink.leave(id),
            WatchEvent::Primed(done) => primed(done, &mut state, &mut sink),
            WatchEvent::Resync { only, done } => {
                if only.is_none() {
                    // 整机重对表：记录文件的表作废，下一次宣告整棵走一遍重建（事件可能丢过）。
                    state.sid_files = None;
                    arm_ears(
                        &mut debouncer,
                        &mut ears,
                        &sock_dir,
                        &mut sock_dir_watched,
                        accounts_ear.as_mut(),
                    );
                    if let Some(ear) = profiles_ear.as_mut() {
                        ear.arm(&mut debouncer);
                    }
                    if let Some(ear) = notes_ear.as_mut() {
                        ear.arm(&mut debouncer);
                    }
                    state.reload_annotations(annotations_table(), &mut sink);
                    sink.send(Frame::changed(Topic::Accounts, None, None, None));
                    // 账号清单动了 ⇒ 需登录的那几个号可能变了。
                    crate::accounts::sign_in::ring_manual();
                }
                let got = resync_sessions(&sessions, &mut state, &mut sink, only.as_deref());
                start_tmux_probe(&mut tmux_inflight, &events_tx, &mut sink);
                let _ = done.send(got);
            }
            WatchEvent::Shutdown => break,
        }
        // 有摘除了的会话在等一份「摘除之后才起」的 tmux 观测来裁去向 ⇒ 起一份（在途的那份是摘除之前起的，它回来再起）。
        if sink.awaits_observation() {
            start_tmux_probe(&mut tmux_inflight, &events_tx, &mut sink);
        }
    }
}

/// Reader-side bookkeeping shared across `process_*` calls.
///
/// Not behind a lock: the reader is single-threaded (one OS thread), so all
/// access is serialized by construction.
struct ReaderState {
    /// 「sid → 它的记录文件」那张表：第一次宣告会话时整棵 `projects/` 走一遍建起来，之后跟着记录文件的事件改
    /// （[`note_jsonl`]）；整机重对表时作废（下一次宣告再建）。`None` = 还没建。
    sid_files: Option<HashMap<String, HashSet<PathBuf>>>,
    /// Per-file consumed byte offset, keyed by [`path_key`]. Reset to 0 on
    /// truncation; the seq lives separately in [`Self::seqs`] and is restarted
    /// together with it by [`process_jsonl`] (seq = 当前文件里的行号).
    offsets: HashMap<PathBuf, ReadCursor>,
    /// 每份 jsonl **已消费前缀的末尾那几个字节**（至多 [`TAIL_PROBE`]）。
    /// 续读之前核一遍：对不上 ⇒ 这份文件在游标之前被原地改写过（整份覆盖、长度没变短）⇒ 当截断办：从 0 重读并出声。
    /// 与 `offsets` 同键、同生同灭（丢游标时一起丢）。
    tails: HashMap<PathBuf, Vec<u8>>,
    /// 已经说过「记录文件不见了」的那几份（每次「在 → 不在」只说一次；再出现就摘掉）。
    gone: HashSet<PathBuf>,
    /// Per-file seq source. 只在「从 0 重读、且已先发出 `session_file_reread`」那一刻归零
    /// （[`process_jsonl`] · [`prime_file_cursor`]）⇒ seq 恒是当前文件内容里的行号。
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
    /// 且后端生命周期 ⊆ 一次 SSH 连接）。
    /// 已挂过 pidfd 看守的 **(pidfile 路径, pid, 进程启动时刻)**〔audit-0805 F11 / 报告 I-7〕。
    ///
    /// ⚠ 第三元 `start` 是 F11 补的。此前键是 `(路径, pid)` 两元 ——
    /// 「同路径换 pid」能重新挂（有测试钉着），**而「同路径、同 pid、不同进程实例」落进去重、
    /// 不再挂**。那恰好就是 **PID 复用**本身，也正是 `spawn_pid_watcher` 头注声称要处理的那格。
    /// 区分进程实例的东西（`starttime`）本来就在 `arm_pid_watcher` 的参数里，只是没进键。
    pid_watched: HashSet<(PathBuf, u32, Option<u64>)>,
    /// Fast membership for the active-session filter: sids currently streaming.
    /// Only sessions whose PID is alive on this host stream; historical jsonl is
    /// NOT pulled (that is the Ctrl+H history browser's job).
    active_sids: HashSet<String>,
    /// 这条流跟的那几家（[`Root`]）：家目录记录树那一家在 [`ReaderState::new`] 里就有；别的由 [`ReaderState::follow`] 加。
    roots: Vec<Root>,
    /// pidfile 那一路宣告的会话是哪一家的（家目录记录树那一家；没有 ⇒ 空串）。
    pidfile_kind: &'static str,
    /// 这条连接的运行簿（[`watch_loop`] 换成 `spawn` 交进来的那一本，[`ReaderState::set_book`]；夹具用自带的一本）。各家的子运行跟踪共用它。
    book: std::sync::Arc<crate::observe::runs::RunBook>,
    /// 每份活会话记录的链索引（主线外清单从它出；键同 `offsets`）。只记链上几个短串，不留正文。
    branches: HashMap<PathBuf, crate::agents::mainline::Chain>,
    /// 每份活会话记录的排队打字时刻表（`queued` 那条的 `at` 换成打字时刻；键同 `offsets`）。上界固定（`TypedTimes::CAP`）。
    /// 冷接那一趟不喂它（要逐行全解析）：打字在冷接之前、插进去在之后的那一句照用它自己的时刻。
    typed: HashMap<PathBuf, crate::agents::record::TypedTimes>,
    /// 认起会话便条、记账号：控制面那一手 ＋ 写句柄（便条与记录住它的数据目录那一根；`None` ⇒ 不认便条）。
    /// 默认 `None`、不进 `new` 的签名（夹具不碰真家目录）；生产由 [`watch_loop`] 注入启动时造的那一份。
    launch: Option<(Reactions, crate::platform::fs::Own)>,
    /// 这台的会话注解（在 monitor 里改的名住这里；叫会话什么的第一档）。默认空、不进 `new` 的签名；生产由 [`watch_loop`] 读、那份文件动了重读。
    annotations: crate::history::history_annotations::Table,
    /// 大记录的骨架落在哪（生产由 [`watch_loop`] 照启动时的写句柄装上；`None`（夹具）⇒ 不读也不落）。
    skeletons: Option<crate::observe::skeleton::Skeletons>,
    /// 预读的线程池（生产由 [`watch_loop`] 装上）。`None`（夹具）⇒ 同一个任务就地跑、就地装回（[`announce`]）。
    pool: Option<PrimePool>,
    /// 有骨架的大记录（键 ＝ 路径键）：实时读进来的接着记，读过门槛就照账上此刻的样子回写一份（[`write_back`]）。
    skel_live: HashMap<PathBuf, SkelLive>,
    /// 预读还没交回的会话 → 那一趟的代号（会话退休 / 重新宣告 ⇒ 旧的那一趟交回时认不上、丢掉）。
    priming: HashMap<String, u64>,
    next_prime: u64,
    /// 装回过的预读一共读了多少字节（判据数它：同一份记录接几条连接都只预读一次）。
    prime_bytes: u64,
    /// 每个宣告过的活会话那一帧 `session_added`（接上来的连接拿它的现值补一份，[`join`]）。
    added: HashMap<String, Frame>,
    /// 宣告时说的是「读记录中」的会话（此刻在干什么只有记录说、记录还没读完）：读完必说一次判出来的那一态（判不出 ⇒ 说不清）。
    reading: HashSet<String>,
}

impl ReaderState {
    /// 这台注解里给这条会话改的名。
    fn custom_title(&self, sid: &str) -> Option<String> {
        self.annotations
            .get(sid)
            .and_then(|e| e.custom_title.clone())
    }

    /// 此刻叫这条会话什么（[`crate::stream::wire::NameFace`]）；没记着它（没宣告过）⇒ 只凭 sid。
    fn name_face(&self, sid: &str) -> crate::stream::wire::NameFace {
        let facts = self
            .sessions
            .values()
            .find(|e| e.sid == sid)
            .map(|e| e.facts.clone())
            .unwrap_or_default();
        crate::stream::wire::NameFace::of(&facts)
    }

    /// 改这条会话叫什么的那几样（`change` 回真 = 改了）；名字真变了 ⇒ 一帧 `session_status`（活动那几格照记着的带全）。
    fn renamed(
        &mut self,
        sid: &str,
        mut change: impl FnMut(&mut crate::history::search_rules::LabelFacts) -> bool,
        sink: &mut FrameSink,
    ) {
        let before = self.name_face(sid);
        let mut status = None;
        for e in self.sessions.values_mut().filter(|e| e.sid == sid) {
            if change(&mut e.facts) {
                status = Some((e.activity, e.waiting_for.clone()));
            }
        }
        let Some((activity, waiting_for)) = status else {
            return;
        };
        let name_face = self.name_face(sid);
        if name_face != before {
            sink.send(Frame::SessionStatus {
                sid: sid.to_string(),
                face: crate::stream::wire::ActivityFace::of(activity),
                name_face,
                waiting_for,
                path: None,
            });
        }
    }

    /// 注解那份文件动了 ⇒ 换上新读的那张表，改了名的活会话各发一帧 `session_status`。
    fn reload_annotations(
        &mut self,
        table: crate::history::history_annotations::Table,
        sink: &mut FrameSink,
    ) {
        self.annotations = table;
        let sids: std::collections::BTreeSet<String> =
            self.sessions.values().map(|e| e.sid.clone()).collect();
        for sid in sids {
            let custom = self.custom_title(&sid);
            self.renamed(
                &sid,
                |f| {
                    let changed = f.custom != custom;
                    f.custom = custom.clone();
                    changed
                },
                sink,
            );
        }
    }

    fn new(projects: PathBuf) -> Self {
        let mut st = Self::bare();
        // 家目录记录树那一家（判活认 pidfile）：根就是 `projects`。
        if let Some(kind) = crate::agents::record_tree_kind() {
            if let Some(face) = crate::agents::record_face(kind) {
                st.pidfile_kind = kind;
                st.add_root(kind, projects, face, false);
            }
        }
        st
    }

    /// 跟上一家（[`crate::agents::followed`] 里的一条）；已在跟的那一家不重加。
    fn follow(&mut self, f: &crate::agents::Followed) {
        if self.roots.iter().any(|r| r.kind == f.kind) {
            return;
        }
        let by_writer = f.live == crate::agents::LiveBy::HeldOpen;
        // 写者那一路认的路径是进程表里读出来的（规范化过）：根也照同一个规范化（`platform::writers::absolute`）。
        let root = crate::platform::writers::absolute(&f.root);
        self.add_root(f.kind, root, f.face, by_writer);
    }

    fn add_root(
        &mut self,
        kind: &'static str,
        root: PathBuf,
        face: crate::agents::RecordFace,
        by_writer: bool,
    ) {
        let runs = crate::observe::runs::RunTrack::new(
            crate::agents::RunFaces::of(&face),
            self.book.clone(),
        );
        self.roots.push(Root {
            kind,
            root,
            face,
            by_writer,
            runs,
        });
    }

    /// 换上这条连接的运行簿（各家的子运行跟踪一起换）。
    fn set_book(&mut self, book: std::sync::Arc<crate::observe::runs::RunBook>) {
        for r in &mut self.roots {
            r.runs.book = book.clone();
        }
        self.book = book;
    }

    /// 这份路径是哪一家的一份会话记录（落在它的根下、形态也对）。
    fn root_of(&self, p: &Path) -> Option<usize> {
        self.roots
            .iter()
            .position(|r| p.starts_with(&r.root) && (r.face.is_session_file)(p))
    }

    /// 这份会话记录的 sid（那一家的文件命名）。
    fn sid_of(&self, p: &Path) -> Option<String> {
        let r = &self.roots[self.root_of(p)?];
        (r.face.sid)(p)
    }

    /// 这份路径是不是某一家「开着即活」的会话记录。
    fn writer_record(&self, p: &Path) -> bool {
        self.root_of(p).is_some_and(|i| self.roots[i].by_writer)
    }

    /// 这个会话此刻的运行表那一帧。
    fn runs_frame(&self, sid: &str) -> Frame {
        self.book.frame(sid)
    }

    /// 会话退休：还算在跑的子运行改成状态不明（表变了就回那一帧），各家的子运行跟踪一起忘掉它。
    fn retire_runs(&mut self, sid: &str) -> Option<Frame> {
        let f = self.book.retire(sid);
        for r in &mut self.roots {
            r.runs.forget(sid);
        }
        f
    }

    fn bare() -> Self {
        ReaderState {
            sid_files: None,
            offsets: HashMap::new(),
            tails: HashMap::new(),
            gone: HashSet::new(),
            seqs: SeqCounter::new(),
            sessions: HashMap::new(),
            events_tx: None,
            pid_watched: HashSet::new(),
            active_sids: HashSet::new(),
            roots: Vec::new(),
            pidfile_kind: "",
            book: std::sync::Arc::default(),
            launch: None,
            skeletons: None,
            skel_live: HashMap::new(),
            branches: HashMap::new(),
            typed: HashMap::new(),
            annotations: Default::default(),
            pool: None,
            priming: HashMap::new(),
            next_prime: 0,
            prime_bytes: 0,
            added: HashMap::new(),
            reading: HashSet::new(),
        }
    }
}

/// 流跟的一家在这条流上的那一份（[`crate::agents::Followed`]）：记录根 · 解释面 · 判活认不认写者 · 它的子运行跟踪。
struct Root {
    kind: &'static str,
    root: PathBuf,
    face: crate::agents::RecordFace,
    /// 判活认写者（没有 pidfile 的那一家，[`crate::agents::LiveBy::HeldOpen`]）。
    by_writer: bool,
    runs: crate::observe::runs::RunTrack,
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
/// **Residual limitation (#, by design)**: `start` is captured at add-time
/// and never persisted. A backend **restart** re-baselines `start` from the
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
    /// 此刻在干什么（名字变了要再发 `session_status`，那一帧带全）。
    activity: Option<crate::agents::SessionActivity>,
    /// 叫这条会话什么的那几样（[`crate::stream::wire::NameFace`]）：宣告时从清单缓存与注解取，之后跟着读到的标题记录 · 第一句 · 注解改。
    /// 不在看的会话行照读（只是不上流），这几样照记。
    facts: crate::history::search_rules::LabelFacts,
    /// 客户端手里这条会话的记录路径（宣告时那一份；之后记录出现 · 换了一份就跟上，并发一帧 `session_status` 带它）。
    path: Option<String>,
}

/// One line read out of a JSONL file, with its assigned per-file seq.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadLine {
    pub seq: u64,
    pub raw: String,
    /// backend-01（gap#2）：本行末尾（含 `\n`）的累计**原始字节** offset，逐字节对齐手机端 `LineFramer`
    /// （计 `\r`、含 `\n`、残行不计）。**在原始字节上算**（非解码后串），故非法 UTF-8/CRLF 不错。
    pub byte_offset: u64,
    /// 本行第一个字节在文件里的偏移（没有自己身份的记录拿它合成 id，`agents::line_id`）。
    pub start: u64,
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
/// Rules (the monitor copy this used to mirror was deleted by CF1 — these are the only ones):
///
/// - read from `cursor.consumed`, but only consume **complete lines** — bytes
///   up to and including the last `\n` in the new region. A torn tail without
///   a trailing `\n` (the CLI caught mid-write) stays in the file: it is
///   neither emitted nor skipped over, and `consumed` stops right before it,
///   so the next event re-reads it once completed (Batch4-F14; the old
///   behaviour emitted the half line — the record was then lost for good after
///   the JSON parse failure — and a torn multibyte tail decayed into U+FFFD).
///   A final line that is complete JSON but never gets its `\n` (writer killed
///   between the two writes) is handed out once the session retires
///   ([`catch_up_session`]), never while the writer is alive;
/// - **truncation**: judged against the high-water mark
///   (`len < cursor.seen_len`), so a rewrite landing inside a pending torn-tail
///   window `[consumed, seen_len)` is still caught → start over from byte 0;
/// - on truncation the byte cursor resets; this pure core does not touch the
///   seq counter — the production caller ([`process_jsonl`]) restarts it right
///   after announcing the re-read (seq = line number in the file now);
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
        // Truncation warn (INVARIANTS §25: re-reads hand out new seqs — must
        // leave a trace; silence made an old mis-folding bug near-impossible to
        // diagnose). len == 0 re-reads nothing, so stay quiet.
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
        // backend-01（gap#2）：**在原始字节上逐行切**（非先解码整段再 `.lines()`）——因为 `byte_offset` 必须是
        // 累计原始字节（对齐手机端 `LineFramer`：计 `\r`、含 `\n`），而解码后串的字节位在非法 UTF-8（U+FFFD 替换
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
                                         // raw = 内容解码 + 剥尾随 '\r'（对齐手机端：raw 无 CRLF/无尾 \n；但 offset **计** `\r`）。
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
                        start: start + pos as u64,
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

/// 游标旁记多少个字节的「末尾指纹」。
///
/// 它防的是**对不齐**：游标之前的内容被原地改写、长度变了（编辑器整份覆盖多了 / 少了几个字）⇒ 从旧偏移读会读到半行。
/// 任何改了前缀长度的改写都会让「游标之前那一截」整体平移 d 个字节；平移后仍逐字节相同，要求这一截以 d 为周期 ——
/// jsonl 每行尾巴上是 uuid / 时间戳（`"uuid":"…"` 36 个字 ＋ 引号），64 字节必然盖得住一整个，没有这种周期。
/// 取更多只是多读几个字节，取更少（< 36）就可能只落在行尾那几个常见的 `"}` 上。
/// ⚠ 买不到的（如实登记）：**长度一字不差**的原地改写（只改了已读过的几行里的同长字）⇒ 对得齐、后面照读，
///   那几行已经画出去的旧样子不会变（「看的，不是管的」）。
pub const TAIL_PROBE: u64 = crate::observe::skeleton::WITNESS;

/// 一次续读前看到的那一份。
enum Look {
    /// 盘上不在了（删了 / 改名走了）。
    Gone,
    /// 在，但这一趟开不了 / 核指纹那一下读不出（权限 · 出错）—— 静默跳过这一趟（游标不动）。读到一半出错的见 [`NewLines`]。
    Unreadable,
    /// 看过了、停在起读处：按块读到尾（[`NewLines`]）。
    Read(NewLines),
}

/// 只读新字节〔audit-0805 F04〕＋先认出「不在了 / 被截短 / 被原地改写」三形，**读之前**就判定，交回停在起读处的块读者
/// （[`NewLines`]，按块读到尾，不把那一截一次读进内存）。
///
/// - 截短：`metadata` 的长度 < 高水位 ⇒ 从 0 读、`reread = Truncated`、行号从 0 数。
/// - 续读：先读游标前 `k = min(TAIL_PROBE, consumed)` 字节与 `tail`（上一趟记下的）逐字节比；对不上 ⇒ 从 0 读、游标归零、
///   `reread = Rewritten`。`tail` 为 `None`（第一次读 / 刚丢过游标）⇒ 不核。
/// - ★ 这一趟看到的文件长度用「真读到多少」算（`metadata` 与读之间文件还在长；多读到的字节这一趟一起处理掉），见 [`NewLines::finish`]。
fn read_tail_from(path: &Path, cursor: ReadCursor, tail: Option<&[u8]>) -> Look {
    read_tail_in_blocks(path, cursor, tail, crate::common::append_read::READ_BLOCK)
}

/// [`read_tail_from`] 的本体（块多大是参数：判据拿小块验「按块扫 == 一整块扫」）。
fn read_tail_in_blocks(path: &Path, cursor: ReadCursor, tail: Option<&[u8]>, block: usize) -> Look {
    use std::io::{Read, Seek, SeekFrom};
    let meta_len = match Ro::metadata(path) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Look::Gone,
        Err(_) => return Look::Unreadable,
    };
    let Ok(mut f) = Ro::open(path) else {
        return Look::Unreadable;
    };
    let from_zero = |f: RoFile, from: ReadCursor, why: RereadWhy| {
        Look::Read(NewLines::new(f, block, 0, Vec::new(), from, Some(why)))
    };
    if meta_len < cursor.seen_len {
        // 截短：从 0 起、行号从 0 数（读的那一下文件又长回去了也照样从 0：这一趟已经判定）。
        if meta_len > 0 {
            tracing::warn!(
                "jsonl truncated (len {meta_len} < seen_len {}), full re-read with new seqs: {}",
                cursor.seen_len,
                path.display()
            );
        }
        return from_zero(f, cursor, RereadWhy::Truncated);
    }
    let consumed = cursor.consumed.min(meta_len);
    let k = TAIL_PROBE.min(consumed);
    let mut probe = vec![0u8; k as usize];
    // 读不满（读的那一下文件缩了）也算读不出：下一趟按 `metadata` 长度判截短。
    let read = f
        .seek(SeekFrom::Start(consumed - k))
        .and_then(|_| f.read_exact(&mut probe));
    if read.is_err() {
        return Look::Unreadable;
    }
    if tail.is_some_and(|t| t != probe.as_slice()) {
        return match f.seek(SeekFrom::Start(0)) {
            Ok(_) => from_zero(f, ReadCursor::default(), RereadWhy::Rewritten),
            Err(_) => Look::Unreadable,
        };
    }
    if f.seek(SeekFrom::Start(cursor.consumed)).is_err() {
        return Look::Unreadable;
    }
    Look::Read(NewLines::new(
        f,
        block,
        cursor.consumed,
        probe,
        cursor,
        None,
    ))
}

/// 一趟续读：从起读处按块读到尾，每块止于一个 `\n`，交给同一段扫描（[`read_new_lines_at`]）出行；
/// 末尾没写完的那截不交（留给下一趟）。手里最多 ≈ 一块 ＋ 正在读的那一行。
struct NewLines {
    blocks: crate::common::append_read::Blocks<RoFile>,
    /// 下一块在文件里的起点（起读处 ＋ 已交出的块）。
    at: u64,
    /// 起读处。
    start: u64,
    /// 这一趟接着的游标（被改写 ⇒ 归零）。
    from: ReadCursor,
    /// 这一趟要从 0 重读、以及为什么（读之前就判定了）。
    reread: Option<RereadWhy>,
    /// 新游标之前那 `TAIL_PROBE` 字节（滚动着记；起读处之前那几字节打底）。
    tail: Vec<u8>,
    /// 读到一半出错了：这一趟停在已交出的最后一块之后。
    broke: bool,
}

impl NewLines {
    fn new(
        f: RoFile,
        block: usize,
        start: u64,
        tail: Vec<u8>,
        from: ReadCursor,
        reread: Option<RereadWhy>,
    ) -> Self {
        Self {
            blocks: crate::common::append_read::Blocks::new(f, block),
            at: start,
            start,
            from,
            reread,
            tail,
            broke: false,
        }
    }

    /// 下一块里的行（号从 `seqs` 接着发）；读到尾 / 读出错 ⇒ `None`。
    fn next(&mut self, key: &str, seqs: &mut SeqCounter) -> Option<Vec<ReadLine>> {
        let whole = match self.blocks.next() {
            Ok(Some(b)) => b,
            Ok(None) => return None,
            Err(_) => {
                self.broke = true;
                return None;
            }
        };
        let at = self.at;
        let end = at + whole.len() as u64;
        // 每块当一段「读到这里为止」的完整区扫：起点就是这块的起点、没有高水位 ⇒ 号 · 字节位与整段扫的逐个相同。
        let here = ReadCursor {
            consumed: at,
            seen_len: 0,
        };
        let (lines, _) = read_new_lines_at(&whole, at, end, here, key, seqs);
        let keep = TAIL_PROBE as usize;
        if whole.len() >= keep {
            self.tail.clear();
            self.tail.extend_from_slice(&whole[whole.len() - keep..]);
        } else {
            self.tail.extend_from_slice(&whole);
            let over = self.tail.len().saturating_sub(keep);
            self.tail.drain(..over);
        }
        self.at = end;
        Some(lines)
    }

    /// 读完：（新游标, 新游标之前那几个指纹字节）。`seen_len` ＝ 这一趟真读到的长度；比旧的高水位还短（读的那一下文件缩了 ·
    /// 读到一半出错）而这一趟不是从 0 读的 ⇒ 留旧的高水位：下一趟按截短从 0 重读并出声。
    fn finish(self) -> (ReadCursor, Vec<u8>) {
        let seen = self.start + self.blocks.read();
        let seen_len = if (seen < self.from.seen_len || self.broke) && self.reread.is_none() {
            seen.max(self.from.seen_len)
        } else {
            seen
        };
        (
            ReadCursor {
                consumed: self.at,
                seen_len,
            },
            self.tail,
        )
    }
}

/// 跟着一份记录往下读（一次性等待用）：与续读同一套读法 —— 只读新字节 · 截短 / 改写从 0 重读 · 残行等 `\n` · 不在了丢游标。
pub(crate) struct Follow {
    path: PathBuf,
    cursor: ReadCursor,
    tail: Vec<u8>,
    seqs: SeqCounter,
}

impl Follow {
    /// 从此刻最后一个完整行之后接着读（已有的行不交出）。
    pub(crate) fn from_end(path: &Path) -> Follow {
        let mut f = Follow {
            path: path.to_path_buf(),
            cursor: ReadCursor::default(),
            tail: Vec::new(),
            seqs: SeqCounter::new(),
        };
        f.more();
        f
    }

    /// 上一趟之后新长出来的完整行（原文）。不在 / 这一趟读不出 ⇒ 空。
    pub(crate) fn more(&mut self) -> Vec<String> {
        let tail = (!self.tail.is_empty()).then_some(self.tail.as_slice());
        match read_tail_from(&self.path, self.cursor, tail) {
            Look::Gone => {
                self.cursor = ReadCursor::default();
                self.tail.clear();
                Vec::new()
            }
            Look::Unreadable => Vec::new(),
            Look::Read(mut new) => {
                let mut out = Vec::new();
                while let Some(lines) = new.next("", &mut self.seqs) {
                    out.extend(lines.into_iter().map(|l| l.raw));
                }
                (self.cursor, self.tail) = new.finish();
                out
            }
        }
    }
}

/// 这份文件的游标、指纹一起丢（不在了 ⇒ 同名再出现从 0 读；那一趟当改写办：先出声、行号从 0 重数）。
fn forget_cursor(state: &mut ReaderState, key: &Path) {
    state.skel_live.remove(key);
    state.offsets.remove(key);
    state.tails.remove(key);
    state.branches.remove(key);
    state.typed.remove(key);
}

/// Read a JSONL file incrementally and send a [`Frame::Line`] per new line. 回交出去几行（补读计数用）。
fn process_jsonl(path: &Path, state: &mut ReaderState, sink: &mut FrameSink) -> usize {
    let Some(session_id) = state.sid_of(path) else {
        return 0;
    };
    // Active-session filter: only stream sessions whose PID is alive.
    // Historical jsonl is never pulled.
    if !state.active_sids.contains(&session_id) {
        return 0;
    }
    // 预读还没交回：游标在那一趟手里，这一截等它交回时接着读（[`primed`]）⇒ 先宣告、读完之前一行都不出。
    if state.priming.contains_key(&session_id) {
        return 0;
    }
    let key = path_key(path);
    let key_str = key.to_string_lossy().into_owned();
    let prev_cursor = state.offsets.get(&key).copied().unwrap_or_default();
    let path_str = path.to_string_lossy().into_owned();
    // F04：只读新字节（截断 / 改写时 `read_tail_from` 判成从 0 读）；d：按块读、一块一块地出行，不把那一截一次读进内存。
    // 活会话的 jsonl 是「看的，不是管的」（之后文件管理器改得动它）：
    //   不在了 ⇒ 出声一次、丢游标（同名再出现从 0 读）；截短 / 原地改写 ⇒ 从 0 重读并出声。都不碰判活。
    let mut new = match read_tail_from(path, prev_cursor, state.tails.get(&key).map(Vec::as_slice))
    {
        Look::Gone => {
            forget_cursor(state, &key);
            if state.gone.insert(key) {
                sink.send(Frame::SessionFileGone {
                    session_id,
                    path: path_str,
                });
            }
            return 0;
        }
        Look::Unreadable => return 0,
        Look::Read(new) => new,
    };
    // 这一趟要从 0 重读 ⇒ 行号从 0 重数（seq ＝ 当前文件里的行号），出声那一帧排在重读的行之前。
    //   来路：`read_tail_from` 认出的截短 / 改写 · 删了之后同名又长出来（读的那一下文件又缩了的，下一趟按截短办，见 [`NewLines::finish`]）。
    let reappeared = state.gone.remove(&key);
    let reread = new
        .reread
        .or_else(|| reappeared.then_some(RereadWhy::Rewritten));
    if reread.is_some() {
        state.seqs.restart(&key_str);
        // 从 0 重读：骨架照的那一份作废（下一次重启整份重扫、落一份新的）。
        state.skel_live.remove(&key);
    }
    // 从 0 重读 ⇒ 链从头记；先前说过有主线外的 ⇒ 重读完这一趟一定再说一次（哪怕变成空）。
    let mut branch_changed = reread.is_some()
        && state
            .branches
            .remove(&key)
            .is_some_and(|c| !c.off().is_empty());
    if let Some(why) = reread {
        sink.send(Frame::SessionFileReread {
            session_id: session_id.clone(),
            path: path_str.clone(),
            why,
        });
    }
    let mut n = 0;
    let mut runs_changed = adopt_runs(state, &session_id, path);
    while let Some(lines) = new.next(&key_str, &mut state.seqs) {
        n += lines.len();
        for line in lines {
            runs_changed |= send_line(&session_id, &path_str, line, state, sink);
        }
    }
    let (new_cursor, tail) = new.finish();
    state.tails.insert(key.clone(), tail);
    state.offsets.insert(key.clone(), new_cursor);
    branch_changed |= settle_chain(state, path);
    write_back(state, &key);
    if runs_changed {
        sink.send(state.runs_frame(&session_id));
    }
    if branch_changed {
        send_branch(state, sink, &session_id, path, reread.is_some());
    }
    n
}

/// 这份主记录交给它那一家的子运行跟踪收（宣告 / 每次读到它时都说）。回：运行表变没变。
fn adopt_runs(state: &mut ReaderState, sid: &str, path: &Path) -> bool {
    match state.root_of(path) {
        Some(i) => state.roots[i].runs.adopt(sid, path),
        None => false,
    }
}

/// **流只发要看的会话**：这条流报了它此刻在看 `ask.sids`（整份换）、要其中哪几个的主线外清单（`ask.branch`，带上它手上各有什么）。
///
/// 刚进名单的会话（先前报过、又不在里面的；没报过 ＝ 全在流上，谁都不算刚进）：补一帧运行表，并回它的每份记录从第几行起上流 ——
/// 号取游标此刻的值：之前读过的那些没上流，之后读到的（含已落盘、还没读到的）都上流 ⇒ 客户端按骨架补 `[0, seq)`、接实时 `[seq, …)`，不重复也不漏。
/// 刚要起主线外清单的会话（这一次的 `branch` 里有、在看，上一次没有 —— 换了一条连接的头一次报都算）：照它手上那份只补缺的那一截
/// （[`FrameSink::branch_to`]）。不要清单的会话（`branch` 里没有）一帧清单都不收。
/// 在看的会话照读照数（行号与运行表、链索引都照常推进），只是那几种帧不上这条流（拦在 [`FrameSink::send`]）。
/// 名单是按连接的（`sub` ＝ 那条连接的号）：补发的帧只给它。预读还没交回的会话不补（读完那一帧之后照名单发，[`primed`]）、也不回续点（它的续点由 `session_primed` 给）。
fn watch_sessions(
    sub: u64,
    ask: crate::stream::wire::WatchAsk,
    state: &mut ReaderState,
    sink: &mut FrameSink,
) -> Vec<crate::stream::wire::WatchFrom> {
    let next: HashSet<String> = ask.sids.into_iter().collect();
    let Some(me) = sink.subs.iter_mut().find(|s| s.id == sub) else {
        return Vec::new();
    };
    let fresh: Vec<String> = match &me.watching {
        None => Vec::new(),
        Some(old) => {
            let mut v: Vec<String> = next
                .difference(old)
                .filter(|s| !state.priming.contains_key(*s))
                .cloned()
                .collect();
            v.sort_unstable();
            v
        }
    };
    // 要清单的：只算在看的那几个（不在看的清单本来就不上流）。
    let held: HashMap<String, crate::stream::wire::BranchHeld> = ask
        .branch
        .into_iter()
        .filter(|h| next.contains(&h.sid))
        .map(|h| (h.sid.clone(), h))
        .collect();
    let mut starting: Vec<String> = held
        .keys()
        .filter(|sid| !me.branching.contains(*sid))
        .cloned()
        .collect();
    starting.sort_unstable();
    me.branching = held.keys().cloned().collect();
    me.branch_held = held
        .into_iter()
        .filter(|(sid, _)| starting.contains(sid))
        .filter_map(|(_, h)| Some((h.path?, (h.at, h.sum?))))
        .collect();
    me.watching = Some(next);
    let mine = |state: &ReaderState, sid: &str| -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = state
            .offsets
            .keys()
            .filter(|k| state.sid_of(k).as_deref() == Some(sid))
            .cloned()
            .collect();
        v.sort();
        v
    };
    let mut from = Vec::new();
    for sid in fresh {
        sink.send_to(sub, state.runs_frame(&sid));
        for p in mine(state, &sid) {
            from.push(crate::stream::wire::WatchFrom {
                sid: sid.clone(),
                path: p.to_string_lossy().into_owned(),
                seq: state.seqs.peek(&p.to_string_lossy()),
            });
        }
    }
    for sid in starting.iter().filter(|s| !state.priming.contains_key(*s)) {
        for p in mine(state, sid) {
            branch_to(state, sink, sub, sid, &p);
        }
    }
    from
}

/// 一条流连接接上来了：先给它一份这台此刻的样子，之后它与别的连接一样收每一帧。
///
/// 那一份 ＝ 每个活会话一帧 `session_added`（宣告那一帧，此刻的活动 · 名字 · 在等什么 · 记录路径换成现值）·
/// 已读完的再一帧 `session_primed`（`lines` ＝ 此刻已计完的行数：之后的行从这里接）· 有子运行的运行表 ·
/// 会话账本的那一份（可重连的 · 清单报完了没有）。主线外清单不在里面：它只给在 `stream-watch` 里要了的连接、照它手上的只补缺的（[`watch_sessions`]）。**不重读记录**：预读整台只做一次（read8a ＋ §5.2 a）。
fn join(sub: Sub, state: &mut ReaderState, sink: &mut FrameSink) {
    let id = sub.id;
    sink.subs.push(sub);
    let mut sids: Vec<String> = state.active_sids.iter().cloned().collect();
    sids.sort_unstable();
    for sid in sids {
        let Some(mut added) = state.added.get(&sid).cloned() else {
            continue;
        };
        let entry = state.sessions.values().find(|e| e.sid == sid);
        if let (
            Frame::SessionAdded {
                face,
                name_face,
                waiting_for,
                path,
                ..
            },
            Some(e),
        ) = (&mut added, entry)
        {
            *face = if state.reading.contains(&sid) {
                crate::stream::wire::ActivityFace::reading()
            } else {
                crate::stream::wire::ActivityFace::of(e.activity)
            };
            *name_face = crate::stream::wire::NameFace::of(&e.facts);
            waiting_for.clone_from(&e.waiting_for);
            if e.path.is_some() {
                path.clone_from(&e.path);
            }
        }
        let path = match &added {
            Frame::SessionAdded { path, .. } => path.clone(),
            _ => None,
        };
        sink.send_to(id, added);
        if state.priming.contains_key(&sid) {
            continue;
        }
        sink.send_to(
            id,
            Frame::SessionPrimed {
                sid: sid.clone(),
                lines: path
                    .as_ref()
                    .map(|p| state.seqs.peek(&path_key(Path::new(p)).to_string_lossy())),
                path,
            },
        );
        if state.book.has_runs(&sid) {
            sink.send_to(id, state.runs_frame(&sid));
        }
    }
    for f in sink
        .ledger
        .as_ref()
        .map(|l| l.snapshot())
        .unwrap_or_default()
    {
        sink.send_to(id, f);
    }
}

/// 这份记录的主线外清单变了 ⇒ 发变了的那一截（`whole` ⇒ 从头整份：从 0 重读 · 读完那一刻）。
/// 从上一次发起清单前几条没动由链索引记着（[`crate::agents::mainline::Chain::take_kept`]）；丢过它一帧的连接拿整份（[`FrameSink::branch`]）。
fn send_branch(state: &mut ReaderState, sink: &mut FrameSink, sid: &str, path: &Path, whole: bool) {
    let (off, kept) = match state.branches.get_mut(&path_key(path)) {
        Some(c) => {
            let kept = c.take_kept();
            (c.off(), if whole { 0 } else { kept })
        }
        None => (&[][..], 0),
    };
    sink.branch(sid, &path.to_string_lossy(), off, kept);
}

/// 给一条连接这份记录此刻的主线外清单（它刚要起这份清单：照它交回的手上那份只补缺的）。
fn branch_to(state: &ReaderState, sink: &mut FrameSink, id: u64, sid: &str, path: &Path) {
    let off = state
        .branches
        .get(&path_key(path))
        .map_or(&[][..], |c| c.off());
    sink.branch_to(id, sid, &path.to_string_lossy(), off);
}

/// 一行记进这份记录的链索引（这一家有链才记）；不算 —— 这一趟读的行都记完了由 [`settle_chain`] 算一次。
fn feed_chain(state: &mut ReaderState, path: &Path, line: &crate::agents::Line<'_>) {
    let Some(fact) = state
        .root_of(path)
        .and_then(|i| state.roots[i].face.chain)
        .and_then(|c| c(line))
    else {
        return;
    };
    state.branches.entry(path_key(path)).or_default().load(fact);
}

/// 这份记录的链索引把记下的一次算掉；回：主线外清单变没变。一趟读进多少行都只算这一次（逐行算是平方：hot1）。
fn settle_chain(state: &mut ReaderState, path: &Path) -> bool {
    state
        .branches
        .get_mut(&path_key(path))
        .is_some_and(crate::agents::mainline::Chain::settle)
}

/// 这条会话的记录路径定下来了 / 换了一份（读到的这一行不在客户端手里那份上）⇒ 记下，发一帧 `session_status` 带上它（活动那几格照记着的带全）。
fn note_path(session_id: &str, path_str: &str, state: &mut ReaderState, sink: &mut FrameSink) {
    let mut status = None;
    for e in state.sessions.values_mut().filter(|e| e.sid == session_id) {
        if e.path.as_deref() != Some(path_str) {
            e.path = Some(path_str.to_string());
            status = Some((e.activity, e.waiting_for.clone()));
        }
    }
    let Some((activity, waiting_for)) = status else {
        return;
    };
    sink.send(Frame::SessionStatus {
        sid: session_id.to_string(),
        face: crate::stream::wire::ActivityFace::of(activity),
        name_face: state.name_face(session_id),
        waiting_for,
        path: Some(path_str.to_string()),
    });
}

/// 一行交出去：`Line` 帧，是轮次结束就紧跟一帧 `TurnEnd`。增量读与写端死后收尾（[`catch_up_session`]）共用这一份。
///
/// 这一行在渲染模型里是什么、是不是一轮的结束，都问注册表里流式那一家的记录解释面（`agents::record_face`）；
/// 本函数只搬。解析不出 ⇒ 帧照发（占号）、不带成品。对账键与派出链接记进运行簿，链事实记进链索引（不算：调用方读完这一趟再 `settle_chain`）。回：运行表变没变。
fn send_line(
    session_id: &str,
    path_str: &str,
    line: ReadLine,
    state: &mut ReaderState,
    sink: &mut FrameSink,
) -> bool {
    // 一行只从文本解一次（瓶颈 #8）：成品 · 链事实 · 运行簿 · 轮次结束 · 此刻在干什么都吃这一份；
    //   这一路几问都要整行 ⇒ 先解好（只要几格的那几问从这一份里取，不再各自解文本）。
    let parsed = crate::agents::Line::whole(&line.raw, line.start);
    feed_chain(state, Path::new(path_str), &parsed);
    let root = state.root_of(Path::new(path_str));
    let face = root.map(|i| state.roots[i].face);
    let translated = face.and_then(|f| (f.parse)(&parsed).ok().flatten());
    // §2.1 不变量并存：Line 逐行照发**每一条**；turn-end 是额外的边沿信号，不替代、不过滤 Line。
    let rec = root
        .map(|i| state.roots[i].runs.main_record(session_id, &parsed))
        .unwrap_or_default();
    // 子运行的记录（适配层 `run_of` 答得出）一轮收尾 ≠ 主运行一轮结束 ⇒ 不报轮次边沿。
    let turn_uuid = face
        .and_then(|f| f.turn_end)
        .and_then(|t| t(&parsed))
        .filter(|_| !rec.in_run);
    let activity = face
        .and_then(|f| f.activity)
        .and_then(|f| f(&parsed))
        .filter(|_| !rec.in_run);
    let skel_key = path_key(Path::new(path_str));
    if state.skel_live.contains_key(&skel_key) {
        let hint = root.is_some_and(|i| state.roots[i].runs.hint(&line.raw));
        if let Some(sl) = state.skel_live.get_mut(&skel_key) {
            if hint {
                sl.runs.push(line.raw.clone());
            }
            if activity.is_some() {
                sl.activity = activity;
            }
        }
    }
    if let Some(body) = translated
        .as_ref()
        .and_then(|t| t.record.as_ref())
        .map(|r| &r.body)
    {
        note_name(session_id, body, state, sink);
    }
    note_path(session_id, path_str, state, sink);
    let (record, cwd) = match translated {
        Some(mut t) => {
            state
                .typed
                .entry(path_key(Path::new(path_str)))
                .or_default()
                .pass(&mut t);
            (t.record, t.cwd)
        }
        None => (None, None),
    };

    sink.send(Frame::Line {
        session_id: session_id.to_string(),
        path: path_str.to_string(),
        seq: line.seq,
        record,
        cwd,
        byte_offset: line.byte_offset, // backend-01 gap#2：累计原始字节（对齐手机端 LineFramer）
        rid: rec.rid,
    });
    // **先 Line 后 TurnEnd**：对齐手机端 β 的按行序处理——TurnEnd 结算时 currentOffset 已含本行。
    // TurnEnd 不带 byte_offset（只 Line 带）。
    if let Some(uuid) = turn_uuid {
        sink.send(Frame::turn_end(
            session_id.to_string(),
            uuid,
            &state.name_face(session_id),
        ));
    }
    // 这一行说此刻进到哪一态（没有进程状态文件的那一家，适配层照记录事件翻）⇒ 变了才发一帧 `session_status`。
    if let Some(a) = activity {
        note_activity(session_id, a, state, sink);
    }
    rec.changed
}

/// 记录说这条会话此刻进到 `a` 这一态 ⇒ 记账；真变了 ⇒ 一帧 `session_status`（带全，与进程状态文件那一路同一形）。
fn note_activity(
    sid: &str,
    a: crate::agents::SessionActivity,
    state: &mut ReaderState,
    sink: &mut FrameSink,
) {
    let mut changed = None;
    for e in state.sessions.values_mut().filter(|e| e.sid == sid) {
        if e.activity != Some(a) {
            e.activity = Some(a);
            changed = Some((e.waiting_for.clone(), e.path.clone()));
        }
    }
    let Some((waiting_for, path)) = changed else {
        return;
    };
    sink.send(Frame::SessionStatus {
        sid: sid.to_string(),
        face: crate::stream::wire::ActivityFace::of(Some(a)),
        name_face: state.name_face(sid),
        waiting_for,
        path,
    });
}

/// 读到的一条记录若改了叫这条会话什么（标题记录 · 还没有第一句时人说的第一句）⇒ 记账；名字真变了 ⇒ 一帧 `session_status`（带全）。
/// 子运行的记录也经这里：子运行里的人话不是这条会话的第一句 —— 它们的记录在子运行自己的文件里、sid 认不到这条会话的账上。
fn note_name(
    sid: &str,
    body: &crate::agents::record::Body,
    state: &mut ReaderState,
    sink: &mut FrameSink,
) {
    use crate::agents::record::Body;
    let change = |f: &mut crate::history::search_rules::LabelFacts| match body {
        Body::Title { text, .. } if f.title.as_deref() != Some(text) => {
            f.title = Some(text.clone());
            true
        }
        Body::Said { who, .. } if f.first.is_empty() => match who.speech() {
            Some(s) => {
                f.first = crate::history::search_rules::truncate_excerpt(&s, 120);
                true
            }
            None => false,
        },
        _ => false,
    };
    state.renamed(sid, change, sink);
}

/// **从游标补读这个会话的 jsonl**（与文件事件同一个 [`process_jsonl`]，不另写一条路）。
///
/// `writer_dead`（只由会话退休那一刻传真）⇒ 补读完之后，游标之后那截没 `\n` 收尾、但本身是**一整个 JSON 对象**的残行
/// 当一行交出去：写端写完 JSON、没来得及写 `\n` 就被杀，这一行从此不会再有文件事件。活着的写端照旧等 `\n`。
/// 这一行**不推进游标、不占计数器**：同一文件日后被续写时，残行与新字节拼成的那一行仍是这个号（冷读的行号空间里它也是
/// 这个号），前端按 seq 去重吸收；冷读（`history_query::line_counts` 口径）不数这截残行 —— 会话已死、没人再写，差的只是它自己。
/// 退休之前先补读还有一层用处：pidfd 比 debounce 快，死前最后几行的文件事件可能在退休之后才到、被判活过滤挡掉。
/// 回补读出几行（`resync` 应答的 `caught_up`；不数收尾残行 —— `resync` 那一路写端活着、不收尾）。
fn catch_up_session(
    sid: &str,
    writer_dead: bool,
    state: &mut ReaderState,
    sink: &mut FrameSink,
) -> usize {
    let mine: Vec<PathBuf> = state
        .offsets
        .keys()
        .filter(|k| state.sid_of(k).as_deref() == Some(sid))
        .cloned()
        .collect();
    let mut n = 0;
    for path in mine {
        n += process_jsonl(&path, state, sink);
        if writer_dead {
            flush_final_line(&path, sid, state, sink);
        }
    }
    n
}

/// [`catch_up_session`] 的收尾那一半：`[consumed, EOF)` 是一整个 JSON 对象 ⇒ 交出去（号 = 计数器现值，不推进）。
fn flush_final_line(path: &Path, sid: &str, state: &mut ReaderState, sink: &mut FrameSink) {
    use std::io::{Seek, SeekFrom};
    let key = path_key(path);
    let Some(cursor) = state.offsets.get(&key).copied() else {
        return;
    };
    // 按块读：读出一整块（有 `\n`）就不往下读了 —— 只剩一截半行时才真读到尾。
    let Ok(mut f) = Ro::open(path) else {
        return;
    };
    if f.seek(SeekFrom::Start(cursor.consumed)).is_err() {
        return;
    }
    let mut blocks =
        crate::common::append_read::Blocks::new(f, crate::common::append_read::READ_BLOCK);
    if !matches!(blocks.next(), Ok(None)) {
        return; // 读不动 / 还有完整行没消费（补读那一步没读动）⇒ 不猜
    }
    let rest = blocks.rest();
    let text = String::from_utf8_lossy(&rest);
    let raw = text.trim_start_matches('\u{feff}').trim_end_matches('\r');
    if !raw.trim_start().starts_with('{')
        || serde_json::from_str::<serde::de::IgnoredAny>(raw).is_err()
    {
        return; // 半行 / 空白 / 不是对象 ⇒ 永不误发
    }
    let key_str = key.to_string_lossy().into_owned();
    let line = ReadLine {
        seq: state.seqs.peek(&key_str),
        raw: raw.to_string(),
        byte_offset: cursor.consumed + rest.len() as u64,
        start: cursor.consumed,
    };
    let runs_changed = send_line(sid, &path.to_string_lossy(), line, state, sink);
    let branch_changed = settle_chain(state, path);
    if runs_changed {
        sink.send(state.runs_frame(sid));
    }
    if branch_changed {
        send_branch(state, sink, sid, path, false);
    }
}

/// ★★ `P0b-Y2`：`<claude_dir>/sessions/` **换了 inode 或刚出现**，重新挂上并重扫。
///
/// # 为什么必须有这一步（九拍排除链的终点）
///
/// inotify 的 watch 绑在 **inode** 上。`rm -rf sessions && mkdir sessions` 之后是另一个
/// inode，旧 watch 还挂在已删的那个上 ⇒ **新目录里发生什么都听不见，且没有任何错误**。
/// 症状是后端「活着、不吭声」—— 08-13 离线对照实测：
///
/// | 组 | 帧 |
/// |---|---|
/// | 控制（不动 `sessions/`） | `hello · line · session_added · session_removed · tmux_sessions×2` |
/// | **删掉再建** | **`hello · tmux_sessions`** |
///
/// 下面那一行**与 `#60` 全链台架量到的 app 路径签名逐字相同**，而台架自己在 backend
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
            tracing::warn!(
                "重挂 sessions watch 失败 {}: {e}（下次事件再试）",
                sessions.display()
            );
        }
    }
}

/// A `sessions/<PID>.json` appeared (or was already present): read it, extract
/// `sessionId`, cache PID→sid, emit [`Frame::SessionAdded`].
///
/// Idempotent: if we already cached the same sid for this path, skip the emit
/// so a debounced modify event does not re-announce an existing session.
/// 回真 = 这一次真往 tmux 里写了身份标签（首次宣告或对账纠正）。
fn process_session_added(path: &Path, state: &mut ReaderState, sink: &mut FrameSink) -> bool {
    let key = path_key(path);
    // PID is the sessions/<PID>.json filename stem.
    let Some(pid) = file_stem_str(path).and_then(|s| s.parse::<u32>().ok()) else {
        return false;
    };
    let Some(bytes) = Ro::read(path).ok() else {
        return false;
    };
    let Some(sid) = parse_session_id(&bytes) else {
        return false;
    };
    // Only ACTIVE if the process is actually alive (mirrors local STILL_ACTIVE).
    // A stale pidfile for a dead process is NOT an active session.
    if !pid_alive(pid) {
        return false;
    }
    // 工作目录是这台家里不出会话的那个目录 ⇒ 不成 tab —— 判据与历史页同一处。
    if pidfile_cwd(&bytes).is_some_and(|c| crate::history::listing::hidden_cwd(&c)) {
        return false;
    }
    // 后台会话（适配层判，`agents::pidfile_background`）照宣告，帧带 `background: true`；成不成 tab 由客户端按这一格定。
    let background = is_background(&bytes);
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
            let activity = meta.as_ref().and_then(crate::agents::pidfile_activity);
            entry.activity = activity;
            let name_face = crate::stream::wire::NameFace::of(&entry.facts);
            sink.send(Frame::SessionStatus {
                sid: sid.clone(),
                face: crate::stream::wire::ActivityFace::of(activity),
                name_face,
                waiting_for: new_waiting,
                path: None,
            });
        }
        // pidfile 重写（sid 没变）顺手对一次标签：外部改掉的 `@ccm_sid` 在这里被纠正。
        // 便条也再认一次（Windows 上起好进程才写便条，可能晚于第一次宣告）。
        adopt_launch_note(state, pid, &sid);
        return tag_identity(pid, &sid).1;
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
    // #34: the poll baseline. Reuse the very ticks the verdict just examined —
    // no second /proc read, so no verdict-to-baseline TOCTOU window.
    let start = match add_time_check(pid, &bytes, path) {
        Ok(ticks) => ticks,
        Err(reason) => {
            tracing::warn!(
                "stale sessions json ignored ({reason}): {} pid {pid} is not the claude that wrote it",
                path.display()
            );
            return false;
        }
    };
    let activity = meta.as_ref().and_then(crate::agents::pidfile_activity);
    announce(
        key,
        Announce {
            pid,
            start,
            sid,
            kind: state.pidfile_kind,
            background,
            // E73：pidfile 的 `attachable`。**只认真正的布尔** —— 字符串 "false" 之类当没写
            //（缺席 = true = 照旧），宁可少一次门控也不要把一个拼错的值当成"不可 attach"。
            attachable: meta
                .as_ref()
                .and_then(|v| v.get("attachable"))
                .and_then(|x| x.as_bool()),
            cwd: meta_str("cwd"),
            name: meta_str("name"),
            status: meta_str("status"),
            waiting_for: meta_str("waitingFor"),
            activity,
        },
        state,
        sink,
    )
}

/// 一条会话的宣告要的事实（判活那一路各自读出来的；之后的事都走 [`announce`] 这一份）。
struct Announce {
    pid: u32,
    /// 那个进程的启动时刻（挂 pidfd 看守时挡 PID 复用）。
    start: Option<u64>,
    sid: String,
    /// 这条会话是哪一家的（线上 `agent_kind`）。
    kind: &'static str,
    background: bool,
    attachable: Option<bool>,
    cwd: Option<String>,
    name: Option<String>,
    status: Option<String>,
    waiting_for: Option<String>,
    activity: Option<crate::agents::SessionActivity>,
}

/// 记一条活会话 ＋ 宣告它（判活那一路已经认定它活着）：记账 · 打标 · 挂 pidfd 看守 · 找它的记录 · `session_added` · 已有的行 / 运行表 / 主线外清单。
/// `key` 是判活那一路认它的那份文件（pidfile，或开着写的那份记录）。同一个 sid 已经宣告过 ⇒ 只记账、打标、挂看守。回真 = 这一次真往 tmux 里写了身份标签。
fn announce(key: PathBuf, a: Announce, state: &mut ReaderState, sink: &mut FrameSink) -> bool {
    let Announce {
        pid, start, sid, ..
    } = a;
    // P2：`key` 下面被 insert 消耗掉，先留一份给 pidfd 看守用。
    let key_for_watch = key.clone();
    // 先定位该 sid 的记录（Batch8-F25：帧要带 path 供 monitor 旁路快照；mtime 降序，first=当前活跃文件。
    // 会话刚起还没写首行时为空 → path=None，此时无历史可拉，后续行天然从 tail 全量到达）。项目目录也从它来，没有记录 ⇒ cwd。
    let jsonls = sid_jsonls(state, &sid);
    let project_dir = jsonls
        .first()
        .and_then(|p| crate::agents::project_dir_of(p))
        .or_else(|| a.cwd.clone());
    // 叫它什么：同一个 sid 已经记着 ⇒ 接着用那一份（第二份 pidfile 不重扫）；否则此刻只有项目目录与注解 ——
    // 记录里的标题 · 第一句（清单缓存冷时要整份扫）交给后台预读那一趟一起做，做完名字变了再发一帧 `session_status`。
    let known = state
        .sessions
        .values()
        .find(|e| e.sid == sid)
        .map(|e| e.facts.clone());
    let fresh_name = known.is_none();
    let facts = known.unwrap_or_else(|| crate::history::search_rules::LabelFacts {
        project_dir: project_dir.clone(),
        custom: state.custom_title(&sid),
        ..Default::default()
    });
    let name_face = crate::stream::wire::NameFace::of(&facts);
    state.sessions.insert(
        key,
        SessionEntry {
            pid,
            sid: sid.clone(),
            status: a.status,
            waiting_for: a.waiting_for.clone(),
            activity: a.activity,
            facts,
            path: jsonls.first().map(|p| p.to_string_lossy().into_owned()),
        },
    );
    // 同一个 sid 已由另一份 pidfile 宣告过（resume 时原进程还活着）⇒ 只记账、打标、挂看守，不再宣告第二次。
    let announced = !state.active_sids.insert(sid.clone());
    // `U-NP④`：身份打标（`@ccm_sid`）—— 接 `shared/ccm` 那条每秒轮询的班，见
    // `control::identity_tag`（跨层边已登记进 `layering_guard`）。放在冒名检查**之后**。
    //
    // 这一次探测的**结局**不再丢：它同时就是「这条会话住在什么容器里」的答案
    // （`identity_tag::Outcome::container`，随下面的 `session_added` 报出去）—— 零新进程、零新节拍。
    // 打不上的那两形（tmux 报错 / sid 形状不对）**说出来** —— 标没写上的会话
    // 之后过不了身份门，而「为什么」原先整条链零线索。
    let (container, wrote) = tag_identity(pid, &sid);
    adopt_launch_note(state, pid, &sid);
    // P2：给这个进程实例挂 pidfd 看守（取代原先每 2s 一遍的判活扫描）。
    arm_pid_watcher(&key_for_watch, pid, start, state);
    if announced {
        return wrote;
    }
    // **先宣告、后读**（read8a）：认出来就报，不等记录。记录交后台预读（推进游标 / 行号到此刻的完整行数 L，零行帧），
    // 读完发 `session_primed` 带 L —— 之后的新行 seq 从 L 起；`[0, L)` 由客户端经骨架 / 尾段补。读完之前这条会话的行帧一帧不出。
    // 此刻在干什么只有记录说（这一家没有进程状态文件）、而记录还没读 ⇒ 照实说「读记录中」，读完再说判出来的那一态（[`primed`]）。
    let reading = a.activity.is_none()
        && jsonls
            .first()
            .and_then(|p| state.root_of(p))
            .is_some_and(|i| state.roots[i].face.activity.is_some());
    if reading {
        state.reading.insert(sid.clone());
    }
    let added = Frame::SessionAdded {
        sid: sid.clone(),
        agent_kind: a.kind.to_string(),
        background: a.background,
        attachable: a.attachable,
        cwd: a.cwd.clone(),
        project_dir,
        name: a.name,
        path: jsonls.first().map(|p| p.to_string_lossy().into_owned()),
        face: if reading {
            crate::stream::wire::ActivityFace::reading()
        } else {
            crate::stream::wire::ActivityFace::of(a.activity)
        },
        name_face,
        waiting_for: a.waiting_for,
        // 判不了 ⇒ `None` ⇒ 不上线（与本字段加进来之前逐字节相同）。
        container,
        // pid 与 verdict 核过的是同一个进程（见 `wire::Frame::SessionAdded::pid`）。
        pid,
    };
    state.added.insert(sid.clone(), added.clone());
    sink.send(added);
    start_prime(&sid, &jsonls, fresh_name, state, sink);
    wrote
}

/// 给一条刚宣告的会话起一趟预读：记下代号、这条会话的内容帧从此刻拦下（[`FrameSink::send`]），交线程池（夹具没有池 ⇒ 就地跑、就地装回）。
/// 这几份记录的游标 · 指纹 · 链索引交给那一趟带走，读完连同新的一起交回 ⇒ 读的过程中本线程不碰它们（它们的文件事件也不读，[`process_jsonl`]）。
fn start_prime(
    sid: &str,
    jsonls: &[PathBuf],
    fresh_name: bool,
    state: &mut ReaderState,
    sink: &mut FrameSink,
) {
    state.next_prime += 1;
    let gen = state.next_prime;
    state.priming.insert(sid.to_string(), gen);
    sink.priming.insert(sid.to_string());
    let files = jsonls
        .iter()
        .map(|p| {
            let key = path_key(p);
            let key_str = key.to_string_lossy().into_owned();
            FilePrime {
                path: p.clone(),
                prev: state.offsets.get(&key).copied().unwrap_or_default(),
                tail: state.tails.get(&key).cloned(),
                seq: state.seqs.peek(&key_str),
                chain: state.branches.remove(&key),
                face: state.root_of(p).map(|i| state.roots[i].face),
                runs: state.root_of(p).map(|i| {
                    crate::observe::runs::RunTrack::new(
                        state.roots[i].runs.faces,
                        state.book.clone(),
                    )
                }),
                skel: state.skeletons.clone(),
                key,
            }
        })
        .collect();
    let job = PrimeJob {
        sid: sid.to_string(),
        gen,
        files,
        label: fresh_name.then(|| jsonls.first().cloned()).flatten(),
    };
    match state.pool.as_mut() {
        Some(pool) => {
            pool.queue.push_back(job);
            dispatch_primes(state, sink);
        }
        None => {
            let (done, later) = run_prime(job);
            primed(done, state, sink);
            store_skeletons(later);
        }
    }
}

/// **先读正在看的**（read8a ②）：池子有空位就从排着的预读里挑下一份交出去 —— 有连接在看（`stream-watch` 名单里点了名）的先，
/// 其余照排队的先后。挑在交出去那一刻（名单一变，下一个空位就按新名单挑）；已经在读的不撤。
/// 排着的那一份对应的会话已退休 / 又被重新宣告（代号对不上）⇒ 扔掉（它带走的游标与链本来就作废了）。
/// 名单是「点了名」才算：没报过名单的连接（＝ 全看）不把谁顶到前面。
fn dispatch_primes(state: &mut ReaderState, sink: &FrameSink) {
    let watched = |sid: &str| {
        sink.subs
            .iter()
            .any(|s| s.watching.as_ref().is_some_and(|w| w.contains(sid)))
    };
    let Some(pool) = state.pool.as_mut() else {
        return;
    };
    pool.queue
        .retain(|j| state.priming.get(&j.sid) == Some(&j.gen));
    while pool.in_flight < pool.pool.size() {
        let at = pool.queue.iter().position(|j| watched(&j.sid)).unwrap_or(0);
        let Some(job) = pool.queue.remove(at) else {
            break;
        };
        pool.in_flight += 1;
        pool.pool.submit(job);
    }
}

// ============ 「开着即活」那一路：没有 pidfile 的那几家 ============
//
// 有进程开着一份会话记录写 ⇒ 这条会话活着（适配层 `Adapter::live` ＝ `LiveBy::HeldOpen` 声明这一家是这样）。认写者看打开方式（`platform::writers`），
// 不看进程名。扫进程表只在三种时候：记录根下新建 / 打开了一份还没认的记录（耳朵交的一批，一批一趟）· 起步与对齐（整树一趟）·
// 内核事件队列溢出过（整树一趟）。摘除：写的那个进程退了（pidfd，与 pidfile 那一路同一个 `PidDied`）· 那份记录被写完关闭、
// 复核已没人开着写。本平台认不出写者（`writers_of` 答 `None`）⇒ 什么都不宣告、什么都不摘（判不了，不冒充判过）。

/// 耳朵交的一批：没认的打开了 ⇒ 候选；认了的被写完关闭 ⇒ 复核。一批只扫一趟进程表。溢出过 ⇒ 整树一趟。
fn on_writers(
    batch: crate::platform::writers::OpenBatch,
    state: &mut ReaderState,
    sink: &mut FrameSink,
) {
    if batch.overflowed {
        settle_all_writers(state, sink, None);
        return;
    }
    let tracked = |st: &ReaderState, p: &Path| st.sessions.contains_key(&path_key(p));
    let mut cands: Vec<PathBuf> = batch
        .opened
        .into_iter()
        .filter(|p| state.writer_record(p) && !tracked(state, p))
        .collect();
    cands.extend(
        batch
            .closed
            .into_iter()
            .filter(|p| state.writer_record(p) && tracked(state, p)),
    );
    if cands.is_empty() {
        return;
    }
    cands.sort();
    cands.dedup();
    let wanted: HashSet<PathBuf> = cands.iter().cloned().collect();
    let Some(found) = crate::platform::writers::writers_of(&|p| wanted.contains(p)) else {
        return;
    };
    for p in cands {
        let w = found.get(&p).and_then(|v| v.first().copied());
        settle_writer(&p, w, state, sink);
    }
}

/// 整树一趟（起步 · 对齐 · 溢出）：各家「开着即活」的记录根下此刻被开着写的 ∪ 在跟的，逐份对上。`only` = 只对这一个 sid。
fn settle_all_writers(state: &mut ReaderState, sink: &mut FrameSink, only: Option<&str>) {
    let roots: Vec<PathBuf> = state
        .roots
        .iter()
        .filter(|r| r.by_writer)
        .map(|r| r.root.clone())
        .collect();
    if roots.is_empty() {
        return;
    }
    let Some(found) =
        crate::platform::writers::writers_of(&|p| roots.iter().any(|r| p.starts_with(r)))
    else {
        return;
    };
    let mut paths: Vec<PathBuf> = found
        .keys()
        .filter(|p| state.writer_record(p))
        .cloned()
        .collect();
    paths.extend(
        state
            .sessions
            .keys()
            .filter(|k| state.writer_record(k))
            .cloned(),
    );
    paths.sort();
    paths.dedup();
    for p in paths {
        if only.is_some_and(|o| state.sid_of(&p).as_deref() != Some(o)) {
            continue;
        }
        let w = found.get(&p).and_then(|v| v.first().copied());
        settle_writer(&p, w, state, sink);
    }
}

/// 一份记录此刻的写者（`None` ＝ 没人开着它写）对上账：没认 ＋ 有写者 ⇒ 宣告；认了 ＋ 没写者 ⇒ 摘；认了 ＋ 换了个写者 ⇒ 改看那一个。
fn settle_writer(
    p: &Path,
    w: Option<crate::platform::writers::Writer>,
    state: &mut ReaderState,
    sink: &mut FrameSink,
) {
    let key = path_key(p);
    match (state.sessions.get(&key).map(|e| e.pid), w) {
        (None, Some(w)) => {
            announce_writer(p, w, state, sink);
        }
        (Some(_), None) => process_session_removed(p, state, sink),
        (Some(pid), Some(w)) if pid != w.pid => {
            if let Some(e) = state.sessions.get_mut(&key) {
                e.pid = w.pid;
            }
            arm_pid_watcher(&key, w.pid, w.start, state);
        }
        _ => {}
    }
}

/// 一份有人开着写的记录 ⇒ 宣告它那条会话（工作目录是这台家里不出会话的那个目录 ⇒ 不成 tab，与 pidfile 那一路同一处判）。
fn announce_writer(
    p: &Path,
    w: crate::platform::writers::Writer,
    state: &mut ReaderState,
    sink: &mut FrameSink,
) -> bool {
    let Some(i) = state.root_of(p) else {
        return false;
    };
    let (kind, face) = (state.roots[i].kind, state.roots[i].face);
    let Some(sid) = (face.sid)(p) else {
        return false;
    };
    let dir = face.project_dir.and_then(|f| f(p));
    if dir
        .as_deref()
        .is_some_and(crate::history::listing::hidden_cwd)
    {
        return false;
    }
    announce(
        path_key(p),
        Announce {
            pid: w.pid,
            start: w.start,
            sid,
            kind,
            background: false,
            attachable: None,
            cwd: dir,
            name: None,
            status: None,
            waiting_for: None,
            activity: None,
        },
        state,
        sink,
    )
}

/// 「这条会话用哪个号起的」：报给控制面（[`Reactions::session_seen`]，`ccm` 给这个 pid 留过便条 ⇒ 记下 `sid → 号`）。
/// 触发时机与打标同一个：pidfile 出现 / 改写那一刻，`(pid, sid)` 只在这时同时在手。
fn adopt_launch_note(state: &ReaderState, pid: u32, sid: &str) {
    if let Some((reactions, own)) = state.launch.as_ref() {
        (reactions.session_seen)(own, pid, sid);
    }
}

/// **身份打标的唯一调用点**（首次宣告 · pidfile 重写 · tmux 探测到达 · `resync` 都经它）：打不上的那两形说出来，
/// 回（容器，这次真写了没有）。`tag()` 自带「值一样就不动」⇒ 对账不多写一次。
fn tag_identity(pid: u32, sid: &str) -> (Option<crate::stream::wire::SessionContainer>, bool) {
    let outcome = crate::control::identity_tag::tag(pid, sid);
    if let Some(note) = outcome.failure_note(pid, sid) {
        tracing::warn!("{note}");
    }
    let wrote = outcome.wrote();
    (outcome.container(), wrote)
}

/// 对在跟的每个会话（`only` 给了就只对那个 sid）重比一次标签；回真写了几个。
fn retag_tracked(state: &ReaderState, only: Option<&str>) -> usize {
    state
        .sessions
        .values()
        .filter(|e| only.is_none_or(|s| s == e.sid))
        .filter(|e| tag_identity(e.pid, &e.sid).1)
        .count()
}

/// Phase 1 的本体：逐个活 pidfile 发 `session_added`，**然后**发一帧 `sessions_replayed`。
///
/// 从 `watch_loop` 里抽出来，为的是「清单报完了」那一帧的**位置**能被直接验
/// （`watcher_tests::sessions_replayed_follows_every_initial_session_added_exactly_once`）——
/// 起整条 `watch_loop` 要挂 inotify、探真机 tmux、装 hook，判据不许碰那些。
///
/// `sessions_replayed` **无条件发**：`sessions/` 不在 = 清单是空的，也是一个说完了的答案。
/// 它排在本函数所有 `session_added` 之后；调用方在 Phase 2 起来之前调本函数（同一个 sink、同一条线程）
/// ⇒ 客户端收到它时，这台机器此刻全部的活会话都已经报过了 —— 靠它把「固定、却没被报过」的会话
/// 从「说不清」落到「已结束」。
fn initial_session_scan(sessions: &Path, state: &mut ReaderState, sink: &mut FrameSink) {
    reconcile_sessions(sessions, state, sink, None);
    sink.send(Frame::SessionsReplayed);
}

/// 起步那一下：先 `arm`（挂好事件耳朵），**再** [`initial_session_scan`]。回 `arm` 的结果（挂不上也照样扫完）。
///
/// 收成一个函数是为了让「挂耳朵在初扫之前」这件事能被直接验
/// （`watcher_tests::a_pidfile_landing_once_the_ears_are_up_is_announced_by_the_initial_scan`）：
/// 挂上之后落下的由事件接、挂上之前落下的由初扫接 ⇒ 两头之间没有谁都看不见的窗。顺序反过来就有（见 `watch_loop` 那段）。
fn arm_then_scan<T>(
    sessions: &Path,
    state: &mut ReaderState,
    sink: &mut FrameSink,
    arm: impl FnOnce() -> T,
) -> T {
    let armed = arm();
    initial_session_scan(sessions, state, sink);
    armed
}

/// 一次对齐的差异（`resync` 的应答）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Reconciled {
    pub(crate) added: usize,
    pub(crate) removed: usize,
    pub(crate) retagged: usize,
    /// 这一趟从游标补读出几行（[`catch_up_session`] 的合计）。
    pub(crate) caught_up: usize,
}

/// 「重新对齐」那一趟的会话部分：对表（与起步同一个 [`reconcile_sessions`]）＋ 每个在跟的会话从游标补读
/// （tab「重新读取」：`only` = 那一个 sid；与退休前那一次补读同一个 [`catch_up_session`]，写端活着 ⇒ 不收尾残行）。
fn resync_sessions(
    sessions: &Path,
    state: &mut ReaderState,
    sink: &mut FrameSink,
    only: Option<&str>,
) -> Reconciled {
    let mut got = reconcile_sessions(sessions, state, sink, only);
    let sids: Vec<String> = state
        .active_sids
        .iter()
        .filter(|s| only.is_none_or(|o| o == s.as_str()))
        .cloned()
        .collect();
    for sid in sids {
        got.caught_up += catch_up_session(&sid, false, state, sink);
    }
    got
}

/// **pidfile 目录对后端的表**：起步初扫与 `resync` 是这同一个函数（「与初探同一个函数」）。
/// 表里有、盘上没了或进程没了 ⇒ 补移除；盘上有 ⇒ `process_session_added`（没跟的补宣告，在跟的走 sid 没变那一支 ⇒ 顺手对账标签）。
/// 只对差异发帧。`only` = 只对这一个 sid（关卡 2「对齐后重试」）。
fn reconcile_sessions(
    sessions: &Path,
    state: &mut ReaderState,
    sink: &mut FrameSink,
    only: Option<&str>,
) -> Reconciled {
    let before = state.active_sids.clone();
    let stale: Vec<PathBuf> = state
        .sessions
        .iter()
        .filter(|(k, e)| only.is_none_or(|s| s == e.sid) && (!k.exists() || !pid_alive(e.pid)))
        .map(|(k, _)| k.clone())
        .collect();
    for k in &stale {
        process_session_removed(k, state, sink);
    }
    let mut retagged = 0;
    if sessions.is_dir() {
        for entry in WalkDir::new(sessions).into_iter().filter_map(Result::ok) {
            let p = entry.path();
            if !is_session_json(p) {
                continue;
            }
            if let Some(want) = only {
                let sid = Ro::read(p).ok().and_then(|b| parse_session_id(&b));
                if sid.as_deref() != Some(want) {
                    continue;
                }
            }
            if process_session_added(p, state, sink) {
                retagged += 1;
            }
        }
    }
    // 没有 pidfile 的那几家：整树一趟（起步与对齐同一个）。
    settle_all_writers(state, sink, only);
    let after = &state.active_sids;
    Reconciled {
        added: after.difference(&before).count(),
        removed: before.difference(after).count(),
        retagged,
        caught_up: 0,
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
    catch_up_session(sid, true, state, sink); // 写端已死：补读 ＋ 收尾残行（D 块）
    state.active_sids.remove(sid);
    state.added.remove(sid);
    state.reading.remove(sid);
    // 预读还在跑 ⇒ 那一趟交回时认不上、丢掉（[`primed`]）。
    state.priming.remove(sid);
    sink.priming.remove(sid);
    if let Some(f) = state.retire_runs(sid) {
        sink.send(f);
    }
    sink.send(Frame::SessionRemoved {
        sid: sid.to_string(),
        cause,
    });
}

/// 每一家的记录根走一遍，找这个 sid 的记录文件（宣告会话时把它已有的那几行带上）。
/// 同 sid 多份（项目目录改名后 resume）时修改时刻新的在前（帧的 `path` / `lines` 取第一份，快照拉错陈文件 = 当前历史全缺）。
fn find_sid_jsonls(state: &ReaderState, sid: &str) -> Vec<std::path::PathBuf> {
    let mut v: Vec<std::path::PathBuf> = record_files(state)
        .filter(|p| state.sid_of(p).as_deref() == Some(sid))
        .collect();
    v.sort_by_key(|p| std::cmp::Reverse(Ro::metadata(p).and_then(|m| m.modified()).ok()));
    v
}

/// 每一家记录根下此刻的全部会话记录文件。
fn record_files(state: &ReaderState) -> impl Iterator<Item = PathBuf> + '_ {
    state
        .roots
        .iter()
        .filter(|r| r.root.is_dir())
        .flat_map(|r| WalkDir::new(&r.root).into_iter().filter_map(Result::ok))
        .map(|e| e.into_path())
        .filter(|p| state.root_of(p).is_some())
}

/// 宣告会话时找它的记录文件：先查 [`ReaderState::sid_files`]（没建 ⇒ 各家的根整棵走一遍建起来）；表里没有这个 sid ⇒ 再走一遍
/// （会话刚起、记录文件的事件还没到）。交出来的同 [`find_sid_jsonls`]：此刻还在的那几份，修改时刻新的在前。
fn sid_jsonls(state: &mut ReaderState, sid: &str) -> Vec<std::path::PathBuf> {
    if state.sid_files.is_none() {
        state.sid_files = Some(walk_sid_files(state));
    }
    let mut v: Vec<std::path::PathBuf> = state
        .sid_files
        .as_ref()
        .and_then(|t| t.get(sid))
        .map(|set| set.iter().filter(|p| p.is_file()).cloned().collect())
        .unwrap_or_default();
    if v.is_empty() {
        let found = find_sid_jsonls(state, sid);
        if let (false, Some(t)) = (found.is_empty(), state.sid_files.as_mut()) {
            t.insert(sid.to_string(), found.iter().cloned().collect());
        }
        return found;
    }
    v.sort_by_key(|p| std::cmp::Reverse(Ro::metadata(p).and_then(|m| m.modified()).ok()));
    v
}

/// 各家的根整棵走一遍：每个 sid 的记录文件。
fn walk_sid_files(state: &ReaderState) -> HashMap<String, HashSet<PathBuf>> {
    let mut t: HashMap<String, HashSet<PathBuf>> = HashMap::new();
    for p in record_files(state) {
        if let Some(sid) = state.sid_of(&p) {
            t.entry(sid).or_default().insert(p);
        }
    }
    t
}

/// 一份路径动了（事件）：是某一家的会话记录 ⇒ 表跟上（在 ⇒ 记下；不在 ⇒ 摘掉）。表还没建 ⇒ 不管（建的时候整棵走）。
fn note_jsonl(state: &mut ReaderState, p: &Path) {
    let Some(sid) = state.sid_of(p) else {
        return;
    };
    let Some(table) = state.sid_files.as_mut() else {
        return;
    };
    if p.exists() {
        table.entry(sid).or_default().insert(p.to_path_buf());
    } else if let Some(set) = table.get_mut(&sid) {
        set.remove(p);
        if set.is_empty() {
            table.remove(&sid);
        }
    }
}

/// 一份记录的预读那一份（[`start_prime`] 从本线程的账上取下来、交给线程池带走）：起点是此刻记着的游标 · 指纹 · 行号 · 链索引。
pub(crate) struct FilePrime {
    path: PathBuf,
    key: PathBuf,
    prev: ReadCursor,
    tail: Option<Vec<u8>>,
    /// 行号计数器此刻的值（这一趟从 0 起 ⇒ 不用它，从 0 数）。
    seq: u64,
    /// 这份记录此刻的链索引（这一趟从 0 起 ⇒ 扔掉、从头记）。
    chain: Option<crate::agents::mainline::Chain>,
    /// 那一家的记录解释面（链事实 · 活动 · 子运行归属问它）。
    face: Option<crate::agents::RecordFace>,
    /// 子运行补记那一半（只用它的运行面与整台那本运行簿，[`crate::observe::runs::RunTrack::prime`]）。
    runs: Option<crate::observe::runs::RunTrack>,
    /// 骨架的家（进程里还没有这份的游标时从它接着读、读完落一份；`None` ⇒ 不读不落）。
    skel: Option<crate::observe::skeleton::Skeletons>,
}

/// 预读那一路落的骨架（[`crate::observe::skeleton`]）：读到哪了 · 行号 · 链索引 · 说到子运行的那几行原文 · 最后一件说了的活动。
#[derive(serde::Serialize, serde::Deserialize)]
struct PrimeSkel {
    mark: crate::observe::skeleton::Mark,
    seq: u64,
    #[serde(with = "crate::agents::mainline::skel")]
    chain: crate::agents::mainline::Chain,
    runs: Vec<String>,
    activity: Option<crate::agents::SessionActivity>,
}

/// 骨架里这一样叫什么（与冷开扫描图那一份分开两个文件）。
const PRIME_SKEL: &str = "prime";

/// 一条会话的预读任务：会话里的几份记录照顺序读（新的在前，同 `session_added.path` 那一格的取法）。
pub(crate) struct PrimeJob {
    sid: String,
    gen: u64,
    files: Vec<FilePrime>,
    /// 要不要顺手取名字那几样（这条会话头一次宣告 ⇒ 第一份记录的标题 · 第一句，走历史清单缓存）。
    label: Option<PathBuf>,
}

/// 一份记录预读完交回的：读到了 ⇒ 新游标 · 指纹 · 行号 · 链索引；不在了 ⇒ `gone`；读不出 ⇒ 都没有（游标不装，下一趟再来）。
pub(crate) struct FileDone {
    key: PathBuf,
    read: Option<FileRead>,
    gone: bool,
    /// 这一趟读了多少字节。
    bytes: u64,
}

pub(crate) struct FileRead {
    cursor: ReadCursor,
    tail: Vec<u8>,
    /// 读完时行号计数器该是的值（＝ 完整行数）。
    seq: u64,
    chain: Option<crate::agents::mainline::Chain>,
    /// 这一趟从 0 起（排队打字时刻表跟着作废）。
    from_zero: bool,
    /// 这份记录有骨架（照它读的 · 或读完落了一份）⇒ 实时接着记要的那几样（[`SkelLive`]）。
    skel: Option<SkelLive>,
}

/// 一份有骨架的大记录在账上接着记的：骨架的家 · 路径 · 上一次落盘时的游标 · 说到子运行的那几行原文 · 最后一件说了的活动 · 正在落的那一趟。
/// 实时读过门槛（[`crate::observe::skeleton::Skeletons::rewrite_after`]）⇒ 照账上此刻的游标 · 见证 · 行号 · 链索引现落一份（[`write_back`]）。
pub(crate) struct SkelLive {
    home: crate::observe::skeleton::Skeletons,
    path: PathBuf,
    stored: u64,
    runs: Vec<String>,
    activity: Option<crate::agents::SessionActivity>,
    writing: Option<std::thread::JoinHandle<()>>,
}

/// 实时读进来的过了门槛 ⇒ 在旁线程上回写这份记录的预读骨架（同一份上一趟还在落 ⇒ 这一次不落：晚一步只是下一次重启多读一截）。
fn write_back(state: &mut ReaderState, key: &Path) {
    let Some(cursor) = state.offsets.get(key).copied() else {
        return;
    };
    let Some(sl) = state.skel_live.get_mut(key) else {
        return;
    };
    if cursor.consumed < sl.stored + sl.home.rewrite_after(sl.stored)
        || sl.writing.as_ref().is_some_and(|h| !h.is_finished())
    {
        return;
    }
    let mtime = Ro::metadata(&sl.path)
        .ok()
        .filter(|m| m.len() == cursor.consumed)
        .and_then(|m| crate::observe::skeleton::mtime_of(&m));
    let body = PrimeSkel {
        mark: crate::observe::skeleton::Mark {
            len: cursor.seen_len.max(cursor.consumed),
            mtime,
            cursor: cursor.consumed,
            witness: state.tails.get(key).cloned().unwrap_or_default(),
        },
        seq: state.seqs.peek(&key.to_string_lossy()),
        chain: state.branches.get(key).cloned().unwrap_or_default(),
        runs: sl.runs.clone(),
        activity: sl.activity,
    };
    sl.stored = cursor.consumed;
    let (home, path) = (sl.home.clone(), sl.path.clone());
    // 线程：一份骨架回写（几十 MB 的写不挡 watcher 线程），落完即退。
    sl.writing = Some(std::thread::spawn(move || {
        home.store(&path, PRIME_SKEL, &body)
    }));
}

/// 一条会话预读完交回本线程的（[`primed`] 装回）。
pub(crate) struct PrimeDone {
    sid: String,
    gen: u64,
    files: Vec<FileDone>,
    /// 第一份记录里最后一件说了「此刻在干什么」的事（这一家的记录说这件事才有）。
    activity: Option<crate::agents::SessionActivity>,
    /// 名字那几样（[`PrimeJob::label`] 要了才有）。
    facts: Option<crate::history::search_rules::LabelFacts>,
}

/// 读完要落的骨架（交回之后再落：几十 MB 的写不挡「读完了」那一帧）。
struct SkelLater {
    home: crate::observe::skeleton::Skeletons,
    path: PathBuf,
    body: PrimeSkel,
}

fn store_skeletons(later: Vec<SkelLater>) {
    for l in later {
        l.home.store(&l.path, PRIME_SKEL, &l.body);
    }
}

/// 线程池里跑的那一趟（夹具没有池时就地跑同一个函数）：几份记录照顺序预读，再取名字。只读盘、不碰 watcher 的账。
/// 另回读完要落的骨架（调用方先交回、再落）。
fn run_prime(job: PrimeJob) -> (PrimeDone, Vec<SkelLater>) {
    let PrimeJob {
        sid,
        gen,
        files,
        label,
    } = job;
    let mut activity = None;
    let mut out = Vec::with_capacity(files.len());
    let mut later = Vec::new();
    for (i, f) in files.into_iter().enumerate() {
        let (done, said, skel) = prime_file_cursor(&sid, f);
        if i == 0 {
            activity = said;
        }
        out.push(done);
        later.extend(skel);
    }
    let done = PrimeDone {
        sid,
        gen,
        files: out,
        activity,
        facts: label.map(|p| crate::history::listing::label_facts_of(&p)),
    };
    (done, later)
}

/// Batch8-F25：宣告之后的预读——把 cursor 与 seq 计数器推进到当前
/// **最后一个完整行**（F14 torn-line 语义：残行不计数、留给 tail 阶段），
/// 不发任何行帧。之后 notify 到来的新行 seq == 此刻完整行数 L（行号语义），
/// 与 monitor 快照侧的 0..L'-1 编号同处一个行号空间，重叠区被 (sid,seq)
/// 去重精确吸收（-batch8 §2）。read8a 起它跑在线程池里：入参是从账上取下来的那一份，交回新的一份。
fn prime_file_cursor(
    session_id: &str,
    job: FilePrime,
) -> (
    FileDone,
    Option<crate::agents::SessionActivity>,
    Option<SkelLater>,
) {
    let FilePrime {
        path,
        key,
        mut prev,
        tail: mut probe,
        mut seq,
        chain: prev_chain,
        face,
        runs,
        skel,
    } = job;
    let chain = face.and_then(|f| f.chain);
    let said_activity = face.and_then(|f| f.activity);
    let key_str = key.to_string_lossy().into_owned();
    let none = |key: PathBuf, gone: bool| FileDone {
        key,
        read: None,
        gone,
        bytes: 0,
    };
    // 进程里还没有这份的游标（重启 · 头一次宣告）⇒ 盘上有骨架就照它：没变 ⇒ 不读；变长了 ⇒ 从骨架的游标接着读（先核见证）；
    // 别的 ⇒ 整份。读完（大记录）落一份新的。
    let fresh = prev == ReadCursor::default() && probe.is_none();
    let home = skel.filter(|_| fresh);
    let mut seeded: Option<PrimeSkel> = None;
    if let Some(k) = home
        .as_ref()
        .and_then(|h| h.load::<PrimeSkel>(&path, PRIME_SKEL))
    {
        let meta = match Ro::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return (none(key, true), None, None)
            }
            Err(_) => return (none(key, false), None, None),
        };
        match k
            .mark
            .look(meta.len(), crate::observe::skeleton::mtime_of(&meta))
        {
            crate::observe::skeleton::Look::Same => {
                if let Some(r) = runs.as_ref() {
                    r.prime(session_id, &said_lines(&k.runs));
                }
                let skel = home.as_ref().map(|h| SkelLive {
                    home: h.clone(),
                    path: path.clone(),
                    stored: k.mark.cursor,
                    runs: k.runs.clone(),
                    activity: k.activity,
                    writing: None,
                });
                let read = FileRead {
                    cursor: ReadCursor {
                        consumed: k.mark.cursor,
                        seen_len: k.mark.len,
                    },
                    tail: k.mark.witness,
                    seq: k.seq,
                    chain: chain.map(|_| k.chain),
                    from_zero: false,
                    skel,
                };
                let done = FileDone {
                    key,
                    read: Some(read),
                    gone: false,
                    bytes: 0,
                };
                return (done, k.activity, None);
            }
            crate::observe::skeleton::Look::Grown => {
                prev = ReadCursor {
                    consumed: k.mark.cursor,
                    seen_len: k.mark.len,
                };
                probe = Some(k.mark.witness.clone());
                seeded = Some(k);
            }
            crate::observe::skeleton::Look::Changed => {}
        }
    }
    // 冷接时游标在 0、而主记录可能上百 MB ⇒ 按块读（[`PrimeReader`]），不攒整份 —— 要的只是行数与说到子运行的那几行。
    // 「不在 ⇒ 丢游标 · 变短 / 改写 ⇒ 从 0 重数」与续读同一套；这里不出声：那句话由之后的 `process_jsonl` 说。
    let mut reader = match PrimeReader::open(&path, prev, probe.as_deref()) {
        Ok(r) => r,
        Err(Look::Gone) => return (none(key, true), None, None),
        Err(_) => return (none(key, false), None, None),
    };
    let from_zero = reader.from == 0;
    // 骨架的见证对不上 ⇒ 当没有骨架（整份）；对得上 ⇒ 行号 · 链 · 子运行那几行 · 活动接着骨架的。
    let (mut said_runs, seed_activity, seed_chain) = match seeded.filter(|_| !from_zero) {
        Some(k) => {
            seq = k.seq;
            if let Some(r) = runs.as_ref() {
                r.prime(session_id, &said_lines(&k.runs));
            }
            (k.runs, k.activity, Some(k.chain))
        }
        None => (Vec::new(), None, None),
    };
    // 从 0 起 ⇒ 行号从 0 数、链从头记；否则接着此刻的计数与链。
    let start = if from_zero && (prev.consumed > 0 || prev.seen_len > 0) {
        0
    } else {
        seq
    };
    let mut c = if from_zero {
        crate::agents::mainline::Chain::default()
    } else {
        seed_chain.or(prev_chain).unwrap_or_default()
    };
    let mut seqs = SeqCounter::new();
    let mut cursor = ReadCursor {
        consumed: reader.from,
        seen_len: reader.from,
    };
    let mut tail = std::mem::take(&mut reader.probe);
    let mut n_lines = 0;
    // 到这一趟读完为止最后一件说了「此刻在干什么」的事（这一家的记录说这件事才问；接着骨架读 ⇒ 起点是骨架记的那一件）。
    let mut activity = seed_activity;
    while let Some(block) = reader.next_block() {
        let Ok(block) = block else {
            // 读到一半出错：游标不装（至少一次语义，下一趟再来）；这一趟记的链一起扔掉。
            return (none(key, false), None, None);
        };
        // 每块恰好止于一个 `\n` ⇒ 当它是「读到这里为止」交给续读同一个扫描（数行），不会有残行。
        let (n, next) = count_new_lines_at(
            &block,
            cursor.consumed,
            cursor.consumed + block.len() as u64,
            cursor,
            &key_str,
            &mut seqs,
        );
        n_lines += n;
        cursor = next;
        // 这一块里派出 / 收场过的子运行补进运行簿（读完之后那一帧运行表带出去）。
        if let Some(r) = runs.as_ref() {
            r.prime(session_id, &block);
            // 要落骨架 ⇒ 过了预筛的那几行原文留着（读回来照样补进簿里）。
            if home.is_some() {
                said_runs.extend(
                    block
                        .split(|&b| b == b'\n')
                        .filter_map(|l| std::str::from_utf8(l).ok())
                        .filter(|t| r.hint(t))
                        .map(str::to_string),
                );
            }
        }
        // 链事实补进链索引（读完之后那一帧主线外清单带出去）。
        if let Some(chain) = chain {
            for line in block.split(|&b| b == b'\n') {
                if let Some(f) = std::str::from_utf8(line)
                    .ok()
                    .and_then(|t| chain(&crate::agents::Line::new(t, 0)))
                {
                    c.load(f);
                }
            }
        }
        if let Some(said) = said_activity {
            // 子运行的记录不说主运行此刻在干什么（与续读那一路同一条：`send_line`）。
            let run_of = face.and_then(|f| f.run_of);
            let main = |line: &crate::agents::Line<'_>| {
                run_of.is_none_or(|f| line.value().and_then(f).is_none())
            };
            for line in block
                .split(|&b| b == b'\n')
                .filter_map(|l| std::str::from_utf8(l).ok())
            {
                let line = crate::agents::Line::new(line, 0);
                if let Some(a) = said(&line).filter(|_| main(&line)) {
                    activity = Some(a);
                }
            }
        }
        tail.extend_from_slice(&block);
        let drop = tail.len().saturating_sub(TAIL_PROBE as usize);
        tail.drain(..drop);
    }
    // 链事实整份记完才算一次（逐条算是平方：hot1）。
    c.settle();
    cursor.seen_len = reader.len.max(cursor.consumed);
    tracing::debug!(
        "primed {key_str}: cursor→{} (+{n_lines} lines suppressed, tail seq starts here; chain walked {})",
        cursor.consumed,
        c.walked()
    );
    let bytes = reader.probed + (reader.len - reader.from);
    let seq = start + n_lines as u64;
    let mut later = None;
    if let Some(h) = home.as_ref().filter(|h| h.wants(reader.len)) {
        // 修改时刻只在读完时长度没再变才记（读的过程中又写进来 ⇒ 不记，下一次就不算「没变」）。
        let mtime = Ro::metadata(&path)
            .ok()
            .filter(|m| m.len() == reader.len)
            .and_then(|m| crate::observe::skeleton::mtime_of(&m));
        later = Some(SkelLater {
            home: h.clone(),
            path: path.clone(),
            body: PrimeSkel {
                mark: crate::observe::skeleton::Mark {
                    len: reader.len,
                    mtime,
                    cursor: cursor.consumed,
                    witness: tail.clone(),
                },
                seq,
                chain: c.clone(),
                runs: said_runs.clone(),
                activity,
            },
        });
    }
    let skel = later.as_ref().map(|l| SkelLive {
        home: l.home.clone(),
        path: path.clone(),
        stored: cursor.consumed,
        runs: said_runs,
        activity,
        writing: None,
    });
    (
        FileDone {
            key,
            read: Some(FileRead {
                cursor,
                tail,
                seq,
                chain: chain.map(|_| c),
                from_zero,
                skel,
            }),
            gone: false,
            bytes,
        },
        activity,
        later,
    )
}

/// 预读交回了（本线程）：装回游标 · 指纹 · 行号 · 链索引，然后按这个次序出帧 ——
/// ① `session_primed`（读完了、从第几行起）② 名字 / 活动变了的 `session_status` ③ 运行表 · 主线外清单
/// ④ 预读期间长出来的行（对这几份记录当场续读一次：从预读停下处接着读 ⇒ 第一帧 `line` 的 `seq` ＝ ① 的 `lines`，之后连续）。
/// 这条会话已退休 / 又被重新宣告（代号对不上）⇒ 丢掉这一趟（它补进运行簿的那几笔一起摘）。
fn primed(done: PrimeDone, state: &mut ReaderState, sink: &mut FrameSink) {
    let PrimeDone {
        sid,
        gen,
        files,
        activity,
        facts,
    } = done;
    // 池子空出一位 ⇒ 按此刻的名单交下一份（不论这一趟认不认得上）。
    if let Some(pool) = state.pool.as_mut() {
        pool.in_flight = pool.in_flight.saturating_sub(1);
    }
    dispatch_primes(state, sink);
    if state.priming.get(&sid) != Some(&gen) {
        if !state.active_sids.contains(&sid) {
            state.book.forget(&sid);
        }
        return;
    }
    state.priming.remove(&sid);
    sink.priming.remove(&sid);
    if let Some(pool) = state.pool.as_ref() {
        let load = pool.pool.load();
        tracing::debug!(
            "prime landed for {sid}: {} still priming here, {} running now (high {}, pool {})",
            state.priming.len(),
            load.running(),
            load.high(),
            pool.pool.size()
        );
    }
    let mut paths = Vec::new();
    for f in files {
        state.prime_bytes += f.bytes;
        let key_str = f.key.to_string_lossy().into_owned();
        match f.read {
            Some(r) => {
                if r.from_zero {
                    state.typed.remove(&f.key);
                }
                state.seqs.set(&key_str, r.seq);
                state.tails.insert(f.key.clone(), r.tail);
                state.offsets.insert(f.key.clone(), r.cursor);
                if let Some(c) = r.chain {
                    state.branches.insert(f.key.clone(), c);
                }
                match r.skel {
                    Some(sl) => {
                        state.skel_live.insert(f.key.clone(), sl);
                    }
                    None => {
                        state.skel_live.remove(&f.key);
                    }
                }
            }
            None if f.gone => forget_cursor(state, &f.key),
            None => {}
        }
        paths.push(f.key);
    }
    let path = state
        .sessions
        .values()
        .find(|e| e.sid == sid)
        .and_then(|e| e.path.clone());
    let lines = path
        .as_ref()
        .map(|p| state.seqs.peek(&path_key(Path::new(p)).to_string_lossy()));
    sink.send(Frame::SessionPrimed {
        sid: sid.clone(),
        path,
        lines,
    });
    if let Some(mut f) = facts {
        let custom = state.custom_title(&sid);
        state.renamed(
            &sid,
            |e| {
                f.project_dir = f.project_dir.take().or_else(|| e.project_dir.clone());
                f.custom = custom.clone();
                let changed = *e != f;
                *e = f.clone();
                changed
            },
            sink,
        );
    }
    // 那一家没有进程状态文件说此刻在干什么 ⇒ 当前那份记录里最后一件说了的事（适配层 `RecordFace.activity` 翻）。
    let unsaid = state
        .sessions
        .values()
        .filter(|e| e.sid == sid)
        .all(|e| e.activity.is_none());
    let was_reading = state.reading.remove(&sid);
    match (activity, unsaid) {
        (Some(a), true) => note_activity(&sid, a, state, sink),
        // 宣告时说的是「读记录中」、读完记录也没说 ⇒ 改口说不清（不留着一句已经不成立的话）。
        (None, true) if was_reading => {
            let e = state.sessions.values().find(|e| e.sid == sid);
            let (waiting_for, path) =
                e.map_or((None, None), |e| (e.waiting_for.clone(), e.path.clone()));
            sink.send(Frame::SessionStatus {
                sid: sid.clone(),
                face: crate::stream::wire::ActivityFace::of(None),
                name_face: state.name_face(&sid),
                waiting_for,
                path,
            });
        }
        _ => {}
    }
    // 这个会话此刻已有的子运行（宣告之前就派出去的那几个）：收进来、从头读一遍，有就发一帧运行表。
    let jsonls: Vec<PathBuf> = paths
        .iter()
        .filter(|k| state.offsets.contains_key(*k))
        .cloned()
        .collect();
    let mut runs_changed = false;
    for p in &jsonls {
        runs_changed |= adopt_runs(state, &sid, p);
    }
    if runs_changed || state.book.has_runs(&sid) {
        sink.send(state.runs_frame(&sid));
    }
    // 历史里已经有主线外的 ⇒ 整份发一次（之后变了才发）。
    for p in &jsonls {
        if state.branches.get(p).is_some_and(|c| !c.off().is_empty()) {
            send_branch(state, sink, &sid, p, true);
        }
    }
    // 预读期间长出来的那一截（文件事件那时没读）。
    for p in &jsonls {
        process_jsonl(p, state, sink);
    }
}

/// 预读的线程池：各会话并行（上限按 CPU 数），一条会话一趟、趟内照顺序读。做完经 watcher 的事件通道交回（`WatchEvent::Primed`）。
/// 池子里的线程只阻塞在任务队列上（[`crate::common::par::Pool`]），不睡、不定时。
/// 排队在本线程（[`dispatch_primes`] 按名单挑），池子里同时至多线程数那么多份 ⇒ 名单一变就能改先后，已交出去的不撤。
pub(crate) struct PrimePool {
    pool: crate::common::par::Pool<PrimeJob>,
    /// 还没交出去的（宣告的先后）。
    queue: std::collections::VecDeque<PrimeJob>,
    /// 交出去了、还没交回的份数。
    in_flight: usize,
}

impl PrimePool {
    fn start(events: std::sync::mpsc::Sender<WatchEvent>) -> Self {
        PrimePool {
            pool: crate::common::par::Pool::new("record-prime", move |job| {
                let (done, later) = run_prime(job);
                let _ = events.send(WatchEvent::Primed(done));
                store_skeletons(later);
            }),
            queue: std::collections::VecDeque::new(),
            in_flight: 0,
        }
    }
}

/// 骨架里那几行子运行原文接成一块（每行一个 `\n`），喂 [`crate::observe::runs::RunTrack::prime`]。
fn said_lines(lines: &[String]) -> Vec<u8> {
    let mut b = Vec::with_capacity(lines.iter().map(|l| l.len() + 1).sum());
    for l in lines {
        b.extend_from_slice(l.as_bytes());
        b.push(b'\n');
    }
    b
}

/// 冷接宣告那一趟每次至多读多少字节（一行比它长就接着读，直到那一行读完）。
const PRIME_BLOCK: usize = 1 << 20;

/// 冷接宣告那一趟的读者：从游标读到最后一个整行，**按块交出**（每块止于一个 `\n`；末尾的残行不交，留给续读）。
/// 起读处与续读（[`read_tail_from`]）同一套：变短 ⇒ 从 0；游标之前的末尾指纹对不上 ⇒ 从 0。
struct PrimeReader {
    file: RoFile,
    /// 从哪一处起读。
    from: u64,
    /// 起读处之前那几个指纹字节（从 0 起 ⇒ 空）。
    probe: Vec<u8>,
    /// 核指纹读了几个字节（对不上也读了）。
    probed: u64,
    /// 到此刻读到的长度（读完才是这一趟看到的文件长度）。
    len: u64,
    carry: Vec<u8>,
    done: bool,
}

impl PrimeReader {
    /// 不在 ⇒ `Err(Look::Gone)`；读不出 ⇒ `Err(Look::Unreadable)`。
    fn open(path: &Path, cursor: ReadCursor, tail: Option<&[u8]>) -> Result<Self, Look> {
        use std::io::{Read, Seek, SeekFrom};
        let meta_len = match Ro::metadata(path) {
            Ok(m) => m.len(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Look::Gone),
            Err(_) => return Err(Look::Unreadable),
        };
        let mut file = Ro::open(path).map_err(|_| Look::Unreadable)?;
        let mut from = 0;
        let mut probe = Vec::new();
        let mut probed = 0;
        if meta_len >= cursor.seen_len {
            let consumed = cursor.consumed.min(meta_len);
            let k = TAIL_PROBE.min(consumed);
            let mut got = vec![0u8; k as usize];
            file.seek(SeekFrom::Start(consumed - k))
                .and_then(|_| file.read_exact(&mut got))
                .map_err(|_| Look::Unreadable)?;
            probed = k;
            if tail.is_none_or(|t| t == got.as_slice()) {
                from = consumed;
                probe = got;
            }
        }
        file.seek(SeekFrom::Start(from))
            .map_err(|_| Look::Unreadable)?;
        Ok(Self {
            file,
            from,
            probe,
            probed,
            len: from,
            carry: Vec::new(),
            done: false,
        })
    }

    /// 下一块整行（读出错 ⇒ `Err`，之后不再有块）；读完 ⇒ `None`。
    fn next_block(&mut self) -> Option<Result<Vec<u8>, ()>> {
        use std::io::Read;
        let mut buf = vec![0u8; PRIME_BLOCK];
        while !self.done {
            let n = match self.file.read(&mut buf) {
                Ok(0) => {
                    self.done = true;
                    break;
                }
                Ok(n) => n,
                Err(_) => {
                    self.done = true;
                    return Some(Err(()));
                }
            };
            self.len += n as u64;
            self.carry.extend_from_slice(&buf[..n]);
            if let Some(last) = self.carry.iter().rposition(|&b| b == b'\n') {
                let rest = self.carry.split_off(last + 1);
                return Some(Ok(std::mem::replace(&mut self.carry, rest)));
            }
        }
        None
    }
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
        let may_be =
            crate::agents::tree_local_face().is_some_and(|f| (f.cmdline_may_be_agent)(&lower));
        if !may_be {
            return AddTimeVerdict::Imposter("cmdline");
        }
    }
    AddTimeVerdict::Alive
}

/// add-time 冒名判定的那一趟 /proc 读：活着且是写它的那个 claude ⇒ `Ok(当前 starttime ticks)`（调用方拿它当 #34 基线），
/// 否则 `Err(原因)`。起步初扫 / 对齐（`process_session_added`）与一次性扫描（[`running_sessions`]）同这一条。
fn add_time_check(pid: u32, bytes: &[u8], path: &Path) -> Result<Option<u64>, &'static str> {
    let current_ticks = proc_starttime(pid);
    match add_time_verdict(
        parse_procstart_ticks(bytes),
        current_ticks,
        start_epoch_from_ticks(current_ticks),
        file_mtime_epoch(path),
        proc_cmdline(pid).as_deref(),
    ) {
        AddTimeVerdict::Imposter(reason) => Err(reason),
        AddTimeVerdict::Alive => Ok(current_ticks),
    }
}

/// pidfile 目录（一次性扫描问注册表里后端盯着的那一家，`agents::pidfile_dir`）。
pub(crate) fn pidfile_dir(agent_home: &Path) -> PathBuf {
    crate::agents::pidfile_dir(agent_home)
}

/// pidfile 里那一家写下的工作目录（`cwd`）。
fn pidfile_cwd(bytes: &[u8]) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()?
        .get("cwd")?
        .as_str()
        .map(str::to_string)
}

/// 这份 pidfile 说的是后台会话（适配层判）；读不成 JSON ⇒ 不是。
fn is_background(bytes: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .is_ok_and(|v| crate::agents::pidfile_background(&v))
}

/// **一次性扫描**：`<agent_home>/sessions/` 下此刻活着的交互会话 `(sid, pid)` —— 判活与起步初扫同一条
/// （pid 在 · 不是后台任务 · add-time 冒名判定过）。给 `ccm` resume 用（由 `main` 注入，control 层不引用 observe）。
pub fn running_sessions(agent_home: &Path) -> Vec<(String, u32)> {
    let dir = pidfile_dir(agent_home);
    let Ok(entries) = Ro::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| is_session_json(p))
        .filter_map(|p| {
            let pid = file_stem_str(&p)?.parse::<u32>().ok()?;
            let bytes = Ro::read(&p).ok()?;
            let sid = parse_session_id(&bytes)?;
            (pid_alive(pid) && !is_background(&bytes) && add_time_check(pid, &bytes, &p).is_ok())
                .then_some((sid, pid))
        })
        .collect()
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
    let mtime = Ro::metadata(path).ok()?.modified().ok()?;
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

/// 这台 watcher 的发帧口：**整台一份**的那几样（会话账本 · 预读还没交回的会话）＋ **每条连接一份**的投影（[`Sub`]）。
///
/// 每一帧先过整台那一层一次（预读没交回的会话的内容帧拦下；会话账本决定它自己发不发、紧跟着补发哪几帧），
/// 再逐条连接按那条连接的名单投进它自己的有界通道（各自记丢帧，`Overflow` 按连接说）。
struct FrameSink {
    /// 此刻接着的连接（[`join`] 加、`Leave` 摘；通道关了的投帧时顺手摘）。
    subs: Vec<Sub>,
    /// 整台的会话账本（`observe::session_ledger`）：每一帧发出去之前过它，
    /// 它补发可重连 / 已结束的成品帧、压住 `sessions_replayed` 直到第一份 tmux 快照。生产由 [`watch_loop`] 装上；
    /// 夹具走 [`FrameSink::new`] 不装（它们钉的是 watcher 自己发的帧）。
    ledger: Option<crate::observe::session_ledger::SessionLedger>,
    /// 预读还没交回的会话：它们的整行帧（[`Frame::content_of`]）一帧不发（[`start_prime`] 加、[`primed`] 摘）。
    priming: HashSet<String>,
}

/// 一条流连接在这台 watcher 上的投影：它的有界通道 · 它的名单 · 它欠的丢帧账。
///
/// The per-connection send half: a bounded-channel sender that turns a wedged pipe
/// into an explicit **overflow signal** (#32) instead of silently losing data.
///
/// A full channel means the writer/SSH pipe is wedged. We still `try_send` (never
/// blocking the notify reader), but now we **count** the frames we had to drop
/// and, once the channel drains enough to accept it, emit a single
/// [`Frame::Overflow`] carrying that count. The client warns the user that live
/// lines were lost. One signal per congestion burst — naturally throttled.
pub(crate) struct Sub {
    id: u64,
    tx: mpsc::Sender<Frame>,
    /// 这条连接看的那一台的时区（按连接现写的那几帧用）。
    tz: crate::Tz,
    /// Frames dropped since the last successfully-sent `Overflow` signal.
    dropped: u64,
    /// 那批丢帧里**不可恢复**的那些的身份。
    ///
    /// 只有计数的 `Overflow` 对内容帧够用（行还在远端 jsonl 里），对状态增量帧不够：
    /// 它是一次差分的结果、别处不存在，客户端拿着「丢了 N 条」没法重同步。
    lost: Vec<LostFrame>,
    /// 身份表触顶过（超出 [`LOST_IDENTITY_CAP`] 的那些只计数、不留身份）。
    lost_truncated: bool,
    /// 这条流此刻在看哪几个会话（`stream-watch` 报的）：`None` ＝ 没报过 ＝ 全看。
    /// 不在名单里的会话，它的整行帧（[`Frame::content_of`]）不投给这条连接，别的帧不挑。
    watching: Option<HashSet<String>>,
    /// 这条连接要哪几个会话的主线外清单（`stream-watch.branch`，只算在看的）：不在里面的会话，清单一帧不投给它。
    branching: HashSet<String>,
    /// 丢过它一帧 `session_branch` 的那几份记录（路径）：只发变了的那一截，丢了一帧手上的就对不上 ⇒ 下一次给整份（[`FrameSink::branch`]）。
    branch_stale: HashSet<String>,
    /// 刚要起清单、还没给它发过的那几份记录：它交回的手上那份（路径 ⇒ 条数 · 摘要）。头一次给它这份时照它只补缺的那一截，用完即摘。
    branch_held: HashMap<String, (u64, String)>,
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
    /// 生产那一份：带会话账本，起步没有连接（[`join`] 接上来）。
    fn hub() -> Self {
        FrameSink {
            subs: Vec::new(),
            ledger: Some(crate::observe::session_ledger::SessionLedger::new()),
            priming: HashSet::new(),
        }
    }

    /// 一份 tmux 观测从这一刻起了（交会话账本：此前摘除的等它回来裁去向）。
    fn observing(&mut self) {
        if let Some(l) = self.ledger.as_mut() {
            l.observing();
        }
    }

    /// 会话账本在等一份还没起的 tmux 观测。
    fn awaits_observation(&self) -> bool {
        self.ledger.as_ref().is_some_and(|l| l.awaits_observation())
    }

    /// 一份 tmux 观测：只交会话账本（有的话），它补发的成品帧照常发；观测本身不上线。
    fn tmux(&mut self, obs: TmuxObservation) {
        let Some(l) = self.ledger.as_mut() else {
            return;
        };
        let (raw, observation) = observation_parts(obs);
        for f in l.on_tmux(&raw, observation) {
            self.fan(f);
        }
    }

    /// 发一帧：预读没交回的会话的内容帧拦下；再过会话账本（有的话）—— 它决定这一帧自己发不发、紧跟着补发哪几帧。
    fn send(&mut self, frame: Frame) {
        if frame
            .content_of()
            .is_some_and(|sid| self.priming.contains(sid))
        {
            return;
        }
        let (pass, extra) = match self.ledger.as_mut() {
            Some(l) => l.on_frame(&frame),
            None => (true, Vec::new()),
        };
        if pass {
            self.fan(frame);
        }
        for f in extra {
            self.fan(f);
        }
    }

    /// 按连接现写的一帧（体按各连接的时区写，如任务清单变了那一帧）：不过会话账本（它只看会话那几帧）。
    fn send_each(&mut self, make: impl Fn(&crate::Tz) -> Frame) {
        for s in &mut self.subs {
            let f = make(&s.tz);
            if s.wants(&f) {
                s.push(f);
            }
        }
        self.subs.retain(|s| !s.tx.is_closed());
    }

    /// 只给一条连接（名单补发 · 接上来那一份）：不过会话账本、不看名单。
    fn send_to(&mut self, id: u64, frame: Frame) {
        if let Some(s) = self.subs.iter_mut().find(|s| s.id == id) {
            s.push(frame);
        }
    }

    /// 逐条连接按名单投（最后一条拿原帧，前面的拿副本）；通道关了的连接顺手摘掉。
    fn fan(&mut self, frame: Frame) {
        let to: Vec<usize> = (0..self.subs.len())
            .filter(|&i| self.subs[i].wants(&frame))
            .collect();
        if let Some((&last, rest)) = to.split_last() {
            for &i in rest {
                self.subs[i].push(frame.clone());
            }
            self.subs[last].push(frame);
        }
        self.subs.retain(|s| !s.tx.is_closed());
    }

    /// 一份记录的主线外清单变了：给在看它的每条连接发「前 `kept` 条照旧，其后是这些」（按页切几帧，[`Frame::branch_pages`]）；
    /// 丢过这份一帧的连接（[`Sub::push`] 记下的）手上的对不上 ⇒ 给它整份。预读没交回的会话不发（同 [`Self::send`]）。
    fn branch(&mut self, sid: &str, path: &str, off: &[String], kept: usize) {
        if self.priming.contains(sid) {
            return;
        }
        let delta = Frame::branch_pages(sid, path, off, kept);
        let mut whole: Option<Vec<Frame>> = None;
        for s in &mut self.subs {
            if !delta.first().is_some_and(|f| s.wants(f)) {
                continue;
            }
            let frames = if s.branch_held.contains_key(path) {
                // 刚要起这份、预读那会儿没发成：照它交回的手上那份补（[`Sub::held_at`]）。
                let at = s.held_at(path, off);
                Frame::branch_pages(sid, path, off, at)
            } else if s.branch_stale.remove(path) {
                whole
                    .get_or_insert_with(|| Frame::branch_pages(sid, path, off, 0))
                    .clone()
            } else {
                delta.clone()
            };
            for f in frames {
                s.push(f);
            }
        }
        self.subs.retain(|s| !s.tx.is_closed());
    }

    /// 给那一条连接这份记录的主线外清单（它刚要起这份）：它交回的手上那份前缀没动、也没丢过帧 ⇒ 只发它缺的那一截
    /// （`at` ＝ 它手上的条数）；否则整份。
    fn branch_to(&mut self, id: u64, sid: &str, path: &str, off: &[String]) {
        if let Some(s) = self.subs.iter_mut().find(|s| s.id == id) {
            let at = s.held_at(path, off);
            for f in Frame::branch_pages(sid, path, off, at) {
                s.push(f);
            }
        }
    }

    /// 那条连接走了。
    fn leave(&mut self, id: u64) {
        self.subs.retain(|s| s.id != id);
    }
}

impl Sub {
    pub(crate) fn new(id: u64, tx: mpsc::Sender<Frame>, tz: crate::Tz) -> Self {
        Sub {
            id,
            tx,
            tz,
            dropped: 0,
            lost: Vec::new(),
            lost_truncated: false,
            watching: None,
            branching: HashSet::new(),
            branch_stale: HashSet::new(),
            branch_held: HashMap::new(),
        }
    }

    /// 这条连接手上这份记录的主线外清单从第几条起要补：它交回的那份（[`Self::branch_held`]，用完即摘）前缀摘要对得上、
    /// 这条连接也没丢过它的帧 ⇒ 它手上的条数；否则 0（整份）。
    fn held_at(&mut self, path: &str, off: &[String]) -> usize {
        let stale = self.branch_stale.remove(path);
        match self.branch_held.remove(path) {
            Some((n, sum)) if !stale => usize::try_from(n)
                .ok()
                .filter(|&n| n <= off.len() && crate::stream::wire::branch_sum(&off[..n]) == sum)
                .unwrap_or(0),
            _ => 0,
        }
    }

    /// 这条连接要不要这一帧（只挑整行帧：名单外的会话不投）。
    fn wants(&self, frame: &Frame) -> bool {
        if let Frame::SessionBranch { sid, .. } = frame {
            return self.branching.contains(sid);
        }
        match (&self.watching, frame.content_of()) {
            (Some(w), Some(sid)) => w.contains(sid),
            _ => true,
        }
    }

    /// Send `frame`, first flushing any owed overflow signal.
    ///
    /// Order matters: we try to emit the pending `Overflow` *before* the real
    /// frame so the client learns "you lost N frames" no later than the next
    /// frame it receives. If the channel is still full, we keep owing the count
    /// (it only ever grows until a send succeeds); a closed channel is a quiet
    /// shutdown (the sink drops this connection).
    fn push(&mut self, frame: Frame) {
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
                if let Frame::SessionBranch { path, .. } = &frame {
                    self.branch_stale.insert(path.clone());
                }
                // **不可恢复的那些要留下身份**，否则客户端只知道
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
                // Writer gone. The sink drops this connection on its next fan-out.
            }
        }
    }
}

/// `true` for a `sessions/<PID>.json` file. We only ever feed this paths under
/// the sessions dir, so an extension check suffices.
fn is_session_json(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "json")
}

fn file_stem_str(p: &Path) -> Option<String> {
    p.file_stem().and_then(|s| s.to_str()).map(str::to_string)
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/watcher_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/watcher_lines_tests.rs"]
mod lines_tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/watcher_watch_tests.rs"]
mod watch_tests;

#[cfg(all(test, target_os = "linux"))]
#[path = "../../../tests/backend/observe/watcher_writer_tests.rs"]
mod writer_tests;

#[cfg(all(test, target_os = "linux"))]
#[path = "../../../tests/backend/observe/watcher_prime_tests.rs"]
mod prime_tests;
