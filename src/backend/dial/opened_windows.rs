//! ↗ 的本机记录：cc-monitor 给远端会话开终端那一刻，跑那一行的那个 shell 把自己的窗口标签写下来；↗ 时本机先照它找窗口，对不上才问那台。
//!
//! - 写：开终端那一行（`dial/terminal.rs`，帧命令 `terminal-ssh` 交了 `sid`）前面接一段前奏（[`prelude`]），
//!   那个 shell 把 `<自己的进程号>-<起始时刻>`（与接入块设的 `LC_CCM_WINDOW` 同一个键）和那台的名字写进 `<数据目录>/opened-windows/<sid>`。
//!   目录由后端渲那一行时建好（[`dir`]）；写不成不挡开终端（前奏吞掉错误）。
//! - 读：`session-window`（`faces/session_window_face.rs`）问远端会话时先读这一份（[`Records`]），
//!   标签那个 shell 在这台还活着、底下开着 ssh ⇒ 从那个 ssh 往上数链（`dial/terminal_processes.rs::opened_chain`），不问那台；
//!   shell 没了 ⇒ 删这一份。
//! - 一份两行：标签 · 那台的名字。换行 LF / CRLF 都认。认不出 ⇒ 当没有。

use crate::platform::fs::Ro;
use crate::platform::shell::dialect::ps_literal;
use crate::platform::Own;
use std::path::{Path, PathBuf};

/// 数据目录底下的那一层。
pub(crate) const DIR: &str = "opened-windows";

/// 一份记录的读上限（两行：标签 · 机器名）。
const MAX_BYTES: u64 = 4096;

/// 一份记录：开那一行的那个 shell 的窗口标签（`<进程号>-<起始时刻>`）· 会话在哪台。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Opened {
    pub(crate) label: String,
    pub(crate) machine: String,
}

/// 一份记录的内容 ⇒ 记录（两行都要有、都不空）。
pub(crate) fn parse(body: &str) -> Option<Opened> {
    let mut lines = body.lines().map(str::trim);
    let label = lines.next().filter(|l| !l.is_empty())?;
    let machine = lines.next().filter(|m| !m.is_empty())?;
    Some(Opened {
        label: label.to_string(),
        machine: machine.to_string(),
    })
}

/// 标签 ⇒ (进程号, 起始时刻)。
pub(crate) fn label_parts(label: &str) -> Option<(u32, u64)> {
    let (pid, start) = label.split_once('-')?;
    Some((pid.parse().ok()?, start.parse().ok()?))
}

/// 前奏：跑这一行的那个 shell 把自己的标签与 `machine` 写进 `file`（先写旁名 `.part` 再挪过去；UTF-8 无 BOM），再接后面那一行。本机终端方言：
/// PowerShell `$PID` ＋ `StartTime.ToFileTime()`（在一个脚本块里跑，变量不留在那个窗口里）；POSIX `$$` ＋ `/proc/$$/stat` 第 22 格（没有 `/proc` ⇒ 不写）。
/// 错误都吞掉，不往那个终端里打字。两个路径与机器名都经本方言的单引号字面量；PowerShell 那一形不带双引号。
pub(crate) fn prelude(powershell: bool, file: &Path, machine: &str) -> String {
    let file = file.to_string_lossy();
    let part = format!("{file}.part");
    if powershell {
        format!(
            "try {{ & {{ param($f, $t, $m) $ErrorActionPreference = 'Stop'; \
             [System.IO.File]::WriteAllText($t, ('{{0}}-{{1}}' -f $PID, [System.Diagnostics.Process]::GetCurrentProcess().StartTime.ToFileTime()) + [char]10 + $m + [char]10); \
             Move-Item -LiteralPath $t -Destination $f -Force }} {file} {part} {machine} }} catch {{}}; ",
            file = ps_literal(&file),
            part = ps_literal(&part),
            machine = ps_literal(machine),
        )
    } else {
        format!(
            "{{ [ -r /proc/$$/stat ] && printf '%s-%s\\n%s\\n' \"$$\" \"$(sed 's/.*) //' /proc/$$/stat | cut -d' ' -f20)\" {machine} > {part} && mv -f {part} {file}; }} 2>/dev/null; ",
            file = shell_quote_core::posix_quote(&file),
            part = shell_quote_core::posix_quote(&part),
            machine = shell_quote_core::posix_quote(machine),
        )
    }
}

/// 那一层目录（渲开终端那一行时建好，0700）；建不成 ⇒ `None`（那一行不带前奏）。
pub(crate) fn dir(own: &Own) -> Option<PathBuf> {
    let made = (|| {
        let d = own.root()?.join(DIR).ok()?;
        own.own_dir(&d).ok()?;
        Some(d.as_path().to_path_buf())
    })();
    if made.is_none() {
        tracing::warn!("opened-windows：那一层目录没建成，开终端那一行不带前奏");
    }
    made
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
        if Ro::metadata(f.as_path()).ok()?.len() > MAX_BYTES {
            return None;
        }
        parse(&Ro::read_to_string(f.as_path()).ok()?)
    }

    fn forget(&self, sid: &str) {
        if let Some(f) = self.file(sid) {
            let _ = self.0.delete_file(&f);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_opened_windows_tests.rs"]
mod tests;
