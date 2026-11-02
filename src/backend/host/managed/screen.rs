//! 托管终端的屏：伪终端吐出来的字节照 `vt100` 算成一屏 ＋ 回滚。历史只住这里（A3-3-13）：
//! 新接上的一端拿 [`Screen::full`] 那份整屏字节，往回看经 [`Screen::capture`]。
//!
//! 终端询问（DA1 · DA2 · 光标位置 · XTVERSION · 状态）由这里照屏的状态答，答的字节由 [`Screen::feed`] 交回、写回伪终端；
//! 不转给哪一端（几端都接着时谁答都不对）。kitty 键盘询问不答（Claude Code 回落）。
//! `ESC[3J`（清回滚）`vt100` 不认 ⇒ 这里补：重建 parser、只留当屏（与真终端一致：Claude Code 清屏重画时发它）。
//! `vt100` 不记的几样模式（焦点 `?1004` · 同步输出 `?2026` · 光标形状）由 [`Extra`] 记，整屏字节里带上（A3-3-15）。

/// 回滚行数：与终端页往回看的上限（`control::terminals::MAX_SCROLLBACK`）对齐，判据钉两边相等。
pub const SCROLLBACK: usize = 2000;

/// DA1 的答：VT220 ＋ ANSI 颜色（与 tmux 答的同形）。
pub(crate) const DA1_REPLY: &[u8] = b"\x1b[?62;22c";
/// DA2 的答：终端型号 0 · 版本 10 · 0。
pub(crate) const DA2_REPLY: &[u8] = b"\x1b[>0;10;0c";

/// 往回抓的那一份：`lines` 从最老到最新（回滚在前、当屏在后），`capped` ＝ 还有更老的行没给（回滚里剩的 · 已被挤出回滚的）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Captured {
    pub(crate) lines: Vec<String>,
    pub(crate) capped: bool,
    pub(crate) rows: u16,
    pub(crate) cols: u16,
    /// 光标（行 · 列，从 0 数），相对当屏。
    pub(crate) cursor: (u16, u16),
}

/// `vt100` 不记、外层终端要跟着开关的那几样（托管这一屏与瘦客户端那一屏都记，同一份认法）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Extra {
    /// 焦点上报 `?1004`。
    pub(crate) focus: bool,
    /// 同步输出 `?2026`（开着 ⇒ 屏照算、先不画）。
    pub(crate) sync: bool,
    /// 光标形状（`CSI n SP q` 的 n；0 ＝ 终端缺省）。
    pub(crate) shape: u16,
}

impl Extra {
    /// parser 没认的一条 CSI：是这几样就记下（`?1004;2026h` 一条里几项也认）。
    pub(crate) fn csi(&mut self, i1: Option<u8>, params: &[&[u16]], c: char) {
        match (i1, c) {
            (Some(b'?'), 'h' | 'l') => {
                let on = c == 'h';
                for p in params {
                    match p.first() {
                        Some(1004) => self.focus = on,
                        Some(2026) => self.sync = on,
                        _ => {}
                    }
                }
            }
            (Some(b' '), 'q') => {
                self.shape = params.first().and_then(|p| p.first()).copied().unwrap_or(0);
            }
            _ => {}
        }
    }

    /// 让一个新终端也处在这几样里的字节（缺省的那几样不写）。
    pub(crate) fn formatted(&self) -> Vec<u8> {
        let mut v = Vec::new();
        if self.focus {
            v.extend_from_slice(b"\x1b[?1004h");
        }
        if self.shape != 0 {
            v.extend_from_slice(format!("\x1b[{} q", self.shape).as_bytes());
        }
        if self.sync {
            v.extend_from_slice(b"\x1b[?2026h");
        }
        v
    }
}

/// parser 在处理字节时碰到的、屏本身不管的那几样。
#[derive(Default)]
struct Hooks {
    replies: Vec<u8>,
    clear_scrollback: bool,
    title: Vec<u8>,
    extra: Extra,
}

impl vt100::Callbacks for Hooks {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.title = title.to_vec();
    }

    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        _i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        let first = params.first().and_then(|p| p.first()).copied().unwrap_or(0);
        match (i1, c, first) {
            (None, 'c', 0) => self.replies.extend_from_slice(DA1_REPLY),
            (Some(b'>'), 'c', 0) => self.replies.extend_from_slice(DA2_REPLY),
            (None, 'n', 5) => self.replies.extend_from_slice(b"\x1b[0n"),
            (None, 'n', 6) => {
                let (r, col) = screen.cursor_position();
                self.replies
                    .extend_from_slice(format!("\x1b[{};{}R", r + 1, col + 1).as_bytes());
            }
            (Some(b'>'), 'q', 0) => self.replies.extend_from_slice(
                format!("\x1bP>|ccm {}\x1b\\", crate::common::id::BUILD_ID).as_bytes(),
            ),
            (None, 'J', 3) => self.clear_scrollback = true,
            _ => self.extra.csi(i1, params, c),
        }
    }
}

/// 一个托管终端的屏。
pub(crate) struct Screen {
    parser: vt100::Parser<Hooks>,
}

impl Screen {
    pub(crate) fn new(rows: u16, cols: u16) -> Self {
        Self {
            parser: vt100::Parser::new_with_callbacks(rows, cols, SCROLLBACK, Hooks::default()),
        }
    }

    /// 喂一块伪终端输出；交回要写回伪终端的那几个答（没有询问 ⇒ 空）。
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.parser.process(bytes);
        if std::mem::take(&mut self.parser.callbacks_mut().clear_scrollback) {
            self.drop_scrollback();
        }
        std::mem::take(&mut self.parser.callbacks_mut().replies)
    }

    /// 清回滚：`vt100` 没有这一下 ⇒ 换一个新 parser，把当屏的整屏字节喂进去（回滚就没了，当屏 · 光标 · 模式照旧）。
    fn drop_scrollback(&mut self) {
        let (rows, cols) = self.size();
        let full = self.full();
        let kept = std::mem::take(self.parser.callbacks_mut());
        let mut fresh = vt100::Parser::new_with_callbacks(rows, cols, SCROLLBACK, Hooks::default());
        fresh.process(&full);
        *fresh.callbacks_mut() = Hooks {
            clear_scrollback: false,
            ..kept
        };
        self.parser = fresh;
    }

    /// 整屏字节：喂给一个同尺寸的新终端 ⇒ 逐格 · 光标 · 模式都相同，标题也带上（新接上的一端 · `resync` 发的就是它）。
    pub(crate) fn full(&self) -> Vec<u8> {
        let mut v = self.parser.screen().state_formatted();
        let title = &self.parser.callbacks().title;
        if !title.is_empty() {
            v.extend_from_slice(b"\x1b]2;");
            v.extend_from_slice(title);
            v.push(0x07);
        }
        v.extend_from_slice(&self.parser.callbacks().extra.formatted());
        v
    }

    /// 那一屏的格子（托管口那一张账的判据也照它比）。
    pub(crate) fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    pub(crate) fn size(&self) -> (u16, u16) {
        self.screen().size()
    }

    pub(crate) fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.screen_mut().set_size(rows, cols);
    }

    /// 往回抓：回滚里最近的 `back` 行 ＋ 当屏（形同 `tmux capture-pane -p -S -<back>`，`color` ⇒ 带 SGR，同 `-e`）。
    /// 抓完看的位置拨回最新一屏。
    pub(crate) fn capture(&mut self, back: usize, color: bool) -> Captured {
        let (rows, cols) = self.size();
        let cursor = self.parser.screen().cursor_position();
        let screen = self.parser.screen_mut();
        screen.set_scrollback(usize::MAX);
        let held = screen.scrollback();
        let take = back.min(held);
        let r = usize::from(rows);
        // 拨到 offset 时当屏第 0 行是回滚里倒数第 offset 行；一次拨一屏，逐屏取。
        let mut lines: Vec<String> = Vec::with_capacity(take + r);
        let mut left = take;
        while left > 0 {
            screen.set_scrollback(left);
            let n = left.min(r);
            lines.extend(row_texts(screen, cols, color).into_iter().take(n));
            left -= n;
        }
        screen.set_scrollback(0);
        lines.extend(row_texts(screen, cols, color));
        Captured {
            lines,
            // 回滚满了 ⇒ 更老的已经被挤掉（`vt100` 不报挤掉几行，满了就算）。
            capped: held > take || held >= SCROLLBACK,
            rows,
            cols,
            cursor,
        }
    }
}

fn row_texts(screen: &vt100::Screen, cols: u16, color: bool) -> Vec<String> {
    if color {
        screen
            .rows_formatted(0, cols)
            .map(|b| {
                let mut s = String::from_utf8_lossy(&b).into_owned();
                if !s.is_empty() {
                    s.push_str("\x1b[m");
                }
                s
            })
            .collect()
    } else {
        screen.rows(0, cols).collect()
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/host/screen_tests.rs"]
mod tests;
