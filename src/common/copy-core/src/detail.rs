//! **一条报错的「复制详情」那几行** —— 出错的那一端（后端 · monitor 壳 · 文件窗口进程）各自写，全仓只这一份排法。
//!
//! 线上那一格 `detail` 只装界面那句**下面**的几行：「项名：值」，项名是闭集 [`Label`]（文案表 `detail.label.*`），
//! 只列有值的项；界面复制时把它接在自己显示的那句下面（首行永远就是屏上那句，不会两处各写一份而对不上）。
//!
//! 不进这里的：会话内容、key / token、请求参数值（命令只写名）。原话截 [`RAW_CAP`] 字节，截了在末尾标「…（截断）」。
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
    /// 用户自己的对象（会话名 · 账号名）。
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

/// 写一份详情：按调用次序一项一行；值是空白的项不出。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Detail {
    lines: Vec<String>,
}

impl Detail {
    pub fn new() -> Detail {
        Detail::default()
    }

    /// 加一项；值去掉首尾空白后是空的 ⇒ 不加。原话（[`Label::Raw`]）截到 [`RAW_CAP`]。
    pub fn item(mut self, label: Label, value: impl AsRef<str>) -> Detail {
        if let Some(line) = line_of(label, value.as_ref()) {
            self.lines.push(line);
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

    /// 排成线上那一格（行间 `\n`，无首尾空行）。
    pub fn render(&self) -> String {
        self.lines.join("\n")
    }
}

/// 已经写好的一份详情后面再接一项（本机壳给转交来的远端详情补「本机」那一行用）。值空 ⇒ 原样。
pub fn append(detail: &str, label: Label, value: &str) -> String {
    match line_of(label, value) {
        Some(line) if detail.trim().is_empty() => line,
        Some(line) => format!("{}\n{line}", detail.trim_end()),
        None => detail.to_string(),
    }
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
