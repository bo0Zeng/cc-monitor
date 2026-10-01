//! Phase-0 backend watcher: tails `<claude_dir>/projects/**.jsonl` plus the
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
//! # 增量读的规则（〔TL1 · 4C〕今天全仓只有这一份）
//!
//! 〔CF1 起 monitor 自己那份 jsonl 读者删了：本机会话内容也走本机后端的 `line` 帧（`设计/00 §2.5 ②`）。
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
//! 〔RENDER2〕[`process_jsonl`] restarts the per-file seq after announcing the
//! re-read (`session_file_reread`) — seq is the line number in the file as it is
//! now, never a number past it (see [`process_jsonl`]).
//!
//! On a mid-read I/O error this reader gives up the whole pass (cursor untouched)
//! — at-least-once-safe. (The deleted monitor copy kept the complete lines it had
//! already consumed; that difference left with it.)
//!
//! ⚠ 这段话此前写的是 "reads via one `fs::read` snapshot" —— 那**曾经是真的**，
//! 而它只评了**错误语义**那一面，对**内存与 IO 后果一个字没记**（audit-0805 B-4）：
//! 每个 debounce 事件把整份 jsonl 读进内存，257 MB 的活跃会话 ⇒ 每次读 257 MB，
//! 只为提取约 500 字节的新行；而本地 monitor 同一件事一直是 `seek` + `read_until`
//! ⇒ **强机器流式、弱机器整读，正好反了**（backend 住树莓派那类小机器）。
//! F04 已改成只读新字节（`read_tail_from` + `read_new_lines_at`），
//! 由 `the_two_tail_readers_do_not_slurp_the_whole_session_file` 钉住不许改回去。
//! ★ 留这段话是因为**「未登记的缺陷」比「登记过的取舍」更难发现** ——
//! 当时那条头注读起来完全像一条深思熟虑的取舍。

use crate::stream::wire::{Frame, LostFrame, RemovalCause, RereadWhy, SeqCounter};
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
    /// 〔RESYNC · `设计/15 §4.1b`〕用户按了「重新对齐」：与起步同一套（耳朵重挂 · pidfile 对表 · 重探 tmux · 账号清单），
    /// 做完把差异回给 `done`。`only` = 只对这一个 sid（关卡 2「对齐后重试」：重验 ＋ 重打）。
    Resync {
        only: Option<String>,
        done: std::sync::mpsc::Sender<Reconciled>,
    },
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
/// 挡的是「backend 退出后 pid 被复用，hook 误伤无关进程」。
fn install_tmux_hooks_best_effort() {
    let me = std::process::id();
    let (Ok(exe), Some(start)) = (std::env::current_exe(), proc_starttime(me)) else {
        tracing::warn!("拿不到自身 exe/starttime ⇒ 跳过装 tmux hook（退回定时探测）");
        return;
    };
    let n = crate::control::tmux_hook::install_hooks(&exe, me, start);
    tracing::info!("tmux hook 已装 {n}/3（会话生/死/改名 → SIGUSR1 → 立刻重探）");
}

/// 〔MIG-3b · `99 §2.1 ㉓②`〕这一批文件事件落在哪几个会话的任务目录里（`<tasks>/<sid>/…` 的第一段；`<tasks>` 自己不算）。
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

/// 〔SR1a〕这一批文件事件里有没有那份账号 manifest。**抽出来是为了判据**：
/// 事件循环本身要起 tmux 探测，单测不许碰用户真实的 tmux server（`C7i`）。
fn manifest_touched<'a>(mut paths: impl Iterator<Item = &'a Path>, manifest: &Path) -> bool {
    paths.any(|p| p == manifest)
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
/// monitor 一连上就该拿到 tmux 状态。全删的话，backend 要等到第一个 hook 触发才会探，
/// 空闲机器上可能是**永远**。所以节拍没了，但这一拍要留下。
///
/// 之后的每一拍都由事件驱动：tmux hook → `--tmux-notify` → SIGUSR1 → `WatchEvent::Poke`
/// （P4），server 生死由 pidfd / socket inotify 管（P3）。**零定时器。**
fn initial_tmux_probe(tx: &std::sync::mpsc::Sender<WatchEvent>) {
    // send 失败 = reader 已经走了 ⇒ 无所谓。
    let _ = tx.send(WatchEvent::TmuxProbeDue);
}

/// 起一次 tmux 探测（一次性后台线程，结果回 `TmuxObserved`）。已有在途的就不起 —— `inflight` 只在这里置位、
/// 只在收到 `TmuxObserved` 时清。回真 = 这次真起了。
fn start_tmux_probe(inflight: &mut bool, tx: &std::sync::mpsc::Sender<WatchEvent>) -> bool {
    if *inflight {
        return false;
    }
    *inflight = true;
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

/// notify-debouncer-mini debounce window.（〔TL1〕从前写「与 monitor 那份读者对齐」—— 那份 CF1 删了。）
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

/// 〔VIS2 · `设计/15 §4.7 S3`〕可重入地挂 `agent_home` 本身；不在 ⇒ 退一层挂它的上一层（同 tmux socket 目录）。回 `true` = 这次刚挂上它本身。
/// 上一层挂上不摘：它可能与别的挂点同一目录（`unwatch` 会连带摘掉），留着也听得见 `agent_home` 被删后重建。
/// 〔GAP1〕`what` = 日志里怎么称呼它（账号目录也走这一个函数，`设计/05 §13.6` 第 3 条）。
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

/// 〔GAP1 · `设计/05 §13.6` 第 3 条〕账号 manifest 所在目录那道耳朵：起步不在 ⇒ 挂它的上一层；
/// 它出现 / 被删重建（它自己路径上的事件）⇒ 按盘上此刻重挂（与 `agent_home` 同一个函数，VIS2 S3 同法）。
struct AccountsEar {
    dir: PathBuf,
    watched: bool,
    parent_watched: bool,
}

impl AccountsEar {
    fn new(dir: &Path) -> Self {
        AccountsEar {
            dir: dir.to_path_buf(),
            watched: false,
            parent_watched: false,
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
    }

    /// 事件落在账号目录自己身上（出现 / 消失）⇒ 重挂，回 `true`（清单可能跟着变了 ⇒ 调用方发一帧）。
    fn on_path(
        &mut self,
        debouncer: &mut notify_debouncer_mini::Debouncer<impl notify::Watcher>,
        p: &Path,
    ) -> bool {
        if p != self.dir.as_path() {
            return false;
        }
        rewatch_agent_home(
            debouncer,
            &self.dir,
            "accounts dir",
            &mut self.watched,
            &mut self.parent_watched,
        );
        true
    }
}

/// 〔VIS2 · S3〕`agent_home` · `projects/` · `sessions/` 三道耳朵：起步 [`Self::arm`]，之后每个事件路径交 [`Self::on_path`]。
/// 抽出来是为了真 inotify 判据与 `watch_loop` 走同一段代码。
struct HomeEars {
    agent_home: PathBuf,
    projects: PathBuf,
    sessions: PathBuf,
    /// 〔MIG-3b · `99 §2.1 ㉓②`〕`<agent 家>/tasks/`（递归）：会话的任务清单变了 ⇒ 一帧 `tasks_changed{sid}`。
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
        // ★★ `P0b-Y2`〔08-13〕：**监视 `agent_home` 本身** —— 这是「子目录出现/被换掉」的唯一耳朵。
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
        // ★★ `projects/` 也走**可重入**挂法〔08-13〕：它是 jsonl 的来源，
        //    被换 inode 之后**行帧再也不来**（实测：删掉重建后写入，line 帧停在 1）。
        //    这是 `sessions/`（第十拍）与 socket 目录（第二十二拍）之后的**同族第三个**。
        rewatch_dir(
            debouncer,
            &self.projects,
            &mut self.projects_watched,
            RecursiveMode::Recursive,
        );
        // 〔MIG-3b〕`tasks/` 同一套可重入挂法（不在 ⇒ 等它作为 `agent_home` 里的一个事件出现再挂）。
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
            // 〔VIS2 · S3〕它出现时是 `agent_home` 里的一个事件 ⇒ `on_path` 重挂 ＋ 重扫（原来那句「不会再重试」是假话）。
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
        // 〔VIS2 · S3〕`agent_home` 刚出现 ⇒ 挂上它本身，再把它下面此刻已在的两个目录挂上（`sessions/` 带重扫）。
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
        // 〔MIG-3b〕`tasks/` 刚出现 / 换了 inode ⇒ 重挂（同族第四个）。
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

/// ★★ `P0b-Y2` 第十六拍〔08-13〕：**tmux socket 目录的推算规则**（零 server 时唯一的耳朵）。
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
}

pub fn spawn(
    agent_home: PathBuf,
    with_bg: bool,
    tail_only: bool,
    with_rbind_token: bool,
    book: std::sync::Arc<crate::observe::runs::RunBook>,
) -> (mpsc::Receiver<Frame>, WatcherPoke) {
    let (tx, rx) = mpsc::channel::<Frame>(CHANNEL_CAPACITY);
    // P4：事件 channel 从 `watch_loop` 内部**上提到这里**造 —— 因为 poke 句柄必须在线程
    // 起来之前就能交给 `main`。**仍然只有这一条 channel**（账本第 1 行不许再开第二条）：
    // 交出去的是同一个 sender 的 clone，`watch_loop` 收的是同一条的 receiver。
    let (events_tx, events_rx) = std::sync::mpsc::channel::<WatchEvent>();
    let poke = WatcherPoke(events_tx.clone());
    // 〔RESYNC〕登记在起线程之前：刚 spawn 完的那一刻来的 SIGUSR1 / `resync` 也够得着它。
    let me = live_enter(events_tx.clone());
    std::thread::Builder::new()
        .name("jsonl-watcher".into())
        .spawn(move || {
            watch_loop(
                agent_home,
                tx,
                with_bg,
                tail_only,
                with_rbind_token,
                book,
                events_tx,
                events_rx,
            );
            live_leave(me);
        })
        .expect("spawn jsonl-watcher thread");
    (rx, poke)
}

/// 〔RESYNC〕此刻在跑的 watcher（常驻后端每条连接一份 ＋ 空转那一份 / stdio 那一份）—— **唯一的名单**：
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

/// 〔RESYNC · `设计/15 §4.1b`〕**手动对齐**：每一份在跑的 watcher 都做一次与起步同一套的对齐，等它们都做完。
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
/// 起步一次；〔RESYNC · `设计/15 §4.1b`〕「重新对齐」再调同一个（各挂法都先摘再挂，重挂无害、漏挂新 inode 致命）。
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
    // 〔SR1a · `设计/05 §13.6 ③`〕**账号清单变了 ⇒ 一帧 `accounts_changed`**。监视 manifest 所在目录
    // （NonRecursive；写 manifest 常是「写临时文件再 rename」，盯文件本身会在 rename 之后失聪）。
    // 〔GAP1〕目录起步不在 / 被删重建 ⇒ 由 `AccountsEar` 挂上一层等它、出现时重挂（原先这里失聪）。
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
    tx: mpsc::Sender<Frame>,
    with_bg: bool,
    tail_only: bool,
    with_rbind_token: bool,
    book: std::sync::Arc<crate::observe::runs::RunBook>,
    events_tx: std::sync::mpsc::Sender<WatchEvent>,
    events_rx: std::sync::mpsc::Receiver<WatchEvent>,
) {
    // U2 Phase D 审计 重要-2：`projects` 这个目录名原本有**五**处，不是 `agents/claudecode/paths.rs`
    // 注释里写的四处 —— 这是第五处（内联的，grep `fn projects_root` 找不到它）。
    // 不收的话「合并去重」承诺的性质（改布局只改一处）根本没拿到。
    let projects = crate::agents::claudecode::paths::projects_root(&agent_home);
    let sessions = pidfile_dir(&agent_home);
    // 〔SR1a〕账号 manifest（`设计/05 §13.6 ③`「账号清单变了」一帧）。
    let accounts_manifest = crate::observe::accounts_query::default_manifest_path();

    let mut state = ReaderState::new(projects.clone(), with_bg, tail_only);
    // 〔`设计/80 §8.7` 步 2〕注入「客户端索要了启动期令牌」这一位。**不进 `new` 的签名**
    // 的理由写在那个字段的头注里（同 `events_tx` 那条既有纪律）。
    state.with_rbind_token = with_rbind_token;
    // 运行簿：与这条连接的流归位共用一本（流按它定归哪个运行）。
    state.runs.book = book;
    // All frames go out through a FrameSink: a bounded-channel sender that counts
    // frames dropped on a full channel and emits a single `Overflow` signal once
    // the channel drains enough to accept it (#32). Never blocks this reader.
    let mut sink = FrameSink::with_ledger(tx);

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

    // --- Phase 1: synchronous initial scan. ---
    // Mirror the LOCAL watcher's `active_filter` (`session_map.is_session_active`):
    // only stream sessions whose PID is alive (sessions/<PID>.json + /proc/<pid>).
    // We scan sessions/ FIRST to build the active set; process_session_added marks
    // the sid active and rescans its jsonl so an already-running session snapshots
    // on startup. We deliberately do NOT walk projects/ unconditionally — pulling
    // every historical jsonl as a Tab is the bug this fixes; browsing history is
    // the Ctrl+H history browser's job (Phase 1 for remote).
    initial_session_scan(&sessions, &mut state, &mut sink);

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
    // 〔VIS2 · S3〕三道耳朵（`agent_home` 不在 ⇒ 先挂它的上一层）＋ tmux socket 目录 ＋ 账号目录：起步与 `resync` 都经 [`arm_ears`]。
    let mut ears = HomeEars::new(&agent_home, &projects, &sessions);
    let sock_dir = tmux_socket_dir();
    // `sock_dir_watched` = 目录**本身**挂上了没有（没挂上时 `arm_ears` 退一层监视它的父）。
    let mut sock_dir_watched = false;
    let mut accounts_ear = accounts_manifest.parent().map(AccountsEar::new);
    arm_ears(
        &mut debouncer,
        &mut ears,
        &sock_dir,
        &mut sock_dir_watched,
        accounts_ear.as_mut(),
    );
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
                // 〔GAP1〕账号目录自己出现 / 消失 ⇒ 先重挂；它也算「清单可能变了」。
                let mut dir_moved = false;
                if let Some(ear) = accounts_ear.as_mut() {
                    for ev in &events {
                        dir_moved |= ear.on_path(&mut debouncer, &ev.path);
                    }
                }
                // 〔SR1a〕一批里 manifest 动了几次都只报一帧（批内合并；下一批再动再报）。
                if manifest_touched(
                    events.iter().map(|ev| ev.path.as_path()),
                    &accounts_manifest,
                ) || dir_moved
                {
                    sink.send(Frame::AccountsChanged);
                }
                // 〔MIG-3b · `99 §2.1 ㉓②`〕任务目录里的动静 ⇒ 每个动过的会话一帧 `tasks_changed`（批内合并）。
                for sid in tasks_touched(events.iter().map(|ev| ev.path.as_path()), &ears.tasks) {
                    sink.send(Frame::TasksChanged { sid });
                }
                for ev in events {
                    let p = ev.path.as_path();
                    // ★★ `P0b-Y2`：**`sessions/` 换了 inode 或刚出现 ⇒ 重挂 + 重扫**；`projects/` 同族；
                    // 〔VIS2 · S3〕`agent_home` 自己刚出现 ⇒ 挂上它、再挂它下面那两个。
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
                    // ⚠ 去掉 `&& !sock_dir_watched`〔08-13〕：目录**被删掉再重建**时
                    //   `sock_dir_watched` 仍是 true ⇒ 那个条件会把重挂整个短路掉，
                    //   于是新 inode 永远没人听（实测：新起的 server 一帧都收不到）。
                    if p == sock_dir.as_path() || p.parent() == Some(sock_dir.as_path()) {
                        watch_sock_dir_if_present(&mut debouncer, &sock_dir, &mut sock_dir_watched);
                    }
                    if watched_socket.as_deref() == Some(p) || in_sock_dir {
                        start_tmux_probe(&mut tmux_inflight, &events_tx);
                        continue;
                    }
                    // 子运行的记录（适配层说住哪）：读新行进运行簿、表变了发一帧；不发 `line`（它们不属于主时间线）。
                    let child = if p.starts_with(&state.projects) {
                        state.runs.on_path(p)
                    } else {
                        None
                    };
                    if let Some((sid, changed)) = child {
                        if changed {
                            sink.send(state.runs.frame(&sid));
                        }
                    } else if is_jsonl(p) {
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
                // 再兜一层去重）。**注意这只让快照更新鲜，不是本 bug 的修复** ——
                // 真正的修复是 `RemovalCause::Superseded`（让 monitor 不必依赖这份快照）。
                //
                // 🔴 〔09-09 订正〕上面这段原先还有一句：「`@ccm_sid` 由 `shared/ccm` 的
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
                    if start_tmux_probe(&mut tmux_inflight, &events_tx) {
                        sid_drifted = false;
                    }
                }
            }
            WatchEvent::Notify(Err(errs)) => tracing::warn!("debouncer failed: {errs:?}"),
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
                start_tmux_probe(&mut tmux_inflight, &events_tx);
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
                    tracing::info!("tmux 会话 {name} 已关闭（快照差分）");
                }
                // 〔MIG-1 续 · V41〕四态观测只喂这条流的会话账本（成品帧由它发），快照本身不上线。
                sink.tmux(obs);
                // 〔RESYNC · `设计/15 §4.1b`〕探测结果到达 ⇒ 顺手对账身份标签（hook 不报选项变化）。
                // 真改了 ⇒ 刚发的那份快照里的 `@ccm_sid` 已过期 ⇒ 再探一次（下一次全是 AlreadyCurrent，不会连环）。
                if retag_tracked(&state, None) > 0 {
                    start_tmux_probe(&mut tmux_inflight, &events_tx);
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
                    sink.tmux(TmuxObservation::NoServer);
                }
            }
            // 〔RESYNC · `设计/15 §4.1b`〕与起步同一套：耳朵重挂 · 账号清单重读（整机时）· pidfile 对表（顺手对账标签）· 重探 tmux。
            WatchEvent::Resync { only, done } => {
                if only.is_none() {
                    arm_ears(
                        &mut debouncer,
                        &mut ears,
                        &sock_dir,
                        &mut sock_dir_watched,
                        accounts_ear.as_mut(),
                    );
                    sink.send(Frame::AccountsChanged);
                }
                let got = resync_sessions(&sessions, &mut state, &mut sink, only.as_deref());
                start_tmux_probe(&mut tmux_inflight, &events_tx);
                let _ = done.send(got);
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
    /// active (so its existing lines stream the moment the session is announced;
    /// 〔TL1〕monitor 那一侧从前的「会话出现就强制重扫」随它自己的读者 CF1 删了，今天只剩这一处).
    projects: PathBuf,
    /// Per-file consumed byte offset, keyed by [`path_key`]. Reset to 0 on
    /// truncation; the seq lives separately in [`Self::seqs`] and is restarted
    /// together with it by [`process_jsonl`] (〔RENDER2〕seq = 当前文件里的行号).
    offsets: HashMap<PathBuf, ReadCursor>,
    /// 〔FW1 · 第四波 4D · D-d〕每份 jsonl **已消费前缀的末尾那几个字节**（至多 [`TAIL_PROBE`]）。
    /// 续读之前核一遍：对不上 ⇒ 这份文件在游标之前被原地改写过（整份覆盖、长度没变短）⇒ 当截断办：从 0 重读并出声。
    /// 与 `offsets` 同键、同生同灭（丢游标时一起丢）。
    tails: HashMap<PathBuf, Vec<u8>>,
    /// 〔FW1 · D-d〕已经说过「记录文件不见了」的那几份（每次「在 → 不在」只说一次；再出现就摘掉）。
    gone: HashSet<PathBuf>,
    /// Per-file seq source. 〔RENDER2〕只在「从 0 重读、且已先发出 `session_file_reread`」那一刻归零
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
    /// Batch7-F24：`--with-bg` 时放行 kind:"bg" 会话（宣告+流行，帧带元信息）；
    /// 默认 false = Batch6-F21 行为（bg 不算会话）。
    with_bg: bool,
    /// Batch8-F25：`--tail-only` 时连接不重放历史——初扫/宣告只推进 cursor 与
    /// seq 计数器到当前完整行数 L（行号语义，之后新行 seq 从 L 起），零行帧；
    /// 历史由 monitor 经 `--read-session` 旁路快照拉取（0..L'-1 由 monitor 编号，
    /// 重叠区被 (sid,seq) 去重吸收）。默认 false = 全量重放（旧 monitor 兼容）。
    tail_only: bool,
    /// 〔`设计/80 §8.7` 步 2〕`--with-rbind-token`：客户端**显式索要**
    /// `session_added` 上的 `rbind_token`（启动期令牌）。
    ///
    /// **默认 false，而且刻意不进 [`ReaderState::new`] 的签名** —— 两个理由：
    /// ① 与 `events_tx` 同一条既有纪律（那条头注逐字：「11 处 `ReaderState::new`
    ///    因此无需改签名」）；
    /// ② 语义上 false 才是对的默认：令牌是敏感数据，**没人索要就不读**
    ///    （论证住 `wire.rs` 那个字段的头注）。
    /// 生产路由 [`watch_loop`] 注入；夹具直接置字段。
    with_rbind_token: bool,
    /// 子运行：运行面 ＋ 这条连接的运行簿（[`watch_loop`] 换成 `spawn` 交进来的那一本；夹具用自带的一本）＋ 子运行记录的游标。
    runs: crate::observe::runs::RunTrack,
}

impl ReaderState {
    fn new(projects: PathBuf, with_bg: bool, tail_only: bool) -> Self {
        ReaderState {
            projects,
            offsets: HashMap::new(),
            tails: HashMap::new(),
            gone: HashSet::new(),
            seqs: SeqCounter::new(),
            sessions: HashMap::new(),
            events_tx: None,
            pid_watched: HashSet::new(),
            active_sids: HashSet::new(),
            with_bg,
            tail_only,
            with_rbind_token: false,
            runs: crate::observe::runs::RunTrack::new(
                crate::agents::stream_run_faces(),
                std::sync::Arc::default(),
            ),
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
}

/// One line read out of a JSONL file, with its assigned per-file seq.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadLine {
    pub seq: u64,
    pub raw: String,
    /// backend-01（gap#2）：本行末尾（含 `\n`）的累计**原始字节** offset，逐字节对齐 aterm `LineFramer`
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
/// Rules (〔TL1〕the monitor copy this used to mirror was deleted by CF1 — these are the only ones):
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
///   (〔RENDER2 · A6〕[`catch_up_session`]), never while the writer is alive;
/// - **truncation**: judged against the high-water mark
///   (`len < cursor.seen_len`), so a rewrite landing inside a pending torn-tail
///   window `[consumed, seen_len)` is still caught → start over from byte 0;
/// - on truncation the byte cursor resets; this pure core does not touch the
///   seq counter — the production caller ([`process_jsonl`]) restarts it right
///   after announcing the re-read (〔RENDER2〕seq = line number in the file now);
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

/// 〔FW1 · 第四波 4D · D-d〕游标旁记多少个字节的「末尾指纹」。
///
/// 它防的是**对不齐**：游标之前的内容被原地改写、长度变了（编辑器整份覆盖多了 / 少了几个字）⇒ 从旧偏移读会读到半行。
/// 任何改了前缀长度的改写都会让「游标之前那一截」整体平移 d 个字节；平移后仍逐字节相同，要求这一截以 d 为周期 ——
/// jsonl 每行尾巴上是 uuid / 时间戳（`"uuid":"…"` 36 个字 ＋ 引号），64 字节必然盖得住一整个，没有这种周期。
/// 取更多只是多读几个字节，取更少（< 36）就可能只落在行尾那几个常见的 `"}` 上。
/// ⚠ 买不到的（如实登记）：**长度一字不差**的原地改写（只改了已读过的几行里的同长字）⇒ 对得齐、后面照读，
///   那几行已经画出去的旧样子不会变（「看的，不是管的」）。
pub const TAIL_PROBE: u64 = 64;

/// 一次续读前看到的那一份。
enum Look {
    /// 盘上不在了（删了 / 改名走了）。
    Gone,
    /// 在，但这一趟读不出来（权限 · 读到一半出错）—— 与从前一样静默跳过这一趟（至少一次语义，游标不动）。
    Unreadable,
    /// 读到了：`chunk` 覆盖 `[chunk_start, file_len)`；`from` 是这一趟该接着的游标（被改写 ⇒ 归零）；
    /// `reread` = 这一趟要从 0 重读、以及为什么。
    Read {
        chunk: Vec<u8>,
        chunk_start: u64,
        file_len: u64,
        from: ReadCursor,
        reread: Option<RereadWhy>,
    },
}

/// 只读新字节：从游标处 `seek` 到 EOF〔audit-0805 F04 第 2 步〕。
///
/// 〔FW1〕返回 [`Look`]：读到的那一形里 `(chunk, chunk_start, file_len)` 直接喂 [`read_new_lines_at`]（配 `from`）。
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
///
/// 只读新字节（F04）＋〔FW1〕先认出「不在了 / 被截短 / 被原地改写」三形。
///
/// - 截短：`metadata` 的长度 < 高水位 ⇒ 整读、`reread = Truncated`（`scan_new_lines` 自己也会判出截断、从 0 起）。
/// - 续读：从 `consumed − k` 读起（`k = min(TAIL_PROBE, consumed)`），多读的那 `k` 字节与 `tail`（上一趟记下的）逐字节比；
///   对不上 ⇒ 整读、游标归零、`reread = Rewritten`。`tail` 为 `None`（第一次读 / 刚丢过游标）⇒ 不核。
/// - ★ `file_len` 用「真读到多少」算（理由见原注：`metadata` 与读之间文件还在长）。
fn read_tail_from(path: &Path, cursor: ReadCursor, tail: Option<&[u8]>) -> Look {
    use std::io::{Read, Seek, SeekFrom};
    let meta_len = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Look::Gone,
        Err(_) => return Look::Unreadable,
    };
    let read_from = |start: u64| -> Option<(Vec<u8>, u64, u64)> {
        let mut f = std::fs::File::open(path).ok()?;
        if start > 0 {
            f.seek(SeekFrom::Start(start)).ok()?;
        }
        let mut chunk = Vec::new();
        f.read_to_end(&mut chunk).ok()?;
        let file_len = start + chunk.len() as u64;
        Some((chunk, start, file_len))
    };
    let whole = |why: RereadWhy| match read_from(0) {
        Some((chunk, chunk_start, file_len)) => Look::Read {
            chunk,
            chunk_start,
            file_len,
            from: ReadCursor::default(),
            reread: Some(why),
        },
        None => Look::Unreadable,
    };
    if meta_len < cursor.seen_len {
        // 截短：游标原样交给扫描（它按高水位判截断、从 0 起、seq 照旧往上）。
        return match read_from(0) {
            Some((chunk, chunk_start, file_len)) => Look::Read {
                chunk,
                chunk_start,
                file_len,
                from: cursor,
                reread: Some(RereadWhy::Truncated),
            },
            None => Look::Unreadable,
        };
    }
    let consumed = cursor.consumed.min(meta_len);
    let k = TAIL_PROBE.min(consumed);
    let Some((chunk, chunk_start, file_len)) = read_from(consumed - k) else {
        return Look::Unreadable;
    };
    if let Some(t) = tail {
        if chunk.get(..k as usize) != Some(t) {
            return whole(RereadWhy::Rewritten);
        }
    }
    Look::Read {
        chunk,
        chunk_start,
        file_len,
        from: cursor,
        reread: None,
    }
}

/// 〔FW1〕扫完之后记下新游标之前那 `TAIL_PROBE` 字节（下一趟续读前核它）。`chunk` 覆盖 `[chunk_start, ..)`。
fn tail_of(chunk: &[u8], chunk_start: u64, consumed: u64) -> Vec<u8> {
    let k = TAIL_PROBE.min(consumed);
    let from = (consumed - k).saturating_sub(chunk_start) as usize;
    let to = (consumed.saturating_sub(chunk_start) as usize).min(chunk.len());
    chunk.get(from..to).map(<[u8]>::to_vec).unwrap_or_default()
}

/// 〔FW1〕这份文件的游标、指纹一起丢（不在了 ⇒ 同名再出现从 0 读；〔RENDER2〕那一趟当改写办：先出声、行号从 0 重数）。
fn forget_cursor(state: &mut ReaderState, key: &Path) {
    state.offsets.remove(key);
    state.tails.remove(key);
}

/// Read a JSONL file incrementally and send a [`Frame::Line`] per new line. 回交出去几行（〔REREAD〕补读计数用）。
fn process_jsonl(path: &Path, state: &mut ReaderState, sink: &mut FrameSink) -> usize {
    let Some(session_id) = file_stem_str(path) else {
        return 0;
    };
    // Active-session filter: only stream sessions whose PID is alive.
    // Historical jsonl is never pulled.
    if !state.active_sids.contains(&session_id) {
        return 0;
    }
    let key = path_key(path);
    let key_str = key.to_string_lossy().into_owned();
    let prev_cursor = state.offsets.get(&key).copied().unwrap_or_default();
    let path_str = path.to_string_lossy().into_owned();
    // F04：只读新字节（截断 / 改写时 `read_tail_from` 自己退回整读）。此前是 `fs::read` 整读 ——
    // 257 MB 会话的每一次文件事件都要把整份读进内存，只为提取约 500 字节的新行。
    // 〔FW1 · D-d〕活会话的 jsonl 是「看的，不是管的」（V119 之后文件管理器改得动它）：
    //   不在了 ⇒ 出声一次、丢游标（同名再出现从 0 读）；截短 / 原地改写 ⇒ 从 0 重读并出声。都不碰判活。
    let (chunk, chunk_start, file_len, from, reread) =
        match read_tail_from(path, prev_cursor, state.tails.get(&key).map(Vec::as_slice)) {
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
            Look::Read {
                chunk,
                chunk_start,
                file_len,
                from,
                reread,
            } => (chunk, chunk_start, file_len, from, reread),
        };
    // 〔RENDER2 · `设计/10 §3.2`〕这一趟要从 0 重读 ⇒ 行号从 0 重数（seq ＝ 当前文件里的行号），出声那一帧排在重读的行之前。
    //   三种来路：`read_tail_from` 认出的截短 / 改写 · 读的那一下文件又缩了（扫描自己按高水位判截断）· 删了之后同名又长出来。
    let reappeared = state.gone.remove(&key);
    let reread = reread
        .or_else(|| (file_len < from.seen_len).then_some(RereadWhy::Truncated))
        .or_else(|| reappeared.then_some(RereadWhy::Rewritten));
    if reread.is_some() {
        state.seqs.restart(&key_str);
    }
    let (lines, new_cursor) = read_new_lines_at(
        &chunk,
        chunk_start,
        file_len,
        from,
        &key_str,
        &mut state.seqs,
    );
    state.tails.insert(
        key.clone(),
        tail_of(&chunk, chunk_start, new_cursor.consumed),
    );
    state.offsets.insert(key, new_cursor);
    if let Some(why) = reread {
        sink.send(Frame::SessionFileReread {
            session_id: session_id.clone(),
            path: path_str.clone(),
            why,
        });
    }
    let n = lines.len();
    let mut runs_changed = state.runs.adopt(&session_id, path);
    for line in lines {
        runs_changed |= send_line(&session_id, &path_str, line, &state.runs, sink);
    }
    if runs_changed {
        sink.send(state.runs.frame(&session_id));
    }
    n
}

/// 一行交出去：`Line` 帧，是轮次结束就紧跟一帧 `TurnEnd`。增量读与写端死后收尾（[`catch_up_session`]）共用这一份。
///
/// 〔MOD〕这一行在渲染模型里是什么、是不是一轮的结束，都问注册表里流式那一家的记录解释面（`agents::stream_record_face`）；
/// 本函数只搬。解析不出 ⇒ 帧照发（占号）、不带成品。对账键与派出链接记进运行簿（回：运行表变没变）。
fn send_line(
    session_id: &str,
    path_str: &str,
    line: ReadLine,
    runs: &crate::observe::runs::RunTrack,
    sink: &mut FrameSink,
) -> bool {
    let face = crate::agents::stream_record_face();
    let parsed = face.and_then(|f| match (f.parse)(&line.raw) {
        Ok(Some(p)) if p.displayable => Some(p),
        _ => None,
    });
    // §2.1 不变量并存：Line 逐行照发**每一条**；turn-end 是额外的边沿信号，不替代、不过滤 Line。
    let rec = runs.main_record(session_id, &line.raw);
    // 子运行的记录（适配层 `run_of` 答得出）一轮收尾 ≠ 主运行一轮结束 ⇒ 不报轮次边沿。
    let turn_uuid = face
        .and_then(|f| f.turn_end)
        .and_then(|t| t(&line.raw))
        .filter(|_| !rec.in_run);
    let (message, cwd) = match parsed {
        Some(p) => (Some(p.message), p.cwd),
        None => (None, None),
    };

    sink.send(Frame::Line {
        session_id: session_id.to_string(),
        path: path_str.to_string(),
        seq: line.seq,
        message,
        cwd,
        byte_offset: line.byte_offset, // backend-01 gap#2：累计原始字节（对齐 aterm LineFramer）
        rid: rec.rid,
    });
    // **先 Line 后 TurnEnd**：对齐 aterm β 的按行序处理——TurnEnd 结算时 currentOffset 已含本行。
    // TurnEnd 不带 byte_offset（只 Line 带）。
    if let Some(uuid) = turn_uuid {
        sink.send(Frame::TurnEnd {
            session_id: session_id.to_string(),
            uuid,
        });
    }
    rec.changed
}

/// 〔RENDER2 · `设计/10 §3.1` A6〕**从游标补读这个会话的 jsonl**（与文件事件同一个 [`process_jsonl`]，不另写一条路）。
///
/// `writer_dead`（只由会话退休那一刻传真）⇒ 补读完之后，游标之后那截没 `\n` 收尾、但本身是**一整个 JSON 对象**的残行
/// 当一行交出去：写端写完 JSON、没来得及写 `\n` 就被杀，这一行从此不会再有文件事件。活着的写端照旧等 `\n`。
/// 这一行**不推进游标、不占计数器**：同一文件日后被续写时，残行与新字节拼成的那一行仍是这个号（冷读的行号空间里它也是
/// 这个号），前端按 seq 去重吸收；冷读（`history_query::line_counts` 口径）不数这截残行 —— 会话已死、没人再写，差的只是它自己。
/// 退休之前先补读还有一层用处：pidfd 比 debounce 快，死前最后几行的文件事件可能在退休之后才到、被判活过滤挡掉。
/// 回补读出几行（〔REREAD · V155〕`resync` 应答的 `caught_up`；不数收尾残行 —— `resync` 那一路写端活着、不收尾）。
fn catch_up_session(
    sid: &str,
    writer_dead: bool,
    state: &mut ReaderState,
    sink: &mut FrameSink,
) -> usize {
    let mine: Vec<PathBuf> = state
        .offsets
        .keys()
        .filter(|k| file_stem_str(k).as_deref() == Some(sid))
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
    use std::io::{Read, Seek, SeekFrom};
    let key = path_key(path);
    let Some(cursor) = state.offsets.get(&key).copied() else {
        return;
    };
    let mut rest = Vec::new();
    let read = std::fs::File::open(path).and_then(|mut f| {
        f.seek(SeekFrom::Start(cursor.consumed))?;
        f.read_to_end(&mut rest)
    });
    if read.is_err() || rest.contains(&b'\n') {
        return; // 读不动 / 还有完整行没消费（补读那一步没读动）⇒ 不猜
    }
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
    };
    if send_line(sid, &path.to_string_lossy(), line, &state.runs, sink) {
        sink.send(state.runs.frame(sid));
    }
}

/// ★★ `P0b-Y2`〔08-13〕：`<claude_dir>/sessions/` **换了 inode 或刚出现**，重新挂上并重扫。
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
/// 〔RESYNC〕回真 = 这一次真往 tmux 里写了身份标签（首次宣告或对账纠正）。
fn process_session_added(path: &Path, state: &mut ReaderState, sink: &mut FrameSink) -> bool {
    let key = path_key(path);
    // PID is the sessions/<PID>.json filename stem.
    let Some(pid) = file_stem_str(path).and_then(|s| s.parse::<u32>().ok()) else {
        return false;
    };
    let Some(bytes) = std::fs::read(path).ok() else {
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
    // Batch6-F21: interactivity gate. CC 2.1.x 的后端后台任务
    // (--fork-session --resume) **会**写 sessions/<PID>.json（kind:"bg" +
    // jobId）——"子会话不注册 pidfile"的旧假设已过期。bg 进程是自己 pidfile
    // 的真作者（F20 身份证据对它们正确地放行），但不是交互会话、不该成 tab。
    // 保守规则（与本地 session_map 一字一致）：kind 字段存在且非 "interactive"
    // 才排除；旧 CC 不写该字段 → 放行。
    if let Some(kind) = non_interactive_kind(&bytes) {
        if !state.with_bg {
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
            return false;
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
        // 〔RESYNC · `设计/15 §4.1b`〕pidfile 重写（sid 没变）顺手对一次标签：外部改掉的 `@ccm_sid` 在这里被纠正。
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
    //
    // 〔U4b · 第四波〕这一次探测的**结局**不再丢：它同时就是「这条会话住在什么容器里」的答案
    // （`identity_tag::Outcome::container`，随下面的 `session_added` 报出去）—— 零新进程、零新节拍。
    // 〔W5-VIS · `设计/15 §4.7 S2`〕打不上的那两形（tmux 报错 / sid 形状不对）**说出来** —— 标没写上的会话
    // 之后过不了身份门，而「为什么」原先整条链零线索。
    let (container, wrote) = tag_identity(pid, &sid);
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
        // 🔴 〔`设计/80 §8.7` 步 2〕**启动期令牌** —— 把「会话身份」从 tmux 上解绑的那个键。
        //
        // ★ **零新节拍**：读它的那一刻就是这一刻。`§8.3` 那一栏逐字「后端**已经在**
        //   inotify `sessions/`……看到 `<PID>.json` 的那一刻，**pid 与 sid 同时在手**」——
        //   本行只是在同一刻多读一个环境变量，没有新循环、没有新通道、没有新平台原语。
        //
        // ★ **位置与上面 `identity_tag::tag(pid, &sid)` 同理**：也在 `pid_alive` +
        //   `add_time_verdict`（procStart 冒名检查）之后 ⇒ 报出去之前已经证过
        //   「这个 pid 真的是写那份 pidfile 的那个 claude」，令牌不会张冠李戴。
        //
        // ★ **默认不读**（`state.with_rbind_token`）：令牌是敏感数据（`§8.6 ③`），
        //   只有显式发了 `--with-rbind-token` 的客户端才拿得到。没索要 ⇒ `None`
        //   ⇒ `skip_serializing_if` ⇒ 这一帧的字节与本字段加进来之前一字不差。
        rbind_token: if state.with_rbind_token {
            crate::control::identity_tag::rbind_token_of(pid)
        } else {
            None
        },
        // 〔U4b〕判不了 ⇒ `None` ⇒ 不上线（与本字段加进来之前逐字节相同）。
        container,
        // 〔LOC1b〕与令牌同一道闸（见 `wire::Frame::SessionAdded::pid`）。pid 与 verdict 核过的是同一个进程。
        pid: state.with_rbind_token.then_some(pid),
    });
    if !state.tail_only {
        for p in &jsonls {
            process_jsonl(p, state, sink);
        }
    }
    // 这个会话此刻已有的子运行（宣告之前就派出去的那几个）：收进来、从头读一遍，有就发一帧运行表。
    let mut runs_changed = false;
    for p in &jsonls {
        runs_changed |= state.runs.adopt(&sid, p);
    }
    if runs_changed {
        sink.send(state.runs.frame(&sid));
    }
    wrote
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

/// 〔RESYNC · `设计/15 §4.1b`〕对在跟的每个会话（`only` 给了就只对那个 sid）重比一次标签；回真写了几个。
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
/// 〔U4b · 第四波〕从 `watch_loop` 里抽出来，为的是「清单报完了」那一帧的**位置**能被直接验
/// （`watcher_tests::sessions_replayed_follows_every_initial_session_added_exactly_once`）——
/// 起整条 `watch_loop` 要挂 inotify、探真机 tmux、装 hook，判据不许碰那些。
///
/// `sessions_replayed` **无条件发**：`sessions/` 不在 = 清单是空的，也是一个说完了的答案。
/// 它排在本函数所有 `session_added` 之后；调用方在 Phase 2 起来之前调本函数（同一个 sink、同一条线程）
/// ⇒ 客户端收到它时，这台机器此刻全部的活会话都已经报过了 —— 靠它把「固定、却没被报过」的会话
/// 从「说不清」落到「已结束」（`设计/30 §3.5.7a`）。
fn initial_session_scan(sessions: &Path, state: &mut ReaderState, sink: &mut FrameSink) {
    reconcile_sessions(sessions, state, sink, None);
    sink.send(Frame::SessionsReplayed);
}

/// 〔RESYNC · `设计/15 §4.1b`〕一次对齐的差异（`resync` 的应答）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Reconciled {
    pub(crate) added: usize,
    pub(crate) removed: usize,
    pub(crate) retagged: usize,
    /// 〔REREAD · V155〕这一趟从游标补读出几行（[`catch_up_session`] 的合计）。
    pub(crate) caught_up: usize,
}

/// 〔RESYNC〕「重新对齐」那一趟的会话部分：对表（与起步同一个 [`reconcile_sessions`]）＋ 每个在跟的会话从游标补读
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

/// **pidfile 目录对后端的表**：起步初扫与 `resync` 是这同一个函数（`设计/15 §4.1b`「与初探同一个函数」）。
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
                let sid = std::fs::read(p).ok().and_then(|b| parse_session_id(&b));
                if sid.as_deref() != Some(want) {
                    continue;
                }
            }
            if process_session_added(p, state, sink) {
                retagged += 1;
            }
        }
    }
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
    catch_up_session(sid, true, state, sink); // 〔RENDER2 · A6〕写端已死：补读 ＋ 收尾残行（D 块）
    state.active_sids.remove(sid);
    state.runs.forget(sid);
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
    // F04：同 `process_jsonl`，只读新字节。〔FW1〕同一个 `read_tail_from`（不在 ⇒ 丢游标；改写 ⇒ 游标归零），
    //   这里不出声：prime 发生在宣告之前，那句话由之后的 `process_jsonl` 说。
    let (chunk, chunk_start, file_len, from) =
        match read_tail_from(path, prev, state.tails.get(&key).map(Vec::as_slice)) {
            Look::Read {
                chunk,
                chunk_start,
                file_len,
                from,
                reread,
            } => {
                // 〔RENDER2〕从 0 重数，同 `process_jsonl`（这里不出声：宣告还没发，下游的快照按行号从 0 拉）。
                if reread.is_some() || file_len < from.seen_len {
                    state.seqs.restart(&key_str);
                }
                (chunk, chunk_start, file_len, from)
            }
            Look::Gone => {
                forget_cursor(state, &key);
                return 0;
            }
            Look::Unreadable => return 0,
        };
    // F04 第 3 步：**只数不建**。此前这里把每行 `to_string` 成 `Vec<ReadLine>`，
    // 而下面只用了 `.len()`（一条 debug 日志）—— 首次 prime 时那一段就是整份文件。
    let (n_lines, cursor) = count_new_lines_at(
        &chunk,
        chunk_start,
        file_len,
        from,
        &key_str,
        &mut state.seqs,
    );
    state
        .tails
        .insert(key.clone(), tail_of(&chunk, chunk_start, cursor.consumed));
    state.offsets.insert(key, cursor);
    tracing::debug!(
        "primed {key_str}: cursor→{} (+{} lines suppressed, tail seq starts here)",
        cursor.consumed,
        n_lines
    );
    // Batch8 审计 D-I2：返回 prime 后的行号计数器现值（= 完整行总数 L），
    // session_added 帧带给 monitor 做快照完整性校验（拉到的行数 < L = 快照
    // 中途断/backend 报错——exit status 拿不到，行数校验更强）。
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

/// 〔F20〕add-time 冒名判定的那一趟 /proc 读：活着且是写它的那个 claude ⇒ `Ok(当前 starttime ticks)`（调用方拿它当 #34 基线），
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

/// pidfile 目录（流模式的耳朵与一次性扫描同一处问适配层）。
fn pidfile_dir(agent_home: &Path) -> PathBuf {
    crate::agents::claudecode::paths::sessions_root(agent_home)
}

/// 〔Batch6-F21〕`kind` 在且不是 `interactive` ⇒ `Some(kind)`（后台任务，不是交互会话）；缺字段（旧 CC）⇒ `None` 放行。
fn non_interactive_kind(bytes: &[u8]) -> Option<String> {
    parse_kind(bytes).filter(|k| k != "interactive")
}

/// 〔FIX · V138 订正〕**一次性扫描**：`<agent_home>/sessions/` 下此刻活着的交互会话 `(sid, pid)` —— 判活与起步初扫同一条
/// （pid 在 · 不是后台任务 · add-time 冒名判定过）。给 `ccm` resume 用（由 `main` 注入，control 层不引用 observe）。
pub fn running_sessions(agent_home: &Path) -> Vec<(String, u32)> {
    let dir = pidfile_dir(agent_home);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| is_session_json(p))
        .filter_map(|p| {
            let pid = file_stem_str(&p)?.parse::<u32>().ok()?;
            let bytes = std::fs::read(&p).ok()?;
            let sid = parse_session_id(&bytes)?;
            (pid_alive(pid)
                && non_interactive_kind(&bytes).is_none()
                && add_time_check(pid, &bytes, &p).is_ok())
            .then_some((sid, pid))
        })
        .collect()
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
    /// 〔MIG-1 · `99 §2.1 ⑬`〕这条流的会话账本（`observe::session_ledger`）：每一帧发出去之前过它，
    /// 它补发可重连 / 已结束的成品帧、压住 `sessions_replayed` 直到第一份 tmux 快照。生产由 [`watch_loop`] 装上；
    /// 夹具走 [`FrameSink::new`] 不装（它们钉的是 watcher 自己发的帧）。
    ledger: Option<crate::observe::session_ledger::SessionLedger>,
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
            ledger: None,
        }
    }

    /// 〔MIG-1〕生产那一份：带会话账本。
    fn with_ledger(tx: mpsc::Sender<Frame>) -> Self {
        FrameSink {
            ledger: Some(crate::observe::session_ledger::SessionLedger::new()),
            ..FrameSink::new(tx)
        }
    }

    /// 〔MIG-1 续 · V41〕一份 tmux 观测：只交会话账本（有的话），它补发的成品帧照常发；观测本身不上线。
    fn tmux(&mut self, obs: TmuxObservation) {
        let Some(l) = self.ledger.as_mut() else {
            return;
        };
        let (raw, observation) = observation_parts(obs);
        for f in l.on_tmux(&raw, observation) {
            self.push(f);
        }
    }

    /// 发一帧：先过会话账本（有的话）—— 它决定这一帧自己发不发、紧跟着补发哪几帧。
    fn send(&mut self, frame: Frame) {
        let (pass, extra) = match self.ledger.as_mut() {
            Some(l) => l.on_frame(&frame),
            None => (true, Vec::new()),
        };
        if pass {
            self.push(frame);
        }
        for f in extra {
            self.push(f);
        }
    }

    /// Send `frame`, first flushing any owed overflow signal.
    ///
    /// Order matters: we try to emit the pending `Overflow` *before* the real
    /// frame so the client learns "you lost N frames" no later than the next
    /// frame it receives. If the channel is still full, we keep owing the count
    /// (it only ever grows until a send succeeds); a closed channel is a quiet
    /// shutdown (the loop checks `is_closed`).
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

fn file_stem_str(p: &Path) -> Option<String> {
    p.file_stem().and_then(|s| s.to_str()).map(str::to_string)
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/watcher_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/watcher_lines_tests.rs"]
mod lines_tests;
