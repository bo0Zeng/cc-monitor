//! **一条报错的「复制详情」那几行** —— 出错的那一端（后端 · monitor 壳 · 文件窗口进程）各自写，全仓只这一份排法。
//!
//! 线上那一格 `detail` 只装界面那句**下面**的几行：「项名：值」，项名是闭集 [`Label`]（文案表 `detail.label.*`），
//! 只列有值的项；界面复制时把它接在自己显示的那句下面（首行永远就是屏上那句，不会两处各写一份而对不上）。
//!
//! 不进这里的：会话内容（会话标题也算：它常常就是用户的第一句话 ⇒「对象」只收标识，[`Target`]）、key / token、请求参数值（命令只写名）。原话截 [`RAW_CAP`] 字节，截了在末尾标「…（截断）」。
//! 项名与值之间用全角冒号、不靠空格对齐（中文在等宽字体下占宽不定，贴到别处会乱）。

use crate::copy_text;

/// 原话最多留多少字节（按字符边界往回退）。
pub const RAW_CAP: usize = 4096;

/// 「项名：值」的项名闭集（文案表 `detail.label.*` 与它两向相等，判据住 `tests/common/copy-core/detail_tests.rs`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    /// 出错那一端的本地时刻（[`stamp`]）。
    At,
    /// 出错的那台：系统 架构 · 版本 (构建)；没连上 ⇒ 名 ＋「（未连上）」。
    Machine,
    /// 本机：远端失败经本机转交时由本机壳补一行（出错的就是本机时不出）。
    Local,
    /// 命令名（不带参数值）。
    Command,
    /// 用户自己的路径。
    Path,
    /// 对象：哪个会话 / 账号 / 模块 / 机器。只经 [`Detail::target`] 写（只收标识，不收会话标题）。
    Target,
    /// 通道断在哪一跳 · 发没发出。
    Hop,
    /// 命令级码 / 系统错误码 / 退出码。
    Code,
    /// 子进程 stderr / 系统报错原文。
    Raw,
}

impl Label {
    /// 全部项名，显示次序。
    pub const ALL: [Label; 9] = [
        Label::At,
        Label::Machine,
        Label::Local,
        Label::Command,
        Label::Path,
        Label::Target,
        Label::Hop,
        Label::Code,
        Label::Raw,
    ];

    /// 这一项给人看的名字（文案表）。
    pub fn said(self) -> String {
        match self {
            Label::At => copy_text("detail.label.at", &[]),
            Label::Machine => copy_text("detail.label.machine", &[]),
            Label::Local => copy_text("detail.label.local", &[]),
            Label::Command => copy_text("detail.label.command", &[]),
            Label::Path => copy_text("detail.label.path", &[]),
            Label::Target => copy_text("detail.label.target", &[]),
            Label::Hop => copy_text("detail.label.hop", &[]),
            Label::Code => copy_text("detail.label.code", &[]),
            Label::Raw => copy_text("detail.label.raw", &[]),
        }
    }
}

/// 「对象」那一项写什么：**只收标识**。会话不写标题（标题常常就是用户的第一句话，详情不含会话内容），写机器 ＋ sid 前 8 位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    /// 哪台上的哪个会话。
    Session { machine: &'a str, sid: &'a str },
    /// 账号名（路由第 2 段那个名字）。
    Account(&'a str),
    /// 出错的模块（日志事件的来源）。
    Module(&'a str),
    /// 连的是哪一台（机器名）。
    Machine(&'a str),
}

/// sid 留几位（够认、不长）。
const SID_SHOWN: usize = 8;

impl Target<'_> {
    fn line(&self) -> String {
        match self {
            Target::Session { machine, sid } => {
                let short: String = sid.chars().take(SID_SHOWN).collect();
                copy_text(
                    "detail.target.session",
                    &[("machine", machine), ("sid", &short)],
                )
            }
            Target::Account(s) | Target::Module(s) | Target::Machine(s) => s.to_string(),
        }
    }
}

/// 「对象」不收自由文本（见 [`Target`]）。
fn no_free_target(label: Label) {
    assert!(
        label != Label::Target,
        "「对象」那一项只经 Detail::target 写（只收标识，不收会话标题）"
    );
}

/// 写一份详情：按调用次序一项一行；值是空白的项不出。补一项按项名次序插（[`Detail::insert`]）；
/// 对端写好的一整份原样当一块（[`Detail::block`]）—— 不拆渲染好的字。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Detail {
    items: Vec<Item>,
}

/// 详情里的一项：自己写的一行（项名 ＋ 那一行）· 对端写好的一整份（原样）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    Line(Label, String),
    Block(String),
}

impl Detail {
    pub fn new() -> Detail {
        Detail::default()
    }

    /// 加一项；值去掉首尾空白后是空的 ⇒ 不加。原话（[`Label::Raw`]）截到 [`RAW_CAP`]。
    /// 「对象」不走这里（自由文本进不来）：拿 [`Label::Target`] 当场拒，改走 [`Detail::target`]。
    pub fn item(mut self, label: Label, value: impl AsRef<str>) -> Detail {
        no_free_target(label);
        if let Some(line) = line_of(label, value.as_ref()) {
            self.items.push(Item::Line(label, line));
        }
        self
    }

    /// 同 [`Detail::item`]，值可缺。
    pub fn maybe(self, label: Label, value: Option<impl AsRef<str>>) -> Detail {
        match value {
            Some(v) => self.item(label, v),
            None => self,
        }
    }

    /// 「对象」那一项（按项名次序插，同 [`Detail::insert`]）。
    pub fn target(mut self, t: Target<'_>) -> Detail {
        let value = t.line();
        if let Some(line) = line_of(Label::Target, &value) {
            let rank = |l: Label| {
                Label::ALL
                    .iter()
                    .position(|x| *x == l)
                    .unwrap_or(usize::MAX)
            };
            let at = self
                .items
                .iter()
                .position(|i| matches!(i, Item::Line(l, _) if rank(*l) > rank(Label::Target)))
                .unwrap_or(self.items.len());
            self.items.insert(at, Item::Line(Label::Target, line));
        }
        self
    }

    /// 对端写好的一整份（远端后端那份转交给界面时，本机壳在它后面补「本机」那一行）。空白 ⇒ 不加。
    pub fn block(mut self, written: &str) -> Detail {
        let w = written.trim_end();
        if !w.trim().is_empty() {
            self.items.push(Item::Block(w.to_string()));
        }
        self
    }

    /// 读回一份按本模块排法写好的详情（对端 / 本机后端写的）：行首是项名的起一项，别的行接在上一项后面；
    /// 「原话」排在最后（[`Label::ALL`]），读到它之后的行一律算原话的续行 —— 原话里恰好有一行以项名打头也不会被拆成别的项。
    /// 开头不是项名的那几行原样当一块。
    pub fn parse(written: &str) -> Detail {
        let mut d = Detail::new();
        let mut raw = false;
        for line in written.trim_end().lines() {
            let label = (!raw)
                .then(|| {
                    Label::ALL
                        .into_iter()
                        .find(|l| line.starts_with(&format!("{}：", l.said())))
                })
                .flatten();
            match (label, d.items.last_mut()) {
                (Some(l), _) => {
                    raw = l == Label::Raw;
                    d.items.push(Item::Line(l, line.to_string()));
                }
                (None, Some(Item::Line(_, s) | Item::Block(s))) => {
                    s.push('\n');
                    s.push_str(line);
                }
                (None, None) => d.items.push(Item::Block(line.to_string())),
            }
        }
        d
    }

    /// 补一项，按 [`Label::ALL`] 的次序插在第一条后排项之前（没有后排项 ⇒ 末尾）；值空 ⇒ 原样。
    pub fn insert(mut self, label: Label, value: impl AsRef<str>) -> Detail {
        no_free_target(label);
        let Some(line) = line_of(label, value.as_ref()) else {
            return self;
        };
        let rank = |l: Label| {
            Label::ALL
                .iter()
                .position(|x| *x == l)
                .unwrap_or(usize::MAX)
        };
        let at = self
            .items
            .iter()
            .position(|i| matches!(i, Item::Line(l, _) if rank(*l) > rank(label)))
            .unwrap_or(self.items.len());
        self.items.insert(at, Item::Line(label, line));
        self
    }

    /// 自己写的那几行里有没有这一项。
    pub fn has(&self, label: Label) -> bool {
        self.items
            .iter()
            .any(|i| matches!(i, Item::Line(l, _) if *l == label))
    }

    /// 自己写的那一行里这一项的值（原话截过的那一形）；没有 ⇒ `None`。
    pub fn value(&self, label: Label) -> Option<&str> {
        let prefix = format!("{}：", label.said());
        self.items.iter().find_map(|i| match i {
            Item::Line(l, s) if *l == label => s.strip_prefix(prefix.as_str()),
            _ => None,
        })
    }

    /// 对端写好的那一整份（有几块接起来）；没有 ⇒ `None`。
    pub fn written(&self) -> Option<&str> {
        self.items.iter().find_map(|i| match i {
            Item::Block(s) => Some(s.as_str()),
            _ => None,
        })
    }

    /// 有没有对端写好的一整份（那一份里有什么由写它的那一端负责）。
    pub fn has_block(&self) -> bool {
        self.items.iter().any(|i| matches!(i, Item::Block(_)))
    }

    /// 排成线上那一格（行间 `\n`，无首尾空行）。
    pub fn render(&self) -> String {
        self.items
            .iter()
            .map(|i| match i {
                Item::Line(_, s) | Item::Block(s) => s.as_str(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// 通道这一跳断了的那一份（monitor 壳与文件窗口进程同一份）：时刻 · 机器（`not_sent` ⇒ 名 ＋「（未连上）」）·
/// 本机（远端时给；出错的就是本机时不给）· 命令 · 断在（可缺）· 码。那几项的取法住通信层 `HopFacts`。
pub fn channel(
    at: &str,
    machine: &str,
    not_sent: bool,
    local: Option<&str>,
    command: &str,
    hop: Option<&str>,
    code: &str,
) -> Detail {
    let machine = if not_sent {
        format!(
            "{machine}（{}）",
            copy_text("detail.value.notConnected", &[])
        )
    } else {
        machine.to_string()
    };
    Detail::new()
        .item(Label::At, at)
        .item(Label::Machine, machine)
        .maybe(Label::Local, local)
        .item(Label::Command, command)
        .maybe(Label::Hop, hop)
        .item(Label::Code, code)
}

/// 一行汇总 `head` 底下几件各自的失败 `(那件的那一句, 它的详情)` ⇒ 复制出去的整段：首行 `head`，每件一段（那一句 ＋ 详情），
/// 段间空一行；详情空的那件不出段；一件都没有 ⇒ `None`（不出按钮）。界面那一侧同一排法（`kit/detail.ts::detailMany`，跨语言金样 `detail-many.golden.json`）。
pub fn many(head: &str, segments: &[(String, String)]) -> Option<String> {
    let segs: Vec<String> = segments
        .iter()
        .filter(|(_, d)| !d.trim().is_empty())
        .map(|(said, d)| format!("{said}\n{}", d.trim_end()))
        .collect();
    (!segs.is_empty()).then(|| format!("{head}\n\n{}", segs.join("\n\n")))
}

fn line_of(label: Label, value: &str) -> Option<String> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    let v = if label == Label::Raw {
        truncate_raw(v)
    } else {
        v.to_string()
    };
    Some(format!("{}：{v}", label.said()))
}

/// 原话截到 [`RAW_CAP`] 字节（按字符边界往回退），截了末尾接「…（截断）」。
pub fn truncate_raw(raw: &str) -> String {
    if raw.len() <= RAW_CAP {
        return raw.to_string();
    }
    let mut end = RAW_CAP;
    while !raw.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}{}",
        &raw[..end],
        copy_text("detail.value.truncated", &[])
    )
}

/// 一个时刻（unix 秒）按给定的本地偏移（东正、秒）写成 `YYYY-MM-DD HH:MM:SS +08:00`。偏移读不出的调用方传 0（写成 `+00:00`，不猜）。
pub fn stamp(unix_secs: i64, offset_secs: i64) -> String {
    let local = unix_secs + offset_secs;
    let (y, m, d) = crate::civil_from_days(local.div_euclid(86_400));
    let s = local.rem_euclid(86_400);
    let sign = if offset_secs < 0 { '-' } else { '+' };
    let off = offset_secs.unsigned_abs() / 60;
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} {sign}{:02}:{:02}",
        s / 3600,
        s % 3600 / 60,
        s % 60,
        off / 60,
        off % 60
    )
}

/// 「机器」那一项的值：`系统 架构 · 后端 构建` / `系统 架构 · cc-monitor 版本 (构建)` 这一类由调用方给齐，这里只把系统名写成人认的样子。
pub fn os_word(os: &str) -> &str {
    match os {
        "linux" => "Linux",
        "macos" => "macOS",
        "windows" => "Windows",
        "freebsd" => "FreeBSD",
        other => other,
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/copy-core/detail_tests.rs"]
mod detail_tests;
