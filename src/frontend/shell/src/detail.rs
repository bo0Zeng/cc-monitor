//! monitor 壳这一端写的「复制详情」那几行（排法只住 `copy_core::detail` 一份）：
//! - 通道这一跳断了（够不着 · 没答 · 本侧撤了 / 坏了）⇒ 壳写全份：时刻 · 机器 · 本机 · 命令 · 断在 · 码；
//! - 对端说「不行」⇒ 那台后端写好的那份原样转交，远端时由壳补一行「本机」（出错的就是本机时不补）；
//! - 壳自己那几条命令失败（[`Said`]）⇒ 时刻 · 本机 · 命令 · 原话。

use crate::chan::wire as w;
use crate::copy_table::copy_text;
use copy_core::detail::{append, os_word, stamp, Detail, Label};
use serde::Serialize;

/// 此刻（本机本地时间 ＋ 偏移）。
pub(crate) fn now() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    stamp(t, host_core::local_offset_at(t))
}

/// 「本机」那一项的值：`cc-monitor 4.1.5 (p9k-…) · Linux x86_64`。
pub(crate) fn local_line() -> String {
    let build = crate::byte_table::my_backend_id().unwrap_or("-");
    format!(
        "cc-monitor {} ({build}) · {} {}",
        crate::machine_state::PRODUCT_VERSION,
        os_word(host_core::OS),
        host_core::ARCH
    )
}

/// 对端那份详情转交给界面：远端 ⇒ 补「本机」一行。
pub(crate) fn relayed(origin: &crate::origin::Origin, remote: &str) -> String {
    if origin.is_local() {
        remote.to_string()
    } else {
        append(remote, Label::Local, &local_line())
    }
}

/// 拒绝体（`{code, message, data?, detail?}`）里那份详情；没有 ⇒ 空串。
pub(crate) fn of_refusal_body(body: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("detail").and_then(|d| d.as_str()).map(str::to_string))
        .unwrap_or_default()
}

/// 通道这一跳（壳自己知道的那几样）的一份详情。`Refused` 不走这里（那份由对端写，见 [`relayed`]）。
/// 那几项的取法住通信层（`HopFacts`，文件窗口进程同一份）。
pub(crate) fn of_channel(origin: &crate::origin::Origin, op: &str, e: &w::CallError) -> String {
    let f = e.hop_facts();
    let name = if origin.is_local() {
        copy_text("detail.label.local", &[])
    } else {
        origin.0.clone()
    };
    let machine = if f.not_sent {
        format!("{name}（{}）", copy_text("detail.value.notConnected", &[]))
    } else {
        name
    };
    Detail::new()
        .item(Label::At, now())
        .item(Label::Machine, machine)
        .maybe(Label::Local, (!origin.is_local()).then(local_line))
        .item(Label::Command, op)
        .maybe(Label::Hop, f.hop)
        .item(Label::Code, f.code)
        .render()
}

/// 一条 ERROR 级日志事件（`monitor-error` 那条 toast）的详情：时刻 · 本机 · 对象（来源模块）· 原话（那条日志）。
pub(crate) fn of_log_event(target: &str, message: &str) -> String {
    Detail::new()
        .item(Label::At, now())
        .item(Label::Local, local_line())
        .item(Label::Target, target)
        .item(Label::Raw, message)
        .render()
}

/// 壳自己那几条命令的失败：给人看的那一句 ＋ 复制详情那几行。全仓壳命令的失败只这一形（ts-rs 导出）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
pub struct Said {
    pub said: String,
    pub detail: String,
}

impl Said {
    /// 一句话 ＋ 命令名 ＋ 下层原话（可缺）。
    pub fn new(said: impl Into<String>, command: &str, raw: Option<&str>) -> Said {
        Said::coded(said, command, None, raw)
    }

    /// 同 [`Said::new`]，多一项码（退出状态 · 系统错误码；可缺）。
    pub fn coded(
        said: impl Into<String>,
        command: &str,
        code: Option<&str>,
        raw: Option<&str>,
    ) -> Said {
        Said {
            said: said.into(),
            detail: Detail::new()
                .item(Label::At, now())
                .item(Label::Local, local_line())
                .item(Label::Command, command)
                .maybe(Label::Code, code)
                .maybe(Label::Raw, raw)
                .render(),
        }
    }
}

impl Said {
    /// 壳问后端一条命令没成（`inbound_client` 那一口）：那台写了详情 ⇒ 原样带上（远端补「本机」一行）；
    /// 没写（老后端 / 没走到那台：断了 · 超时 · 不认这条）⇒ 壳写时刻 · 本机 · 命令 · 码。
    pub(crate) fn of_call(
        said: String,
        command: &str,
        origin: &crate::origin::Origin,
        e: &crate::inbound_client::CallError,
    ) -> Said {
        use crate::inbound_client::CallError;
        let detail = match e {
            CallError::Remote { detail, .. } if !detail.trim().is_empty() => {
                relayed(origin, detail)
            }
            other => {
                let code = match other {
                    CallError::Remote { code, .. } | CallError::Unavailable { code, .. } => {
                        code.as_str()
                    }
                    CallError::Unsupported { .. } => "unsupported",
                    CallError::TooManyPending => "too_many_pending",
                    CallError::Disconnected => "disconnected",
                    CallError::Cancelled => "cancelled",
                    CallError::Timeout { .. } => "timeout",
                };
                Detail::new()
                    .item(Label::At, now())
                    .item(Label::Local, local_line())
                    .item(Label::Command, command)
                    .item(Label::Code, code)
                    .render()
            }
        };
        Said { said, detail }
    }
}

impl Said {
    /// 壳自己那一下没成（后台那条线程没回来 · 窗口建不出来这一类，用户做不了什么）：那一句「cc-monitor 程序出错」，原话进详情。
    pub(crate) fn crashed(raw: impl std::fmt::Display) -> Said {
        Said::with_raw(copy_text("rsShellCmd.self.crashed", &[]), raw)
    }

    /// 一句话 ＋ 下层原话（命令名由最外层 [`Said::named`] 补）。
    pub(crate) fn with_raw(said: String, raw: impl std::fmt::Display) -> Said {
        Said {
            said,
            detail: Detail::new()
                .item(Label::At, now())
                .item(Label::Local, local_line())
                .item(Label::Raw, raw.to_string())
                .render(),
        }
    }

    /// 详情里「原话」那一项的值（到下一个项名为止；没有 ⇒ `None`）。
    pub(crate) fn raw(&self) -> String {
        let label = format!("{}：", Label::Raw.said());
        let others: Vec<String> = Label::ALL
            .iter()
            .filter(|l| **l != Label::Raw)
            .map(|l| format!("{}：", l.said()))
            .collect();
        let mut out: Vec<&str> = Vec::new();
        let mut inside = false;
        for line in self.detail.lines() {
            if let Some(v) = line.strip_prefix(&label) {
                inside = true;
                out.push(v);
            } else if inside && others.iter().any(|p| line.starts_with(p)) {
                break;
            } else if inside {
                out.push(line);
            }
        }
        out.join("\n")
    }

    /// 交给只收 `io::Error` 的那一口（放程序那条路）：那一句 ＋ 原话合成一条，原话不丢。
    pub(crate) fn into_io(self) -> std::io::Error {
        let raw = self.raw();
        if raw.is_empty() {
            std::io::Error::other(self.said)
        } else {
            std::io::Error::other(format!("{}: {raw}", self.said))
        }
    }

    /// 壳命令的失败补上命令名（复制详情的「命令」那一项）：详情里已经有一项「命令」（自己写的 · 后端写的）⇒ 原样。
    /// 每条壳命令的最外层经这里一次（`#[tauri::command]` 那几十条，判据扫着）。
    pub(crate) fn named(self, command: &str) -> Said {
        let label = format!("{}：", Label::Command.said());
        if self.detail.lines().any(|l| l.starts_with(&label)) {
            return self;
        }
        // 「命令」排在本机之后、其余几项之前（[`Label::ALL`] 的次序）：插在第一条后排项名的前面；没有后排项 ⇒ 接在末尾。
        let later: Vec<String> = [
            Label::Path,
            Label::Target,
            Label::Hop,
            Label::Code,
            Label::Raw,
        ]
        .iter()
        .map(|l| format!("{}：", l.said()))
        .collect();
        let lines: Vec<&str> = self.detail.lines().collect();
        let detail = match lines
            .iter()
            .position(|l| later.iter().any(|p| l.starts_with(p)))
        {
            Some(at) => {
                let mut v: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
                v.insert(at, format!("{label}{command}"));
                v.join("\n")
            }
            None => append(&self.detail, Label::Command, command),
        };
        Said {
            detail,
            said: self.said,
        }
    }
}

impl std::fmt::Display for Said {
    /// 只出那一句（详情不跟着进任何拼出来的句子 · 日志里要原话的另取 [`Said::raw`]）。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.said)
    }
}

/// 壳命令里那句话（`?` 与 `.into()` 经这里）：详情带时刻与本机。
impl From<String> for Said {
    fn from(said: String) -> Said {
        Said {
            said,
            detail: Detail::new()
                .item(Label::At, now())
                .item(Label::Local, local_line())
                .render(),
        }
    }
}

impl From<&str> for Said {
    fn from(said: &str) -> Said {
        Said::from(said.to_string())
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/detail_tests.rs"]
mod tests;
