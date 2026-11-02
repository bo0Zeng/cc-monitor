//! ↗ 的本机记录：cc-monitor 给远端会话开终端那一刻，跑那一行的那个 shell 把自己的窗口标签写下来；↗ 时本机先照它找窗口，对不上才问那台。
//!
//! - 写：开终端那一行（`dial/terminal.rs`，帧命令 `terminal-ssh` 交了 `sid`）前面接一段前奏、后面接一句收尾（[`window`]）：
//!   那个 shell 先把 `<自己的进程号>-<起始时刻>`（与接入块设的 `LC_CCM_WINDOW` 同一个键）和那台的名字写进 `<数据目录>/opened-windows/<sid>`，
//!   跑那一行（ssh），ssh 一退就删掉它。记录在 ＝ 开窗那条 ssh 还开着。
//!   目录由后端渲那一行时建好（[`dir`]）；写不成不挡开终端（前奏吞掉错误）。
//! - 读：`session-window`（`faces/session_window_face.rs`）问远端会话时先读这一份（[`Records`]），
//!   标签那个 shell 在这台还活着、它自己或底下开着 ssh ⇒ 从那个 ssh 往上数链（`dial/terminal_processes.rs::opened_chain`），不问那台；
//!   shell 没了 ⇒ 删这一份。
//! - 扫：每回建那一层目录（开新终端）时删掉 shell 已经不在的那几份（[`sweep`]），从没被 ↗ 问过的也不留。
//! - 一份两行：标签 · 那台的名字。换行 LF / CRLF 都认。认不出 ⇒ 当没有。

use crate::common::seen_terminal::WindowLabel;
use crate::platform::fs::Ro;
use crate::platform::shell::dialect::ps_literal;
use crate::platform::Own;
use copy_core::{Discard, Why};
use relay_route_core::{Cap, Over};
use std::path::{Path, PathBuf};

/// 数据目录底下的那一层。
pub(crate) const DIR: &str = "opened-windows";

/// 一份记录的读上限（两行：标签 · 机器名）；超了 ⇒ 日志里说是哪一份，当没有（照问那台）。
const MAX_BYTES: Cap = Cap {
    bytes: 4096,
    over: Over::SkipAndSay,
};

/// 一份记录：开那一行的那个 shell 的窗口标签 · 会话在哪台。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Opened {
    pub(crate) label: WindowLabel,
    pub(crate) machine: String,
}

/// 一份记录的内容 ⇒ 记录（第一行是标签、第二行不空）。
pub(crate) fn parse(body: &str) -> Option<Opened> {
    let mut lines = body.lines().map(str::trim);
    let label = lines.next()?.parse().ok()?;
    let machine = lines.next().filter(|m| !m.is_empty())?;
    Some(Opened {
        label,
        machine: machine.to_string(),
    })
}

/// 前奏：跑这一行的那个 shell 把自己的标签与 `machine` 写进 `file`（先写旁名 `.part` 再挪过去），再接后面那一行。
/// 写法按本机终端方言，住 `platform/shell/{posix, powershell}.rs::note_own_window`；三个词经本方言的单引号字面量。
fn prelude(powershell: bool, file: &Path, machine: &str) -> String {
    let file = file.to_string_lossy();
    let part = format!("{file}.part");
    if powershell {
        crate::platform::shell::powershell::note_own_window(
            &ps_literal(&file),
            &ps_literal(&part),
            &ps_literal(machine),
        )
    } else {
        crate::platform::shell::posix::note_own_window(
            &shell_quote_core::posix_quote(&file),
            &shell_quote_core::posix_quote(&part),
            &shell_quote_core::posix_quote(machine),
        )
    }
}

/// 开窗那一串：[`prelude`] ＋ `line` ＋ 收尾（`line` 跑完删掉 `file`，写法住 `platform/shell/{posix, powershell}.rs::then_forget_own_window`）。
/// 于是「记录在」＝「开窗那条 ssh 还开着」：那条 ssh 怎么退的都一样，退了记录就没了；窗口整个关掉、收尾没跑 ⇒ shell 也没了，[`sweep`] 删。
pub(crate) fn window(powershell: bool, file: &Path, machine: &str, line: &str) -> String {
    let head = prelude(powershell, file, machine);
    let path = file.to_string_lossy();
    let tail = if powershell {
        crate::platform::shell::powershell::then_forget_own_window(line, &ps_literal(&path))
    } else {
        crate::platform::shell::posix::then_forget_own_window(
            line,
            &shell_quote_core::posix_quote(&path),
        )
    };
    format!("{head}{tail}")
}

/// 那一层目录（渲开终端那一行时建好，0700）；建不成 ⇒ `None`（那一行不带前奏）。
/// 建好顺手读一趟这台的进程表、[`sweep`] 掉 shell 已经不在的那几份；读不成就不扫。
pub(crate) fn dir(own: &Own) -> Option<PathBuf> {
    let made = (|| {
        let d = own.root()?.join(DIR).ok()?;
        own.own_dir(&d).ok()?;
        Some(d.as_path().to_path_buf())
    })();
    if made.is_none() {
        tracing::warn!("opened-windows：那一层目录没建成，开终端那一行不带前奏");
        return None;
    }
    match crate::platform::connection_and_process_tables()
        .and_then(|raw| crate::dial::terminal_processes::parse_tables(&raw))
    {
        Ok(t) => sweep(own, &t.procs),
        Err(e) => tracing::debug!("opened-windows：进程表没读成，这一回不扫：{e}"),
    }
    made
}

/// 删掉标签那个 shell 已经不在的那几份（窗口关了、从没被 ↗ 问过的也一样）：记录不随用随长。
/// 只看名字是会话 id 的那几份（旁名 `.part` 是正写到一半的，不动）；内容认不出的当坏的删。
/// 进程表是空的 ⇒ 是没读成，一份都不删。
pub(crate) fn sweep(
    own: &Own,
    procs: &std::collections::HashMap<u32, crate::dial::terminal_processes::Proc>,
) {
    if procs.is_empty() {
        return;
    }
    let Some(d) = own.root().and_then(|r| r.join(DIR).ok()) else {
        return;
    };
    let records = OnDisk(own.clone());
    let Ok(entries) = Ro::read_dir(d.as_path()) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(sid) = name.to_str().filter(|s| shell_quote_core::session_id_ok(s)) else {
            continue;
        };
        let alive = records
            .get(sid)
            .is_some_and(|r| crate::dial::terminal_processes::shell_still_there(&r.label, procs));
        if !alive {
            records.forget(sid);
        }
    }
}

/// 读 · 删记录（判据喂替身）。
pub(crate) trait Records: Send + 'static {
    /// `sid` 那一份；没有 · 认不出 ⇒ `None`。
    fn get(&self, sid: &str) -> Option<Opened>;
    /// 那个 shell 没了：删 `sid` 那一份。
    fn forget(&self, sid: &str);
}

/// 生产那一份：数据目录底下的 [`DIR`]。
pub(crate) struct OnDisk(pub(crate) Own);

impl OnDisk {
    fn file(&self, sid: &str) -> Option<crate::platform::fs::OwnPath> {
        if !shell_quote_core::session_id_ok(sid) {
            return None;
        }
        self.0.root()?.join(DIR).ok()?.join(sid).ok()
    }
}

impl Records for OnDisk {
    fn get(&self, sid: &str) -> Option<Opened> {
        let f = self.file(sid)?;
        // 不在 ⇒ 没记过；超上限那一档在读里 `warn!` 是哪一份。
        parse(&Ro::read_string_capped(f.as_path(), MAX_BYTES).ok()?)
    }

    fn forget(&self, sid: &str) {
        if let Some(f) = self.file(sid) {
            self.0.delete_file(&f).discard(Why::CleanupAfterFailure); // 删不掉下一次 ↗ 再清；它对不上进程，不会被认
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_opened_windows_tests.rs"]
mod tests;
