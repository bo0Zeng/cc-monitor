//! xterm 那一套键码（与 `TERM=xterm-256color` 一致）：一下 ⇒ 送进终端的字节，与反过来。
//!
//! 全仓只此一张表：后端托管终端送键（`key_bytes`）· 手机键条送输入 · 软键盘带修饰 · 旧转义按钮迁移（经 JNI）都从这里出。
//! 编码 [`presses_bytes`] 与解码 [`presses_of`] 读同一张 [`KEYS`]；解码只多认几个别名（`~` 号 1/7 ＝ Home、4/8 ＝ End；
//! SS3 也认 CSI 字母那几颗），编码只出规范的那一形。

/// 一颗特殊键（手机软键盘打不出的那几颗）。线上名（手机库里存的字）见 [`Key::wire`]。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Esc,
    Tab,
    Enter,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

/// 一颗键送出去的那一形。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Form {
    /// 单字节（Esc · Tab · 回车 · 退格）：Alt 前缀 ESC；Shift-Tab 是 `ESC [ Z`；Ctrl 与别的 Shift 不变。
    Byte(u8),
    /// `ESC [ <字母>`，带修饰 `ESC [ 1 ; <参数> <字母>`。
    Letter(u8),
    /// `ESC O <字母>`，带修饰同 [`Form::Letter`]。
    Ss3(u8),
    /// `ESC [ <号> ~`，带修饰 `ESC [ <号> ; <参数> ~`。
    Tilde(u8),
}

/// ★ 那一张表：键 · 线上名 · 送出去的那一形。一颗一行。
const KEYS: [(Key, &str, Form); 25] = [
    (Key::Esc, "esc", Form::Byte(0x1b)),
    (Key::Tab, "tab", Form::Byte(0x09)),
    (Key::Enter, "enter", Form::Byte(0x0d)),
    (Key::Backspace, "backspace", Form::Byte(0x7f)),
    (Key::Delete, "delete", Form::Tilde(3)),
    (Key::Up, "up", Form::Letter(b'A')),
    (Key::Down, "down", Form::Letter(b'B')),
    (Key::Left, "left", Form::Letter(b'D')),
    (Key::Right, "right", Form::Letter(b'C')),
    (Key::Home, "home", Form::Letter(b'H')),
    (Key::End, "end", Form::Letter(b'F')),
    (Key::PageUp, "pgup", Form::Tilde(5)),
    (Key::PageDown, "pgdn", Form::Tilde(6)),
    (Key::F1, "f1", Form::Ss3(b'P')),
    (Key::F2, "f2", Form::Ss3(b'Q')),
    (Key::F3, "f3", Form::Ss3(b'R')),
    (Key::F4, "f4", Form::Ss3(b'S')),
    (Key::F5, "f5", Form::Tilde(15)),
    (Key::F6, "f6", Form::Tilde(17)),
    (Key::F7, "f7", Form::Tilde(18)),
    (Key::F8, "f8", Form::Tilde(19)),
    (Key::F9, "f9", Form::Tilde(20)),
    (Key::F10, "f10", Form::Tilde(21)),
    (Key::F11, "f11", Form::Tilde(23)),
    (Key::F12, "f12", Form::Tilde(24)),
];

/// 解码才认的 `~` 号别名（老终端的 Home · End）。
const TILDE_ALIASES: [(u8, Key); 4] = [(1, Key::Home), (4, Key::End), (7, Key::Home), (8, Key::End)];

const ESC: u8 = 0x1b;

impl Key {
    /// 表里的每一颗，照表的先后。
    pub const ALL: [Key; 25] = {
        let mut out = [Key::Esc; 25];
        let mut i = 0;
        while i < 25 {
            out[i] = KEYS[i].0;
            i += 1;
        }
        out
    };

    /// 线上名（小写英文）。
    pub fn wire(self) -> &'static str {
        row(self).1
    }

    /// [`Key::wire`] 的逆；不认得 ⇒ `None`。
    pub fn from_wire(s: &str) -> Option<Key> {
        KEYS.iter().find(|r| r.1 == s).map(|r| r.0)
    }
}

fn row(key: Key) -> &'static (Key, &'static str, Form) {
    KEYS.iter().find(|r| r.0 == key).expect("每颗键在表里恰一行")
}

fn form(key: Key) -> Form {
    row(key).2
}

/// 一下带的修饰。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// 修饰的线上名，照记号里的先后。
const MOD_WORDS: [&str; 3] = ["ctrl", "alt", "shift"];

impl Mods {
    fn any(self) -> bool {
        self.ctrl || self.alt || self.shift
    }

    /// xterm 的修饰参数：1 ＋ Shift(1) ＋ Alt(2) ＋ Ctrl(4)；不带修饰 ⇒ `None`。
    fn param(self) -> Option<u32> {
        self.any()
            .then(|| 1 + u32::from(self.shift) + 2 * u32::from(self.alt) + 4 * u32::from(self.ctrl))
    }

    /// [`Mods::param`] 的逆：没给 · 1 ⇒ 不带；出界 ⇒ `None`。
    fn from_param(p: Option<u32>) -> Option<Mods> {
        let bits = match p {
            None | Some(1) => return Some(Mods::default()),
            Some(p) => p.checked_sub(1)?,
        };
        (1..=7).contains(&bits).then_some(Mods {
            shift: bits & 1 != 0,
            alt: bits & 2 != 0,
            ctrl: bits & 4 != 0,
        })
    }

    fn flags(self) -> [bool; 3] {
        [self.ctrl, self.alt, self.shift]
    }
}

/// 「送输入」里的一下。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Press {
    /// 照原样打出的一段字。
    Text(String),
    /// 一颗特殊键，可带修饰（`Shift-Tab` · `Ctrl-End`）。
    Special(Key, Mods),
    /// 带修饰的一个字（`Ctrl-B` · `Alt-x`）。
    Chord(char, Mods),
}

impl Press {
    /// 一下的记号（JNI 两侧与金样同这一形）：`t<字>` · `s<修饰>:<键>` · `c<修饰>:<字>`，修饰是线上名的逗号连写（照 ctrl · alt · shift 的先后）。
    pub fn token(&self) -> String {
        let mods = |m: Mods| {
            MOD_WORDS
                .iter()
                .zip(m.flags())
                .filter(|(_, on)| *on)
                .map(|(w, _)| *w)
                .collect::<Vec<_>>()
                .join(",")
        };
        match self {
            Press::Text(t) => format!("t{t}"),
            Press::Special(k, m) => format!("s{}:{}", mods(*m), k.wire()),
            Press::Chord(c, m) => format!("c{}:{c}", mods(*m)),
        }
    }

    /// [`Press::token`] 的逆；形不对 · 名不认得 · 修饰没照先后 ⇒ `None`。
    pub fn from_token(s: &str) -> Option<Press> {
        if let Some(text) = s.strip_prefix('t') {
            return Some(Press::Text(text.to_string()));
        }
        let kind = s.chars().next()?;
        let (mods, rest) = s.get(1..)?.split_once(':')?;
        let mods = if mods.is_empty() {
            Mods::default()
        } else {
            let words: Vec<&str> = mods.split(',').collect();
            let on = |w: &str| words.contains(&w);
            let m = Mods {
                ctrl: on("ctrl"),
                alt: on("alt"),
                shift: on("shift"),
            };
            let p = Press::Special(Key::Esc, m);
            (p.token() == format!("s{mods}:esc")).then_some(m)?
        };
        let press = match kind {
            's' => Press::Special(Key::from_wire(rest)?, mods),
            'c' => {
                let mut cs = rest.chars();
                let c = cs.next()?;
                if cs.next().is_some() {
                    return None;
                }
                Press::Chord(c, mods)
            }
            _ => return None,
        };
        Some(press)
    }
}

/// Ctrl 组合的标准控制码（xterm 语义）：字母（大小写同码）与 `@ [ \ ] ^ _` 取 `& 0x1F`；空格 ⇒ NUL；`?` ⇒ DEL。
/// 别的（数字 · 标点）没有标准 Ctrl 码 ⇒ `None`：不能盲取 `& 0x1F`，Ctrl-1 · Ctrl-3 会成 XON · XOFF、冻住终端输出。
fn ctrl_code(b: u8) -> Option<u8> {
    match b {
        b'a'..=b'z' | b'A'..=b'Z' | b'@' | b'['..=b'_' => Some(b & 0x1f),
        b' ' => Some(0x00),
        b'?' => Some(0x7f),
        _ => None,
    }
}

/// 一个字带修饰：只对单字节的字生效（多字节的字原样）。Shift ＝ 小写字母变大写；Ctrl ＝ [`ctrl_code`]；Alt ＝ 前缀 ESC。叠加序 Shift → Ctrl → Alt。
fn chord_bytes(c: char, m: Mods, out: &mut Vec<u8>) {
    let mut buf = [0u8; 4];
    let raw = c.encode_utf8(&mut buf).as_bytes();
    if !m.any() || raw.len() != 1 {
        out.extend_from_slice(raw);
        return;
    }
    let mut b = raw[0];
    if m.shift {
        b = b.to_ascii_uppercase();
    }
    if m.ctrl {
        b = ctrl_code(b).unwrap_or(b);
    }
    if m.alt {
        out.push(ESC);
    }
    out.push(b);
}

fn special_bytes(k: Key, m: Mods, out: &mut Vec<u8>) {
    let csi = |out: &mut Vec<u8>, head: &str, fin: u8| {
        out.extend_from_slice(&[ESC, b'[']);
        out.extend_from_slice(head.as_bytes());
        out.push(fin);
    };
    match (form(k), m.param()) {
        (Form::Byte(0x09), _) if m.shift => out.extend_from_slice(&[ESC, b'[', b'Z']),
        (Form::Byte(b), _) => {
            if m.alt {
                out.push(ESC);
            }
            out.push(b);
        }
        (Form::Letter(c) | Form::Ss3(c), Some(p)) => csi(out, &format!("1;{p}"), c),
        (Form::Letter(c), None) => csi(out, "", c),
        (Form::Ss3(c), None) => out.extend_from_slice(&[ESC, b'O', c]),
        (Form::Tilde(n), Some(p)) => csi(out, &format!("{n};{p}"), b'~'),
        (Form::Tilde(n), None) => csi(out, &n.to_string(), b'~'),
    }
}

/// 几下 ⇒ 送进终端的字节，一下接一下拼起来：字照 UTF-8；带修饰的字照 [`chord_bytes`]；特殊键照表。
pub fn presses_bytes(presses: &[Press]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in presses {
        match p {
            Press::Text(t) => out.extend_from_slice(t.as_bytes()),
            Press::Chord(c, m) => chord_bytes(*c, *m, &mut out),
            Press::Special(k, m) => special_bytes(*k, *m, &mut out),
        }
    }
    out
}

/// 控制字节 ⇒ 一下：Tab · 回车（`\n` 与 `\r`）· 退格；别的 ⇒ Ctrl-字（[`ctrl_code`] 的逆，字母取小写）。
fn control_press(b: u8) -> Option<Press> {
    let single = KEYS.iter().find(|r| r.2 == Form::Byte(b) && r.0 != Key::Esc);
    if let Some(r) = single {
        return Some(Press::Special(r.0, Mods::default()));
    }
    if b == 0x0a {
        return Some(Press::Special(Key::Enter, Mods::default()));
    }
    (b < 0x20 && b != ESC).then(|| {
        Press::Chord(
            char::from(b | 0x40).to_ascii_lowercase(),
            Mods {
                ctrl: true,
                ..Mods::default()
            },
        )
    })
}

/// `ESC [` 之后（`from` 指参数头）的一段 ⇒ 一下与下一个位置；认不出 ⇒ `None`。
fn csi(bytes: &[u8], from: usize) -> Option<(Press, usize)> {
    let end = from + bytes[from..].iter().take_while(|b| b.is_ascii_digit() || **b == b';').count();
    let fin = *bytes.get(end)?;
    let params: Vec<u32> = std::str::from_utf8(&bytes[from..end])
        .ok()?
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().ok())
        .collect::<Option<_>>()?;
    if params.len() > 2 {
        return None;
    }
    if fin == b'Z' {
        let p = Press::Special(
            Key::Tab,
            Mods {
                shift: true,
                ..Mods::default()
            },
        );
        return params.is_empty().then_some((p, end + 1));
    }
    let key = if fin == b'~' {
        let n = u8::try_from(*params.first()?).ok()?;
        KEYS.iter()
            .find(|r| r.2 == Form::Tilde(n))
            .map(|r| r.0)
            .or_else(|| TILDE_ALIASES.iter().find(|a| a.0 == n).map(|a| a.1))?
    } else {
        if params.first().is_some_and(|p| *p != 1) {
            return None;
        }
        KEYS.iter()
            .find(|r| r.2 == Form::Letter(fin) || r.2 == Form::Ss3(fin))
            .map(|r| r.0)?
    };
    let mods = Mods::from_param(params.get(1).copied())?;
    Some((Press::Special(key, mods), end + 1))
}

/// ESC 起的一下：CSI · SS3 · Alt-字 · 单独的 Esc。
fn esc_press(bytes: &[u8], i: usize) -> Option<(Press, usize)> {
    let n = bytes.get(i + 1).copied();
    let ss3 = (n == Some(b'O'))
        .then(|| bytes.get(i + 2))
        .flatten()
        .and_then(|c| KEYS.iter().find(|r| r.2 == Form::Ss3(*c) || r.2 == Form::Letter(*c)))
        .map(|r| r.0);
    match (n, ss3) {
        (Some(b'['), _) => csi(bytes, i + 2),
        (_, Some(k)) => Some((Press::Special(k, Mods::default()), i + 3)),
        (Some(c @ 0x21..=0x7e), _) => Some((
            Press::Chord(
                char::from(c),
                Mods {
                    alt: true,
                    ..Mods::default()
                },
            ),
            i + 2,
        )),
        _ => Some((Press::Special(Key::Esc, Mods::default()), i + 1)),
    }
}

/// 一串送进终端的字节 ⇒ 几下：认得 CSI / SS3 的特殊键（带 xterm 修饰）· `ESC [ Z` · Alt-字 · 单独的 Esc · 控制字节 · 可打印的字（UTF-8，连着的并成一段）。
/// 别的一概认不出 ⇒ `None`（坏 UTF-8 也是）。
pub fn presses_of(bytes: &[u8]) -> Option<Vec<Press>> {
    let mut out: Vec<Press> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let (press, next) = if b == ESC {
            esc_press(bytes, i)?
        } else if b < 0x20 || b == 0x7f {
            (control_press(b)?, i + 1)
        } else {
            let end = i + bytes[i..].iter().take_while(|c| **c >= 0x20 && **c != 0x7f).count();
            (Press::Text(std::str::from_utf8(&bytes[i..end]).ok()?.to_string()), end)
        };
        match (out.last_mut(), press) {
            (Some(Press::Text(last)), Press::Text(more)) => last.push_str(&more),
            (_, press) => out.push(press),
        }
        i = next;
    }
    Some(out)
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/keys_tests.rs"]
mod tests;
