//! 瘦客户端的键表：照 tmux 的缺省键（A3-3-3-1，用户 10-27 定）。一张表，`前缀 ?` 也照它印。
//!
//! - 前缀缺省 `Ctrl+b`（[`DEFAULT_PREFIX`]）；`前缀 前缀` 把一个前缀字节送进去（tmux `send-prefix`）。
//!   前缀之后按了表里没有的键 ⇒ 什么都不做（同 tmux）。前缀可改：只认单个 `Ctrl+<键>`（[`prefix_of`]），不读 `~/.tmux.conf`。
//! - 翻看（`copy-mode`）里照 tmux 缺省 emacs 表；`VISUAL` / `EDITOR` 含 `vi` 时用 vi 表（[`vi_from`]，同 tmux）。选中复制这一版不做。
//!
//! 这里只把字节变成动作，不碰终端、不碰托管口；送不送（只看的那一端不送）由 `attach.rs` 定。

/// 缺省前缀：`Ctrl+b`。
pub(crate) const DEFAULT_PREFIX: u8 = 0x02;

/// 前缀之后的那一个键各做什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Act {
    /// 原样送进去。
    Send(Vec<u8>),
    /// `前缀 d`：断开（会话照跑）。
    Detach,
    /// `前缀 D`：列连着的各端、谁持有输入。
    ListClients,
    /// `前缀 r`：重画。
    Redraw,
    /// `前缀 ?`：列键。
    ListKeys,
    /// `前缀 [`：进翻看。
    CopyMode,
    /// `前缀 PgUp`：进翻看并上翻一页。
    CopyModePageUp,
    /// `前缀 x`：先问「结束？(y/n)」。
    AskEnd,
    /// 问过之后按了 `y`。
    End,
    /// 问过之后按了别的。
    CancelEnd,
    /// `前缀 s`：列这台的终端、选一个切过去。
    ChooseTerminal,
}

/// 表：键（给人看的写法）· 动作 · tmux 里同义的那条命令。`前缀 ?` 照它印。
const TABLE: &[(&str, &str)] = &[
    ("C-b", "send-prefix"),
    ("d", "detach-client"),
    ("D", "choose-client"),
    ("r", "refresh-client"),
    ("?", "list-keys"),
    ("[", "copy-mode"),
    ("PPage", "copy-mode -u"),
    ("x", "confirm-before kill-pane"),
    ("s", "choose-tree"),
];

fn act_of(key: &[u8], prefix: u8) -> Option<Act> {
    Some(match key {
        [b] if *b == prefix => Act::Send(vec![prefix]),
        b"d" => Act::Detach,
        b"D" => Act::ListClients,
        b"r" => Act::Redraw,
        b"?" => Act::ListKeys,
        b"[" => Act::CopyMode,
        b"\x1b[5~" => Act::CopyModePageUp,
        b"x" => Act::AskEnd,
        b"s" => Act::ChooseTerminal,
        _ => return None,
    })
}

/// `C-b` · `Ctrl+b` · `^B` 这几种写法的单个 `Ctrl+<字母>` ⇒ 那个控制字节；别的（`M-a` · 单字母 · 多个键）⇒ `None`。
pub(crate) fn prefix_of(s: &str) -> Option<u8> {
    let t = s.trim();
    let rest = t
        .strip_prefix("C-")
        .or_else(|| t.strip_prefix("Ctrl+"))
        .or_else(|| t.strip_prefix("ctrl+"))
        .or_else(|| t.strip_prefix('^'))?;
    let mut cs = rest.chars();
    let c = cs.next()?;
    if cs.next().is_some() || !c.is_ascii_alphabetic() {
        return None;
    }
    Some(c.to_ascii_lowercase() as u8 - b'a' + 1)
}

/// 一个前缀字节给人看的写法（`0x02` ⇒ `C-b`）。
pub(crate) fn prefix_label(prefix: u8) -> String {
    if (1..=26).contains(&prefix) {
        format!("C-{}", (b'a' + prefix - 1) as char)
    } else {
        format!("0x{prefix:02x}")
    }
}

/// `前缀 ?` 印的那一张（一行一个键，同 `tmux list-keys` 的样子）。
pub(crate) fn list_keys(prefix: u8) -> String {
    let p = prefix_label(prefix);
    TABLE
        .iter()
        .map(|(k, cmd)| {
            let key = if *k == "C-b" { p.as_str() } else { k };
            format!("bind-key -T prefix {key:<6} {cmd}    ({p} {key})")
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Plain,
    /// 按过前缀，等下一个键（转义序列收齐了才算一个键）。
    Prefixed(Vec<u8>),
    /// 问过「结束？」，等 y / 别的。
    Confirm,
}

/// 平常那一层：前缀与前缀之后那一个键。
pub(crate) struct Keys {
    prefix: u8,
    state: State,
}

impl Keys {
    pub(crate) fn new(prefix: u8) -> Self {
        Self {
            prefix,
            state: State::Plain,
        }
    }

    /// 喂外层终端读到的一块字节，交回要做的事（原样送的那几段合成一个 `Send`）。
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Vec<Act> {
        let mut acts: Vec<Act> = Vec::new();
        let mut plain: Vec<u8> = Vec::new();
        let flush = |plain: &mut Vec<u8>, acts: &mut Vec<Act>| {
            if !plain.is_empty() {
                acts.push(Act::Send(std::mem::take(plain)));
            }
        };
        for &b in bytes {
            match &mut self.state {
                State::Plain if b == self.prefix => {
                    flush(&mut plain, &mut acts);
                    self.state = State::Prefixed(Vec::new());
                }
                State::Plain => plain.push(b),
                State::Confirm => {
                    self.state = State::Plain;
                    acts.push(if b == b'y' || b == b'Y' { Act::End } else { Act::CancelEnd });
                }
                State::Prefixed(key) => {
                    key.push(b);
                    if !key_complete(key) {
                        continue;
                    }
                    let key = std::mem::take(key);
                    self.state = State::Plain;
                    match act_of(&key, self.prefix) {
                        Some(Act::AskEnd) => {
                            self.state = State::Confirm;
                            acts.push(Act::AskEnd);
                        }
                        Some(Act::Send(b)) => plain.extend_from_slice(&b),
                        Some(a) => acts.push(a),
                        None => {}
                    }
                }
            }
        }
        flush(&mut plain, &mut acts);
        acts
    }
}

/// 一个键收齐了没有：单字节 · `ESC [ … 终字节` · `ESC O x` · `ESC x`（Alt+x）。
fn key_complete(k: &[u8]) -> bool {
    match k {
        [] => false,
        [0x1b] => false,
        [0x1b, b'['] | [0x1b, b'O'] => false,
        [0x1b, b'[', rest @ ..] => rest.last().is_some_and(|c| (0x40..=0x7e).contains(c)),
        [0x1b, b'O', _] => true,
        [0x1b, _] => true,
        _ => true,
    }
}

/// tmux 的规矩：`VISUAL`（没有才看 `EDITOR`）里含 `vi` ⇒ 翻看用 vi 表。
pub(crate) fn vi_from(visual: Option<&str>, editor: Option<&str>) -> bool {
    visual
        .filter(|v| !v.is_empty())
        .or(editor)
        .is_some_and(|e| e.contains("vi"))
}

/// 翻看里的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Copy {
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
    Search { text: String, forward: bool },
    Next,
    Prev,
    Quit,
}

/// 翻看那一层的键。
pub(crate) struct CopyKeys {
    vi: bool,
    pending: Vec<u8>,
    /// 在输入搜的字：`Some((往下搜?, 字))`。
    typing: Option<(bool, String)>,
    shown: String,
}

impl CopyKeys {
    pub(crate) fn new(vi: bool) -> Self {
        Self {
            vi,
            pending: Vec::new(),
            typing: None,
            shown: String::new(),
        }
    }

    /// 搜的那一行此刻的样子（`/abc`）；没在输入 ⇒ `None`。
    pub(crate) fn prompt(&self) -> Option<&str> {
        self.typing.as_ref().map(|_| self.shown.as_str())
    }

    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Vec<Copy> {
        let mut out = Vec::new();
        for &b in bytes {
            if let Some((fwd, text)) = &mut self.typing {
                match b {
                    b'\r' | b'\n' => {
                        let (forward, text) = (*fwd, std::mem::take(text));
                        self.typing = None;
                        if !text.is_empty() {
                            out.push(Copy::Search { text, forward });
                        }
                    }
                    0x1b | 0x03 | 0x07 => self.typing = None,
                    0x7f | 0x08 => {
                        text.pop();
                    }
                    c if c >= 0x20 => text.push(c as char),
                    _ => {}
                }
                if let Some((fwd, text)) = &self.typing {
                    self.shown = format!("{}{text}", if *fwd { '/' } else { '?' });
                }
                continue;
            }
            self.pending.push(b);
            if !key_complete(&self.pending) {
                // 单独一个 ESC：vi 表里就是退出（后面没跟东西的那种由下一块判；块尾的 ESC 当退出）。
                continue;
            }
            let key = std::mem::take(&mut self.pending);
            if let Some(a) = self.act(&key) {
                out.push(a);
            }
        }
        // 块尾只剩一个 ESC：按 Esc 处理（外层终端把单独一个 Esc 键送成一个字节、不跟别的）。
        if self.pending == [0x1b] {
            self.pending.clear();
            out.push(Copy::Quit);
        }
        out
    }

    fn act(&mut self, key: &[u8]) -> Option<Copy> {
        let common = match key {
            b"\x1b[A" | b"\x1bOA" => Some(Copy::Up),
            b"\x1b[B" | b"\x1bOB" => Some(Copy::Down),
            b"\x1b[5~" => Some(Copy::PageUp),
            b"\x1b[6~" => Some(Copy::PageDown),
            b"q" => Some(Copy::Quit),
            b"n" => Some(Copy::Next),
            b"N" => Some(Copy::Prev),
            b"/" | b"?" => {
                let fwd = key == b"/";
                self.typing = Some((fwd, String::new()));
                self.shown = if fwd { "/".into() } else { "?".into() };
                return None;
            }
            _ => None,
        };
        if common.is_some() {
            return common;
        }
        if self.vi {
            match key {
                b"k" => Some(Copy::Up),
                b"j" => Some(Copy::Down),
                b"\x02" => Some(Copy::PageUp),
                b"\x06" => Some(Copy::PageDown),
                b"g" => Some(Copy::Top),
                b"G" => Some(Copy::Bottom),
                _ => None,
            }
        } else {
            match key {
                b"\x10" => Some(Copy::Up),
                b"\x0e" => Some(Copy::Down),
                b"\x1bv" => Some(Copy::PageUp),
                b"\x16" => Some(Copy::PageDown),
                b"\x1b<" => Some(Copy::Top),
                b"\x1b>" => Some(Copy::Bottom),
                b"\x07" => Some(Copy::Quit),
                _ => None,
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/host/keys_tests.rs"]
mod tests;
