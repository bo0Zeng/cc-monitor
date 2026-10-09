//! 终端实时预览（L2）：订阅名单里一个终端的画面，有变化就推一整屏。
//!
//! # 线上三条帧命令（`Run::Builtin`：要碰本连接的票表与应答通道，同传输那几条；只在帧面）
//!
//! | 命令 | `args` | 应答 | |
//! |---|---|---|---|
//! | `terminal-follow` | `{terminal \| sid, ticket}` | — | 订上：当场推第一帧 `terminal_screen {ticket, seq: 1, view}` |
//! | `terminal-follow-ack` | `{ticket, seq}` | — | 第 `seq` 帧画完了：画面有变化就推下一帧 |
//! | `terminal-unfollow` | `{ticket}` | — | 退订（幂等）；不发收尾帧 |
//!
//! 订阅停了（终端没了 · 看着它的那条路断了 · 一屏太大）⇒ 推 `terminal_follow_end {ticket, why, said}`（`said` 是给人看的那一句），忘掉这张票。
//! 订不上 ⇒ 失败应答带 `data: {live}`：`snapshot_only`（这台只能快照：没装 tmux · tmux 太老）· `stopped`（别的）。
//!
//! # 票的一生（退订早到也收得干净）
//!
//! 票是客户端铸的、**只用一次**。订阅那一问要起几个 tmux（在阻塞线程池里），退订那一问就地做完 —— 两问可能交错，甚至退订先到
//! （壳在界面那条画面流撤掉时替它退订，与界面发的订阅各走各的）。所以：
//! - 订阅进来**先占位**（查重 · 查上限 · 占上，一把锁里做完），起好 tmux 再把占位换成真的；起不成 ⇒ 让出占位。
//! - 退订碰到占位 ⇒ 摘掉；起好的那一下发现占位没了 ⇒ 当场收掉刚起的客户端、回 `ok`。
//! - 退订碰到不在册的票 ⇒ 记进「退过」（有界，最近 [`DROPPED_KEPT`] 张）；之后这张票的订阅那一问不起客户端、回 `ok`。

//! # 怎么知道画面变了（零定时器）
//!
//! 每张票起一个 tmux **控制模式**客户端（`attach -f read-only,ignore-size`：只读、不改窗格尺寸，tmux 3.2 起才有），
//! 它把那个会话里每个窗格的输出逐段报成 `%output %<窗格> …`；读到我们那个窗格的（或任何别的通知：窗格关了、布局变了…）⇒ 记一笔「变了」。
//! 画面本身照旧由抓一屏那一条现抓（与 `terminal-preview` 同一份成品、同一个指纹），不在这里做终端模拟。
//!
//! # 节奏（一帧在途）
//!
//! 推出一帧之后，等客户端回执（`terminal-follow-ack`）才推下一帧；中间的变化合并成回执之后的那一次抓屏，画面没变（指纹相同）不推。
//! 节奏因此归看的那一方（画完才回执，界面另有两次回执之间的最短间隔），后端不起任何节拍。
//!
//! # 存亡
//!
//! - 票表挂在本连接上（`stream/inbound` 的读循环持有）：连接走了 ⇒ 票表 `Drop` ⇒ 每张票的订阅线程收到「停」⇒ 杀那个控制模式客户端、收尸。
//! - 每张票两条线程：读控制模式输出的那一条（读到头 ＝ 客户端退了）· 订阅本身那一条（抓屏 · 推帧 · 等回执）。都是阻塞读，不醒来。
//! - 那个控制模式客户端不算「连着几个终端窗口」：名单 `clients` 与 ↗ 的 `list-clients` 都把控制模式客户端滤掉；
//!   tmux 快照里那一列「有人连着」（`session_attached`）会因它变成 1，今天没有读者（判据钉着读出来的不变）。
//! - **契约里写明**：订着时那台 tmux 里多一个只读客户端 —— 用户自己配的 `client-attached` / `client-detached` 钩子会被它触发，
//!   我们管不了；本仓自己装的钩子里没有客户端那一族（判据钉着）。

use copy_core::copy_text;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc as smpsc, Arc, Mutex};

use serde_json::Value;
use tokio::sync::mpsc;

use super::terminals::{self, On};
use crate::stream::wire::{FollowEnd, Frame};

/// 订上。
pub const FOLLOW: &str = "terminal-follow";
/// 第 `seq` 帧画完了。
pub const FOLLOW_ACK: &str = "terminal-follow-ack";
/// 退订。
pub const UNFOLLOW: &str = "terminal-unfollow";

/// 一条流连接上同时在册的订阅数上限（有界资源：每张票一个 tmux 客户端 ＋ 两条线程）。
pub const MAX_FOLLOWS_PER_CONNECTION: usize = 8;

/// 一帧画面（`view` 序列化之后）的字节上限：超了不推，改推收尾帧 `too_big`。
/// 依据：200 列 × 60 行、每格都着色的满屏约 150 KiB；512 KiB 留三倍多余量，又远小于客户端一行的上限（64 MiB）。
pub const SCREEN_FRAME_CAP: usize = 512 * 1024;

/// 「退过、却还没订过」的票记几张（退订先于订阅到的那一刻用；票只用一次，记最近的就够）。
const DROPPED_KEPT: usize = 64;

/// 票的长度上限（客户端给的不透明串）。
const MAX_TICKET_BYTES: usize = 128;

/// 只读、不改尺寸的控制模式客户端从哪个版本起有（`attach -f read-only,ignore-size`）。
const MIN_TMUX: (u32, u32) = (3, 2);

/// `#{version}` 够不够（`3.6` · `3.2a` · `next-3.4` 认；`3.1c` · `master` · 空串不认）。
pub(crate) fn tmux_version_ok(v: &str) -> bool {
    let v = v.trim().trim_start_matches("next-");
    let mut it = v.split('.');
    let major = it.next().and_then(|m| m.parse::<u32>().ok());
    let minor = it
        .next()
        .map(|m| m.trim_end_matches(|c: char| c.is_ascii_alphabetic()))
        .and_then(|m| m.parse::<u32>().ok());
    match (major, minor) {
        (Some(a), Some(b)) => (a, b) >= MIN_TMUX,
        _ => false,
    }
}

/// 订阅线程收的事。
enum Ev {
    /// 那个终端可能变了（控制模式报了输出或别的通知）。
    Changed,
    /// 第几帧画完了。
    Ack(u64),
    /// 退订 / 连接走了。
    Stop,
    /// 控制模式客户端退了（读到头）。
    Closed,
}

struct Follow {
    tx: smpsc::Sender<Ev>,
}

/// 票表里的一格：正在起（占位，带一个号认是哪一次占的）· 订着。
enum Slot {
    Starting(u64),
    On(Follow),
}

/// 本连接的票表：在册的票 ＋ 「退过、还没订过」的票。
#[derive(Default)]
struct Book {
    slots: HashMap<String, Slot>,
    dropped: std::collections::VecDeque<String>,
    seats: u64,
}

type Tickets = Arc<Mutex<Book>>;

fn lock(t: &Tickets) -> std::sync::MutexGuard<'_, Book> {
    t.lock().unwrap_or_else(|p| p.into_inner())
}

/// 订阅那一问占上的位（起好 tmux 之后凭它登记；起不成凭它让出）。
pub(crate) struct Seat {
    ticket: String,
    n: u64,
}

/// 本连接的订阅票表。随连接的读循环一起死（最后一份放手 ⇒ 全部退订）。可以 `clone`（起订阅那一下在阻塞线程池里做，带一份过去）。
#[derive(Clone)]
pub(crate) struct Desk(Arc<Inner>);

struct Inner {
    replies: mpsc::Sender<Frame>,
    /// 问哪台 tmux：生产 `None`（默认 socket）；判据交隔离 socket。
    socket: Option<String>,
    tickets: Tickets,
}

type CmdErr = (&'static str, String);

fn bad_args(detail: &str) -> CmdErr {
    ("bad_args", crate::common::contract::malformed(detail))
}

fn ticket_of(args: &Value) -> Option<String> {
    match args.get("ticket") {
        Some(Value::String(t)) if !t.is_empty() && t.len() <= MAX_TICKET_BYTES => Some(t.clone()),
        _ => None,
    }
}

/// 订不上（码）之后实时那一格落在哪：`snapshot_only`（这台只能快照）· `stopped`（别的）。进失败应答的 `data.live`。
pub(crate) fn live_after_refusal(code: &str) -> &'static str {
    match code {
        "tmux_too_old" | "no_tmux" => "snapshot_only",
        _ => "stopped",
    }
}

/// 停因 ⇒ 给人看的那一句（进 `terminal_follow_end` 的 `said`）。
pub(crate) fn end_said(why: FollowEnd) -> String {
    match why {
        FollowEnd::Gone => copy_text("beTermFollow.end.gone", &[]),
        FollowEnd::Lost => copy_text("beTermFollow.end.lost", &[]),
        FollowEnd::TooBig => copy_text("beTermFollow.end.tooBig", &[]),
    }
}

/// 抓那个终端的一屏失败了（或控制模式客户端退了之后再看一眼）⇒ 停因：终端 / tmux 都没了 ⇒ `gone`；还在 ⇒ `lost`。
fn end_of<T>(seen: &Result<T, CmdErr>) -> FollowEnd {
    match seen {
        Err(("no_such_session" | "no_server", _)) => FollowEnd::Gone,
        _ => FollowEnd::Lost,
    }
}

impl Desk {
    pub(crate) fn new(replies: mpsc::Sender<Frame>) -> Desk {
        Desk::on_socket(replies, None)
    }

    pub(crate) fn on_socket(replies: mpsc::Sender<Frame>, socket: Option<String>) -> Desk {
        Desk(Arc::new(Inner {
            replies,
            socket,
            tickets: Arc::new(Mutex::new(Book::default())),
        }))
    }

    /// 三条命令之一 ⇒ 一帧应答（判据与 CLI 式调用用；帧面分派里 `terminal-follow` 走阻塞线程池，见 [`Desk::follow`]）。
    pub(crate) fn answer_wire(&self, cmd: &str, id: &str, args: &Value) -> Frame {
        let r = match cmd {
            FOLLOW => self.follow(args),
            FOLLOW_ACK => self.ack(args),
            UNFOLLOW => self.unfollow(args),
            other => Err((
                "unknown_command",
                crate::common::contract::malformed(&format!("unknown follow command `{other}`")),
            )),
        };
        match r {
            Ok(()) => Frame::ok(id),
            Err((code, m)) => Frame::refused(id, cmd, code, &m),
        }
    }

    /// 订上：占位 · 认终端（问一次名单）· 看 tmux 版本 · 起控制模式客户端 · 登记 · 起订阅线程（它当场推第一帧）。
    /// 会起几个 tmux ⇒ 帧面放进阻塞线程池。这张票退过 ⇒ 什么都不起、回 `Ok`。
    pub(crate) fn follow(&self, args: &Value) -> Result<(), CmdErr> {
        let Some(seat) = self.reserve(args)? else {
            return Ok(());
        };
        let r = self.take_seat(&seat, args);
        if r.is_err() {
            self.release(&seat);
        }
        r
    }

    /// 占位：查重 · 查上限 · 占上（一把锁里）。这张票退过 ⇒ `None`（订阅那一问到得比退订晚，不必再起）。
    fn reserve(&self, args: &Value) -> Result<Option<Seat>, CmdErr> {
        let ticket = ticket_of(args)
            .ok_or_else(|| bad_args("`ticket` must be a non-empty string (at most 128 bytes)"))?;
        let mut g = lock(&self.0.tickets);
        if let Some(at) = g.dropped.iter().position(|t| *t == ticket) {
            g.dropped.remove(at);
            return Ok(None);
        }
        if g.slots.contains_key(&ticket) {
            return Err(bad_args("this `ticket` is already following"));
        }
        if g.slots.len() >= MAX_FOLLOWS_PER_CONNECTION {
            return Err((
                "too_many_follows",
                copy_text(
                    "beTermFollow.register.tooMany",
                    &[("max", &MAX_FOLLOWS_PER_CONNECTION.to_string())],
                ),
            ));
        }
        g.seats += 1;
        let n = g.seats;
        g.slots.insert(ticket.clone(), Slot::Starting(n));
        Ok(Some(Seat { ticket, n }))
    }

    /// 让出占位（起不成）：还是这一次占的那一格才摘。
    fn release(&self, seat: &Seat) {
        let mut g = lock(&self.0.tickets);
        if matches!(g.slots.get(&seat.ticket), Some(Slot::Starting(n)) if *n == seat.n) {
            g.slots.remove(&seat.ticket);
        }
    }

    /// 凭占位起：认终端 · 看版本 · 起控制模式客户端，再把占位换成真的；占位已经被退订摘掉 ⇒ 收掉刚起的客户端、回 `Ok`。
    fn take_seat(&self, seat: &Seat, args: &Value) -> Result<(), CmdErr> {
        let me = &self.0;
        let on = On {
            socket: me.socket.as_deref(),
        };
        let t = terminals::follow_target_on(on, args)?;
        let v = terminals::tmux_version_on(on)?;
        if !tmux_version_ok(&v) {
            return Err((
                "tmux_too_old",
                copy_text(
                    "beTermFollow.tmux.tooOld",
                    &[("version", v.as_str()), ("need", "3.2")],
                ),
            ));
        }
        // 控制模式客户端：只读、不改尺寸，挂在那个终端所在的 tmux 会话上。它自己不要 `$TMUX`（不当成嵌套），
        // 但要与别的几条问同一台 server：没给 socket 时照 `$TMUX` 里那一台。
        let socket = me.socket.clone().or_else(|| {
            std::env::var("TMUX")
                .ok()
                .and_then(|t| t.split(',').next().map(str::to_string))
                .filter(|s| !s.is_empty())
        });
        let mut child = crate::platform::child::Child::new("tmux");
        if let Some(s) = &socket {
            child = child.args(["-S", s.as_str()]);
        }
        let child = child.env_remove("TMUX").env_remove("TMUX_PANE").args([
            "-C",
            "attach-session",
            "-f",
            "read-only,ignore-size",
            "-t",
            t.session.as_str(),
        ]);
        let mut proc = child
            .stream()
            .map_err(super::capture_pane::tmux_unavailable)?;
        let out = proc.take_stdout().ok_or_else(|| {
            (
                "unobservable",
                crate::common::contract::malformed("no output stream from the tmux control client"),
            )
        })?;
        let (tx, rx) = smpsc::channel::<Ev>();
        let pending = Arc::new(AtomicBool::new(false));
        {
            let mut g = lock(&me.tickets);
            match g.slots.get(&seat.ticket) {
                Some(Slot::Starting(n)) if *n == seat.n => {
                    g.slots
                        .insert(seat.ticket.clone(), Slot::On(Follow { tx: tx.clone() }));
                }
                // 起的这几下当中退订到了 ⇒ 刚起的客户端随 `proc` 一起收。
                _ => return Ok(()),
            }
        }
        spawn_reader(out, t.pane.clone(), tx, Arc::clone(&pending));
        let worker = Worker {
            ticket: seat.ticket.clone(),
            target: t.target,
            socket: me.socket.clone(),
            replies: me.replies.clone(),
            tickets: Arc::clone(&me.tickets),
            pending,
        };
        std::thread::spawn(move || worker.run(rx, proc));
        Ok(())
    }

    /// 帧面那一口：[`Desk::follow`] 挪进阻塞线程池做（起 tmux 那几下不占 worker，同换号重启那几步）。
    pub(crate) async fn follow_off_worker(self, args: Value) -> Result<(), CmdErr> {
        tokio::task::spawn_blocking(move || self.follow(&args))
            .await
            .unwrap_or_else(|e| {
                Err((
                    "unobservable",
                    crate::common::contract::malformed(&format!("follow task: {e}")),
                ))
            })
    }

    /// 第 `seq` 帧画完了。不认的票（停了 / 没订过）⇒ `not_known`。
    pub(crate) fn ack(&self, args: &Value) -> Result<(), CmdErr> {
        let ticket =
            ticket_of(args).ok_or_else(|| bad_args("`ticket` must be a non-empty string"))?;
        let seq = args
            .get("seq")
            .and_then(Value::as_u64)
            .ok_or_else(|| bad_args("`seq` must be a positive integer"))?;
        match lock(&self.0.tickets).slots.get(&ticket) {
            Some(Slot::On(f)) if f.tx.send(Ev::Ack(seq)).is_ok() => Ok(()),
            _ => Err((
                "not_known",
                crate::common::contract::malformed(
                    "no such follow (it ended or was never started)",
                ),
            )),
        }
    }

    /// 退订（幂等）。正在起的 ⇒ 摘掉占位（起好的那一下自己收）；不在册的 ⇒ 记进「退过」（订阅那一问也许还在路上）。
    pub(crate) fn unfollow(&self, args: &Value) -> Result<(), CmdErr> {
        let ticket =
            ticket_of(args).ok_or_else(|| bad_args("`ticket` must be a non-empty string"))?;
        let mut g = lock(&self.0.tickets);
        match g.slots.remove(&ticket) {
            Some(Slot::On(f)) => {
                let _ = f.tx.send(Ev::Stop);
            }
            Some(Slot::Starting(_)) => {}
            None => {
                if !g.dropped.contains(&ticket) {
                    if g.dropped.len() >= DROPPED_KEPT {
                        g.dropped.pop_front();
                    }
                    g.dropped.push_back(ticket);
                }
            }
        }
        Ok(())
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        for (_, s) in lock(&self.tickets).slots.drain() {
            if let Slot::On(f) = s {
                let _ = f.tx.send(Ev::Stop);
            }
        }
    }
}

/// 读控制模式客户端的输出：我们那个窗格的 `%output`、或任何别的通知（窗格关了 · 布局变了 · 会话没了…）⇒ 「变了」；
/// 别的窗格的输出不管。连着几笔「变了」只报一次（订阅线程取走之前不再报）。读到头 ⇒ 「客户端退了」。
fn spawn_reader(
    out: std::process::ChildStdout,
    pane: Option<String>,
    tx: smpsc::Sender<Ev>,
    pending: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let mut rd = BufReader::new(out);
        let mut line = Vec::new();
        loop {
            line.clear();
            match rd.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if !line.starts_with(b"%") || line.starts_with(b"%begin") || line.starts_with(b"%end") {
                continue;
            }
            if let Some(rest) = line.strip_prefix(b"%output ") {
                let who = rest.split(|&b| b == b' ').next().unwrap_or_default();
                if pane.as_deref().is_some_and(|p| p.as_bytes() != who) {
                    continue;
                }
            }
            if !pending.swap(true, Ordering::AcqRel) && tx.send(Ev::Changed).is_err() {
                break;
            }
        }
        let _ = tx.send(Ev::Closed);
    });
}

/// 一张票的订阅本身：抓屏 · 推帧 · 等回执。
struct Worker {
    ticket: String,
    target: String,
    socket: Option<String>,
    replies: mpsc::Sender<Frame>,
    tickets: Tickets,
    pending: Arc<AtomicBool>,
}

/// 一次抓屏之后怎么走。
enum Step {
    Go,
    End(FollowEnd),
    /// 看的那一方没了（应答通道关了）。
    Quit,
}

impl Worker {
    fn run(self, rx: smpsc::Receiver<Ev>, proc: crate::platform::child::Streaming) {
        let mut seq = 0u64;
        let mut in_flight = false;
        let mut dirty = true;
        let mut last: Option<String> = None;
        let mut step = self.capture(&mut seq, &mut in_flight, &mut dirty, &mut last);
        while matches!(step, Step::Go) {
            step = match rx.recv() {
                Ok(Ev::Changed) => {
                    self.pending.store(false, Ordering::Release);
                    dirty = true;
                    if in_flight {
                        Step::Go
                    } else {
                        self.capture(&mut seq, &mut in_flight, &mut dirty, &mut last)
                    }
                }
                Ok(Ev::Ack(n)) => {
                    if n == seq {
                        in_flight = false;
                    }
                    if dirty && !in_flight {
                        self.capture(&mut seq, &mut in_flight, &mut dirty, &mut last)
                    } else {
                        Step::Go
                    }
                }
                Ok(Ev::Closed) => {
                    // 客户端退了：终端还在 ⇒ 看着它的那条路断了；不在 ⇒ 终端没了。
                    let on = On {
                        socket: self.socket.as_deref(),
                    };
                    Step::End(end_of(&terminals::screen_view_on(on, &self.target)))
                }
                Ok(Ev::Stop) | Err(_) => Step::Quit,
            };
        }
        drop(proc); // 杀控制模式客户端、收尸
        if let Step::End(why) = step {
            lock(&self.tickets).slots.remove(&self.ticket);
            let _ = self.replies.blocking_send(Frame::TerminalFollowEnd {
                ticket: self.ticket.clone(),
                why,
                said: end_said(why),
            });
        }
    }

    /// 变了、而且没有在途的那一帧 ⇒ 抓一屏；指纹与上一帧相同就不推。
    fn capture(
        &self,
        seq: &mut u64,
        in_flight: &mut bool,
        dirty: &mut bool,
        last: &mut Option<String>,
    ) -> Step {
        *dirty = false;
        let on = On {
            socket: self.socket.as_deref(),
        };
        let view = match terminals::screen_view_on(on, &self.target) {
            Ok(v) => v,
            seen @ Err(_) => return Step::End(end_of(&seen)),
        };
        let fp = view
            .get("screen")
            .and_then(Value::as_str)
            .map(str::to_string);
        if fp.is_some() && fp == *last {
            return Step::Go;
        }
        if serde_json::to_string(&view).map_or(usize::MAX, |s| s.len()) > SCREEN_FRAME_CAP {
            return Step::End(FollowEnd::TooBig);
        }
        *seq += 1;
        *in_flight = true;
        *last = fp;
        match self.replies.blocking_send(Frame::TerminalScreen {
            ticket: self.ticket.clone(),
            seq: *seq,
            view,
        }) {
            Ok(()) => Step::Go,
            Err(_) => Step::Quit,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/terminal_follow_tests.rs"]
mod tests;
