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
//! - 票表挂在本连接上（`stream/inbound` 的读循环持有）：连接走了 ⇒ 票表 `Drop` ⇒ 每张票的订阅线程收到「停」⇒ 关那个控制模式客户端的输入、等它自己退、收尸
//!   （不直接杀，起落也一个一个来：tmux 3.6a 的 server 会因控制模式客户端被杀、或几个同时起落而段错误，见 [`SHIFT`]）。
//! - 每张票两条线程：读控制模式输出的那一条（读到头 ＝ 客户端退了）· 订阅本身那一条（抓屏 · 推帧 · 等回执）。都是阻塞读，不醒来。
//! - 那个控制模式客户端不算「连着几个终端窗口」：名单 `clients` 与 ↗ 的 `list-clients` 都把控制模式客户端滤掉；
//!   tmux 快照里那一列「有人连着」（`session_attached`）会因它变成 1，今天没有读者（判据钉着读出来的不变）。
//! - **契约里写明**：订着时那台 tmux 里多一个只读客户端 —— 用户自己配的 `client-attached` / `client-detached` 钩子会被它触发，
//!   我们管不了；本仓自己装的钩子里没有客户端那一族（判据钉着）。

use crate::wire::FrameMake as _;
use copy_core::copy_text;
use copy_core::{Discard, Why};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc as smpsc, Arc, Mutex};

use serde_json::Value;
use tokio::sync::mpsc;

use super::terminals::{self, On};
use crate::wire::{FollowEnd, Frame};
use cmd_wire::session::TermInput;

/// 订上。
pub const FOLLOW: &str = <TerminalFollow as crate::wire::typed::Command>::NAME;
/// 第 `seq` 帧画完了。
pub const FOLLOW_ACK: &str = <TerminalFollowAck as crate::wire::typed::Command>::NAME;
/// 退订。
pub const UNFOLLOW: &str = <TerminalUnfollow as crate::wire::typed::Command>::NAME;

/// 一条流连接上同时在册的订阅数上限（有界资源：每张票一个 tmux 客户端 ＋ 两条线程）。
pub const MAX_FOLLOWS_PER_CONNECTION: usize = 8;

/// 一帧画面（`view` 序列化之后）的字节上限：超了不推，改推收尾帧 `too_big`。
/// 依据：200 列 × 60 行、每格都着色的满屏约 150 KiB；512 KiB 留三倍多余量，又远小于客户端一行的上限（64 MiB）。
pub(crate) const SCREEN_FRAME_CAP: cap_core::Cap = cap_core::Cap {
    bytes: 512 * 1024,
    over: cap_core::Over::Reject,
};

/// 「退过、却还没订过」的票记几张（退订先于订阅到的那一刻用；票只用一次，记最近的就够）。
const DROPPED_KEPT: usize = 64;

/// 票的长度上限（客户端给的不透明串）。
const MAX_TICKET_BYTES: usize = 128;

/// 只读、不改尺寸的控制模式客户端从哪个版本起有（`attach -f read-only,ignore-size`）。
const MIN_TMUX: (u32, u32) = (3, 2);

/// `#{version}` 够不够（`3.6` · `3.2a` · `next-3.4` 认；`3.1c` · `master` · 空串不认）。
pub fn tmux_version_ok(v: &str) -> bool {
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
pub struct Seat {
    ticket: String,
    n: u64,
}

/// 本连接的订阅票表。随连接的读循环一起死（最后一份放手 ⇒ 全部退订）。可以 `clone`（起订阅那一下在阻塞线程池里做，带一份过去）。
#[derive(Clone)]
pub struct Desk(Arc<Inner>);

struct Inner {
    replies: mpsc::Sender<Frame>,
    /// 问哪台 tmux：生产 `None`（默认 socket）；判据交隔离 socket。
    socket: Option<String>,
    tickets: Tickets,
}

use crate::common::said::Said;

fn bad_args(detail: &str) -> Said {
    crate::wire::spec::fail("bad_args", crate::common::contract::malformed(detail))
}

/// 票非空、不超长（客户端给的不透明串）。
fn ticket_ok(t: &str) -> Result<(), Said> {
    if t.is_empty() || t.len() > MAX_TICKET_BYTES {
        return Err(bad_args(
            "`ticket` must be a non-empty string (at most 128 bytes)",
        ));
    }
    Ok(())
}

/// `terminal-follow` 的入参：订阅票 ＋ 目标（`terminal` 与 `sid` 恰给一个）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct FollowReq {
    /// 订阅票（客户端铸的不透明串，至多 128 字节，只用一次）；之后的 `terminal_screen` / `terminal_follow_end` 帧带它。
    pub(crate) ticket: String,
    /// 目标：名单里的句柄（与 `sid` 恰给一个）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub(crate) terminal: Option<String>,
    /// 目标：会话 id（与 `terminal` 恰给一个）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub(crate) sid: Option<String>,
}

/// `terminal-follow-ack` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct AckReq {
    pub ticket: String,
    /// 画完的那一帧的序号。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub seq: u64,
}

/// `terminal-unfollow` 的入参住 cmd-wire（壳替界面照同一份发）。
use cmd_wire::terminal::UnfollowReq;

/// `terminal-follow` 的码表：`bad_args` 打头；订不上的每一种都带 `data: {live}`（[`FollowErr`]）。
const FOLLOW_CODES: &[&str] = &[
    "bad_args",
    "bad_target",
    "not_known",
    "ambiguous",
    "no_tmux",
    "tmux_too_old",
    "too_many_follows",
    "unobservable",
    "child_timed_out",
];

/// `terminal-follow` 订不上：码 ＋ 那一句（原话跟着那一句走）。投出去时带上实时那一格落在「只能快照」还是「停了」
/// （[`live_after_refusal`]；入参形状不对那一种不带）。
#[derive(Debug)]
pub struct FollowErr(Said);

impl FollowErr {
    /// 下层那一份失败 ⇒ 这一条的码；码表外的落 `unobservable`（原码进原话）。
    pub fn of(e: Said) -> FollowErr {
        crate::wire::typed::of_fail_said(e, |said| FollowErr(Said::new("unobservable", said)), said)
    }
}

/// 订实时画面被拒成 `code` 时给人看的那一句；太老的 tmux（带版本）与满了（带上限）处理器那句就是成品 ⇒ `None`。
pub fn said(code: &str) -> Option<String> {
    Some(match code {
        "tmux_too_old" | "too_many_follows" => return None,
        "no_tmux" => copy_text("beTermFollow.refuse.noTmux", &[]),
        "not_known" => copy_text("beTermFollow.end.gone", &[]),
        "ambiguous" => copy_text("terminal.why.ambiguous", &[]),
        _ => copy_text("beTermFollow.refuse.other", &[]),
    })
}

impl crate::wire::typed::Codes for FollowErr {
    const ALL: &'static [&'static str] = FOLLOW_CODES;
    fn into_said(self) -> crate::common::said::Said {
        let FollowErr(s) = self;
        let code = s.code_or_fallback().to_string();
        let f = crate::wire::spec::fail(&code, s);
        if code == "bad_args" {
            return f;
        }
        match serde_json::to_value(live_after_refusal(&code)) {
            Ok(v) => f.with_data(v),
            Err(_) => f,
        }
    }
    fn of_code(code: &str, said: String) -> Result<Self, String> {
        match FOLLOW_CODES.iter().find(|c| **c == code) {
            Some(c) => Ok(FollowErr(Said::new(c, said))),
            None => Err(said),
        }
    }
    const DATA: &'static [&'static str] = FOLLOW_CODES.split_at(1).1;
    type Data = crate::wire::typed::Each<Refused>;
}

crate::wire::typed::codes! {
    /// `terminal-follow-ack` 的码：不认的票（停了 / 没订过）。
    pub enum AckErr {
        NotKnown = "not_known",
    }
}

crate::wire::typed::codes! {
    /// `terminal-unfollow` 没有别的码（幂等）。
    pub enum UnfollowErr {}
}

/// `terminal-follow`（帧面硬臂：要碰本连接的票表与应答通道；订上那一下起几个 tmux ⇒ 挪进阻塞线程池）。
pub struct TerminalFollow;

impl crate::wire::typed::Command for TerminalFollow {
    const NAME: &'static str = "terminal-follow";
    type Req = FollowReq;
    type Reply = ();
    type Err = FollowErr;
}

/// `terminal-follow-ack`（帧面硬臂，就地记账）。
pub struct TerminalFollowAck;

impl crate::wire::typed::Command for TerminalFollowAck {
    const NAME: &'static str = "terminal-follow-ack";
    type Req = AckReq;
    type Reply = ();
    type Err = AckErr;
}

/// `terminal-unfollow`（帧面硬臂，就地记账、幂等）。
pub struct TerminalUnfollow;

impl crate::wire::typed::Command for TerminalUnfollow {
    crate::wire::typed::by_call!(UnfollowReq);
    type Err = UnfollowErr;
}

/// 订不上之后实时那一格落在哪。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "FollowLive",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "snake_case")]
pub enum Live {
    /// 这台只能快照（没装 tmux · tmux 太老）。
    SnapshotOnly,
    /// 别的。
    Stopped,
}

/// `terminal-follow` 订不上时失败应答的 `data`（订上了不回 `data`：画面走 `terminal_screen` 帧）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "FollowRefused",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub struct Refused {
    pub(crate) live: Live,
}

/// 订不上（码）⇒ 失败应答的 `data`：`{live}`。
pub fn live_after_refusal(code: &str) -> Refused {
    Refused {
        live: match code {
            "tmux_too_old" | "no_tmux" => Live::SnapshotOnly,
            _ => Live::Stopped,
        },
    }
}

/// 停因 ⇒ 给人看的那一句（进 `terminal_follow_end` 的 `said`）。
pub fn end_said(why: FollowEnd) -> String {
    match why {
        FollowEnd::Gone => copy_text("beTermFollow.end.gone", &[]),
        FollowEnd::Lost => copy_text("terminal.liveWhy.lost", &[]),
        FollowEnd::TooBig => copy_text("beTermFollow.end.tooBig", &[]),
    }
}

/// 抓那个终端的一屏失败了（或控制模式客户端退了之后再看一眼）⇒ 停因：终端 / tmux 都没了 ⇒ `gone`；还在 ⇒ `lost`。
fn end_of<T>(seen: &Result<T, Said>) -> FollowEnd {
    match seen {
        Err(f) if matches!(f.code.as_deref(), Some("no_such_session" | "no_server")) => {
            FollowEnd::Gone
        }
        _ => FollowEnd::Lost,
    }
}

impl Desk {
    pub fn new(replies: mpsc::Sender<Frame>) -> Desk {
        Desk::on_socket(replies, None)
    }

    pub fn on_socket(replies: mpsc::Sender<Frame>, socket: Option<String>) -> Desk {
        Desk(Arc::new(Inner {
            replies,
            socket,
            tickets: Arc::new(Mutex::new(Book::default())),
        }))
    }

    /// 三条命令之一 ⇒ 一帧应答（判据与 CLI 式调用用；帧面分派里 `terminal-follow` 走阻塞线程池，见 [`Desk::follow`]）。
    pub fn answer_wire(
        &self,
        cmd: &str,
        id: &str,
        args: &Value,
        tz: &crate::common::time::Tz,
    ) -> Frame {
        use crate::wire::typed::answer;
        match cmd {
            FOLLOW => {
                answer::<TerminalFollow>(id, args, |r| self.follow(&r, tz).map_err(FollowErr::of))
            }
            FOLLOW_ACK => answer::<TerminalFollowAck>(id, args, |r| {
                self.ack(&r)
                    .map_err(crate::wire::typed::of_fail_with(AckErr::NotKnown))
            }),
            UNFOLLOW => answer::<TerminalUnfollow>(id, args, |r| {
                self.unfollow(&r)
                    .map_err(crate::wire::typed::of_fail_with(UnfollowErr::BadArgs))
            }),
            other => Frame::refused(
                id,
                other,
                "unknown_command",
                &crate::common::contract::malformed(&format!("unknown follow command `{other}`")),
            ),
        }
    }

    /// 订上：占位 · 认终端（问一次名单）· 看 tmux 版本 · 起控制模式客户端 · 登记 · 起订阅线程（它当场推第一帧）。
    /// 会起几个 tmux ⇒ 帧面放进阻塞线程池。这张票退过 ⇒ 什么都不起、回 `Ok`。
    /// `tz` ＝ 看的那一台的时区（订阅那一问带来的）：之后推的每一帧的抓屏时刻按它写。
    pub fn follow(&self, req: &FollowReq, tz: &crate::common::time::Tz) -> Result<(), Said> {
        let Some(seat) = self.reserve(&req.ticket)? else {
            return Ok(());
        };
        let r = self.take_seat(&seat, req, tz);
        if r.is_err() {
            self.release(&seat);
        }
        r
    }

    /// 占位：查重 · 查上限 · 占上（一把锁里）。这张票退过 ⇒ `None`（订阅那一问到得比退订晚，不必再起）。
    pub fn reserve(&self, ticket: &str) -> Result<Option<Seat>, Said> {
        ticket_ok(ticket)?;
        let ticket = ticket.to_string();
        let mut g = lock(&self.0.tickets);
        if let Some(at) = g.dropped.iter().position(|t| *t == ticket) {
            g.dropped.remove(at);
            return Ok(None);
        }
        if g.slots.contains_key(&ticket) {
            return Err(bad_args("this `ticket` is already following"));
        }
        if g.slots.len() >= MAX_FOLLOWS_PER_CONNECTION {
            return Err(Said::new(
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
    pub fn release(&self, seat: &Seat) {
        let mut g = lock(&self.0.tickets);
        if matches!(g.slots.get(&seat.ticket), Some(Slot::Starting(n)) if *n == seat.n) {
            g.slots.remove(&seat.ticket);
        }
    }

    /// 凭占位起：认终端 · 看版本 · 起控制模式客户端，再把占位换成真的；占位已经被退订摘掉 ⇒ 收掉刚起的客户端、回 `Ok`。
    pub fn take_seat(
        &self,
        seat: &Seat,
        req: &FollowReq,
        tz: &crate::common::time::Tz,
    ) -> Result<(), Said> {
        let me = &self.0;
        if let Some(sock) = terminals::hosted_sock_of(req.terminal.as_deref(), req.sid.as_deref())?
        {
            return self.take_hosted(seat, &sock, tz);
        }
        let on = On {
            socket: me.socket.as_deref(),
        };
        let t = terminals::follow_target_on(on, req.terminal.as_deref(), req.sid.as_deref())?;
        let (proc, out) = open_client(me.socket.as_deref(), &t.session)?;
        let (tx, rx) = smpsc::channel::<Ev>();
        let pending = Arc::new(AtomicBool::new(false));
        {
            let mut g = lock(&me.tickets);
            match g.slots.get(&seat.ticket) {
                Some(Slot::Starting(n)) if *n == seat.n => {
                    g.slots
                        .insert(seat.ticket.clone(), Slot::On(Follow { tx: tx.clone() }));
                }
                // 起的这几下当中退订到了 ⇒ 刚起的客户端收掉（关输入、等它自己退，另起一条线程等，不占这一问）。
                _ => {
                    spawn_reader(out, None, tx, pending);
                    std::thread::spawn(move || detach(proc, &rx));
                    return Ok(());
                }
            }
        }
        spawn_reader(out, t.pane.clone(), tx, Arc::clone(&pending));
        let worker = Worker {
            ticket: seat.ticket.clone(),
            source: Source::Tmux {
                target: t.target,
                socket: me.socket.clone(),
            },
            replies: me.replies.clone(),
            tickets: Arc::clone(&me.tickets),
            pending,
            tz: tz.clone(),
        };
        std::thread::spawn(move || worker.run(rx, Conn::Tmux(proc)));
        Ok(())
    }

    /// 托管那一形凭占位起：经托管口开一条只看的连接（`hello{want: view, who: backend}`），读线记「变了」；
    /// 抓屏照旧走托管口 `capture`，帧里带此刻的持有者。托管口连接失败 ⇒ `not_known`（同 tmux 那一行不在）。不设定时器：同 tmux 那一形，一次只有一帧在送。
    fn take_hosted(
        &self,
        seat: &Seat,
        sock: &std::path::Path,
        tz: &crate::common::time::Tz,
    ) -> Result<(), Said> {
        let me = &self.0;
        let (conn, held, buf) = open_hosted(sock)?;
        let held = Arc::new(Mutex::new(held));
        let ended = Arc::new(AtomicBool::new(false));
        let (tx, rx) = smpsc::channel::<Ev>();
        let pending = Arc::new(AtomicBool::new(false));
        spawn_hosted_reader(
            Arc::clone(&conn),
            buf,
            Arc::clone(&held),
            Arc::clone(&ended),
            tx.clone(),
            Arc::clone(&pending),
        );
        {
            let mut g = lock(&me.tickets);
            match g.slots.get(&seat.ticket) {
                Some(Slot::Starting(n)) if *n == seat.n => {
                    g.slots.insert(seat.ticket.clone(), Slot::On(Follow { tx }));
                }
                _ => {
                    std::thread::spawn(move || let_go(Conn::Hosted(conn), &rx, false));
                    return Ok(());
                }
            }
        }
        let worker = Worker {
            ticket: seat.ticket.clone(),
            source: Source::Hosted {
                sock: sock.to_path_buf(),
                held,
                ended,
            },
            replies: me.replies.clone(),
            tickets: Arc::clone(&me.tickets),
            pending,
            tz: tz.clone(),
        };
        std::thread::spawn(move || worker.run(rx, Conn::Hosted(conn)));
        Ok(())
    }

    /// 判据那一口：直接订托管口 `sock` 那个终端（不经名单认）。
    #[cfg(any(test, feature = "test-support"))]
    pub fn follow_hosted_at(
        &self,
        ticket: &str,
        sock: &std::path::Path,
        tz: &crate::common::time::Tz,
    ) -> Result<(), Said> {
        let Some(seat) = self.reserve(ticket)? else {
            return Ok(());
        };
        let r = self.take_hosted(&seat, sock, tz);
        if r.is_err() {
            self.release(&seat);
        }
        r
    }

    /// 帧面那一口：[`Desk::follow`] 挪进阻塞线程池做（起 tmux 那几下不占 worker，同换号重启那几步）。
    pub async fn follow_off_worker(
        self,
        req: FollowReq,
        tz: crate::common::time::Tz,
    ) -> Result<(), Said> {
        tokio::task::spawn_blocking(move || self.follow(&req, &tz))
            .await
            .unwrap_or_else(|e| {
                Err(crate::wire::spec::fail(
                    "unobservable",
                    crate::common::contract::malformed("follow task did not finish"),
                )
                .and_raw(Some(&e.to_string())))
            })
    }

    /// 第 `seq` 帧画完了。不认的票（停了 / 没订过）⇒ `not_known`。
    pub fn ack(&self, req: &AckReq) -> Result<(), Said> {
        ticket_ok(&req.ticket)?;
        match lock(&self.0.tickets).slots.get(&req.ticket) {
            Some(Slot::On(f)) if f.tx.send(Ev::Ack(req.seq)).is_ok() => Ok(()),
            _ => Err(Said::new(
                "not_known",
                crate::common::contract::malformed(
                    "no such follow (it ended or was never started)",
                ),
            )),
        }
    }

    /// 退订（幂等）。正在起的 ⇒ 摘掉占位（起好的那一下自己收）；不在册的 ⇒ 记进「退过」（订阅那一问也许还在路上）。
    pub(crate) fn unfollow(&self, req: &UnfollowReq) -> Result<(), Said> {
        ticket_ok(&req.ticket)?;
        let ticket = req.ticket.clone();
        let mut g = lock(&self.0.tickets);
        match g.slots.remove(&ticket) {
            Some(Slot::On(f)) => {
                f.tx.send(Ev::Stop).discard(Why::PeerGone); // 订阅线程已自己停了（终端没了、推了收尾帧）
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
                f.tx.send(Ev::Stop).discard(Why::PeerGone); // 订阅线程已自己停了
            }
        }
    }
}

/// 起一个只读、不改尺寸的控制模式客户端，挂在 `session` 那个 tmux 会话上，读到 attach 那一问答完再交回（起落排 [`SHIFT`]）。
/// tmux 太老（不到 3.2）⇒ `tmux_too_old`。实时预览与切权限档（[`Watch`]）都从这里起。
fn open_client(
    socket: Option<&str>,
    session: &str,
) -> Result<
    (
        crate::platform::child::Streaming,
        BufReader<std::process::ChildStdout>,
    ),
    Said,
> {
    let v = terminals::tmux_version_on(On { socket })?;
    if !tmux_version_ok(&v) {
        return Err(Said::new(
            "tmux_too_old",
            copy_text(
                "beTermFollow.tmux.tooOld",
                &[("version", v.as_str()), ("need", "3.2")],
            ),
        ));
    }
    // 控制模式客户端：只读、不改尺寸，挂在那个终端所在的 tmux 会话上。它自己不要 `$TMUX`（不当成嵌套），
    // 但要与别的几条问同一台 server：没给 socket 时照 `$TMUX` 里那一台。
    let socket = socket.map(str::to_string).or_else(|| {
        std::env::var("TMUX")
            .ok()
            .and_then(|t| t.split(',').next().map(str::to_string))
            .filter(|s| !s.is_empty())
    });
    #[expect(
        clippy::disallowed_methods,
        reason = "终端实时预览：起一个只读、不改尺寸的 tmux 控制模式客户端（`-C attach-session -f read-only,ignore-size`），只读它报的输出事件；不写文件，订阅一停就杀组收尸"
    )]
    let child = crate::host::tmux::child(socket.as_deref())
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .args([
            "-C",
            "attach-session",
            "-f",
            "read-only,ignore-size",
            "-t",
            session,
        ]);
    // 起落一个一个来（见 [`SHIFT`]）：占着它起客户端、等 attach 那一问答完，再放开。
    let shift = SHIFT.lock().unwrap_or_else(|p| p.into_inner());
    let mut proc = child
        .stream()
        .map_err(super::capture_pane::tmux_unavailable)?;
    let mut out = BufReader::new(proc.take_stdout().ok_or_else(|| {
        crate::wire::spec::fail(
            "unobservable",
            crate::common::contract::malformed("no output stream from the tmux control client"),
        )
    })?);
    attached(&mut out);
    drop(shift);
    Ok((proc, out))
}

/// 等重画的那一下最多等多久（按一下键之后，那一家重画屏底那一行通常在一百毫秒以内）。
#[expect(
    clippy::disallowed_methods,
    reason = "切权限档按一下键之后等重画那一次有界等待的期限（`REDRAW_WAIT`，交给 `self.rx.recv_timeout(REDRAW_WAIT)`）：重画由控制模式客户端的 `%output` 当场叫醒，到…"
)]
const REDRAW_WAIT: std::time::Duration = std::time::Duration::from_millis(2_000);

/// 一次性地看一个终端「变没变」：tmux 那一形起一个控制模式客户端、托管那一形开一条托管口只看的连接（同实时预览那两条），
/// 之后每问一次 [`Watch::changed`] 等下一次变化，带期限 [`REDRAW_WAIT`]（一次一等、不循环；问几次由调用方定、有上限）。
/// 放手 ⇒ 照 [`let_go`]（tmux 关输入等它自己退、不杀，同 [`detach`]；托管说 `bye`）。
pub struct Watch {
    conn: Option<Conn>,
    rx: smpsc::Receiver<Ev>,
    pending: Arc<AtomicBool>,
    closed: bool,
}

impl Watch {
    /// 起：挂在那个终端所在的 tmux 会话上，只看它那个窗格的输出（没有窗格 ⇒ 整个会话）。
    pub fn open(socket: Option<&str>, t: &terminals::FollowTarget) -> Result<Watch, Said> {
        let (proc, out) = open_client(socket, &t.session)?;
        let (tx, rx) = smpsc::channel::<Ev>();
        let pending = Arc::new(AtomicBool::new(false));
        spawn_reader(out, t.pane.clone(), tx, Arc::clone(&pending));
        Ok(Watch {
            conn: Some(Conn::Tmux(proc)),
            rx,
            pending,
            closed: false,
        })
    }

    /// 托管那一形：经托管口 `sock` 开一条只看的连接（同实时预览那一条），托管进程每推一帧（输出 · 尺寸 · 整屏重来 · 持有者变了）记一笔「变了」；
    /// 不起 tmux、不轮询，等法同 [`Watch::changed`]。托管口不应 ⇒ `not_known`。
    pub fn hosted(sock: &std::path::Path) -> Result<Watch, Said> {
        let (conn, held, buf) = open_hosted(sock)?;
        let (tx, rx) = smpsc::channel::<Ev>();
        let pending = Arc::new(AtomicBool::new(false));
        spawn_hosted_reader(
            Arc::clone(&conn),
            buf,
            Arc::new(Mutex::new(held)),
            Arc::new(AtomicBool::new(false)),
            tx,
            Arc::clone(&pending),
        );
        Ok(Watch {
            conn: Some(Conn::Hosted(conn)),
            rx,
            pending,
            closed: false,
        })
    }

    /// 忘掉此刻之前报过的变化（按键之前调：之后等到的才是按键带来的）。
    pub fn forget(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            if matches!(ev, Ev::Closed) {
                self.closed = true;
            }
        }
        self.pending.store(false, Ordering::Release);
    }

    /// 等下一次变化，至多 [`REDRAW_WAIT`]：变了 ⇒ `true`；到期 · 客户端退了 ⇒ `false`。
    #[expect(
        clippy::disallowed_methods,
        reason = "切权限档（`session-permission`）按一下键之后等那个终端重画的一次有界等待（`terminal_follow::Watch::changed`）：变化由 tmux 控制模式客户端的 `%output` …"
    )]
    pub fn changed(&mut self) -> bool {
        if self.closed {
            return false;
        }
        match self.rx.recv_timeout(REDRAW_WAIT) {
            Ok(Ev::Changed) => {
                self.pending.store(false, Ordering::Release);
                true
            }
            Ok(Ev::Closed) => {
                self.closed = true;
                false
            }
            _ => false,
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            let_go(conn, &self.rx, self.closed);
        }
    }
}

/// 读控制模式客户端的输出：我们那个窗格的 `%output` / `%extended-output` / `%pane-mode-changed`、或布局变了（`%layout-change`）⇒ 「变了」；
/// 别的窗格的输出或窗口/会话元数据（如自动改名、会话列表变更等）不当成画面变动。连着几笔「变了」只报一次（订阅线程取走之前不再报）。读到头 ⇒ 「客户端退了」。
fn spawn_reader(
    mut rd: BufReader<std::process::ChildStdout>,
    pane: Option<String>,
    tx: smpsc::Sender<Ev>,
    pending: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let mut line = Vec::new();
        loop {
            line.clear();
            match rd.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let is_change = if let Some(rest) = line
                .strip_prefix(b"%output ")
                .or_else(|| line.strip_prefix(b"%extended-output "))
                .or_else(|| line.strip_prefix(b"%pane-mode-changed "))
                .or_else(|| line.strip_prefix(b"%pane-exited "))
            {
                let who = rest
                    .split(|&b| b == b' ' || b == b'\n')
                    .next()
                    .unwrap_or_default();
                pane.as_deref().is_none_or(|p| p.as_bytes() == who)
            } else {
                line.starts_with(b"%layout-change ")
            };
            if !is_change {
                continue;
            }
            if !pending.swap(true, Ordering::AcqRel) && tx.send(Ev::Changed).is_err() {
                break;
            }
        }
        tx.send(Ev::Closed).discard(Why::PeerGone); // 订阅线程已停（退订 / 推了收尾帧）⇒ 没人收
    });
}

/// 控制模式客户端的**起落一个一个来**：起一个（到 attach 那一问答完）· 收一个（到它退出）都占着这把锁，同一时刻只有一个在过渡。
///
/// 🔴 tmux 3.6a 的 server 在这两种情形下会段错误，那台 server 整个没了（上面所有会话一起没，用户自己的终端也在里面；
///   内核日志 `tmux: server … segfault`）—— 10-09 在隔离 server 上实测：
///   - 控制模式客户端被 SIGKILL 掉，哪怕一次只有一个：六台 server 四百轮里没了五台；
///   - 两个以上控制模式客户端同时起落（各自关输入正常断开）：六台全没（每台百来轮）；
///   - 关输入断开、起落一个一个来：同样的负载（十组压满 CPU）、六台各四百多次起落，一台没崩。
///
/// ⇒ 不杀、只关输入（[`detach`]）；起与收都排在这把锁后面。锁是进程级的（判据各起各的隔离 server，也一起排，慢不了几毫秒）。
static SHIFT: Mutex<()> = Mutex::new(());

/// 读到 attach 那一问答完：`%end`（接上了）· `%error`（没接上，客户端随即退出）· 读到头（已经退了）。之前那几行（`%begin`）不要。
fn attached(out: &mut BufReader<std::process::ChildStdout>) {
    let mut line = Vec::new();
    loop {
        line.clear();
        match out.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) if line.starts_with(b"%end") || line.starts_with(b"%error") => return,
            Ok(_) => {}
        }
    }
}

/// 收一个还连着的控制模式客户端：占着 [`SHIFT`] 关它的输入（控制模式里输入读到头 ＝ 断开）、等读输出那条线程报「客户端退了」
/// （读到头），再放手（收尸 —— 组里这时已经没人）。不直接杀（为什么见 [`SHIFT`]）。
/// 等多久归 tmux：输入一关它就走断开那一套、随即退出；server 卡死时它也卡着，这条线程跟着停在这里（阻塞读，不醒），
///   server 缓过来或没了它就退、这里随之收尸。
fn detach(mut proc: crate::platform::child::Streaming, rx: &smpsc::Receiver<Ev>) {
    let _shift = SHIFT.lock().unwrap_or_else(|p| p.into_inner());
    proc.close_stdin();
    while !matches!(rx.recv(), Ok(Ev::Closed) | Err(_)) {}
    drop(proc);
}

/// 托管口那一下没成 ⇒ 同 tmux 那一行不在（不另立码）。
fn hosted_gone(e: &std::io::Error) -> Said {
    crate::wire::spec::fail(
        "not_known",
        crate::common::contract::malformed("the hosted terminal did not answer"),
    )
    .and_raw(Some(&e.to_string()))
}

/// 托管口报的持有者 ⇒ 名单那一格的读法（同 `host::managed::listed_at`）。
fn held_by(h: crate::host::managed::wire::Holder) -> crate::host::HeldBy {
    use crate::host::managed::wire::WhoKind;
    match h.kind {
        WhoKind::Terminal => crate::host::HeldBy::Terminal,
        WhoKind::Backend => crate::host::HeldBy::Front(h.front.unwrap_or_default()),
    }
}

/// 开一条只看的连接、读到 `welcome`（此刻的持有者）；之后读那一半不设期限（有字节才醒）。交回连接 · 持有者 · 读剩下的半帧。
#[expect(
    clippy::type_complexity,
    reason = "交回的三样只在这一处拆开用，另起类型名只为过这条提示"
)]
fn open_hosted(
    sock: &std::path::Path,
) -> Result<
    (
        Arc<own_chan::BlockingStream>,
        Option<crate::host::HeldBy>,
        Vec<u8>,
    ),
    Said,
> {
    use crate::host::managed::client;
    use crate::host::managed::wire::{Ctl, Frame as HostFrame};
    let s = client::open(sock, None).map_err(|e| hosted_gone(&e))?;
    let mut buf = Vec::new();
    let held = loop {
        match client::next_frame(&s, &mut buf).map_err(|e| hosted_gone(&e))? {
            HostFrame::Ctl(Ctl::Welcome { holder, .. }) => break holder.map(held_by),
            HostFrame::Ctl(Ctl::Ended { .. }) => {
                return Err(hosted_gone(&std::io::ErrorKind::NotConnected.into()))
            }
            _ => {}
        }
    };
    s.set_read_timeout(None).map_err(|e| hosted_gone(&e))?;
    Ok((Arc::new(s), held, buf))
}

/// 读那条只看的连接：`out` · 尺寸变了 · 整屏重来 · 持有者变了（记下新的）⇒ 「变了」（连着几笔只报一次，同 [`spawn_reader`]）；
/// `ended` ⇒ 记下终端没了；读到头 ⇒ 「客户端退了」。
fn spawn_hosted_reader(
    conn: Arc<own_chan::BlockingStream>,
    mut buf: Vec<u8>,
    held: Arc<Mutex<Option<crate::host::HeldBy>>>,
    ended: Arc<AtomicBool>,
    tx: smpsc::Sender<Ev>,
    pending: Arc<AtomicBool>,
) {
    use crate::host::managed::client;
    use crate::host::managed::wire::{Ctl, Frame as HostFrame};
    std::thread::spawn(move || {
        loop {
            let changed = match client::next_frame(&conn, &mut buf) {
                Ok(HostFrame::Out(_) | HostFrame::Ctl(Ctl::Size { .. } | Ctl::Resync { .. })) => {
                    true
                }
                Ok(HostFrame::Ctl(Ctl::Holder { holder })) => {
                    *held.lock().unwrap_or_else(|p| p.into_inner()) = holder.map(held_by);
                    true
                }
                Ok(HostFrame::Ctl(Ctl::Ended { .. })) => {
                    ended.store(true, Ordering::Release);
                    break;
                }
                Ok(_) => false,
                Err(_) => break,
            };
            if changed && !pending.swap(true, Ordering::AcqRel) && tx.send(Ev::Changed).is_err() {
                break;
            }
        }
        tx.send(Ev::Closed).discard(Why::PeerGone); // 订阅线程已停（退订 / 推了收尾帧）⇒ 没人收
    });
}

/// 订的是哪一种宿主上的终端：tmux（抓屏的句柄 ＋ 问哪台）· 托管（托管口 ＋ 读线记下的持有者 ＋ 收没收到 `ended`）。
enum Source {
    Tmux {
        target: String,
        socket: Option<String>,
    },
    Hosted {
        sock: std::path::PathBuf,
        held: Arc<Mutex<Option<crate::host::HeldBy>>>,
        ended: Arc<AtomicBool>,
    },
}

impl Source {
    /// 这一刻那一屏 ＋ 输入那一格（tmux 恒 `shared`；托管 ＝ 读线记下的持有者，`mine` 按桌面那一问算）。
    fn shot(
        &self,
        tz: &crate::common::time::Tz,
    ) -> Result<(cmd_wire::session::Preview, TermInput), Said> {
        match self {
            Source::Tmux { target, socket } => terminals::screen_view_on(
                On {
                    socket: socket.as_deref(),
                },
                target,
                tz,
            )
            .map(|v| (v, TermInput::Shared)),
            Source::Hosted { sock, held, .. } => {
                let v = terminals::hosted_screen_at(sock, tz)?;
                let h = held.lock().unwrap_or_else(|p| p.into_inner()).clone();
                Ok((
                    v,
                    terminals::input_of_row(&crate::host::Input::Held(h), None),
                ))
            }
        }
    }

    /// 看着它的那条路收了之后的停因：托管口说过 `ended` ⇒ `gone`；否则再看一眼（终端 / tmux 都没了 ⇒ `gone`，还在 ⇒ `lost`）。
    fn end_after_close(&self, tz: &crate::common::time::Tz) -> FollowEnd {
        if let Source::Hosted { ended, .. } = self {
            if ended.load(Ordering::Acquire) {
                return FollowEnd::Gone;
            }
        }
        end_of(&self.shot(tz))
    }
}

/// 看着它的那条路：tmux 控制模式客户端 · 托管口上那条只看的连接。
enum Conn {
    Tmux(crate::platform::child::Streaming),
    Hosted(Arc<own_chan::BlockingStream>),
}

/// 放手那条路：`closed` ＝ 它已经退了（只收尸）；没退 ⇒ tmux 照 [`detach`]，托管说 `bye`、等读线读到头。
fn let_go(conn: Conn, rx: &smpsc::Receiver<Ev>, closed: bool) {
    match conn {
        Conn::Tmux(p) if closed => drop(p),
        Conn::Tmux(p) => detach(p, rx),
        Conn::Hosted(_) if closed => {}
        Conn::Hosted(c) => {
            crate::host::managed::client::bye(&c);
            while !matches!(rx.recv(), Ok(Ev::Closed) | Err(_)) {}
        }
    }
}

/// 一张票的订阅本身：抓屏 · 推帧 · 等回执。
struct Worker {
    ticket: String,
    source: Source,
    replies: mpsc::Sender<Frame>,
    tickets: Tickets,
    pending: Arc<AtomicBool>,
    /// 看的那一台的时区（订阅那一问带来的）。
    tz: crate::common::time::Tz,
}

/// 一次抓屏之后怎么走。
enum Step {
    Go,
    End(FollowEnd),
    /// 看的那一方没了（应答通道关了）。
    Quit,
}

impl Worker {
    fn run(self, rx: smpsc::Receiver<Ev>, conn: Conn) {
        let mut seq = 0u64;
        let mut in_flight = false;
        let mut dirty = true;
        let mut last: Option<(String, TermInput)> = None;
        let mut step = self.capture(&mut seq, &mut in_flight, &mut dirty, &mut last);
        let mut closed = false;
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
                    closed = true;
                    Step::End(self.source.end_after_close(&self.tz))
                }
                Ok(Ev::Stop) | Err(_) => Step::Quit,
            };
        }
        let_go(conn, &rx, closed);
        if let Step::End(why) = step {
            lock(&self.tickets).slots.remove(&self.ticket);
            // 订它的那条流连接已经走了 ⇒ 收尾帧没人收（票表随连接一起丢）
            self.replies
                .blocking_send(Frame::TerminalFollowEnd(crate::wire::TerminalFollowEnd {
                    ticket: self.ticket.clone(),
                    why,
                    said: end_said(why),
                }))
                .discard(Why::PeerGone);
        }
    }

    /// 变了、而且没有在途的那一帧 ⇒ 抓一屏；指纹与输入那一格都与上一帧相同就不推（持有者变了、画面没变也推）。
    fn capture(
        &self,
        seq: &mut u64,
        in_flight: &mut bool,
        dirty: &mut bool,
        last: &mut Option<(String, TermInput)>,
    ) -> Step {
        *dirty = false;
        let (shot, input) = match self.source.shot(&self.tz) {
            Ok(v) => v,
            seen @ Err(_) => return Step::End(end_of(&seen)),
        };
        let fp = Some((shot.screen.clone(), input.clone()));
        if fp == *last {
            return Step::Go;
        }
        if serde_json::to_string(&shot).map_or(usize::MAX, |s| s.len()) > SCREEN_FRAME_CAP.as_len()
        {
            return Step::End(FollowEnd::TooBig);
        }
        *seq += 1;
        *in_flight = true;
        *last = fp;
        match self
            .replies
            .blocking_send(Frame::TerminalScreen(crate::wire::TerminalScreen {
                ticket: self.ticket.clone(),
                seq: *seq,
                view: shot,
                input,
            })) {
            Ok(()) => Step::Go,
            Err(_) => Step::Quit,
        }
    }
}
