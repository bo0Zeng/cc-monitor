//! 瘦客户端：用户真终端里的 `ccm --attach <名|句柄>`（A3-3-3）。
//!
//! 连托管口、`hello{want: input}`（最新连上的真终端持有输入）；进外层终端的备用屏、退出还原。
//! 本地一个 `vt100` 照托管伪终端的尺寸跑：`welcome` / `resync` 整屏重来，`out` 接着算，只把变了的画出去（`state_diff`），
//! 拍子跟着 `out` 走，不用定时器。不持有时外层标题前加「[只看] 」，敲的键不送，底行提示「输入在 … · 回车接管」
//! （这个回车不送进去），下一个键收掉提示。外层窗口改尺寸 ⇒ 发 `resize`（托管只认持有者的）。
//! 关真终端窗口 · ssh 断 ⇒ 只算断开，会话照跑。退出码：断开 0，会话结束带子进程的码。
//!
//! 外层终端（键 · 尺寸 · 画）由 [`Outer`] 交进来：生产是本进程的 stdin / stdout（[`stdio_outer`]），判据交一个假的。

use super::keys::{self, Act, Copy, CopyKeys, Keys};
use super::wire::{self, ClientRow, Ctl, Frame, Holder, Size, Want, Who, WhoKind, PROTO};
use copy_core::copy_text;
use copy_core::said::Said;
use std::io::Write;
use tokio::sync::mpsc::UnboundedReceiver;

/// 进 / 出外层备用屏，出来时把里面可能开着的几样模式关掉（括号粘贴 · 光标键 · 小键盘 · 鼠标上报 · 焦点），光标显出来。
const ENTER_OUTER: &[u8] = b"\x1b[?1049h\x1b[H\x1b[2J";
const LEAVE_OUTER: &[u8] =
    b"\x1b[?2004l\x1b[?1l\x1b>\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?1004l\x1b[?25h\x1b[m\x1b[?1049l";

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

/// parser 碰到的、屏本身不管的：标题 · 铃。
#[derive(Default)]
struct Hooks {
    title: Option<Vec<u8>>,
    bells: u32,
}

impl vt100::Callbacks for Hooks {
    fn audible_bell(&mut self, _: &mut vt100::Screen) {
        self.bells += 1;
    }
    fn set_window_title(&mut self, _: &mut vt100::Screen, t: &[u8]) {
        self.title = Some(t.to_vec());
    }
}

struct Client {
    outer: Outer,
    parser: vt100::Parser<Hooks>,
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
}

/// 接一个托管终端，直到断开 / 会话结束 / 切到另一个。外层终端的进出由调用方（[`stdio_outer`] 那一侧）管。
pub(crate) async fn session(
    sock: &std::path::Path,
    mut outer: Outer,
    list: &(dyn Fn() -> Vec<(String, String)> + Sync),
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
    let _ = outer.out.write_all(ENTER_OUTER);
    let _ = outer.out.flush();
    let in_tmux = outer.in_tmux;
    let (prefix, vi) = (outer.prefix, outer.vi);
    let mut c = Client {
        outer,
        parser: vt100::Parser::new_with_callbacks(24, 80, 0, Hooks::default()),
        you: 0,
        holder: None,
        over: Over::None,
        keys: Keys::new(prefix),
        copy: CopyKeys::new(vi),
        title: Vec::new(),
        taking: false,
        typed_while_taking: Vec::new(),
    };
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
                    let _ = ftx.send(None);
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
                    let _ = wire::write_frame(&mut wr, &Frame::In(b)).await;
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
                    if wire::write_frame(&mut wr, &f).await.is_err() {
                        break;
                    }
                }
                if let Some(l) = left {
                    break l;
                }
            }
            r = c.outer.resized.recv() => {
                if r.is_some() {
                    if let Some(s) = (c.outer.size)() {
                        let _ = wire::write_frame(&mut wr, &Frame::Ctl(Ctl::Resize { rows: s.rows, cols: s.cols })).await;
                    }
                }
            }
        }
    };
    if matches!(left, Left::Detached | Left::Switch(_) | Left::OuterGone) {
        let _ = wire::write_frame(&mut wr, &Frame::Ctl(Ctl::Bye)).await;
    }
    reader.abort();
    let _ = c.outer.out.write_all(LEAVE_OUTER);
    let _ = c.outer.out.flush();
    Ok(left)
}

impl Client {
    fn rows(&self) -> u16 {
        self.parser.screen().size().0
    }

    fn holding(&self) -> bool {
        self.holder.as_ref().is_some_and(|h| h.mine)
    }

    fn write(&mut self, b: &[u8]) {
        let _ = self.outer.out.write_all(b);
    }

    fn flush(&mut self) {
        let _ = self.outer.out.flush();
    }

    /// 整屏重画（当屏 ＋ 标题；盖着的那一层不画）。
    fn redraw(&mut self) {
        let full = self.parser.screen().state_formatted();
        self.write(b"\x1b[H\x1b[2J");
        self.write(&full);
        self.retitle(true);
        self.flush();
    }

    fn retitle(&mut self, force: bool) {
        let inner = self.parser.callbacks().title.clone().unwrap_or_default();
        let mut t = Vec::new();
        if !self.holding() {
            t.extend_from_slice(copy_text("beHost.attach.viewOnly", &[]).as_bytes());
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
                let prev = self.parser.screen().clone();
                let bells = self.parser.callbacks().bells;
                self.parser.process(&b);
                if matches!(self.over, Over::None | Over::Hint) {
                    let diff = self.parser.screen().state_diff(&prev);
                    self.write(&diff);
                    if self.parser.callbacks().bells > bells {
                        self.write(b"\x07");
                    }
                    self.retitle(false);
                    if self.over == Over::Hint {
                        let said = self.holder_said();
                        self.bottom_line(&said);
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
                    self.flush();
                }
            }
            Ctl::HeldElsewhere { holder } => {
                self.holder = holder;
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
        self.parser =
            vt100::Parser::new_with_callbacks(size.rows, size.cols, 0, Hooks { title, bells: 0 });
        self.parser.process(screen.as_bytes());
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

/// `ccm --attach <名|句柄>` 认到托管终端时走这里：接上、切换、直到断开或结束。交回进程退出码。
pub(crate) async fn attach(own: &crate::platform::fs::Own, first: &str) -> i32 {
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
        let left = session(&sock, outer, &list).await;
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
