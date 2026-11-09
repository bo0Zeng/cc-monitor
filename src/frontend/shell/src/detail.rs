//! monitor 壳这一端写的「复制详情」那几行（排法只住 `copy_core::detail` 一份）：
//! - 通道这一跳断了（够不着 · 没答 · 本侧撤了 / 坏了）⇒ 壳写全份：时刻 · 机器 · 本机 · 命令 · 断在 · 码；
//! - 对端说「不行」⇒ 那台后端写好的那份原样转交，远端时由壳补一行「本机」（出错的就是本机时不补）；
//! - 壳问后端一条命令没成（[`of_call`]）⇒ 后端交来的那一份 `cmd_wire::failed::Failed` 原样往外交，壳不拆、不重排；
//! - 壳自己的错（进程里是 `copy_core::said::Said`，投出去那一下 [`Shell::named`] 成交出去那一形 `cmd_wire::failed::Failed`）
//!   ⇒ 时刻 · 本机 · 命令 · 码 · 原话（`copy_core::detail::failure`，后端排它自己的错也调它）。

use crate::chan::wire as w;
use cmd_wire::failed::Failed;
use copy_core::copy_text;
use copy_core::detail::{os_word, stamp, Detail, Host, Label};
use copy_core::said::Said;

/// 此刻（本机本地时间 ＋ 偏移）。
pub(crate) fn now() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    stamp(t, platform_core::tz::offset_secs(t).unwrap_or(0))
}

/// 「本机」那一项的值：`cc-monitor 4.1.5 (p9k-…) · Linux x86_64`。
pub(crate) fn local_line() -> String {
    let build = crate::byte_table::my_backend_id().unwrap_or("-");
    format!(
        "cc-monitor {} ({build}) · {} {}",
        crate::PRODUCT_VERSION,
        os_word(host_core::OS),
        host_core::ARCH
    )
}

/// 对端那份详情转交出去：远端 ⇒ 补「本机」一行。
pub(crate) fn relayed(origin: &crate::origin::Origin, remote: &str) -> String {
    if origin.is_local() {
        remote.to_string()
    } else {
        Detail::new()
            .block(remote)
            .item(Label::Local, local_line())
            .render()
    }
}

/// 通道上对端说「不行」的那一格（拒绝体 `{code, said, detail, data?}`）交出去之前：远端 ⇒ 体里那份详情补「本机」一行
/// （主界面与文件窗口都经通道宿主这一口，补一次、只在这里）；别的层原样。
pub(crate) fn relay_refusal(origin: &crate::origin::Origin, e: w::CallError) -> w::CallError {
    let w::CallError::Peer {
        why: w::PeerFault::Refused { body },
    } = &e
    else {
        return e;
    };
    if origin.is_local() {
        return e;
    }
    let Ok(serde_json::Value::Object(mut v)) = serde_json::from_slice::<serde_json::Value>(&body.0)
    else {
        return e;
    };
    let wrote = v
        .get("detail")
        .and_then(|d| d.as_str())
        .unwrap_or_default()
        .to_string();
    v.insert("detail".into(), relayed(origin, &wrote).into());
    w::err_from_wire(
        w::WireErr::Refused,
        serde_json::to_vec(&v).unwrap_or_default(),
    )
}

/// 拒绝体（`{code, said, detail, data?}`）里那份详情；没有 ⇒ 空串。
pub(crate) fn of_refusal_body(body: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("detail").and_then(|d| d.as_str()).map(str::to_string))
        .unwrap_or_default()
}

/// 通道这一跳（壳自己知道的那几样）的一份详情。`Refused` 不走这里（那份由对端写，见 [`relayed`]）。
/// 那几项的取法住通信层（`HopFacts`），排法住 `copy_core::detail::channel`（文件窗口进程同一份）。
pub(crate) fn of_channel(origin: &crate::origin::Origin, op: &str, e: &w::CallError) -> String {
    let f = e.hop_facts();
    let name = if origin.is_local() {
        copy_text("detail.label.local", &[])
    } else {
        origin.to_string()
    };
    let local = (!origin.is_local()).then(local_line);
    copy_core::detail::channel(
        &now(),
        &name,
        f.not_sent,
        local.as_deref(),
        op,
        f.hop.as_deref(),
        &f.code,
    )
    .render()
}

/// 壳问后端一条命令（`command`）没成：后端答了失败 ⇒ 它交来的那一份原样（码 · 复制详情 · `data`；那一句取调用方照分流定的 `said`，
/// 对端的句子空着时那里补「原因不明」那一句）；没走到那台（断了 · 超时 · 不认这条 …）⇒ 壳自己的错，码进详情，命令就是问的那一条。
pub(crate) fn of_call(said: String, command: &str, e: &crate::inbound_client::CallError) -> Failed {
    use crate::inbound_client::CallError;
    let code = match e {
        CallError::Remote(f) => return Failed { said, ..f.clone() },
        CallError::Unavailable { code, .. } => code.as_str(),
        CallError::Differs { .. } => "backend_differs",
        CallError::Unsupported { .. } => "unsupported",
        CallError::TooManyPending => "too_many_pending",
        CallError::Disconnected => "disconnected",
        CallError::Cancelled => "cancelled",
        CallError::Timeout { .. } => "timeout",
    };
    Said::from(said).coded(code).named(command)
}

/// 壳自己的错（`copy_core::said::Said`）在壳里的那几口：出句、接手下层、投出去。
pub(crate) trait Shell: Sized {
    /// 文案表里的那一句（没有下层原话；有的走 `Said::with_raw`）。
    fn of(key: &str, args: &[(&str, &str)]) -> Self;

    /// 壳自己那一下没成（后台那条线程没回来 · 窗口建不出来这一类，用户做不了什么）：那一句「cc-monitor 程序出错」，原话进详情。
    fn crashed(raw: impl std::fmt::Display) -> Self;

    /// 换一句给人看的话，码 · 原话照旧（外层接手下层那一形失败时说得更具体）。
    fn restate(said: String, from: Self) -> Self;

    /// 下层原话（日志里照样看得到下层说了什么）；没有 ⇒ 空串。
    fn raw(&self) -> String;

    /// 交给只收 `io::Error` 的那一口（放程序那条路）：那一句 ＋ 原话合成一条，原话不丢。
    fn into_io(self) -> std::io::Error;

    /// 投出去（交出去那一形）：复制详情 ＝ 时刻 · 本机 · 命令 · 码 · 原话（`copy_core::detail::failure`）；码缺 ⇒ 兜底码。
    /// 每条壳命令的最外层经这里一次（`#[tauri::command]` 那几十条，判据扫着）。
    fn named(self, command: &str) -> Failed;
}

impl Shell for Said {
    fn of(key: &str, args: &[(&str, &str)]) -> Said {
        Said::from(copy_text(key, args))
    }

    fn crashed(raw: impl std::fmt::Display) -> Said {
        Said::with_raw(copy_text("rsShellCmd.self.crashed", &[]), raw)
    }

    fn restate(said: String, from: Said) -> Said {
        from.wrap(|_| said)
    }

    fn raw(&self) -> String {
        self.raw.clone().unwrap_or_default()
    }

    fn into_io(self) -> std::io::Error {
        std::io::Error::other(self.logged())
    }

    fn named(self, command: &str) -> Failed {
        let detail = copy_core::detail::failure(
            &now(),
            Host::Local(&local_line()),
            Some(command),
            self.code.as_deref(),
            self.raw.as_deref(),
        );
        Failed {
            code: self.code_or_fallback().to_string(),
            said: self.said,
            detail,
            data: self.data,
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/detail_tests.rs"]
mod tests;
