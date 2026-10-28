//! 托管口的那一张账：连着的各端 · 谁持有输入 · 伪终端尺寸 · 各端的发送队列（A3-3-2 · A3-3-14）。
//!
//! [`Hub`] 不碰套接字也不碰伪终端：收事件（伪终端吐了字节 · 一端送来一帧 · 一端走了），改账，往各端队列里放帧，
//! 交回要对伪终端做的事（[`Effect`]）。托管进程（`run.rs`）把它接到真的套接字与伪终端上；判据直接喂事件。
//!
//! 慢端：每端一条有界队列（[`QUEUE_CAP`] 字节）。放不下 ⇒ 清空队列，换成一帧 `resync`（此刻的整屏），之后的 `out` 接在它后面 ——
//! 读伪终端永远不等任何一端（同 watcher 丢帧后整份重报）。
//!
//! 持有者：最新连上、要输入的真终端持有；`take` 谁都能抢（桌面 · 手机经后端接管也走它）。`in` · `resize` 只认持有者，
//! 别人的 `in` 回 `held-elsewhere`、一个字节都不送。后端的 `say`（会话输入队投递那一句）不看持有者、照写、不换持有者；
//! 只在持有者输入行里有没交的字时不写（回 `said {pending}`，投递照排着）。持有者走了或交还 ⇒ 交给最近连上的另一个要输入的真终端，没有就空着。
//! 伪终端尺寸 ＝ 持有者报的尺寸；没人持有 ⇒ 保持上一次的；后端从不改尺寸。

use super::screen::Screen;
use super::wire::{ClientRow, Ctl, Frame, Holder, Size, Want, Who, WhoKind, PROTO};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

/// 一端的发送队列上限（字节）。比最大的一屏大得多，只有真不读的端才会塞满。
pub(crate) const QUEUE_CAP: usize = 4 * 1024 * 1024;

/// 一端的发送队列：托管那边放、写那一端的任务取。
#[derive(Default)]
pub(crate) struct Queue {
    frames: VecDeque<Vec<u8>>,
    bytes: usize,
    /// 放不下过几次（判据与诊断看）。
    pub(crate) overflows: u32,
    /// 收尾：之后不再放；取完就关这条连接。
    closed: bool,
}

impl Queue {
    /// 取走眼下排着的全部帧（已编好）；`closed` 且取空 ⇒ 该关了。
    pub(crate) fn drain(&mut self) -> (Vec<Vec<u8>>, bool) {
        let v: Vec<Vec<u8>> = self.frames.drain(..).collect();
        self.bytes = 0;
        (v, self.closed)
    }
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
    /// 关掉这一端的连接（它说了 `bye`，或送来的帧不对）。
    Drop(u64),
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
    /// 持有者自上次回车以来敲进、还没交的字数（[`typed_after`]）：不是 0 ⇒ 后端那一句（`say`）不写。换了持有者清零。
    typed: usize,
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
            typed: 0,
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

    /// 伪终端吐了一块：算进屏、发给每一端；交回要写回伪终端的终端询问答。
    pub(crate) fn pty_out(&mut self, bytes: &[u8]) -> Vec<Effect> {
        let answers = self.screen.feed(bytes);
        let frame = super::wire::encode(&Frame::Out(bytes.to_vec()));
        let ids: Vec<u64> = self.greeted_ids();
        for id in ids {
            self.push_encoded(id, frame.clone(), false);
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
        if self.holder == Some(id) {
            self.hand_over(None)
        } else {
            Vec::new()
        }
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
                    self.typed = typed_after(self.typed, &bytes);
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
            Ctl::Tag { sid, agent } => vec![Effect::Tag { sid, agent }],
            Ctl::End => vec![Effect::End],
            Ctl::Bye => vec![Effect::Drop(id)],
            // 下行的动词从上行来 · 认不出的 · 第二次 `hello`：不理。
            _ => Vec::new(),
        }
    }

    /// 后端说一句：不看谁持有，照写；持有者输入行里有没交的字 ⇒ 不写、回 `pending`。真终端说 `say` ⇒ 不是它该说的，断它。
    fn say(&mut self, id: u64, text: String) -> Vec<Effect> {
        if self.clients.get(&id).map(|c| c.who.kind) != Some(WhoKind::Backend) {
            return vec![Effect::Drop(id)];
        }
        let pending = self.typed > 0;
        self.push(id, Frame::Ctl(Ctl::Said { pending }));
        if pending {
            Vec::new()
        } else {
            vec![Effect::Write(text.into_bytes())]
        }
    }

    fn hello(&mut self, id: u64, want: Want, size: Option<Size>, who: Who) -> Vec<Effect> {
        let terminal = who.kind == WhoKind::Terminal;
        if let Some(c) = self.clients.get_mut(&id) {
            c.greeted = true;
            c.want = want;
            c.size = if terminal { size } else { None };
            c.who = who;
        }
        // 先发 welcome（整屏 ＋ 此刻的持有者），再按需换持有者：换了会接着推 `holder` · `size`。
        let holder = self.holder_for(id);
        let welcome = Ctl::Welcome {
            proto: PROTO,
            id: self.id.clone(),
            size: self.size(),
            you: id,
            holder,
            screen: String::from_utf8_lossy(&self.screen.full()).into_owned(),
        };
        self.push(id, Frame::Ctl(welcome));
        if terminal && want == Want::Input && !self.ended {
            self.hand_over(Some(id))
        } else {
            Vec::new()
        }
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
        self.typed = 0;
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

    fn greeted_ids(&self) -> Vec<u64> {
        self.clients
            .iter()
            .filter(|(_, c)| c.greeted)
            .map(|(id, _)| *id)
            .collect()
    }

    fn push(&mut self, id: u64, f: Frame) {
        self.push_encoded(id, super::wire::encode(&f), true);
    }

    /// 放进一端的队列；放不下 ⇒ 清空、换成一帧 `resync`（此刻的整屏，已含刚算进去的那一块 `out`）。
    /// 控制帧（`keep`）不被整屏顶掉：排在 `resync` 后面照发。
    fn push_encoded(&mut self, id: u64, bytes: Vec<u8>, keep: bool) {
        let Some(c) = self.clients.get(&id) else {
            return;
        };
        let out = c.out.clone();
        let mut q = out.lock();
        if q.closed {
            return;
        }
        if q.bytes + bytes.len() > QUEUE_CAP {
            q.frames.clear();
            q.bytes = 0;
            q.overflows = q.overflows.saturating_add(1);
            let resync = super::wire::encode(&Frame::Ctl(Ctl::Resync {
                size: self.size(),
                screen: String::from_utf8_lossy(&self.screen.full()).into_owned(),
            }));
            q.bytes = resync.len();
            q.frames.push_back(resync);
            if keep {
                q.bytes += bytes.len();
                q.frames.push_back(bytes);
            }
        } else {
            q.bytes += bytes.len();
            q.frames.push_back(bytes);
        }
        drop(q);
        out.bell.notify_one();
    }
}

/// 持有者送进 `bytes` 之后，输入行里还没交的字数（`before` 是送之前的）。
/// 回车 · 换行 · Ctrl-C · Ctrl-U ⇒ 清零；退格（DEL · BS）⇒ 减一；转义序列（方向键 · 焦点 · 鼠标）不算；
/// 别的可见字符（UTF-8 的续字节不另算）⇒ 加一。光标挪到行中间再删这类照字数近似。
pub(crate) fn typed_after(before: usize, bytes: &[u8]) -> usize {
    let mut n = before;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        i += 1;
        match b {
            b'\r' | b'\n' | 0x03 | 0x15 => n = 0,
            0x7f | 0x08 => n = n.saturating_sub(1),
            0x1b => match bytes.get(i) {
                // CSI：到终止字节（0x40..=0x7e）为止；括号粘贴那一对也是 CSI，粘的正文照算字。
                Some(b'[') => {
                    i += 1;
                    while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                        i += 1;
                    }
                    i += 1;
                }
                // SS3（`ESC O x`）：连 ESC 三个字节。
                Some(b'O') => i += 2,
                Some(_) => i += 1,
                None => {}
            },
            0x80..=0xbf => {}
            0x20.. => n += 1,
            _ => {}
        }
    }
    n
}

#[cfg(test)]
#[path = "../../../../tests/backend/host/serve_tests.rs"]
mod tests;
