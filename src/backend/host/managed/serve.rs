//! 托管口的那一张账：连着的各端 · 谁持有输入 · 伪终端尺寸 · 各端的发送队列（A3-3-2 · A3-3-14）。
//!
//! [`Hub`] 不碰套接字也不碰伪终端：收事件（伪终端吐了字节 · 一端送来一帧 · 一端走了），改账，往各端队列里放帧，
//! 交回要对伪终端做的事（[`Effect`]）。托管进程（`run.rs`）把它接到真的套接字与伪终端上；判据直接喂事件。
//!
//! 慢端：每端一条有界队列（[`QUEUE_CAP`] 字节）。放不下 ⇒ 清空队列，换成一帧 `resync`（此刻的整屏），之后的 `out` 接在它后面 ——
//! 读伪终端永远不等任何一端（同 watcher 丢帧后整份重报）。
//!
//! 有没有人看：伪终端的输出只给看屏的端（[`Hub::watchers`]）；一个都没有 ⇒ 只记屏，不编帧、不碰队。
//! 合帧：队尾还没取走的那一段 `out` 不先编，新来的接在后面，放控制帧或被取走那一下才编成一帧（顺序照旧）；
//! 写那一半排头只有 `out` 时先等一个窗口再取（`run::write_out`），于是每端每个窗口最多一帧 `out`。
//!
//! 持有者：最新连上、要输入的真终端持有；`take` 谁都能抢（桌面 · 手机经后端接管也走它）。`in` · `resize` 只认持有者，
//! 别人的 `in` 回 `held-elsewhere`、一个字节都不送。后端的 `say`（会话输入队投递那一句）不看持有者、不看输入行、照写、不换持有者。
//! 持有者走了或交还 ⇒ 交给最近连上的另一个要输入的真终端，没有就空着。
//! 伪终端尺寸 ＝ 持有者报的尺寸；没人持有 ⇒ 保持上一次的；后端从不改尺寸。

use super::screen::Screen;
use super::wire::{ClientRow, Ctl, Frame, Holder, Size, Want, Who, WhoKind, PROTO};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

/// 一端的发送队列上限（字节）。比最大的一屏大得多，只有真不读的端才会塞满；塞满了清掉积压、换一帧 `resync` 整屏。
pub(crate) const QUEUE_CAP: cap_core::Cap = cap_core::Cap {
    bytes: 4 * 1024 * 1024,
    over: cap_core::Over::TruncateAndSay,
};

/// 一端的发送队列：托管那边放、写那一端的任务取。
#[derive(Default)]
pub(crate) struct Queue {
    frames: VecDeque<Vec<u8>>,
    /// 排在最后、还没编成帧的那一段 `out`：新来的 `out` 接在后面，放控制帧或被取走那一下才编成一帧（合帧）。
    tail: Vec<u8>,
    bytes: usize,
    /// 放不下过几次（判据与诊断看）。
    pub(crate) overflows: u32,
    /// 收尾：之后不再放；取完就关这条连接。
    closed: bool,
}

impl Queue {
    /// 取走眼下排着的全部帧（已编好；没编的那一段 `out` 这一下编成一帧排在最后）；`closed` 且取空 ⇒ 该关了。
    pub(crate) fn drain(&mut self) -> (Vec<Vec<u8>>, bool) {
        self.seal();
        let v: Vec<Vec<u8>> = self.frames.drain(..).collect();
        self.bytes = 0;
        (v, self.closed)
    }

    /// 排着的只有还没编的那一段 `out`（写那一半照它决定先等一个窗口再取，[`super::run`] 的 `write_out`）。
    pub(crate) fn only_out(&self) -> bool {
        self.frames.is_empty() && !self.tail.is_empty()
    }

    /// 没编的那一段 `out` 编成一帧、排到最后。
    fn seal(&mut self) {
        if !self.tail.is_empty() {
            let raw = std::mem::take(&mut self.tail);
            let n = raw.len();
            let f = super::wire::encode(&Frame::Out(raw));
            self.bytes = self.bytes - n + f.len();
            self.frames.push_back(f);
        }
    }

    fn clear(&mut self) {
        self.frames.clear();
        self.tail.clear();
        self.bytes = 0;
    }
}

/// 往一端的队里放的一样：编好的控制帧 · 一段还没编的 `out`。
enum Item<'a> {
    Ctl(Vec<u8>),
    Out(&'a [u8]),
}

/// 一端的队列，连同叫醒写那一端的铃。
#[derive(Clone, Default)]
pub(crate) struct Outbox {
    pub(crate) q: Arc<Mutex<Queue>>,
    pub(crate) bell: Arc<tokio::sync::Notify>,
}

impl Outbox {
    fn lock(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.q.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// 交回给托管进程、要对伪终端（或整个托管）做的事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Effect {
    /// 写进伪终端（持有者的键 · 终端询问的答）。
    Write(Vec<u8>),
    /// 改伪终端尺寸。
    Resize(Size),
    /// 结束：给子进程组 SIGHUP。
    End,
    /// 记下后端写来的会话身份（落 `facts.json`）。
    Tag { sid: String, agent: String },
    /// 记下里面那个 ccm 起 agent 的那一跳与它包没包一层（落 `facts.json` 的 `hop_pid` · `hop_wrapped`）。
    Hop { by: u32, pid: u32, wrapped: bool },
    /// 关掉这一端的连接（它说了 `bye`，或送来的帧不对）。
    Drop(u64),
    /// 有没有真终端接着变了（第一个接上 ⇒ `true` · 最后一个走了 ⇒ `false`；落 `facts.json`）。
    Attached(bool),
}

struct Client {
    who: Who,
    want: Want,
    size: Option<Size>,
    pid: Option<u32>,
    since_ms: i64,
    last_ms: i64,
    /// 连上的先后（越大越新）。
    seq: u64,
    /// 说过 `hello` 没有：没说之前只收 `hello`。
    greeted: bool,
    out: Outbox,
}

/// 一个托管终端的那一张账。
pub(crate) struct Hub {
    id: String,
    screen: Screen,
    clients: BTreeMap<u64, Client>,
    holder: Option<u64>,
    next: u64,
    ended: bool,
}

impl Hub {
    pub(crate) fn new(id: &str, size: Size) -> Self {
        Self {
            id: id.to_string(),
            screen: Screen::new(size.rows, size.cols),
            clients: BTreeMap::new(),
            holder: None,
            next: 1,
            ended: false,
        }
    }

    pub(crate) fn size(&self) -> Size {
        let (rows, cols) = self.screen.size();
        Size { rows, cols }
    }

    /// 一条连接进来了（还没说 `hello`）：交回它的连接号与队列。
    pub(crate) fn connect(&mut self, pid: Option<u32>, now_ms: i64) -> (u64, Outbox) {
        let id = self.next;
        self.next += 1;
        let out = Outbox::default();
        self.clients.insert(
            id,
            Client {
                who: Who {
                    kind: WhoKind::Backend,
                    client: None,
                },
                want: Want::View,
                size: None,
                pid,
                since_ms: now_ms,
                last_ms: now_ms,
                seq: id,
                greeted: false,
                out: out.clone(),
            },
        );
        (id, out)
    }

    /// 伪终端吐了一块：算进屏、接在每个看屏的端的队尾（[`Hub::watchers`]；一个都没有 ⇒ 只记屏）；交回要写回伪终端的终端询问答。
    pub(crate) fn pty_out(&mut self, bytes: &[u8]) -> Vec<Effect> {
        let answers = self.screen.feed(bytes);
        for id in self.watchers() {
            self.push_out(id, bytes);
        }
        if answers.is_empty() {
            Vec::new()
        } else {
            vec![Effect::Write(answers)]
        }
    }

    /// 子进程退了：每一端收 `ended`、关队列。
    pub(crate) fn child_exited(&mut self, code: Option<i32>) {
        self.ended = true;
        for id in self.greeted_ids() {
            self.push(id, Frame::Ctl(Ctl::Ended { code }));
        }
        for c in self.clients.values() {
            c.out.lock().closed = true;
            c.out.bell.notify_one();
        }
    }

    /// 一端走了（断线 · `bye` 之后关掉）。
    pub(crate) fn gone(&mut self, id: u64) -> Vec<Effect> {
        let Some(c) = self.clients.remove(&id) else {
            return Vec::new();
        };
        c.out.lock().closed = true;
        c.out.bell.notify_one();
        let mut effects = if self.holder == Some(id) {
            self.hand_over(None)
        } else {
            Vec::new()
        };
        if c.greeted && c.who.kind == WhoKind::Terminal && self.terminals() == 0 {
            effects.push(Effect::Attached(false));
        }
        effects
    }

    /// 此刻接着的真终端（说过 hello 的瘦客户端）几个。
    fn terminals(&self) -> usize {
        self.clients
            .values()
            .filter(|c| c.greeted && c.who.kind == WhoKind::Terminal)
            .count()
    }

    /// 一端送来一帧。
    pub(crate) fn frame(&mut self, id: u64, f: Frame, now_ms: i64) -> Vec<Effect> {
        let Some(c) = self.clients.get_mut(&id) else {
            return Vec::new();
        };
        c.last_ms = now_ms;
        if !c.greeted {
            return match f {
                Frame::Ctl(Ctl::Hello {
                    want, size, who, ..
                }) => self.hello(id, want, size, who),
                _ => vec![Effect::Drop(id)],
            };
        }
        match f {
            Frame::In(bytes) => {
                if self.holder == Some(id) {
                    vec![Effect::Write(bytes)]
                } else {
                    let holder = self.holder_for(id);
                    self.push(id, Frame::Ctl(Ctl::HeldElsewhere { holder }));
                    Vec::new()
                }
            }
            Frame::Out(_) => vec![Effect::Drop(id)],
            Frame::Ctl(ctl) => self.ctl(id, ctl),
        }
    }

    fn ctl(&mut self, id: u64, ctl: Ctl) -> Vec<Effect> {
        match ctl {
            Ctl::Take => {
                if self.holder == Some(id) {
                    return Vec::new();
                }
                self.hand_over(Some(id))
            }
            Ctl::Release => {
                if self.holder != Some(id) {
                    return Vec::new();
                }
                // 交还：别人里最近连上、要输入的真终端接；没有 ⇒ 空着。
                let next = self.fallback(Some(id));
                self.set_holder(next)
            }
            Ctl::Resize { rows, cols } => {
                let s = Size { rows, cols };
                if let Some(c) = self.clients.get_mut(&id) {
                    if c.who.kind == WhoKind::Terminal {
                        c.size = Some(s);
                    }
                }
                if self.holder == Some(id) {
                    self.resize_to(s)
                } else {
                    Vec::new()
                }
            }
            Ctl::Capture { back, color } => {
                let got = self.screen.capture(back, color);
                self.push(
                    id,
                    Frame::Ctl(Ctl::Captured {
                        lines: got.lines,
                        capped: got.capped,
                        size: Size {
                            rows: got.rows,
                            cols: got.cols,
                        },
                        cursor: [got.cursor.0, got.cursor.1],
                    }),
                );
                Vec::new()
            }
            Ctl::Clients => {
                let clients = self.rows_for(id);
                self.push(id, Frame::Ctl(Ctl::ClientsAre { clients }));
                Vec::new()
            }
            Ctl::Say { text } => self.say(id, text),
            Ctl::Interrupt => self.interrupt(id),
            Ctl::Tag { sid, agent } => vec![Effect::Tag { sid, agent }],
            Ctl::Hop { by, pid, wrapped } => vec![Effect::Hop { by, pid, wrapped }],
            Ctl::End => vec![Effect::End],
            Ctl::Bye => vec![Effect::Drop(id)],
            // 下行的动词从上行来 · 认不出的 · 第二次 `hello`：不理。
            _ => Vec::new(),
        }
    }

    /// 后端中断这一轮（A3-3-32-10）：写一个 ESC、回 `interrupted`；不看持有者、不看半句、不换持有者。真终端说它 ⇒ 断它（它有自己的 `in`）。
    fn interrupt(&mut self, id: u64) -> Vec<Effect> {
        if self.clients.get(&id).map(|c| c.who.kind) != Some(WhoKind::Backend) {
            return vec![Effect::Drop(id)];
        }
        self.push(id, Frame::Ctl(Ctl::Interrupted));
        vec![Effect::Write(b"\x1b".to_vec())]
    }

    /// 后端说一句：不看谁持有、不看输入行里有什么，照写、回 `said`。真终端说 `say` ⇒ 不是它该说的，断它。
    fn say(&mut self, id: u64, text: String) -> Vec<Effect> {
        if self.clients.get(&id).map(|c| c.who.kind) != Some(WhoKind::Backend) {
            return vec![Effect::Drop(id)];
        }
        self.push(id, Frame::Ctl(Ctl::Said));
        vec![Effect::Write(text.into_bytes())]
    }

    fn hello(&mut self, id: u64, want: Want, size: Option<Size>, who: Who) -> Vec<Effect> {
        let terminal = who.kind == WhoKind::Terminal;
        if let Some(c) = self.clients.get_mut(&id) {
            c.greeted = true;
            c.want = want;
            c.size = if terminal { size } else { None };
            c.who = who;
        }
        // 先按需换持有者，welcome 说换过之后的持有者与尺寸，排在这一端队列最前：客户端一收到 welcome 就画出整屏、
        // 也知道是不是自己持有，紧跟着敲的键照这个身份走（持有 ⇒ 送进去，不当成「回车接管」）。
        // 换持有者推给它的 `holder` · `size` 排在 welcome 后面，与它说的一致。
        let mut effects = if terminal && want == Want::Input && !self.ended {
            self.hand_over(Some(id))
        } else {
            Vec::new()
        };
        if terminal && self.terminals() == 1 {
            effects.push(Effect::Attached(true));
        }
        let welcome = super::wire::encode(&Frame::Ctl(Ctl::Welcome {
            proto: PROTO,
            id: self.id.clone(),
            size: self.size(),
            you: id,
            holder: self.holder_for(id),
            screen: String::from_utf8_lossy(&self.screen.full()).into_owned(),
        }));
        if let Some(c) = self.clients.get(&id) {
            let mut q = c.out.lock();
            q.bytes += welcome.len();
            q.frames.push_front(welcome);
            drop(q);
            c.out.bell.notify_one();
        }
        effects
    }

    /// 换持有者到 `to`（`None` ⇒ 照「最近连上的另一个要输入的真终端」挑）。
    fn hand_over(&mut self, to: Option<u64>) -> Vec<Effect> {
        let next = match to {
            Some(id) => Some(id),
            None => self.fallback(None),
        };
        self.set_holder(next)
    }

    fn set_holder(&mut self, next: Option<u64>) -> Vec<Effect> {
        if next == self.holder {
            return Vec::new();
        }
        self.holder = next;
        for id in self.greeted_ids() {
            let holder = self.holder_for(id);
            self.push(id, Frame::Ctl(Ctl::Holder { holder }));
        }
        // 尺寸跟着新持有者（它是真终端、报过尺寸才改）。
        match next.and_then(|h| self.clients.get(&h)).and_then(|c| c.size) {
            Some(s) => self.resize_to(s),
            None => Vec::new(),
        }
    }

    fn resize_to(&mut self, s: Size) -> Vec<Effect> {
        if s == self.size() || s.rows == 0 || s.cols == 0 {
            return Vec::new();
        }
        self.screen.resize(s.rows, s.cols);
        for id in self.greeted_ids() {
            self.push(
                id,
                Frame::Ctl(Ctl::Size {
                    rows: s.rows,
                    cols: s.cols,
                }),
            );
        }
        vec![Effect::Resize(s)]
    }

    /// 除了 `except`，最近连上、要输入的真终端。
    fn fallback(&self, except: Option<u64>) -> Option<u64> {
        self.clients
            .iter()
            .filter(|(id, c)| {
                Some(**id) != except
                    && c.greeted
                    && c.who.kind == WhoKind::Terminal
                    && c.want == Want::Input
            })
            .max_by_key(|(_, c)| c.seq)
            .map(|(id, _)| *id)
    }

    fn holder_for(&self, viewer: u64) -> Option<Holder> {
        let h = self.holder?;
        let c = self.clients.get(&h)?;
        Some(Holder {
            client: h,
            kind: c.who.kind,
            front: c.who.client.clone(),
            pid: c.pid,
            mine: h == viewer,
        })
    }

    fn rows_for(&self, viewer: u64) -> Vec<ClientRow> {
        self.clients
            .iter()
            .filter(|(_, c)| c.greeted)
            .map(|(id, c)| ClientRow {
                client: *id,
                kind: c.who.kind,
                front: c.who.client.clone(),
                pid: c.pid,
                since_ms: c.since_ms,
                last_ms: c.last_ms,
                holder: self.holder == Some(*id),
                mine: *id == viewer,
            })
            .collect()
    }

    /// 看屏的端：说过 `hello` 的各端，除去后端持有输入的那一条（它 `take` 之后只送键、只读持有者变了与结束；
    /// 看屏的前端另开一条只看的连接）。跟 `attached`（真终端接着）不是一回事：后端只看的连接也看屏。
    fn watchers(&self) -> Vec<u64> {
        self.clients
            .iter()
            .filter(|(id, c)| {
                c.greeted && !(c.who.kind == WhoKind::Backend && self.holder == Some(**id))
            })
            .map(|(id, _)| *id)
            .collect()
    }

    fn greeted_ids(&self) -> Vec<u64> {
        self.clients
            .iter()
            .filter(|(_, c)| c.greeted)
            .map(|(id, _)| *id)
            .collect()
    }

    /// 放一帧控制帧：先把队尾没编的那一段 `out` 编成帧（顺序照旧），再排它。
    fn push(&mut self, id: u64, f: Frame) {
        self.put(id, Item::Ctl(super::wire::encode(&f)));
    }

    /// 一段 `out` 接在一端的队尾（还不编帧）。
    fn push_out(&mut self, id: u64, raw: &[u8]) {
        self.put(id, Item::Out(raw));
    }

    /// 放进一端的队列；放不下 ⇒ 清空、换成一帧 `resync`（此刻的整屏，已含刚算进去的那一块 `out`）。
    /// 控制帧不被整屏顶掉：排在 `resync` 后面照发；`out` 已在整屏里，不再放。
    fn put(&mut self, id: u64, item: Item<'_>) {
        let Some(c) = self.clients.get(&id) else {
            return;
        };
        let out = c.out.clone();
        let mut q = out.lock();
        if q.closed {
            return;
        }
        let len = match &item {
            Item::Ctl(b) => b.len(),
            Item::Out(b) => b.len(),
        };
        if q.bytes + len > QUEUE_CAP.as_len() {
            q.clear();
            q.overflows = q.overflows.saturating_add(1);
            let resync = super::wire::encode(&Frame::Ctl(Ctl::Resync {
                size: self.size(),
                screen: String::from_utf8_lossy(&self.screen.full()).into_owned(),
            }));
            q.bytes = resync.len();
            q.frames.push_back(resync);
            if let Item::Ctl(b) = item {
                q.bytes += b.len();
                q.frames.push_back(b);
            }
        } else {
            match item {
                Item::Ctl(b) => {
                    q.seal();
                    q.bytes += b.len();
                    q.frames.push_back(b);
                }
                Item::Out(b) => {
                    q.bytes += b.len();
                    q.tail.extend_from_slice(b);
                }
            }
        }
        drop(q);
        out.bell.notify_one();
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/host/serve_tests.rs"]
mod tests;
