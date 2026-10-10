//! 起新会话那一趟的票：界面每次点［新建］带一张票（不透明的串），期限到了没等到回话（结果未知）⇒ 同一张票再问一次。
//! 这台按票认出是不是同一趟：
//! - 那一趟已经起好了 ⇒ 原样回那一份应答（不起第二个 —— 远端卡住、回话晚到时，界面再问一次就落到那个会话上）；
//! - 那一趟还在起 ⇒ 回 `launch_pending`（界面说「还在起 · 稍后再核」，不重起）；
//! - 没见过这张票（那一趟根本没到这台）· 或那一趟没起成 ⇒ 照常起一次。
//!
//! 只记在这个进程里（常驻后端活多久记多久），最多记 [`KEEP`] 张，满了丢最早的（只为认出「刚才那一趟」，不是账）。

use std::collections::VecDeque;
use std::sync::Mutex;

/// 最多记几张票。
pub(crate) const KEEP: usize = 64;
/// 票的上限长度（界面给的是一个 UUID）。
const TICKET_MAX: usize = 64;

/// 一张票那一趟此刻怎样。
#[derive(Debug, Clone, PartialEq)]
enum Slot {
    Running,
    Started(serde_json::Value),
}

/// 按票记的那几趟（判据用自己的一份；生产用 [`TICKETS`]）。
#[derive(Default)]
pub(crate) struct Tickets(Mutex<VecDeque<(String, Slot)>>);

/// 生产那一份。
pub(crate) static TICKETS: Tickets = Tickets(Mutex::new(VecDeque::new()));

/// 票合不合格：非空、有界、只许字母数字与 `-`（它会进日志与应答）。
pub(crate) fn ticket_ok(t: &str) -> bool {
    !t.is_empty()
        && t.len() <= TICKET_MAX
        && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// 一趟的结局：起好了的那一份应答 · 不行（码 ＋ 那一句 ＋ `data`）。
pub(crate) type Run<E> = Result<serde_json::Value, E>;

impl Tickets {
    /// 带票起一趟：见过且起好了 ⇒ 原样回那一份（`run` 不跑）；正在起 ⇒ `pending()`；别的 ⇒ 跑 `run`，起好了记下、没起成忘掉这张票。
    /// 没票 ⇒ 照常跑（老界面）。
    pub(crate) fn with<E>(
        &self,
        ticket: Option<&str>,
        pending: impl FnOnce() -> E,
        run: impl FnOnce() -> Run<E>,
    ) -> Run<E> {
        let Some(t) = ticket else { return run() };
        {
            let mut q = self.0.lock().unwrap_or_else(|p| p.into_inner());
            match q.iter().find(|(k, _)| k == t).map(|(_, s)| s.clone()) {
                Some(Slot::Started(v)) => return Ok(v),
                Some(Slot::Running) => return Err(pending()),
                None => {
                    if q.len() >= KEEP {
                        q.pop_front();
                    }
                    q.push_back((t.to_string(), Slot::Running));
                }
            }
        }
        let r = run();
        let mut q = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let at = q.iter().position(|(k, _)| k == t);
        match (&r, at) {
            (Ok(v), Some(i)) => q[i].1 = Slot::Started(v.clone()),
            (Err(_), Some(i)) => {
                q.remove(i);
            }
            _ => {}
        }
        r
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/session_new_ticket_tests.rs"]
mod tests;
