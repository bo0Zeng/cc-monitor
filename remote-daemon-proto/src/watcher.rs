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
//! this daemon reads via one `fs::read` snapshot and gives up the whole pass
//! (cursor untouched). Both are at-least-once-safe.

use crate::wire::{Frame, SeqCounter};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
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
// - **轮询 B**（**刻意留到 P5**）：`TMUX_EMIT_INTERVAL` = 8s 的 `tmux ls`。P2 把它从
//   "主循环里的节流判断"搬进一条独立 ticker 线程（见 `spawn_tmux_ticker`），
//   使主循环**现在**就是最终形态；P5 删 ticker 线程即可，不必再动循环结构。

/// P2：`watch_loop` 消费的统一事件。
enum WatchEvent {
    /// 文件系统事件（`notify-debouncer-mini` 经 [`DebouncerSink`] 转投进来）。
    Notify(DebounceEventResult),
    /// **pidfd 醒了**：这个 pidfile 当时追踪的那个进程实例已退出。
    ///
    /// 带 `pid` 是为了挡**陈旧唤醒**：同一 pidfile 路径可能已换成别的 pid
    /// （`/clear` 原地换 sid、PID 复用写同路径），或已被移除。消费侧比对
    /// `state.sessions[key].pid == pid` 才退休 ⇒ 天然幂等。
    PidDied { key: PathBuf, pid: u32 },
    /// 一次性 `tmux ls` 探测线程的结果。
    TmuxObserved(TmuxObservation),
    /// tmux 探测节拍——**本 crate 剩下的唯一定时器**，住在独立 ticker 线程里。**P5 删。**
    TmuxProbeDue,
    /// 预留给 P5：删掉 ticker 之后，主循环需要一条**显式**的停机信号才能及时
    /// 发现 stdout 写端已关（`sink.is_closed()` 现在靠 ticker 每 8s 醒一次来复查）。
    ///
    /// **P5 必须做**：删 ticker 的同时把写端关闭接到这个变体上，否则 reader 线程
    /// 会一直阻塞在 `recv()`（进程退出时才随之消亡——不是泄漏，但不再"没人听就停读"）。
    #[allow(dead_code)]
    Shutdown,
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

/// P2：`pidfd_open(2)`。绑的是**进程实例本身**而不是 pid 数字 ⇒ **PID 复用在机制上
/// 不存在**（不是"检测得更准"，是"无从发生"）。这是本工作区唯一一条正确性改进。
///
/// 需 Linux 5.3+（本机 7.0）。失败最常见的是 `ESRCH`——目标在 open 之前就没了。
fn pidfd_open(pid: u32) -> std::io::Result<std::os::fd::OwnedFd> {
    use std::os::fd::FromRawFd;
    // SAFETY：`SYS_pidfd_open` 只读地为目标进程创建一个 fd，不解引用任何指针。
    // 返回值 <0 时按 errno 处理、不构造 OwnedFd；>=0 时它是一个我们独占的新 fd，
    // 交给 OwnedFd 接管所有权（唯一持有者，Drop 时 close）。
    let rc = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0 as libc::c_uint) };
    if rc < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY：rc 是刚由内核分配、无人持有的合法 fd。
    Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(rc as i32) })
}

/// P2：给一个 pidfile 挂 pidfd 看守。**零轮询**——线程阻塞在无超时 `poll(2)` 上，
/// 由**内核**在目标进程终止时唤醒。
///
/// 三条判据，按顺序：
/// 1. `pidfd_open` 失败（`ESRCH` 等）⇒ 目标已不在 ⇒ 立刻发 `PidDied`。
/// 2. open 成功后**再读一次** `proc_starttime` 与 add 时捕获的基线比对
///    （复用既有纯函数 `is_same_live_process`）：不符 = 在"读 pidfile"与
///    "开 pidfd"之间发生了 PID 复用 ⇒ 我们开到的是冒名者 ⇒ 发 `PidDied`。
///    **这就是原先那套 procStart 启发式的全部去处**——从"每 2s 复查一遍"
///    降级为"开 pidfd 时校验一次"，之后靠内核，不再需要周期比对。
/// 3. 起线程 `poll(pidfd, POLLIN, -1)`；醒了发 `PidDied`。
///
/// **线程数的界**：每个被追踪的 (pidfile, pid) 最多一条，实际是个位数
/// （一台机器上同时活着的 CC 交互会话数）。线程活到目标进程真正退出为止——
/// 若 pidfile 先被删而进程仍在，那条线程会继续等，等到进程退出时发一条
/// **陈旧唤醒**，被消费侧的 pid 比对挡掉（无副作用）。
///
/// **`poll` 真出错（非 `EINTR`）时刻意不发 `PidDied`**：宁可让会话留在 live、
/// 等 pidfile 删除或断连来收，也不因一次系统调用失败就误归档——与本文件
/// `is_same_live_process` 头注那条「瞬时读失败绝不误归档」同一条纪律。
fn spawn_pid_watcher(
    key: PathBuf,
    pid: u32,
    expected_start: Option<u64>,
    tx: std::sync::mpsc::Sender<WatchEvent>,
) {
    let fd = match pidfd_open(pid) {
        Ok(fd) => fd,
        Err(e) => {
            tracing::debug!("pidfd_open pid {pid} 失败（目标已不在？）: {e}");
            let _ = tx.send(WatchEvent::PidDied { key, pid });
            return;
        }
    };
    // 判据 2：open 之后复核身份（挡 pidfile 读取 → pidfd_open 之间的 PID 复用）。
    // 用既有的 `session_alive`（存在性 + 同实例），语义正是这里要的；顺带覆盖
    // "pidfd 开成功但进程在这一瞬已退出"。
    if !session_alive(pid, expected_start) {
        tracing::warn!("pidfd 开到的 pid {pid} 与 pidfile 基线不符（PID 复用）⇒ 当死");
        let _ = tx.send(WatchEvent::PidDied { key, pid });
        return;
    }
    let builder = std::thread::Builder::new().name(format!("pidfd-{pid}"));
    let spawned = builder.spawn(move || {
        use std::os::fd::AsRawFd;
        let raw = fd.as_raw_fd();
        let mut pfd = libc::pollfd {
            fd: raw,
            events: libc::POLLIN,
            revents: 0,
        };
        loop {
            // SAFETY：pfd 是栈上的一个合法 pollfd，nfds=1 与之匹配；timeout=-1 = 无限等。
            // `fd` 的所有权在本闭包里，poll 期间不会被 close。
            let n = unsafe { libc::poll(&mut pfd as *mut libc::pollfd, 1, -1) };
            if n >= 0 {
                let _ = tx.send(WatchEvent::PidDied { key, pid });
                return;
            }
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue; // EINTR：接着等
            }
            // 真错误：**不**报死（见头注）。
            tracing::warn!("pidfd poll pid {pid} 失败、放弃看守（不报死）: {err}");
            return;
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("起 pidfd 看守线程失败 pid {pid}: {e}");
    }
}

/// P2：挂 pidfd 看守的幂等入口。`events_tx` 为 `None`（单元测试）时什么都不做。
fn arm_pid_watcher(key: &Path, pid: u32, expected_start: Option<u64>, state: &mut ReaderState) {
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    if !state.pid_watched.insert((key.to_path_buf(), pid)) {
        return; // 这个 (pidfile, pid) 已经挂过了
    }
    spawn_pid_watcher(key.to_path_buf(), pid, expected_start, tx);
}

/// P2：tmux 探测节拍线程——**本 crate 剩下的唯一定时器**。
///
/// 把 8s 节流从主循环里搬出来，是为了让主循环**现在**就变成账本第 1 行的最终形态
/// （无超时 `recv()`）；**P5 删掉本函数即可**，不必再动循环结构。
/// 立刻发一拍再进入 sleep ⇒ 保留原「首轮立即发」行为（monitor 连上尽快拿到 tmux 状态）。
fn spawn_tmux_ticker(tx: std::sync::mpsc::Sender<WatchEvent>) {
    let builder = std::thread::Builder::new().name("tmux-ticker".to_string());
    let spawned = builder.spawn(move || {
        // send 失败 = reader 走了 ⇒ 线程自然结束。
        while tx.send(WatchEvent::TmuxProbeDue).is_ok() {
            std::thread::sleep(TMUX_EMIT_INTERVAL);
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("起 tmux ticker 线程失败: {e}");
    }
}

/// Bounded channel capacity between the reader and the stdout writer.
///
/// Large enough to absorb a `/resume` history burst without dropping, small
/// enough that a wedged writer cannot grow memory without bound. A full channel
/// drops frames with a warning (Phase-0 gap, see module docs).
pub const CHANNEL_CAPACITY: usize = 10_000;

/// notify-debouncer-mini debounce window, matching `../src-tauri/src/watcher.rs`.
const DEBOUNCE_MS: u64 = 100;

/// B2：daemon 周期在**本机**跑 `tmux ls` 并经 `TmuxSessions` 帧上报的节流间隔——替掉 monitor 每 8s
/// 新建 SSH 跑 tmux ls 的刷屏轮询（灰延迟 ≈ 本值 × monitor 对账 threshold）。
const TMUX_EMIT_INTERVAL: Duration = Duration::from_secs(8);

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
    /// **确证**零会话（rc=0 空 stdout，或 rc=1）⇒ monitor 可安全 retire。
    ZeroSessions,
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
fn tmux_probe_script() -> String {
    format!(
        "if command -v tmux >/dev/null 2>&1; then exec tmux ls -F '{TMUX_LS_FMT}' 2>/dev/null; else exit {TMUX_PROBE_NO_TMUX_RC}; fi"
    )
}

/// P1：把探测的 (rc, stdout) 折成四态。**纯函数、可单测**（判据只有 rc + stdout 空否，
/// 刻意**不看 stderr**——P0 实测 stderr 有两种措辞，且拿英文消息当判据本身就是错的）。
///
/// `code == None` = 被信号杀（如 tmux 卡死后探测线程连带被清）⇒ 观测无效。
fn classify_tmux_probe(code: Option<i32>, stdout: &str) -> TmuxObservation {
    match code {
        Some(0) if stdout.trim().is_empty() => TmuxObservation::ZeroSessions,
        Some(0) => TmuxObservation::Sessions(stdout.to_string()),
        // rc=1 = server 不在。**一处刻意的保守**：socket 权限异常这类罕见情形也会落这里
        // ⇒ 理论上可能误 retire。缓解：socket 路径 uid 隔离（`/tmp/tmux-<uid>/`），同 uid 下
        // 权限异常几乎不可能。**P3 落地后有更强判据**：那时 daemon 持有 server 的 pidfd，
        // 「pidfd 说 server 活着但 tmux ls rc=1」= 真异常 ⇒ 归 `Unobservable`。
        // 该升级**不改帧契约**（`ZeroSessions` 语义不变），所以 P1 现在就能安全落地。
        Some(1) => TmuxObservation::ZeroSessions,
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

/// P1：四态 → wire。**`raw` 载荷刻意与 P1 之前逐字节一致**，新信息全部走 additive 的
/// `observation` 字段 ⇒ **旧 monitor 行为零变化**（有会话时它照旧解析 raw；零会话/出错时
/// 它看到空 raw、照旧保守跳过 = 今天的行为，无回归）。新 monitor 读 `observation` 才能
/// 区分"确证零会话"与"观测失败"，从而修掉灰灯卡死。
fn observation_to_frame(obs: TmuxObservation) -> Frame {
    match obs {
        TmuxObservation::Sessions(raw) => Frame::TmuxSessions {
            raw,
            // 有会话时**刻意省略**：raw 非空本身就说明是有会话，省略保持热路径字节不变。
            observation: None,
        },
        TmuxObservation::ZeroSessions => Frame::TmuxSessions {
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
/// `claude_dir` is the resolved `~/.claude` (or `$CLAUDE_CONFIG_DIR`). The
/// reader watches `<claude_dir>/projects/` recursively and
/// `<claude_dir>/sessions/`.
pub fn spawn(claude_dir: PathBuf, with_bg: bool, tail_only: bool) -> mpsc::Receiver<Frame> {
    let (tx, rx) = mpsc::channel::<Frame>(CHANNEL_CAPACITY);
    // notify-debouncer-mini is a synchronous std::sync::mpsc API; run it on a
    // blocking thread and hand frames to the async writer over tokio mpsc.
    std::thread::Builder::new()
        .name("jsonl-watcher".into())
        .spawn(move || watch_loop(claude_dir, tx, with_bg, tail_only))
        .expect("spawn jsonl-watcher thread");
    rx
}

/// The reader half: initial walkdir scan, then the live debouncer loop.
///
/// Runs on its own OS thread. `tx` is the bounded sender; it is wrapped in a
/// [`FrameSink`] whose [`FrameSink::send`] never blocks the notify callback and
/// turns dropped frames into an [`Frame::Overflow`] signal (#32).
fn watch_loop(claude_dir: PathBuf, tx: mpsc::Sender<Frame>, with_bg: bool, tail_only: bool) {
    let projects = claude_dir.join("projects");
    let sessions = claude_dir.join("sessions");

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
    let (events_tx, events_rx) = std::sync::mpsc::channel::<WatchEvent>();
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
    // Watch projects recursively; watch sessions (flat) for PID.json add/remove.
    if projects.is_dir() {
        if let Err(e) = debouncer
            .watcher()
            .watch(&projects, RecursiveMode::Recursive)
        {
            tracing::error!("watch failed for {}: {e}", projects.display());
        }
    } else {
        tracing::warn!("projects dir does not exist: {}", projects.display());
    }
    if sessions.is_dir() {
        if let Err(e) = debouncer
            .watcher()
            .watch(&sessions, RecursiveMode::NonRecursive)
        {
            tracing::error!("watch failed for {}: {e}", sessions.display());
        }
    } else {
        tracing::warn!("sessions dir does not exist: {}", sessions.display());
    }

    // B2 审计（`run_tmux_ls` 无超时 → 阻塞会冻结整个 reader）：`tmux ls` 一律跑在**一次性后台
    // 线程**里，主循环只收结果。gate 在「无在途探测」上 → 最多同时一个探测线程；即便远端 tmux
    // 卡死（D-state/socket 卡住/NFS home），也只泄漏这一个后台线程，reader 永不冻结。
    let mut tmux_inflight = false;
    // P2：轮询 B（8s）从"主循环里的节流判断"搬进独立 ticker 线程 ⇒ 主循环变成最终形态。
    // **P5 删掉这一行 + `spawn_tmux_ticker` 即可**（并按 `WatchEvent::Shutdown` 的注释接停机信号）。
    spawn_tmux_ticker(events_tx.clone());

    // P2：**无超时** `recv()`——本循环再没有任何定时器（轮询 A 已由 pidfd 取代）。
    // 事件源：notify（经 DebouncerSink）· pidfd 看守线程 · tmux 探测/节拍线程。
    // **P3/P4 只往 `WatchEvent` 加变体 + 加发送方，不许再挂独立定时器。**
    // `while let Ok(..)` = 所有发送端都掉了就结束（等价于原来的 Disconnected 分支）。
    while let Ok(event) = events_rx.recv() {
        match event {
            WatchEvent::Notify(Ok(events)) => {
                for ev in events {
                    let p = ev.path.as_path();
                    if is_jsonl(p) && !is_subagent_path(p) {
                        // process_jsonl skips sids not in active_sids.
                        process_jsonl(p, &mut state, &mut sink);
                    } else if is_session_json(p) {
                        // notify coalesces to "something happened to this path";
                        // decide add vs remove by current existence on disk.
                        if p.exists() {
                            process_session_added(p, &mut state, &mut sink);
                        } else {
                            process_session_removed(p, &mut state, &mut sink);
                        }
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
                if state.sessions.get(&key).map(|e| e.pid) == Some(pid) {
                    if let Some(e) = state.sessions.remove(&key) {
                        retire_sid_if_unreferenced(&e.sid, &mut state, &mut sink);
                    }
                }
            }
            WatchEvent::TmuxProbeDue => {
                if !tmux_inflight {
                    tmux_inflight = true;
                    let tx = events_tx.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(WatchEvent::TmuxObserved(run_tmux_ls()));
                    });
                }
            }
            WatchEvent::TmuxObserved(obs) => {
                tmux_inflight = false;
                // P1：四态 → wire（`raw` 载荷不变、新信息走 additive `observation`）。
                sink.send(observation_to_frame(obs));
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
    pid_watched: HashSet<(PathBuf, u32)>,
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
pub fn read_new_lines(
    bytes: &[u8],
    cursor: ReadCursor,
    key: &str,
    seqs: &mut SeqCounter,
) -> (Vec<ReadLine>, ReadCursor) {
    let len = bytes.len() as u64;
    // Truncation guard against the high-water mark (see ReadCursor docs).
    let truncated = len < cursor.seen_len;
    let start = if truncated { 0 } else { cursor.consumed };
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
    let mut consumed: u64 = 0;
    if start < len {
        let slice = &bytes[start as usize..];
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
                let seq = seqs.next(key);
                out.push(ReadLine {
                    seq,
                    raw: raw.to_string(),
                    byte_offset: start + line_end as u64,
                });
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
        ReadCursor {
            consumed: start + consumed,
            seen_len: len,
        },
    )
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
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return,
    };
    let key = path_key(path);
    let key_str = key.to_string_lossy().into_owned();
    let prev_cursor = state.offsets.get(&key).copied().unwrap_or_default();
    let (lines, new_cursor) = read_new_lines(&bytes, prev_cursor, &key_str, &mut state.seqs);
    state.offsets.insert(key, new_cursor);
    let path_str = path.to_string_lossy().into_owned();
    for line in lines {
        // daemon-09（phase②）：turn-end 边沿在 raw **之外**额外算——先解析（畸形→None、不影响 Line）。
        // 在 raw move 进 Line 帧前抽出（避免 clone raw）。§2.1 不变量并存：Line 逐行照发**每一条**。
        let turn_uuid: Option<String> = serde_json::from_str::<serde_json::Value>(&line.raw)
            .ok()
            .and_then(|v| crate::turn_detect::turn_end_uuid(&v).map(str::to_string));
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
                retire_sid_if_unreferenced(&old.sid, state, sink);
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
        retire_sid_if_unreferenced(&old.sid, state, sink);
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
        retire_sid_if_unreferenced(&e.sid, state, sink);
    }
}

/// Batch6-F22：sid 退休的**唯一**出口——`sessions` 表中已无任何存活 entry 持有
/// 该 sid 时才清 active_sids + 发 [`Frame::SessionRemoved`]。同 sid 多 pidfile
/// （resume 时原进程未死）场景下，先死的那个只解绑、不误杀整个 tab。
/// 调用方约定：先从 `state.sessions` remove 掉当事 entry 再调本函数。
fn retire_sid_if_unreferenced(sid: &str, state: &mut ReaderState, sink: &mut FrameSink) {
    let still_referenced = state.sessions.values().any(|e| e.sid == sid);
    if still_referenced {
        tracing::debug!("sid {sid} still referenced by another pidfile; not retiring");
        return;
    }
    state.active_sids.remove(sid);
    sink.send(Frame::SessionRemoved {
        sid: sid.to_string(),
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
    let Ok(bytes) = std::fs::read(path) else {
        return 0;
    };
    let key = path_key(path);
    let key_str = key.to_string_lossy().into_owned();
    let prev = state.offsets.get(&key).copied().unwrap_or_default();
    let (lines, cursor) = read_new_lines(&bytes, prev, &key_str, &mut state.seqs);
    state.offsets.insert(key, cursor);
    tracing::debug!(
        "primed {key_str}: cursor→{} (+{} lines suppressed, tail seq starts here)",
        cursor.consumed,
        lines.len()
    );
    // Batch8 审计 D-I2：返回 prime 后的行号计数器现值（= 完整行总数 L），
    // session_added 帧带给 monitor 做快照完整性校验（拉到的行数 < L = 快照
    // 中途断/daemon 报错——exit status 拿不到，行数校验更强）。
    state.seqs.peek(&key_str)
}

/// Whether `pid` currently exists as a process on this host (existence only).
///
/// Linux (the daemon's real target): `/proc/<pid>` existence. This is the
/// add-time gate; the reuse-proof check is [`session_alive`].
fn pid_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        // Non-Linux (Windows compile/smoke only — not the real target): treat as
        // alive so the cross-platform smoke still exercises the pipeline.
        let _ = pid;
        true
    }
}

/// The PID's procStart (start time), used to defend against PID reuse (#34).
///
/// Linux: the `starttime` field (jiffies since boot) from `/proc/<pid>/stat`.
/// Non-Linux (Windows smoke): `None` — liveness then degrades to existence only.
fn proc_starttime(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        parse_starttime_from_stat(&stat)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
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
        if !lower.trim().is_empty() && !lower.contains("claude") && !lower.contains("node") {
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

/// Parse the boot time (`btime <epoch-secs>` line) out of `/proc/stat` content.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_btime(proc_stat: &str) -> Option<u64> {
    proc_stat.lines().find_map(|l| {
        l.strip_prefix("btime ")
            .and_then(|v| v.trim().parse::<u64>().ok())
    })
}

/// `/proc` time values are exported in USER_HZ ticks, which is a compile-time
/// constant 100 on every mainstream Linux arch (independent of the kernel's
/// internal HZ) — hardcoding avoids a libc dependency for sysconf(_SC_CLK_TCK).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const USER_HZ: u64 = 100;

/// Starttime ticks → wall-clock epoch seconds: `/proc/stat` btime + ticks/USER_HZ.
///
/// btime is read FRESH on every call, deliberately un-cached: the kernel
/// computes it per-read as (wall clock − CLOCK_BOOTTIME), so an NTP **step**
/// moves it. A cached value taken before a backwards step would leave a
/// constant offset that mis-kills every future real session with no self-heal
/// (F20 audit I-1). Session-add is rare; one small /proc read is free.
fn start_epoch_from_ticks(ticks: Option<u64>) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let btime = std::fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|s| parse_btime(&s))?;
        Some(btime + ticks? / USER_HZ)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = ticks;
        None
    }
}

/// The pidfile's mtime as epoch seconds (None on any error → check skipped).
fn file_mtime_epoch(path: &Path) -> Option<u64> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    mtime
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// `/proc/<pid>/cmdline`, NUL separators turned into spaces, lossily decoded.
/// None when unreadable (vanished PID, permissions) → check skipped.
fn proc_cmdline(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let spaced: Vec<u8> = bytes
            .into_iter()
            .map(|b| if b == 0 { b' ' } else { b })
            .collect();
        Some(String::from_utf8_lossy(&spaced).into_owned())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// Parse the `starttime` (field 22) out of a `/proc/<pid>/stat` line.
///
/// **The comm gotcha**: field 2 is `(comm)` and the executable name can contain
/// spaces and parentheses (e.g. `(my proc)` or `((odd))`). Splitting the whole
/// line on whitespace is therefore wrong. The robust parse — used by ps/htop —
/// is to find the **last** `')'`, then count fields in the remainder: the first
/// token after it is field 3 (`state`), so `starttime` (field 22) is token index
/// `22 - 3 = 19` (0-based) of the post-`)` whitespace split.
///
/// Only called from the Linux branch of [`proc_starttime`] (and by unit tests on
/// every platform); on a non-Linux build the function body is unreferenced.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_starttime_from_stat(stat: &str) -> Option<u64> {
    /// 0-based index of `starttime` (field 22) within the tokens that follow the
    /// closing paren of `comm` (field 3 = `state` is token 0).
    const STARTTIME_IDX_AFTER_COMM: usize = 22 - 3;
    let after_comm = &stat[stat.rfind(')')? + 1..];
    after_comm
        .split_whitespace()
        .nth(STARTTIME_IDX_AFTER_COMM)?
        .parse::<u64>()
        .ok()
}

/// Reuse-proof liveness for an ACTIVE session (#34): the PID must still exist
/// **and** (when a procStart was captured at add-time) its current procStart
/// must match. A mismatch means the OS reused the PID for a different process —
/// the original session has ended.
///
/// Wires the real `/proc` reads into the pure [`is_same_live_process`] decision.
fn session_alive(pid: u32, expected_start: Option<u64>) -> bool {
    let exists = pid_alive(pid);
    // Only read the current start if the PID exists (a read on a vanished PID is
    // pointless and would just be `None` anyway).
    let current_start = if exists { proc_starttime(pid) } else { None };
    is_same_live_process(exists, expected_start, current_start)
}

/// Pure liveness decision (testable without a real `/proc`), given whether the
/// PID currently **exists**, the procStart **captured** at add-time, and the
/// procStart **read now**.
///
/// Key correctness rule (#34): a PID reuse only ever shows up as a
/// *successfully-read, DIFFERENT* current start. So the only case that declares
/// "dead by reuse" is `(Some(captured), Some(current))` with `captured != current`.
/// Every other arm where the PID still exists returns alive — in particular a
/// **transient `/proc/<pid>/stat` read failure** (`current == None`) must NOT
/// false-archive a process that demonstrably still exists (that would be a
/// regression vs. the Phase-0 existence-only check). If the process is truly
/// gone, `exists` is already `false` and we return dead.
fn is_same_live_process(
    exists: bool,
    expected_start: Option<u64>,
    current_start: Option<u64>,
) -> bool {
    if !exists {
        return false;
    }
    match (expected_start, current_start) {
        // Baseline captured AND current readable: same process iff equal.
        (Some(captured), Some(current)) => captured == current,
        // No baseline, or current unreadable right now: existence is all we can
        // assert. Do not archive a still-existing PID on missing start info.
        _ => true,
    }
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
}

impl FrameSink {
    fn new(tx: mpsc::Sender<Frame>) -> Self {
        FrameSink { tx, dropped: 0 }
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
            }) {
                Ok(()) => {
                    tracing::warn!(
                        "recovered from frame-channel overflow; signalled {} dropped frame(s)",
                        self.dropped
                    );
                    self.dropped = 0;
                }
                // Still wedged: keep owing the count, retry on the next send.
                Err(mpsc::error::TrySendError::Full(_)) => {}
                Err(mpsc::error::TrySendError::Closed(_)) => return,
            }
        }
        match self.tx.try_send(frame) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.dropped += 1;
                tracing::warn!(
                    "frame channel full (cap {CHANNEL_CAPACITY}); dropping frame \
                     ({} dropped since last overflow signal)",
                    self.dropped
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
    p.extension().is_some_and(|e| e == "jsonl")
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

/// Case-fold the path on Windows so notify's NTFS case variance does not double
/// emit; on other platforms keep the path verbatim. Mirrors `watcher.rs`.
#[cfg(windows)]
fn path_key(p: &Path) -> PathBuf {
    PathBuf::from(p.to_string_lossy().to_ascii_lowercase())
}

#[cfg(not(windows))]
fn path_key(p: &Path) -> PathBuf {
    p.to_path_buf()
}

#[cfg(test)]
mod tests {
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
        spawn_pid_watcher(key.clone(), pid, None, tx);

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
        spawn_pid_watcher(key.clone(), pid, None, tx);
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
        spawn_pid_watcher(key.clone(), pid, bogus, tx);
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
    #[test]
    fn events_channel_is_created_before_the_initial_scan() {
        let src = include_str!("watcher.rs");
        let tx_at = src
            .find("state.events_tx = Some(events_tx.clone());")
            .expect("找不到 events_tx 注入点——守卫锚点漂了，先修锚点别改断言");
        let scan_at = src
            .find("// --- Phase 1: synchronous initial scan. ---")
            .expect("找不到 Phase 1 锚点——守卫锚点漂了");
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

    /// tmux ticker：**首拍立即发**（保留 P2 之前"monitor 连上尽快拿到 tmux 状态"的行为）。
    /// 不等第二拍——那要 8s，不值当放进单元测试。
    #[test]
    fn tmux_ticker_fires_immediately() {
        let (tx, rx) = std::sync::mpsc::channel::<WatchEvent>();
        spawn_tmux_ticker(tx);
        match rx
            .recv_timeout(Duration::from_secs(2))
            .expect("ticker 必须立刻发第一拍")
        {
            WatchEvent::TmuxProbeDue => {}
            _ => panic!("期望 TmuxProbeDue"),
        }
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
        // rc=0 + 空 → server 活但零会话（exit-empty off）
        assert_eq!(
            classify_tmux_probe(Some(0), ""),
            TmuxObservation::ZeroSessions
        );
        assert_eq!(
            classify_tmux_probe(Some(0), "  \n"),
            TmuxObservation::ZeroSessions,
            "只有空白也算空"
        );
        // rc=1 → server 不在（两种 stderr 措辞都走这里，刻意不看 stderr）
        assert_eq!(
            classify_tmux_probe(Some(1), ""),
            TmuxObservation::ZeroSessions
        );
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
        for (obs, token) in [
            (TmuxObservation::ZeroSessions, OBS_ZERO_SESSIONS),
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

        // ② rc=0 但不输出 → ZeroSessions（exit-empty off 那格）
        write_fake("#!/bin/sh\nexit 0\n");
        assert_eq!(run(&path_with_fake), TmuxObservation::ZeroSessions);

        // ③ rc=1（真 tmux 在 server 不在时就是这个）→ ZeroSessions
        //    **这一格是 P1 的核心**：改回 `|| true` 会让它变成 rc=0+空 ⇒ 仍是 ZeroSessions，
        //    所以本格单独看不出回归；真正钉住 `exec` 的是 ④。
        write_fake("#!/bin/sh\necho 'no server running on /tmp/x' >&2\nexit 1\n");
        assert_eq!(run(&path_with_fake), TmuxObservation::ZeroSessions);

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
            matches!(rx.try_recv(), Ok(Frame::SessionRemoved { sid }) if sid == "sid-1"),
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
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionRemoved { sid }) if sid == "shared-sid"));
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
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionRemoved { sid }) if sid == "solo"));
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
    /// The existence-dependent assertions only hold on Linux: on non-Linux
    /// `pid_alive` is a hardcoded `true` smoke stub (and `proc_starttime` is
    /// `None`), so `session_alive` is `true` for everything there. The
    /// reuse-detection logic — the whole point of #34 — is Linux-only, matching
    /// the `/proc` runtime target.
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
        sink.send(Frame::SessionRemoved { sid: "f".into() });
        assert_eq!(sink.dropped, 0, "overflow signal flushed, counter reset");
        assert!(
            matches!(rx.try_recv(), Ok(Frame::Overflow { dropped: 3 })),
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
            cwd: None,
            name: None,
            path: None,
            lines: None,
            status: None,
            waiting_for: None,
        });
        assert!(matches!(rx.try_recv(), Ok(Frame::SessionAdded { .. })));
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
}
