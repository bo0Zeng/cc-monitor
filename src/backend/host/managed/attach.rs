//! 瘦客户端：用户真终端里的 `ccm --attach <名|句柄>`（A3-3-3）。
//!
//! 连托管口、`hello{want: input}`（最新连上的真终端持有输入）；进外层终端的备用屏、退出还原。
//! 本地一个 `vt100` 照托管伪终端的尺寸跑：`welcome` / `resync` 整屏重来，`out` 接着算，只把变了的画出去（`contents_diff`），
//! 拍子跟着 `out` 走，不用定时器。不持有时外层标题前加「[只看] 」，敲的键不送，底行提示「输入在 … · 回车接管」
//! （这个回车不送进去），下一个键收掉提示。外层窗口改尺寸 ⇒ 发 `resize`（托管只认持有者的）。
//! 关真终端窗口 · ssh 断 ⇒ 只算断开，会话照跑。退出码：断开 0，会话结束带子进程的码。
//!
//! 外层终端（键 · 尺寸 · 画）由 [`Outer`] 交进来：生产是本进程的 stdin / stdout（[`stdio_outer`]），判据交一个假的。
//!
//! 尺寸（A3-3-14）：伪终端的尺寸跟持有者；外层比它小（只看的一端 · 持有者刚改了尺寸、托管还没跟上）⇒ 只画左上那一块，
//! 不缩放、不折行；外层大 ⇒ 多出来的空着。只看的一端标题前缀带伪终端的尺寸。
//! 终端模式（A3-3-15）：外层的输入模式由这里自己管（[`Modes`]），持有者照里面开的开；只看的不开鼠标 · 括号粘贴 · 焦点。
//! 里面开着同步输出 `?2026` ⇒ 屏照算、先不画，关上那一下一次画完。

use super::keys::{self, Act, Copy, CopyKeys, Keys};
use super::screen::Extra;
use super::wire::{self, ClientRow, Ctl, Frame, Holder, Size, Want, Who, WhoKind, PROTO};
use copy_core::copy_text;
use copy_core::said::Said;
use std::io::Write;
use tokio::sync::mpsc::UnboundedReceiver;
use vt100::{MouseProtocolEncoding, MouseProtocolMode};

/// 进 / 出外层备用屏，出来时把里面可能开着的几样模式关掉（括号粘贴 · 光标键 · 小键盘 · 鼠标上报 · 焦点 · 光标形状），光标显出来。
const ENTER_OUTER: &[u8] = b"\x1b[?1049h\x1b[H\x1b[2J";
const LEAVE_OUTER: &[u8] =
    b"\x1b[?2004l\x1b[?1l\x1b>\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1004l\x1b[0 q\x1b[?25h\x1b[m\x1b[?1049l";

/// 外层终端此刻开着的输入模式（瘦客户端自己记：照「里面开了什么 × 是不是持有者」补齐 / 关掉）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Modes {
    keypad: bool,
    cursor_keys: bool,
    paste: bool,
    mouse: MouseProtocolMode,
    encoding: MouseProtocolEncoding,
    focus: bool,
    shape: u16,
}

impl Modes {
    /// 从 `prev` 变到这一份要写给外层的字节。
    fn diff(&self, prev: &Modes) -> Vec<u8> {
        let mut b = Vec::new();
        let flag = |b: &mut Vec<u8>, n: u16, on: bool, was: bool| {
            if on != was {
                b.extend_from_slice(format!("\x1b[?{n}{}", if on { 'h' } else { 'l' }).as_bytes());
            }
        };
        if self.keypad != prev.keypad {
            b.extend_from_slice(if self.keypad { b"\x1b=" } else { b"\x1b>" });
        }
        flag(&mut b, 1, self.cursor_keys, prev.cursor_keys);
        flag(&mut b, 2004, self.paste, prev.paste);
        flag(&mut b, 1004, self.focus, prev.focus);
        if self.mouse != prev.mouse {
            b.extend_from_slice(b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l");
            let on = match self.mouse {
                MouseProtocolMode::None => None,
                MouseProtocolMode::Press => Some(9),
                MouseProtocolMode::PressRelease => Some(1000),
                MouseProtocolMode::ButtonMotion => Some(1002),
                MouseProtocolMode::AnyMotion => Some(1003),
            };
            if let Some(n) = on {
                b.extend_from_slice(format!("\x1b[?{n}h").as_bytes());
            }
        }
        if self.encoding != prev.encoding {
            b.extend_from_slice(b"\x1b[?1005l\x1b[?1006l");
            match self.encoding {
                MouseProtocolEncoding::Default => {}
                MouseProtocolEncoding::Utf8 => b.extend_from_slice(b"\x1b[?1005h"),
                MouseProtocolEncoding::Sgr => b.extend_from_slice(b"\x1b[?1006h"),
            }
        }
        if self.shape != prev.shape {
            b.extend_from_slice(format!("\x1b[{} q", self.shape).as_bytes());
        }
        b
    }
}

/// 外层终端：读到的键（一块一块）· 尺寸变了的信号 · 此刻的尺寸 · 往外画的那一头。
pub(crate) struct Outer {
    pub(crate) keys: UnboundedReceiver<Vec<u8>>,
    pub(crate) resized: UnboundedReceiver<()>,
    pub(crate) size: Box<dyn Fn() -> Option<Size> + Send>,
    pub(crate) out: Box<dyn Write + Send>,
    pub(crate) prefix: u8,
    pub(crate) vi: bool,
    /// 外层是 tmux（环境里有 `TMUX`）：接上那一刻提示一次前缀按两次。
    pub(crate) in_tmux: bool,
}

/// 一趟接上怎么收的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Left {
    /// 断开（`前缀 d`），会话照跑。
    Detached,
    /// 会话结束了，带子进程的码。
    Ended(Option<i32>),
    /// 托管那边断了（没说 `ended`）。
    Lost,
    /// `前缀 s` 选了另一个托管终端：接它（交句柄）。
    Switch(String),
    /// 外层终端没了（stdin 读到头：关窗 · ssh 断）。
    OuterGone,
}

/// 底行 / 整屏盖在上面的那一层。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Over {
    None,
    /// 不持有时的提示行。
    Hint,
    /// 「结束？(y/n)」。
    Confirm,
    /// 一张单子（列键 · 列各端），下一个键收掉。
    Sheet,
    /// 选一个托管终端（`前缀 s`）。
    Choose(Vec<(String, String)>),
    /// 翻看。
    Copy(CopyView),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CopyView {
    lines: Vec<String>,
    /// 视口最底一行离最末一行有几行（0 ＝ 最新一屏）。
    up: usize,
    found: Option<(String, bool)>,
}

/// parser 碰到的、屏本身不管的：标题 · 铃 · `vt100` 不记的那几样模式（焦点 · 同步输出 · 光标形状）。
#[derive(Default)]
struct Hooks {
    title: Option<Vec<u8>>,
    bells: u32,
    extra: Extra,
}

impl vt100::Callbacks for Hooks {
    fn audible_bell(&mut self, _: &mut vt100::Screen) {
        self.bells += 1;
    }
    fn set_window_title(&mut self, _: &mut vt100::Screen, t: &[u8]) {
        self.title = Some(t.to_vec());
    }
    fn unhandled_csi(
        &mut self,
        _: &mut vt100::Screen,
        i1: Option<u8>,
        _: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        self.extra.csi(i1, params, c);
    }
}

struct Client {
    outer: Outer,
    parser: vt100::Parser<Hooks>,
    /// 外层此刻的尺寸（问不出 ⇒ 当它与伪终端一样大）。
    view: Option<Size>,
    /// 上次画到外层的那一屏与当时外层的尺寸（`None` ⇒ 下一次整屏画）。
    drawn: Option<(vt100::Screen, Size)>,
    /// 外层此刻开着的输入模式。
    modes: Modes,
    you: u64,
    holder: Option<Holder>,
    over: Over,
    keys: Keys,
    copy: CopyKeys,
    title: Vec<u8>,
    /// 发了 `take`、持有者还没换过来。
    taking: bool,
    /// 那段时间里敲的键（换过来就送；没换过来 ⇒ 丢）。
    typed_while_taking: Vec<u8>,
    /// 底行「排队 N 条」：会话那台写好的那一句（`history::session_queue::line_of_host`），照摆；没有 ⇒ 不盖。
    queued: Option<String>,
}

/// 接一个托管终端，直到断开 / 会话结束 / 切到另一个。外层终端的进出由调用方（[`stdio_outer`] 那一侧）管。
/// `queued`：底行那一句（[`QueueEars`] 说来的），来一句照摆一句。
pub(crate) async fn session(
    sock: &std::path::Path,
    mut outer: Outer,
    list: &(dyn Fn() -> Vec<(String, String)> + Sync),
    mut queued: UnboundedReceiver<Option<String>>,
) -> Result<Left, Said> {
    let stream = own_chan::connect(sock).await.map_err(|e| {
        Said::with_raw(
            copy_text(
                "beHost.attach.connectFailed",
                &[("path", &sock.display().to_string())],
            ),
            &e,
        )
    })?;
    let (mut rd, mut wr) = tokio::io::split(stream);
    let size = (outer.size)();
    wire::write_frame(
        &mut wr,
        &Frame::Ctl(Ctl::Hello {
            proto: PROTO,
            build: crate::BUILD_ID.to_string(),
            want: Want::Input,
            size,
            who: Who {
                kind: WhoKind::Terminal,
                client: None,
            },
        }),
    )
    .await
    .map_err(|e| Said::with_raw(copy_text("beHost.attach.lost", &[]), &e))?;
    put(&mut *outer.out, ENTER_OUTER);
    let in_tmux = outer.in_tmux;
    let mut c = Client::new(outer);
    let (ftx, mut frx) = tokio::sync::mpsc::unbounded_channel::<Option<Frame>>();
    let reader = tokio::spawn(async move {
        loop {
            match wire::read_frame(&mut rd).await {
                Ok(Some(f)) => {
                    if ftx.send(Some(f)).is_err() {
                        return;
                    }
                }
                Ok(None) | Err(_) => {
                    if ftx.send(None).is_err() {
                        tracing::debug!("瘦客户端：托管口断了，这一趟已经在收尾");
                    }
                    return;
                }
            }
        }
    });
    let mut told_tmux = !in_tmux;
    let left = loop {
        tokio::select! {
            f = frx.recv() => {
                let Some(Some(f)) = f else { break Left::Lost };
                if let Some(l) = c.frame(f) {
                    break l;
                }
                // 接管那一下到 `holder` 回来之间敲的键：回来了就送。
                if c.holding() && !c.typed_while_taking.is_empty() {
                    let b = std::mem::take(&mut c.typed_while_taking);
                    send(&mut wr, &Frame::In(b)).await;
                }
                if !told_tmux && c.you != 0 {
                    told_tmux = true;
                    c.bottom_line(&copy_text("beHost.attach.inTmux", &[]));
                }
            }
            k = c.outer.keys.recv() => {
                let Some(k) = k else {
                    break Left::OuterGone;
                };
                let (sends, left) = c.key(&k, list);
                for f in sends {
                    if !send(&mut wr, &f).await {
                        break;
                    }
                }
                if let Some(l) = left {
                    break l;
                }
            }
            Some(line) = queued.recv() => c.queued_now(line),
            r = c.outer.resized.recv() => {
                if r.is_some() {
                    // 托管只认持有者的；只看的也报（接管那一下托管照它改），自己这边照新尺寸重裁。
                    if let Some(s) = c.outer_resized() {
                        send(&mut wr, &Frame::Ctl(Ctl::Resize { rows: s.rows, cols: s.cols })).await;
                    }
                }
            }
        }
    };
    if matches!(left, Left::Detached | Left::Switch(_) | Left::OuterGone) {
        send(&mut wr, &Frame::Ctl(Ctl::Bye)).await;
    }
    reader.abort();
    put(&mut *c.outer.out, LEAVE_OUTER);
    Ok(left)
}

/// 往外层终端写一段并冲出去；写不进（终端没了）⇒ 记一行，这一趟由读键那一侧读到头收。
fn put(out: &mut dyn Write, b: &[u8]) {
    if let Err(e) = out.write_all(b).and_then(|()| out.flush()) {
        tracing::debug!("瘦客户端：外层终端写不进：{e}");
    }
}

/// 往托管口发一帧；发不出去（托管那边断了）⇒ 记一行、交 `false`，读那一侧会收到断开。
async fn send<W: tokio::io::AsyncWrite + Unpin>(wr: &mut W, f: &Frame) -> bool {
    match wire::write_frame(wr, f).await {
        Ok(()) => true,
        Err(e) => {
            tracing::debug!("瘦客户端：托管口写不进：{e}");
            false
        }
    }
}

impl Client {
    fn new(outer: Outer) -> Self {
        let (prefix, vi) = (outer.prefix, outer.vi);
        let view = (outer.size)();
        Client {
            outer,
            parser: vt100::Parser::new_with_callbacks(24, 80, 0, Hooks::default()),
            view,
            drawn: None,
            modes: Modes::default(),
            you: 0,
            holder: None,
            over: Over::None,
            keys: Keys::new(prefix),
            copy: CopyKeys::new(vi),
            title: Vec::new(),
            taking: false,
            typed_while_taking: Vec::new(),
            queued: None,
        }
    }

    /// 底行那一句换了：没别的盖着 ⇒ 盖上新的；清了 ⇒ 整屏重画（那一行照里面）。盖着的那一层收掉时照此刻的画。
    fn queued_now(&mut self, line: Option<String>) {
        if line == self.queued {
            return;
        }
        self.queued = line;
        if self.over != Over::None {
            return;
        }
        if self.queued.is_some() {
            self.show_queued();
        } else {
            self.redraw();
        }
    }

    /// 没别的盖着、有那一句 ⇒ 盖在最底一行。
    fn show_queued(&mut self) {
        if self.over == Over::None {
            if let Some(l) = self.queued.clone() {
                self.bottom_line(&l);
            }
        }
    }

    /// 盖在上面的那几层（底行 · 单子 · 翻看）按外层的行数摆。
    fn rows(&self) -> u16 {
        self.view.map_or(self.parser.screen().size().0, |v| v.rows)
    }

    /// 外层改了尺寸：记下、照新尺寸重画；交回新尺寸（报给托管）。
    fn outer_resized(&mut self) -> Option<Size> {
        let s = (self.outer.size)()?;
        self.view = Some(s);
        if matches!(self.over, Over::None | Over::Hint) {
            self.redraw();
        }
        Some(s)
    }

    /// 外层该开的输入模式：持有者照里面开的；只看的不开鼠标 · 括号粘贴 · 焦点（敲的不送，开了只会吞掉外层原生的选字）。
    fn wanted_modes(&self) -> Modes {
        let s = self.parser.screen();
        let x = &self.parser.callbacks().extra;
        let hold = self.holding();
        Modes {
            keypad: s.application_keypad(),
            cursor_keys: s.application_cursor(),
            paste: hold && s.bracketed_paste(),
            mouse: if hold {
                s.mouse_protocol_mode()
            } else {
                MouseProtocolMode::None
            },
            encoding: if hold {
                s.mouse_protocol_encoding()
            } else {
                MouseProtocolEncoding::Default
            },
            focus: hold && x.focus,
            shape: x.shape,
        }
    }

    fn sync_modes(&mut self) {
        let want = self.wanted_modes();
        if want != self.modes {
            let b = want.diff(&self.modes);
            self.write(&b);
            self.modes = want;
        }
    }

    /// 把本地屏画到外层：`full` 或外层 / 伪终端尺寸变了 ⇒ 清屏整屏画，否则照上次画出去的那一屏 diff。
    /// 外层装得下 ⇒ 整屏的字节；装不下 ⇒ 只画左上那一块（每行先定位、不折行），光标落在外面就藏起来。
    /// 里面开着同步输出 ⇒ 这一回不画（下一回整屏画）。交回画了没有。
    fn paint(&mut self, full: bool) -> bool {
        if self.parser.callbacks().extra.sync {
            if full {
                self.drawn = None;
            }
            return false;
        }
        let now = self.parser.screen().clone();
        let (prow, pcol) = now.size();
        let view = self.view.unwrap_or(Size {
            rows: prow,
            cols: pcol,
        });
        let prev = match (&self.drawn, full) {
            (Some((s, v)), false) if s.size() == now.size() && *v == view => Some(s),
            _ => None,
        };
        let mut b = Vec::new();
        if prev.is_none() {
            b.extend_from_slice(b"\x1b[m\x1b[H\x1b[2J");
        }
        if view.rows >= prow && view.cols >= pcol {
            match prev {
                Some(p) => b.extend_from_slice(&now.contents_diff(p)),
                None => b.extend_from_slice(&now.contents_formatted()),
            }
        } else {
            let w = view.cols.min(pcol);
            let rows: Vec<Vec<u8>> = match prev {
                Some(p) => now.rows_diff(p, 0, w).collect(),
                None => now.rows_formatted(0, w).collect(),
            };
            for (i, r) in rows.iter().enumerate().take(usize::from(view.rows)) {
                if !r.is_empty() {
                    b.extend_from_slice(format!("\x1b[{};1H\x1b[m", i + 1).as_bytes());
                    b.extend_from_slice(r);
                }
            }
            b.extend_from_slice(b"\x1b[m");
            let (cr, cc) = now.cursor_position();
            if !now.hide_cursor() && cr < view.rows && cc < view.cols {
                b.extend_from_slice(format!("\x1b[{};{}H\x1b[?25h", cr + 1, cc + 1).as_bytes());
            } else {
                b.extend_from_slice(b"\x1b[?25l");
            }
        }
        self.write(&b);
        self.drawn = Some((now, view));
        self.sync_modes();
        true
    }

    fn holding(&self) -> bool {
        self.holder.as_ref().is_some_and(|h| h.mine)
    }

    fn write(&mut self, b: &[u8]) {
        if let Err(e) = self.outer.out.write_all(b) {
            tracing::debug!("瘦客户端：外层终端写不进：{e}");
        }
    }

    fn flush(&mut self) {
        if let Err(e) = self.outer.out.flush() {
            tracing::debug!("瘦客户端：外层终端冲不出去：{e}");
        }
    }

    /// 整屏重画（当屏 ＋ 标题；盖着的那一层不画）。
    fn redraw(&mut self) {
        self.paint(true);
        self.retitle(true);
        self.show_queued();
        self.flush();
    }

    fn retitle(&mut self, force: bool) {
        let inner = self.parser.callbacks().title.clone().unwrap_or_default();
        let mut t = Vec::new();
        if !self.holding() {
            // 「[只看 120×40] 」：伪终端的尺寸（跟持有者），外层小于它时看到的是左上那一块。
            let (rows, cols) = self.parser.screen().size();
            let size = format!("{cols}×{rows}");
            t.extend_from_slice(copy_text("beHost.attach.viewOnly", &[("size", &size)]).as_bytes());
        }
        t.extend_from_slice(&inner);
        if force || t != self.title {
            self.title = t.clone();
            let mut seq = b"\x1b]2;".to_vec();
            seq.extend_from_slice(&t);
            seq.push(0x07);
            self.write(&seq);
        }
    }

    /// 最底一行盖一句（反显），光标放回原处。
    fn bottom_line(&mut self, text: &str) {
        let rows = self.rows();
        let seq = format!("\x1b7\x1b[{rows};1H\x1b[7m{text}\x1b[K\x1b[m\x1b8");
        self.write(seq.as_bytes());
        self.flush();
    }

    /// 整屏盖一张单子（翻看 · 列键 · 列各端）。
    fn sheet(&mut self, lines: &[String]) {
        self.write(b"\x1b[?25l\x1b[H\x1b[2J");
        let rows = usize::from(self.rows());
        for (i, l) in lines.iter().take(rows).enumerate() {
            let seq = format!("\x1b[{};1H{l}\x1b[m", i + 1);
            self.write(seq.as_bytes());
        }
        self.flush();
    }

    fn take_over(&mut self) {
        if self.over != Over::None {
            self.over = Over::None;
            self.redraw();
        }
    }

    fn holder_said(&self) -> String {
        let who = match &self.holder {
            None => copy_text("beHost.attach.whoNobody", &[]),
            Some(h) => match (h.kind, h.front.as_deref()) {
                (WhoKind::Terminal, _) => copy_text("beHost.attach.whoTerminal", &[]),
                (WhoKind::Backend, Some("mobile")) => copy_text("beHost.attach.whoMobile", &[]),
                (WhoKind::Backend, _) => copy_text("beHost.attach.whoDesktop", &[]),
            },
        };
        copy_text("beHost.attach.heldBy", &[("who", &who)])
    }

    /// 托管那边来了一帧；交回 `Some` ⇒ 这一趟收了。
    fn frame(&mut self, f: Frame) -> Option<Left> {
        match f {
            Frame::Out(b) => {
                let bells = self.parser.callbacks().bells;
                self.parser.process(&b);
                if matches!(self.over, Over::None | Over::Hint) {
                    let painted = self.paint(false);
                    if self.parser.callbacks().bells > bells {
                        self.write(b"\x07");
                    }
                    self.retitle(false);
                    if painted && self.over == Over::Hint {
                        let said = self.holder_said();
                        self.bottom_line(&said);
                    } else if painted {
                        self.show_queued();
                    }
                    self.flush();
                }
            }
            Frame::Ctl(ctl) => return self.ctl(ctl),
            Frame::In(_) => {}
        }
        None
    }

    fn ctl(&mut self, ctl: Ctl) -> Option<Left> {
        match ctl {
            Ctl::Welcome {
                size,
                you,
                holder,
                screen,
                ..
            } => {
                self.you = you;
                self.holder = holder;
                self.reset(size, &screen);
            }
            Ctl::Resync { size, screen } => self.reset(size, &screen),
            Ctl::Size { rows, cols } => {
                self.parser.screen_mut().set_size(rows, cols);
                if matches!(self.over, Over::None | Over::Hint) {
                    self.redraw();
                }
            }
            Ctl::Holder { holder } => {
                self.holder = holder;
                if self.holding() {
                    self.taking = false;
                }
                if self.holding() && self.over == Over::Hint {
                    self.over = Over::None;
                    self.redraw();
                } else {
                    self.retitle(false);
                }
                // 持有者换了 ⇒ 外层的模式照新身份补齐 / 关掉。
                self.sync_modes();
                self.flush();
            }
            Ctl::HeldElsewhere { holder } => {
                self.holder = holder;
                self.sync_modes();
                if self.over == Over::None {
                    self.over = Over::Hint;
                    let said = self.holder_said();
                    self.bottom_line(&said);
                }
            }
            Ctl::Captured { lines, .. } => {
                let up = match &self.over {
                    Over::Copy(v) => v.up,
                    _ => 0,
                };
                let mut v = CopyView {
                    lines,
                    up,
                    found: None,
                };
                if up == usize::MAX {
                    v.up = usize::from(self.rows().saturating_sub(1));
                }
                self.over = Over::Copy(v);
                self.draw_copy();
            }
            Ctl::ClientsAre { clients } => {
                let lines = self.client_lines(&clients);
                self.over = Over::Sheet;
                self.sheet(&lines);
            }
            Ctl::Ended { code } => return Some(Left::Ended(code)),
            _ => {}
        }
        None
    }

    fn reset(&mut self, size: Size, screen: &str) {
        let title = self.parser.callbacks().title.clone();
        self.parser = vt100::Parser::new_with_callbacks(
            size.rows,
            size.cols,
            0,
            Hooks {
                title,
                ..Hooks::default()
            },
        );
        self.parser.process(screen.as_bytes());
        self.drawn = None;
        if matches!(self.over, Over::None | Over::Hint) {
            self.redraw();
        }
    }

    fn client_lines(&self, clients: &[ClientRow]) -> Vec<String> {
        clients
            .iter()
            .map(|r| {
                let kind = match (r.kind, r.front.as_deref()) {
                    (WhoKind::Terminal, _) => copy_text("beHost.attach.whoTerminal", &[]),
                    (WhoKind::Backend, Some("mobile")) => copy_text("beHost.attach.whoMobile", &[]),
                    (WhoKind::Backend, _) => copy_text("beHost.attach.whoDesktop", &[]),
                };
                let pid = r.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into());
                let mark = match (r.holder, r.mine) {
                    (true, true) => "* <",
                    (true, false) => "*",
                    (false, true) => "  <",
                    _ => "",
                };
                format!("{:>3}  {kind}  pid {pid}  {mark}", r.client)
            })
            .collect()
    }

    fn draw_copy(&mut self) {
        let Over::Copy(v) = &self.over else { return };
        let rows = usize::from(self.rows());
        let total = v.lines.len();
        let max_up = total.saturating_sub(rows);
        let up = v.up.min(max_up);
        let end = total - up;
        let start = end.saturating_sub(rows);
        let mut lines: Vec<String> = v.lines[start..end].to_vec();
        let badge = format!("\x1b[7m[{up}/{max_up}]\x1b[m");
        if let Some(first) = lines.first_mut() {
            first.push_str("\x1b[m\x1b[K  ");
            first.push_str(&badge);
        } else {
            lines.push(badge);
        }
        if let Some(p) = self.copy.prompt() {
            let p = p.to_string();
            if lines.len() >= rows {
                lines.pop();
            }
            while lines.len() + 1 < rows {
                lines.push(String::new());
            }
            lines.push(format!("\x1b[7m{p}\x1b[K"));
        }
        self.sheet(&lines);
    }

    fn copy_key(&mut self, k: &[u8]) {
        let acts = self.copy.feed(k);
        let rows = usize::from(self.rows());
        let mut quit = false;
        if let Over::Copy(v) = &mut self.over {
            let max_up = v.lines.len().saturating_sub(rows);
            for a in acts {
                match a {
                    Copy::Up => v.up = (v.up + 1).min(max_up),
                    Copy::Down => v.up = v.up.saturating_sub(1),
                    Copy::PageUp => v.up = (v.up + rows.saturating_sub(1)).min(max_up),
                    Copy::PageDown => v.up = v.up.saturating_sub(rows.saturating_sub(1)),
                    Copy::Top => v.up = max_up,
                    Copy::Bottom => v.up = 0,
                    Copy::Quit => quit = true,
                    Copy::Search { text, forward } => {
                        v.found = Some((text, forward));
                        search(v, rows, false);
                    }
                    Copy::Next => search(v, rows, false),
                    Copy::Prev => search(v, rows, true),
                }
            }
        }
        if quit {
            self.over = Over::None;
            self.redraw();
        } else {
            self.draw_copy();
        }
    }

    /// 外层敲了一块键：交回要发给托管的帧与（收了的话）怎么收的。
    fn key(
        &mut self,
        k: &[u8],
        list: &(dyn Fn() -> Vec<(String, String)> + Sync),
    ) -> (Vec<Frame>, Option<Left>) {
        // 盖着的那几层先吃键。
        match &self.over {
            Over::Copy(_) => {
                self.copy_key(k);
                return (Vec::new(), None);
            }
            Over::Sheet => {
                self.take_over();
                return (Vec::new(), None);
            }
            Over::Choose(items) => {
                let items = items.clone();
                self.take_over();
                let pick = k
                    .first()
                    .and_then(|b| (*b as char).to_digit(10))
                    .map(|d| d as usize);
                if let Some((id, _)) = pick.and_then(|d| items.get(d)) {
                    return (Vec::new(), Some(Left::Switch(id.clone())));
                }
                return (Vec::new(), None);
            }
            _ => {}
        }
        let mut sends = Vec::new();
        for a in self.keys.feed(k) {
            match a {
                Act::Send(b) => {
                    if self.holding() {
                        sends.push(Frame::In(b));
                    } else if self.taking {
                        self.typed_while_taking.extend_from_slice(&b);
                    } else if self.over == Over::Hint && b.contains(&b'\r') {
                        // 回车接管：这个回车不送进去；回车后面跟着的那一段等换过来再送。
                        self.over = Over::None;
                        self.taking = true;
                        if let Some(at) = b.iter().position(|&c| c == b'\r') {
                            self.typed_while_taking.extend_from_slice(&b[at + 1..]);
                        }
                        sends.push(Frame::Ctl(Ctl::Take));
                        self.redraw();
                    } else if self.over == Over::Hint {
                        self.take_over();
                    } else {
                        self.over = Over::Hint;
                        let said = self.holder_said();
                        self.bottom_line(&said);
                    }
                }
                Act::Detach => return (sends, Some(Left::Detached)),
                Act::ListClients => sends.push(Frame::Ctl(Ctl::Clients)),
                Act::Redraw => {
                    self.over = Over::None;
                    self.redraw();
                }
                Act::ListKeys => {
                    let lines: Vec<String> = keys::list_keys(self.keys_prefix())
                        .split("\r\n")
                        .map(str::to_string)
                        .collect();
                    self.over = Over::Sheet;
                    self.sheet(&lines);
                }
                Act::CopyMode | Act::CopyModePageUp => {
                    self.over = Over::Copy(CopyView {
                        lines: Vec::new(),
                        up: if a == Act::CopyModePageUp {
                            usize::MAX
                        } else {
                            0
                        },
                        found: None,
                    });
                    sends.push(Frame::Ctl(Ctl::Capture {
                        back: super::screen::SCROLLBACK,
                        color: true,
                    }));
                }
                Act::AskEnd => {
                    self.over = Over::Confirm;
                    self.bottom_line(&copy_text("beHost.attach.confirmEnd", &[]));
                }
                Act::End => {
                    self.take_over();
                    sends.push(Frame::Ctl(Ctl::End));
                }
                Act::CancelEnd => self.take_over(),
                Act::ChooseTerminal => {
                    let items: Vec<(String, String)> = list().into_iter().take(10).collect();
                    let lines: Vec<String> = items
                        .iter()
                        .enumerate()
                        .map(|(i, (_, name))| format!("({i})  {name}"))
                        .collect();
                    self.over = Over::Choose(items);
                    self.sheet(&lines);
                }
            }
        }
        (sends, None)
    }

    fn keys_prefix(&self) -> u8 {
        self.outer.prefix
    }
}

/// 翻看里搜：从视口往老（或往新）找第一行含那段字的，把它摆到视口最底那一行。
fn search(v: &mut CopyView, rows: usize, reverse: bool) {
    let Some((text, forward)) = v.found.clone() else {
        return;
    };
    let older = forward != reverse;
    let total = v.lines.len();
    let max_up = total.saturating_sub(rows);
    let bottom = total.saturating_sub(1 + v.up);
    let plain = |l: &str| strip_sgr(l).contains(&text);
    let hit = if older {
        (0..bottom).rev().find(|&i| plain(&v.lines[i]))
    } else {
        (bottom + 1..total).find(|&i| plain(&v.lines[i]))
    };
    if let Some(i) = hit {
        v.up = (total - 1 - i).min(max_up);
    }
}

fn strip_sgr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find("\x1b[") {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 2..];
        let end = tail
            .char_indices()
            .find(|(_, d)| ('\x40'..='\x7e').contains(d))
            .map_or(tail.len(), |(i, d)| i + d.len_utf8());
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// 本进程的 stdin / stdout 当外层终端（要在 tokio 运行时里调）。stdin 不是终端 ⇒ 拒。交回外层与原始模式的守卫（落地还原）。
pub(crate) fn stdio_outer(prefix: u8) -> Result<(Outer, crate::platform::term::Raw), Said> {
    if !crate::platform::term::stdin_is_terminal() {
        return Err(copy_text("beHost.attach.notTerminal", &[]).into());
    }
    let raw = crate::platform::term::enter_raw()
        .map_err(|e| Said::with_raw(copy_text("beHost.attach.rawFailed", &[]), &e))?;
    let resized = crate::platform::term::resizes()
        .map_err(|e| Said::with_raw(copy_text("beHost.attach.rawFailed", &[]), &e))?;
    let (ktx, krx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    std::thread::spawn(move || {
        use std::io::Read as _;
        let mut stdin = std::io::stdin().lock();
        let mut buf = [0u8; 4096];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => return,
                Ok(n) => {
                    if ktx.send(buf[..n].to_vec()).is_err() {
                        return;
                    }
                }
            }
        }
    });
    let get = |k: &str| std::env::var(k).ok();
    Ok((
        Outer {
            keys: krx,
            resized,
            size: Box::new(|| {
                crate::platform::term::size().map(|(rows, cols)| Size { rows, cols })
            }),
            out: Box::new(std::io::stdout()),
            prefix,
            vi: keys::vi_from(get("VISUAL").as_deref(), get("EDITOR").as_deref()),
            in_tmux: get("TMUX").is_some_and(|t| !t.is_empty()),
        },
        raw,
    ))
}

/// 底行那一句的耳朵：盯 `id` 那个托管终端那条会话的队，那一句变了就说（`ccm` 入口交 `history::session_queue::host_line_ears`）。
pub(crate) type QueueEars = fn(
    &crate::platform::fs::Own,
    &str,
    Box<dyn Fn(Option<String>) + Send>,
) -> Result<crate::platform::watch_file::Watching, String>;

/// `ccm --attach <名|句柄>` 认到托管终端时走这里：接上、切换、直到断开或结束。交回进程退出码。
pub(crate) async fn attach(own: &crate::platform::fs::Own, first: &str, ears: QueueEars) -> i32 {
    let mut id = first.to_string();
    loop {
        let sock = match super::home::sock_of(own, &id) {
            Ok(s) => s,
            Err(s) => {
                eprintln!("ccm: {}", s.logged());
                return 2;
            }
        };
        let (outer, raw) = match stdio_outer(keys::DEFAULT_PREFIX) {
            Ok(o) => o,
            Err(s) => {
                eprintln!("ccm: {}", s.logged());
                return 2;
            }
        };
        let list = || {
            super::home::alive(own)
                .into_iter()
                .map(|f| (f.id, f.name))
                .collect::<Vec<_>>()
        };
        // 底行那一句：盯不上 ⇒ 记一行、底行不显示，接照接。
        let (qtx, qrx) = tokio::sync::mpsc::unbounded_channel();
        let heard = ears(
            own,
            &id,
            Box::new(move |l| {
                let _ = qtx.send(l);
            }),
        );
        if let Err(e) = &heard {
            tracing::warn!("瘦客户端：托管终端 {id} 的队没盯上，底行不显示排队：{e}");
        }
        let left = session(&sock, outer, &list, qrx).await;
        drop(heard);
        drop(raw);
        match left {
            Ok(Left::Switch(next)) => id = next,
            Ok(Left::Detached | Left::OuterGone) => return 0,
            Ok(Left::Ended(code)) => return code.unwrap_or(0),
            Ok(Left::Lost) => {
                eprintln!("ccm: {}", copy_text("beHost.attach.lost", &[]));
                return 1;
            }
            Err(s) => {
                eprintln!("ccm: {}", s.logged());
                return 2;
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/host/attach_tests.rs"]
mod tests;
