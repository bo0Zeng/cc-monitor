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
        os_word(std::env::consts::OS),
        std::env::consts::ARCH
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
pub(crate) fn of_channel(origin: &crate::origin::Origin, op: &str, e: &w::CallError) -> String {
    let machine = if origin.is_local() {
        copy_text("detail.label.local", &[])
    } else {
        origin.0.clone()
    };
    let (machine, hop, code) = match e {
        w::CallError::Hop { at, reach, why } => {
            let machine = if *reach == w::Reach::NotSent {
                format!("{machine}{}", copy_text("detail.value.notConnected", &[]))
            } else {
                machine
            };
            (
                machine,
                Some(format!("{}:{} {reach:?}", at.idx, at.tag)),
                format!("{why:?}"),
            )
        }
        w::CallError::Peer {
            why: w::PeerFault::Unsupported,
        } => (machine, None, "unsupported".to_string()),
        w::CallError::Peer {
            why: w::PeerFault::Refused { .. },
        } => (machine, None, "refused".to_string()),
        w::CallError::Ours { why, runs_on } => (
            machine,
            None,
            if *runs_on {
                format!("{why:?} runs_on")
            } else {
                format!("{why:?}")
            },
        ),
    };
    Detail::new()
        .item(Label::At, now())
        .item(Label::Machine, machine)
        .maybe(Label::Local, (!origin.is_local()).then(local_line))
        .item(Label::Command, op)
        .maybe(Label::Hop, hop)
        .item(Label::Code, code)
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
        Said {
            said: said.into(),
            detail: Detail::new()
                .item(Label::At, now())
                .item(Label::Local, local_line())
                .item(Label::Command, command)
                .maybe(Label::Raw, raw)
                .render(),
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/detail_tests.rs"]
mod tests;
