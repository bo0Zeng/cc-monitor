//! 瘦客户端：用户真终端里的 `ccm --attach <名|句柄>`（A3-3-3）。
//!
//! 连托管口、`hello{want: input}`（最新连上的真终端持有输入）；进外层终端的备用屏、退出还原。
//! 本地一个 `vt100` 照托管伪终端的尺寸跑：`welcome` / `resync` 整屏重来，`out` 接着算，只把变了的画出去（`contents_diff`），
//! 拍子跟着 `out` 走（只有模板里有时刻那一格才另起一个分钟拍）。不持有时外层标题前加「[只看] 」，敲的键不送，最底一行提示
//! 「输入在 … · 回车接管」（这个回车不送进去），下一个键收掉提示。外层窗口改尺寸 ⇒ 发 `resize`（托管只认持有者的）。
//!
//! 状态行（照 tmux 的 `status`）：偏好里的模板要那一行 ⇒ 外层最底一行只归它，报给托管的尺寸是外层少一行；
//! 那一行照模板写（`status`），提示 · 「结束？」· 在 tmux 里那几句也在那一行；模板空 ⇒ 不留，提示与排队照旧盖最底一行。
//! 各格的值由上层装进来的耳朵说（[`install_status`]）。
//! 关真终端窗口 · ssh 断 ⇒ 只算断开，会话照跑。退出码：断开 0，会话结束带子进程的码。
//!
//! 外层终端（键 · 尺寸 · 画）由 [`Outer`] 交进来：生产是本进程的 stdin / stdout（[`stdio_outer`]），判据交一个假的。
//!
//! 尺寸（A3-3-14）：伪终端的尺寸跟持有者；外层比它小（只看的一端 · 持有者刚改了尺寸、托管还没跟上）⇒ 只画左上那一块，
//! 不缩放、不折行；外层大 ⇒ 多出来的空着。只看的一端标题前缀带伪终端的尺寸。
//! 终端模式（A3-3-15）：外层的输入模式由这里自己管（[`Modes`]），持有者照里面开的开；只看的不开括号粘贴 · 焦点（鼠标上报恒开，见下）。
//! 里面开着同步输出 `?2026` ⇒ 屏照算、先不画，关上那一下一次画完。
//!
//! 鼠标（A3-3-33，照 tmux `mouse on`）：外层的鼠标上报由这里自己开（SGR 1006；按着拖动，持有者且里面要任意移动时跟着开），
//! 出外层备用屏那一下关掉。里面开了鼠标上报 ⇒ 持有者照里面的模式与编码交进去（只看的丢）；没开 ⇒ 滚轮往上进翻看、
//! 翻看里滚轮上下各 [`WHEEL_LINES`] 行、往下滚到底退回实时；左键拖动在本地屏上反显选区，松手经 OSC 52 交外层剪贴板、选区留着；
//! 有选区时 Ctrl+C 再交一次、收掉选区、不送中断，别的键或再按一下先收掉选区。外层原生选字按住 Shift（外层终端自己的规矩）。
//! 外层读不到鼠标事件（老的 Windows 控制台只给鼠标事件记录、不变成序列）⇒ 没有滚轮翻看，`前缀 [` 照旧。
//! 剪贴板（照 tmux `set-clipboard`）：里面写剪贴板的那一句（OSC 52：程序自己选完字要外层复制）不上屏、重算屏会吞掉 ⇒
//! parser 接住，算完那一块原样转给外层（持有者与只看的都转，不看盖着哪一层）；读剪贴板的询问不转。

use super::hold;
use super::hold::HolderWho;
use super::keys::{self, Act, Copy, CopyKeys, Keys, Mice, Mouse, Piece, Wheel};
use super::screen::Extra;
use super::status::{self, Snap, Template, Values};
use super::wire::{self, ClientRow, Ctl, Frame, Holder, Size, Want, Who, WhoKind, PROTO};
use copy_core::copy_text;
use copy_core::said::Said;
use copy_core::{Discard, Why};
use std::io::Write;
use tokio::sync::mpsc::UnboundedReceiver;
use vt100::{MouseProtocolEncoding, MouseProtocolMode};

/// 进 / 出外层备用屏，出来时把里面可能开着的几样模式关掉（括号粘贴 · 光标键 · 小键盘 · 鼠标上报 · 焦点 · 光标形状），光标显出来。
const ENTER_OUTER: &[u8] = b"\x1b[?1049h\x1b[H\x1b[2J";
const LEAVE_OUTER: &[u8] =
    b"\x1b[?2004l\x1b[?1l\x1b>\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1004l\x1b[0 q\x1b[?25h\x1b[m\x1b[?1049l";

/// 滚轮一格翻几行（同 tmux 缺省）。
const WHEEL_LINES: usize = 5;

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

/// 最底一行 / 整屏盖在上面的那一层。
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

/// 写剪贴板那一句（OSC 52，BEL 收尾）：`ty` 选哪块剪贴板，`b64` 是 base64 过的字。里面写的照转、这边拖选复制的，都拼成这一形。
fn osc52(ty: &[u8], b64: &[u8]) -> Vec<u8> {
    let mut v = b"\x1b]52;".to_vec();
    v.extend_from_slice(ty);
    v.push(b';');
    v.extend_from_slice(b64);
    v.push(0x07);
    v
}

/// parser 碰到的、屏本身不管的：标题 · 铃 · 写剪贴板 · `vt100` 不记的那几样模式（焦点 · 同步输出 · 光标形状）。
#[derive(Default)]
struct Hooks {
    title: Option<Vec<u8>>,
    bells: u32,
    extra: Extra,
    /// 里面写剪贴板的那几句（拼好的 OSC 52），算完这一块转给外层、清空。
    clip: Vec<u8>,
}

impl vt100::Callbacks for Hooks {
    fn audible_bell(&mut self, _: &mut vt100::Screen) {
        self.bells += 1;
    }
    fn set_window_title(&mut self, _: &mut vt100::Screen, t: &[u8]) {
        self.title = Some(t.to_vec());
    }
    fn copy_to_clipboard(&mut self, _: &mut vt100::Screen, ty: &[u8], data: &[u8]) {
        self.clip.extend_from_slice(&osc52(ty, data));
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
    /// 状态行的模板（解析好的）与各格的值（`holder` · `time` 由这里照那一刻填）。
    status: Template,
    values: Values,
    /// 状态行那一行上次写出去的字（`None` ⇒ 下一次照写）。模板空时是盖在最底一行的那一句。
    drawn_status: Option<String>,
    /// 临时占那一行的一句（「结束？」· 在 tmux 里）：收掉照模板重画。
    notice: Option<String>,
    /// 这台的时区（时刻那一格照它写）。
    tz: crate::common::time::Tz,
    /// 外层送来的字节里认鼠标上报。
    mice: Mice,
    /// 已经出了外层备用屏（建时进，[`Client::leave`] 出一次；落地时还没出 ⇒ 那一下出）。
    left: bool,
    /// 这边自己拖出来的选区（里面没要鼠标时）。
    sel: Option<Sel>,
}

/// 一段选区：按下那一格 · 此刻那一格（行 · 列，从 0 数，落在伪终端那一屏里）；拖过没有；还按着没有。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sel {
    anchor: (u16, u16),
    head: (u16, u16),
    moved: bool,
    held: bool,
}

impl Sel {
    /// 两端，靠前的在前（含）。
    fn span(&self) -> ((u16, u16), (u16, u16)) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.leave();
    }
}

/// 接一个托管终端，直到断开 / 会话结束 / 切到另一个。外层终端的进出由调用方（[`stdio_outer`] 那一侧）管。
/// `first`：接上那一刻的状态行（模板 ＋ 值）；`snaps`：之后变了的（[`StatusEars`] 说来的），来一份照画一份。
/// `name` ＝ 断开 · 结束之后那一行里的名（`ccm --attach` 认的那个）。
pub(crate) async fn session(
    sock: &std::path::Path,
    name: &str,
    outer: Outer,
    list: &(dyn Fn() -> Vec<(String, String)> + Sync),
    first: Snap,
    mut snaps: UnboundedReceiver<Snap>,
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
    let in_tmux = outer.in_tmux;
    let mut c = Client::new(outer, first);
    let size = c.pty_size();
    wire::write_frame(
        &mut wr,
        &Frame::Ctl(Ctl::Hello {
            proto: PROTO,
            build: crate::common::id::BUILD_ID.to_string(),
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
    // 时刻那一格按分钟走：模板里有要分钟拍的那一格才看这个拍子（`Template::ticks`）；第一拍对齐到下一个整分。
    let into_minute = crate::common::time::now_secs() % 60;
    #[expect(
        clippy::disallowed_methods,
        reason = "状态行的时刻那一格（时:分）要按整分重画：模板里有它才看分钟拍，没有它分钟拍不起作用；去掉时刻那一格就不用它"
    )]
    let mut minute = tokio::time::interval_at(
        tokio::time::Instant::now() + std::time::Duration::from_secs(60 - into_minute),
        std::time::Duration::from_secs(60),
    );
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
                    c.passing_notice(copy_text("beHost.attach.inTmux", &[]));
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
            Some(s) = snaps.recv() => {
                if let Some(s) = c.snap_now(s) {
                    send(&mut wr, &Frame::Ctl(Ctl::Resize { rows: s.rows, cols: s.cols })).await;
                }
            }
            _ = minute.tick(), if c.ticks() => c.draw_status(false),
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
    c.leave();
    if let Some(line) = left_line(&left, name) {
        put(&mut *c.outer.out, format!("{line}\r\n").as_bytes());
    }
    Ok(left)
}

/// 不持有时的提示那一句：名单那一句（[`holder_said`]）＋「 · 回车接管」。
pub fn held_line(holder: Option<&Holder>) -> String {
    copy_text("beHost.attach.heldBy", &[("said", &holder_said(holder))])
}

/// 输入在谁手里那一句（名单那一句 [`hold::said_of`]，按收的这一端算）：提示那一句与状态行 `holder` 那一格都照它。
fn holder_said(holder: Option<&Holder>) -> String {
    match holder {
        None => hold::said_of(HolderWho::None, false),
        Some(h) => hold::said_of(hold::who_of(h.kind, h.front.as_deref()), h.mine),
    }
}

/// `前缀 D` 那张单子：一端一行（种类那一列是 [`hold::word_of`] 的短词），末行一次图例。
pub(crate) fn client_lines(clients: &[ClientRow]) -> Vec<String> {
    let mut lines: Vec<String> = clients
        .iter()
        .map(|r| {
            let kind = hold::word_of(hold::who_of(r.kind, r.front.as_deref()));
            let pid = r.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into());
            let mark = match (r.holder, r.mine) {
                (true, true) => "* <",
                (true, false) => "*",
                (false, true) => "  <",
                _ => "",
            };
            format!("{:>3}  {kind}  pid {pid}  {mark}", r.client)
        })
        .collect();
    lines.push(copy_text("beHost.attach.clientsLegend", &[]));
    lines
}

/// 还原外层之后往原屏印的那一行（同 tmux 的 `[detached …]` · `[exited]`）：断开 · 结束各一句；别的收法不印。
fn left_line(left: &Left, name: &str) -> Option<String> {
    match left {
        Left::Detached => Some(copy_text("beHost.attach.detached", &[("name", name)])),
        // 退出码不插进话里（C-Y5）：它是这个进程自己的退出码，外层 shell 照样拿得到。
        Left::Ended(_) => Some(copy_text("beHost.attach.ended", &[("name", name)])),
        Left::Lost | Left::Switch(_) | Left::OuterGone => None,
    }
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
    /// 建的那一下进外层备用屏。
    fn new(mut outer: Outer, first: Snap) -> Self {
        let (prefix, vi) = (outer.prefix, outer.vi);
        let view = (outer.size)();
        put(&mut *outer.out, ENTER_OUTER);
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
            status: Template::parse(&first.template),
            values: first.values,
            drawn_status: None,
            notice: None,
            tz: crate::common::time::Tz::here(),
            mice: Mice::default(),
            left: false,
            sel: None,
        }
    }

    /// 出外层备用屏、关掉开着的模式（鼠标上报在内）；只出一次。
    fn leave(&mut self) {
        if !std::mem::replace(&mut self.left, true) {
            put(&mut *self.outer.out, LEAVE_OUTER);
        }
    }

    /// 状态行那一份换了（耳朵说来的）：照新的模板 · 值重画那一行；要不要那一行变了 ⇒ 整屏重画、交回新的伪终端尺寸（报给托管）。
    fn snap_now(&mut self, s: Snap) -> Option<Size> {
        let had = self.has_row();
        let t = Template::parse(&s.template);
        if t == self.status && s.values == self.values {
            return None;
        }
        self.status = t;
        self.values = s.values;
        if had == self.has_row() {
            if matches!(self.over, Over::None | Over::Hint) || self.has_row() {
                self.draw_status(false);
            }
            return None;
        }
        self.drawn_status = None;
        if matches!(self.over, Over::None | Over::Hint) {
            self.redraw();
        }
        self.pty_size()
    }

    /// 临时占那一行的一句换了（`None` ⇒ 收掉，照模板重画）。
    fn notice_now(&mut self, n: Option<String>) {
        if n != self.notice {
            self.notice = n;
            self.draw_status(true);
        }
    }

    /// 一句过路的提示（在 tmux 里）：留了那一行 ⇒ 占那一行、下一个键收掉；没留 ⇒ 盖一次最底一行，下一次画屏照里面冲掉。
    fn passing_notice(&mut self, n: String) {
        if self.has_row() {
            self.notice_now(Some(n));
        } else {
            self.bottom_line(&n);
        }
    }

    /// 留不留状态行那一行：模板不空、外层至少两行。
    fn has_row(&self) -> bool {
        self.status.shows() && self.view.is_none_or(|v| v.rows >= 2)
    }

    /// 要不要分钟拍（模板里有时刻那一格）。
    fn ticks(&self) -> bool {
        self.has_row() && self.status.ticks()
    }

    /// 报给托管的尺寸：外层（留了状态行 ⇒ 少一行）。外层问不出 ⇒ 不报。
    fn pty_size(&self) -> Option<Size> {
        let v = self.view?;
        Some(Size {
            rows: v.rows - u16::from(self.has_row()),
            cols: v.cols,
        })
    }

    /// 此刻最底一行该是什么：留了那一行 ⇒ 临时那一句，或（提示 ＋）照模板写的那一句；没留 ⇒ 盖在最底一行的那一句（没有 ⇒ `None`）。
    fn status_text(&self) -> Option<String> {
        let cols = usize::from(self.view.map_or(self.parser.screen().size().1, |v| v.cols));
        let sep = copy_text("kit.text.sep", &[]);
        let queue = self
            .values
            .get("queue")
            .map(|q| q.trim().to_string())
            .filter(|q| !q.is_empty());
        if !self.has_row() {
            if let Some(n) = &self.notice {
                return Some(n.clone());
            }
            return match (self.over == Over::Hint, queue) {
                (true, Some(q)) => Some([held_line(self.holder.as_ref()), q].join(&sep)),
                (true, None) => Some(held_line(self.holder.as_ref())),
                (false, q) => q,
            };
        }
        if let Some(n) = &self.notice {
            return Some(status::cut(n, cols));
        }
        let mut v = self.values.clone();
        v.insert(status::HOLDER.into(), holder_said(self.holder.as_ref()));
        let now = i64::try_from(crate::common::time::now_secs()).unwrap_or(i64::MAX);
        v.insert(
            status::TIME.into(),
            crate::common::time::hm(self.tz.local(now)),
        );
        let (l, r) = self.status.halves(&v);
        let l = if self.over == Over::Hint {
            let hint = held_line(self.holder.as_ref());
            if l.is_empty() {
                hint
            } else {
                [hint, l].join(&sep)
            }
        } else {
            l
        };
        Some(status::line_of(&l, &r, cols))
    }

    /// 画最底那一行：留了那一行 ⇒ 字变了（或 `force`）才写；没留 ⇒ 有那一句就盖（里面的字可能盖过它，每次都写），
    /// 那一句没了 ⇒ 整屏重画（那一行照里面）。
    fn draw_status(&mut self, force: bool) {
        let text = self.status_text();
        if self.has_row() {
            let text = text.unwrap_or_default();
            if !force && self.drawn_status.as_deref() == Some(text.as_str()) {
                return;
            }
            let row = self.view.map_or(self.parser.screen().size().0, |v| v.rows);
            self.line_at(row, &text);
            self.drawn_status = Some(text);
            return;
        }
        match text {
            Some(t) => {
                self.bottom_line(&t);
                self.drawn_status = Some(t);
            }
            None => {
                if self.drawn_status.take().is_some() {
                    self.paint(true);
                    self.flush();
                }
            }
        }
    }

    /// 盖在上面的那几层（单子 · 翻看）与伪终端那一块按外层的行数摆（留了状态行 ⇒ 少一行）。
    fn rows(&self) -> u16 {
        self.view.map_or(self.parser.screen().size().0, |v| {
            v.rows - u16::from(self.has_row())
        })
    }

    /// 外层改了尺寸：记下、照新尺寸重画；交回新尺寸（报给托管）。
    fn outer_resized(&mut self) -> Option<Size> {
        let s = (self.outer.size)()?;
        self.view = Some(s);
        self.drawn_status = None;
        if matches!(self.over, Over::None | Over::Hint) {
            self.redraw();
        }
        self.pty_size()
    }

    /// 外层该开的输入模式：持有者照里面开的；只看的不开括号粘贴 · 焦点（敲的不送）。
    /// 鼠标上报恒开（SGR 1006 · 按着拖动：滚轮与拖选要用）；持有者且里面要任意移动 ⇒ 外层跟着开那一档。
    fn wanted_modes(&self) -> Modes {
        let s = self.parser.screen();
        let x = &self.parser.callbacks().extra;
        let hold = self.holding();
        let inner = s.mouse_protocol_mode();
        Modes {
            keypad: s.application_keypad(),
            cursor_keys: s.application_cursor(),
            paste: hold && s.bracketed_paste(),
            mouse: match inner {
                MouseProtocolMode::AnyMotion if hold => inner,
                _ => MouseProtocolMode::ButtonMotion,
            },
            encoding: MouseProtocolEncoding::Sgr,
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
        let view = self.view.map_or(
            Size {
                rows: prow,
                cols: pcol,
            },
            |v| Size {
                rows: self.rows(),
                cols: v.cols,
            },
        );
        let prev = match (&self.drawn, full) {
            (Some((s, v)), false) if s.size() == now.size() && *v == view => Some(s),
            _ => None,
        };
        let mut b = Vec::new();
        if prev.is_none() {
            b.extend_from_slice(b"\x1b[m\x1b[H\x1b[2J");
            self.drawn_status = None;
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
        self.draw_sel(view);
        self.sync_modes();
        true
    }

    /// 选区反显盖在画好的屏上（只盖外层装得下的那一块；光标存了再放回）。画进 `drawn` 的是没盖的那一屏，
    /// 收掉或改小选区 ⇒ 整屏重画（[`Client::repaint_sel`]）。
    fn draw_sel(&mut self, view: Size) {
        let Some(sel) = self.sel else { return };
        let ((r0, c0), (r1, c1)) = sel.span();
        let s = self.parser.screen();
        let cols = s.size().1.min(view.cols);
        let mut b = b"\x1b7".to_vec();
        let wide = |r: u16, c: u16| s.cell(r, c).is_some_and(vt100::Cell::is_wide);
        let tail = |r: u16, c: u16| s.cell(r, c).is_some_and(vt100::Cell::is_wide_continuation);
        for r in r0..=r1.min(view.rows.saturating_sub(1)) {
            let mut from = if r == r0 { c0 } else { 0 };
            let mut to = if r == r1 { c1 + 1 } else { cols }.min(cols);
            // 宽字两格当一格：起点落在右半格 ⇒ 往左挪到它的左半格；终点落在左半格 ⇒ 盖住整个字，盖不下（最右一列）⇒ 不盖它。
            if from > 0 && tail(r, from) {
                from -= 1;
            }
            if to > 0 && wide(r, to - 1) {
                to = if to < cols { to + 1 } else { to - 1 };
            }
            if from >= to {
                continue;
            }
            b.extend_from_slice(format!("\x1b[{};{}H\x1b[m\x1b[7m", r + 1, from + 1).as_bytes());
            for c in from..to {
                match s.cell(r, c) {
                    Some(cell) if cell.is_wide_continuation() => {}
                    Some(cell) if cell.has_contents() => {
                        b.extend_from_slice(cell.contents().as_bytes())
                    }
                    _ => b.push(b' '),
                }
            }
            b.extend_from_slice(b"\x1b[m");
        }
        b.extend_from_slice(b"\x1b8");
        self.write(&b);
    }

    /// 选区变了（拖到别处 · 收掉）：整屏重画，再按此刻的选区盖上。
    fn repaint_sel(&mut self) {
        self.paint(true);
        self.draw_status(true);
        self.flush();
    }

    /// 选区里的字（照本地屏取），经 OSC 52 交外层剪贴板（同里面写剪贴板转给外层的那一形）。
    fn copy_sel(&mut self) {
        let Some(sel) = self.sel else { return };
        let ((r0, c0), (r1, c1)) = sel.span();
        let s = self.parser.screen();
        // 宽字两格当一格（同 [`Client::draw_sel`]）：起点在右半格 ⇒ 从左半格起；终点在左半格 ⇒ 连右半格。
        let c0 = if c0 > 0
            && s.cell(r0, c0)
                .is_some_and(vt100::Cell::is_wide_continuation)
        {
            c0 - 1
        } else {
            c0
        };
        let end = if s.cell(r1, c1).is_some_and(vt100::Cell::is_wide) {
            c1 + 2
        } else {
            c1 + 1
        };
        let text = s.contents_between(r0, c0, r1, end);
        if text.is_empty() {
            return;
        }
        let seq = osc52(b"c", cmd_wire::b64::b64_encode(text.as_bytes()).as_bytes());
        self.write(&seq);
        self.flush();
    }

    /// 收掉选区（有的话），整屏重画。
    fn drop_sel(&mut self) {
        if self.sel.take().is_some() {
            self.repaint_sel();
        }
    }

    /// 里面没要鼠标时左键的按 · 拖 · 放：做选区。别的键（中键 · 右键）丢。
    fn select(&mut self, m: Mouse) {
        let code = m.code & !(4 | 8 | 16);
        let motion = code & 32 != 0;
        if code & !32 != 0 {
            return;
        }
        let (rows, cols) = self.parser.screen().size();
        let at = (
            m.row.saturating_sub(1).min(rows.saturating_sub(1)),
            m.col.saturating_sub(1).min(cols.saturating_sub(1)),
        );
        match (m.press, motion) {
            (true, false) => {
                self.drop_sel();
                self.sel = Some(Sel {
                    anchor: at,
                    head: at,
                    moved: false,
                    held: true,
                });
            }
            (true, true) => {
                if let Some(s) = self.sel.as_mut().filter(|s| s.held) {
                    if s.head != at || !s.moved {
                        s.head = at;
                        s.moved = true;
                        self.repaint_sel();
                    }
                }
            }
            (false, _) => match self.sel {
                Some(s) if s.held && s.moved => {
                    if let Some(s) = self.sel.as_mut() {
                        s.head = at;
                        s.held = false;
                    }
                    self.repaint_sel();
                    self.copy_sel();
                }
                Some(s) if s.held => self.sel = None,
                _ => {}
            },
        }
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
        self.draw_status(true);
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
        self.line_at(rows, text);
    }

    /// 第 `row` 行（从 1 数）写一句反显，光标放回原处。
    fn line_at(&mut self, row: u16, text: &str) {
        let seq = format!("\x1b7\x1b[{row};1H\x1b[m\x1b[7m{text}\x1b[K\x1b[m\x1b8");
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
        if self.has_row() {
            self.draw_status(true);
        }
        self.flush();
    }

    fn take_over(&mut self) {
        if self.over != Over::None {
            self.over = Over::None;
            self.notice = None;
            self.redraw();
        }
    }

    /// 托管那边来了一帧；交回 `Some` ⇒ 这一趟收了。
    fn frame(&mut self, f: Frame) -> Option<Left> {
        match f {
            Frame::Out(b) => {
                let bells = self.parser.callbacks().bells;
                self.parser.process(&b);
                let clip = std::mem::take(&mut self.parser.callbacks_mut().clip);
                if !clip.is_empty() {
                    self.write(&clip);
                    self.flush();
                }
                if matches!(self.over, Over::None | Over::Hint) {
                    let painted = self.paint(false);
                    if self.parser.callbacks().bells > bells {
                        self.write(b"\x07");
                    }
                    self.retitle(false);
                    if painted {
                        self.draw_status(false);
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
                    self.draw_status(false);
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
                    self.draw_status(true);
                }
            }
            Ctl::Captured { lines, .. } => {
                // 历史回来之前已经退出翻看（`q` · 滚到底）⇒ 不再进。
                let Over::Copy(v) = &self.over else {
                    return None;
                };
                let rows = usize::from(self.rows());
                let up = if v.up == usize::MAX {
                    rows.saturating_sub(1)
                } else {
                    v.up
                };
                let up = up.min(lines.len().saturating_sub(rows));
                self.over = Over::Copy(CopyView {
                    lines,
                    up,
                    found: None,
                });
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
        client_lines(clients)
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

    /// 外层送来一块字节（键与鼠标上报混着）：交回要发给托管的帧与（收了的话）怎么收的。
    fn key(
        &mut self,
        k: &[u8],
        list: &(dyn Fn() -> Vec<(String, String)> + Sync),
    ) -> (Vec<Frame>, Option<Left>) {
        let mut sends = Vec::new();
        for p in self.mice.split(k) {
            match p {
                Piece::Mouse(m) => sends.extend(self.mouse(m)),
                Piece::Bytes(b) => {
                    let (s, left) = self.keys_in(&b, list);
                    sends.extend(s);
                    if left.is_some() {
                        return (sends, left);
                    }
                }
            }
        }
        (sends, None)
    }

    /// 一次鼠标上报：翻看里滚轮翻；里面开了鼠标上报 ⇒ 持有者照里面的交进去；没开 ⇒ 滚轮往上进翻看、左键拖选（[`Client::select`]）。别的丢。
    fn mouse(&mut self, m: Mouse) -> Option<Frame> {
        match &self.over {
            Over::Copy(_) => {
                if let Some(w) = m.wheel() {
                    self.wheel_in_copy(w);
                }
                return None;
            }
            Over::None | Over::Hint => {}
            Over::Confirm | Over::Sheet | Over::Choose(_) => return None,
        }
        let s = self.parser.screen();
        let mode = s.mouse_protocol_mode();
        if mode != MouseProtocolMode::None {
            if !self.holding() {
                return None;
            }
            return m
                .encode(mode, s.mouse_protocol_encoding(), s.size())
                .map(Frame::In);
        }
        match m.wheel() {
            Some(Wheel::Up) => Some(self.enter_copy(WHEEL_LINES)),
            Some(Wheel::Down) => None,
            None => {
                self.select(m);
                None
            }
        }
    }

    /// 进翻看、先上翻 `up` 行（`usize::MAX` ⇒ 一页）；交回要历史的那一帧。
    fn enter_copy(&mut self, up: usize) -> Frame {
        self.sel = None;
        self.over = Over::Copy(CopyView {
            lines: Vec::new(),
            up,
            found: None,
        });
        Frame::Ctl(Ctl::Capture {
            back: super::screen::SCROLLBACK,
            color: true,
        })
    }

    /// 翻看里滚一格；往下滚到底 ⇒ 退回实时。历史还没回来 ⇒ 照记着，回来时落在那儿。
    fn wheel_in_copy(&mut self, w: Wheel) {
        let rows = usize::from(self.rows());
        let Over::Copy(v) = &mut self.over else {
            return;
        };
        let got = !v.lines.is_empty();
        v.up = match w {
            Wheel::Up if got => (v.up + WHEEL_LINES).min(v.lines.len().saturating_sub(rows)),
            Wheel::Up => v.up.saturating_add(WHEEL_LINES),
            Wheel::Down => v.up.saturating_sub(WHEEL_LINES),
        };
        if v.up == 0 && w == Wheel::Down {
            self.over = Over::None;
            self.redraw();
        } else if got {
            self.draw_copy();
        }
    }

    /// 外层敲的一段键（鼠标上报已经挑出去）。
    fn keys_in(
        &mut self,
        k: &[u8],
        list: &(dyn Fn() -> Vec<(String, String)> + Sync),
    ) -> (Vec<Frame>, Option<Left>) {
        // 过路的那一句（在 tmux 里）：下一个键收掉，照模板重画。
        if self.over == Over::None && self.notice.is_some() {
            self.notice_now(None);
        }
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
                Act::Send(mut b) => {
                    // 有选区：Ctrl+C 是复制（再交一次剪贴板、不送中断）；别的键先收掉选区、照常走。
                    if self.sel.is_some() {
                        if b.contains(&0x03) {
                            self.copy_sel();
                            b.retain(|&c| c != 0x03);
                        }
                        self.drop_sel();
                        if b.is_empty() {
                            continue;
                        }
                    }
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
                        self.draw_status(true);
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
                    let up = if a == Act::CopyModePageUp {
                        usize::MAX
                    } else {
                        0
                    };
                    sends.push(self.enter_copy(up));
                }
                Act::AskEnd => {
                    self.over = Over::Confirm;
                    self.notice_now(Some(copy_text("beHost.attach.confirmEnd", &[])));
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
    let crate::platform::term::Input { keys: krx, resized } = crate::platform::term::input()
        .map_err(|e| Said::with_raw(copy_text("beHost.attach.rawFailed", &[]), &e))?;
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

/// 状态行的耳朵：盯 `id` 那个托管终端那几格与偏好里的模板，起来那一下先说一次、之后变了就说（后端入口装 `faces::status_line_face::ears`）。
pub type StatusEars = fn(
    &crate::platform::fs::Own,
    &str,
    Box<dyn Fn(Snap) + Send + Sync>,
) -> Result<crate::platform::watch_file::Watching, String>;

static EARS: std::sync::OnceLock<StatusEars> = std::sync::OnceLock::new();

/// 装上状态行的耳朵（进程里第一次装的那一个算数）。host 不引上层 ⇒ 由后端入口在分流到 `ccm` 之前装（`stream/entry.rs`）。
pub fn install_status(f: StatusEars) {
    EARS.get_or_init(|| f);
}

/// 没装耳朵时那一份：只照这台偏好里的模板，各格空着。
fn bare_snap(own: &crate::platform::fs::Own) -> Snap {
    Snap {
        template: template_at(own),
        values: Values::new(),
    }
}

/// `own` 那个家里这台偏好那一份的落点（家目录说不出 ⇒ `None`）。
pub fn prefs_at(own: &crate::platform::fs::Own) -> Option<std::path::PathBuf> {
    own.at_home(relay_route_core::PREFS_REL)
        .ok()
        .map(|p| p.as_path().to_path_buf())
}

/// `own` 那个家里偏好照用的那一条状态行模板（现读；没设 ⇒ 随包的缺省，读不懂 · 没有家照缺省办）。瘦客户端与耳朵都照它。
pub fn template_at(own: &crate::platform::fs::Own) -> String {
    let prefs = prefs_at(own).map_or_else(Default::default, |p| {
        crate::common::machine_prefs::read_at(&p).prefs()
    });
    status::template_of(prefs.status_line.as_deref()).to_string()
}

/// `ccm --attach <名|句柄>` 认到托管终端时走这里：接上、切换、直到断开或结束。交回进程退出码。
pub async fn attach(own: &crate::platform::fs::Own, first: &str) -> i32 {
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
        // 状态行：耳朵起来那一下先说一次（就是接上那一刻的那一份）；没装 · 盯不上 ⇒ 记一行，照偏好里的模板、各格空着，接照接。
        let (qtx, mut qrx) = tokio::sync::mpsc::unbounded_channel();
        let heard = EARS.get().map(|ears| {
            ears(
                own,
                &id,
                Box::new(move |s| {
                    qtx.send(s).discard(Why::PeerGone); // 接着的那一趟已经断开 ⇒ 没人要这一份
                }),
            )
        });
        if let Some(Err(e)) = &heard {
            tracing::warn!("瘦客户端：托管终端 {id} 的状态行没盯上，各格空着：{e}");
        }
        let first_snap = qrx.try_recv().unwrap_or_else(|_| bare_snap(own));
        let name = list()
            .into_iter()
            .find(|(i, _)| *i == id)
            .map_or_else(|| id.clone(), |(_, n)| n);
        let left = session(&sock, &name, outer, &list, first_snap, qrx).await;
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

#[cfg(test)]
#[path = "../../../../tests/backend/host/attach_status_tests.rs"]
mod status_tests;

#[cfg(test)]
#[path = "../../../../tests/backend/host/attach_keys_tests.rs"]
mod keys_tests;
