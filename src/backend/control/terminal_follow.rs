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
//! 订阅停了（终端没了 · 看着它的那条路断了 · 一屏太大）⇒ 推 `terminal_follow_end {ticket, why}`，忘掉这张票。
//!
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

type Tickets = Arc<Mutex<HashMap<String, Follow>>>;

fn lock(t: &Tickets) -> std::sync::MutexGuard<'_, HashMap<String, Follow>> {
    t.lock().unwrap_or_else(|p| p.into_inner())
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

impl Desk {
    pub(crate) fn new(replies: mpsc::Sender<Frame>) -> Desk {
        Desk::on_socket(replies, None)
    }

    pub(crate) fn on_socket(replies: mpsc::Sender<Frame>, socket: Option<String>) -> Desk {
        Desk(Arc::new(Inner {
            replies,
            socket,
            tickets: Arc::new(Mutex::new(HashMap::new())),
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
            Err((code, m)) => Frame::err(id, code, &m),
        }
    }

    /// 订上：认终端（问一次名单）· 看 tmux 版本 · 起控制模式客户端 · 起订阅线程（它当场推第一帧）。会起几个 tmux ⇒ 帧面放进阻塞线程池。
    pub(crate) fn follow(&self, args: &Value) -> Result<(), CmdErr> {
        let me = &self.0;
        let ticket = ticket_of(args)
            .ok_or_else(|| bad_args("`ticket` must be a non-empty string (at most 128 bytes)"))?;
        {
            let g = lock(&me.tickets);
            if g.contains_key(&ticket) {
                return Err(bad_args("this `ticket` is already following"));
            }
            if g.len() >= MAX_FOLLOWS_PER_CONNECTION {
                return Err((
                    "too_many_follows",
                    copy_text(
                        "beTermFollow.register.tooMany",
                        &[("max", &MAX_FOLLOWS_PER_CONNECTION.to_string())],
                    ),
                ));
            }
        }
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
            // 两次同票的 follow 并发进来：后到的那一个不登记（它起的客户端随 `proc` 一起收）。
            if g.contains_key(&ticket) || g.len() >= MAX_FOLLOWS_PER_CONNECTION {
                return Err(bad_args("this `ticket` is already following"));
            }
            g.insert(ticket.clone(), Follow { tx: tx.clone() });
        }
        spawn_reader(out, t.pane.clone(), tx, Arc::clone(&pending));
        let worker = Worker {
            ticket,
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
        match lock(&self.0.tickets).get(&ticket) {
            Some(f) if f.tx.send(Ev::Ack(seq)).is_ok() => Ok(()),
            _ => Err((
                "not_known",
                crate::common::contract::malformed(
                    "no such follow (it ended or was never started)",
                ),
            )),
        }
    }

    /// 退订（幂等）。
    pub(crate) fn unfollow(&self, args: &Value) -> Result<(), CmdErr> {
        let ticket =
            ticket_of(args).ok_or_else(|| bad_args("`ticket` must be a non-empty string"))?;
        if let Some(f) = lock(&self.0.tickets).remove(&ticket) {
            let _ = f.tx.send(Ev::Stop);
        }
        Ok(())
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        for (_, f) in lock(&self.tickets).drain() {
            let _ = f.tx.send(Ev::Stop);
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
                    match terminals::screen_view_on(on, &self.target) {
                        Ok(_) => Step::End(FollowEnd::Lost),
                        Err(_) => Step::End(FollowEnd::Gone),
                    }
                }
                Ok(Ev::Stop) | Err(_) => Step::Quit,
            };
        }
        drop(proc); // 杀控制模式客户端、收尸
        if let Step::End(why) = step {
            lock(&self.tickets).remove(&self.ticket);
            let _ = self.replies.blocking_send(Frame::TerminalFollowEnd {
                ticket: self.ticket.clone(),
                why,
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
            Err(("no_such_session", _)) => return Step::End(FollowEnd::Gone),
            Err(_) => return Step::End(FollowEnd::Lost),
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
