//! **等起好的会话报到**（帧命令 `session-arrive`）：`session-open` · `bus-spawn` 回包里的 `arrival` 那一格原样交回来，这台等那一条报到。
//! 等多久 ＝ 请求信封的期限（减余量）。报到了 ⇒ 那个 sid；没见到 ⇒ 那一句（起在 tmux 里的抓那一屏末几行当原话）＋ 那个终端还在就给句柄。
//! 认「是不是它」住观测层（`observe::arrivals`，认的是界面也收的那一帧 `session_added`），抓屏住 `terminals`，都由入口经 [`Host`] 交进来。

pub(crate) use super::session_restart::MAX_WAIT_MS;
use cmd_wire::session::{ArriveAsk, ArriveBy, ArriveEnd};
use std::future::Future;

/// 原话取那一屏最后几行（非空行）。
const WORDS_LINES: usize = 6;

/// 起在的那个 tmux 会话此刻的样子：在（句柄 ＋ 那一屏原文）· 不在 · 读不到（那一句）。
pub(crate) enum Screen {
    There { terminal: String, text: String },
    Gone,
    Unread(String),
}

/// 入口交进来的两样（生产那一份在 `faces/session_arrive_face.rs`）。
pub(crate) trait Host: Send + Sync + 'static {
    /// `ms` 毫秒内等到 `since` 之后报到、`by` 认得出的那一条 ⇒ 它的 sid；没等到 ⇒ `None`。
    fn arrival(
        &self,
        by: &ArriveBy,
        since: u64,
        ms: u64,
    ) -> impl Future<Output = Option<String>> + Send;
    /// 名叫 `session` 的那个 tmux 会话此刻的样子。
    fn screen(&self, session: &str) -> impl Future<Output = Screen> + Send;
}

/// 那一屏 ⇒ 原话（最后几行非空行）。
pub(crate) fn last_words(text: &str) -> String {
    let kept: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    kept[kept.len().saturating_sub(WORDS_LINES)..].join("\n")
}

/// 没见到那一句（第一行标题）＋ 那个终端的句柄（还在才有）。`waited_ms` 是实际等了多久。
pub(crate) fn missed(waited_ms: u64, session: Option<&str>, screen: Option<Screen>) -> ArriveEnd {
    use copy_core::copy_text;
    let dur = copy_core::short_duration(waited_ms);
    let (body, terminal) = match (session, screen) {
        (Some(name), Some(Screen::There { terminal, text })) => {
            let words = last_words(&text);
            let body = if words.is_empty() {
                copy_text(
                    "beSessionArrive.missed.emptyScreen",
                    &[("dur", &dur), ("name", name)],
                )
            } else {
                copy_text(
                    "beSessionArrive.missed.words",
                    &[("dur", &dur), ("name", name), ("words", &words)],
                )
            };
            (body, Some(terminal))
        }
        (Some(name), Some(Screen::Gone)) => (
            copy_text(
                "beSessionArrive.missed.gone",
                &[("dur", &dur), ("name", name)],
            ),
            None,
        ),
        (Some(name), Some(Screen::Unread(why))) => (
            copy_text(
                "beSessionArrive.missed.noScreen",
                &[("dur", &dur), ("name", name), ("why", &why)],
            ),
            None,
        ),
        _ => (
            copy_text("beSessionArrive.missed.inWindow", &[("dur", &dur)]),
            None,
        ),
    };
    ArriveEnd::Missed {
        said: format!("{}\n{body}", copy_text("beSessionArrive.missed.title", &[])),
        terminal,
    }
}

/// `session-arrive` 本体。`wait_ms`：发起方期限折成的毫秒数（[`MAX_WAIT_MS`] 封顶）。
/// 坏入参 ⇒ `Err(那一句)`，不等。
pub(crate) async fn run<H: Host>(
    ask: ArriveAsk,
    wait_ms: u64,
    host: &H,
) -> Result<ArriveEnd, String> {
    match &ask.by {
        ArriveBy::Sid { sid } if !shell_quote_core::session_id_ok(sid) => {
            return Err(crate::common::contract::malformed(&format!(
                "not a session id: {sid:?}"
            )));
        }
        ArriveBy::Cwd { cwd } if cwd.is_empty() => {
            return Err(crate::common::contract::malformed(
                "`cwd` must not be empty",
            ));
        }
        _ => {}
    }
    let ms = wait_ms.min(MAX_WAIT_MS);
    if let Some(sid) = host.arrival(&ask.by, ask.since, ms).await {
        return Ok(ArriveEnd::Arrived { sid });
    }
    let screen = match &ask.session {
        Some(name) => Some(host.screen(name).await),
        None => None,
    };
    Ok(missed(ms, ask.session.as_deref(), screen))
}

#[cfg(test)]
#[path = "../../../tests/backend/control/session_arrive_tests.rs"]
mod tests;
