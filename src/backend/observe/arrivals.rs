//! **起好的会话报到没有**：整台一本「此刻活着的会话 ⇒ 报到序号 · 工作目录」，挂在 watcher 发帧的出口上（`watcher::FrameSink`），
//! 与界面看到的 `session_added` 是同一帧；活着的 sid 由一个新进程接上（换号重启：旧的还没走、那一帧不重发）由 watcher 另报一声（[`Arrivals::again`]）。
//! 等的一方交「认它的那一格 ＋ 只认哪个序号之后报到的」，见到了当场交回 sid。不起线程、不定时。

use cmd_wire::session::ArriveBy;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::wire::Frame;

/// 等的那一方的耳朵：几份 watcher 共用一个，头一个见到的交、之后的都空着。
pub(crate) type Tell = Arc<Mutex<Option<tokio::sync::oneshot::Sender<String>>>>;

/// 交一件「等它」给这台每一份在跑的 watcher；回那一声的收端（没有在跑的 ⇒ 当场断）。
pub(crate) fn expect(by: ArriveBy, since: u64) -> tokio::sync::oneshot::Receiver<String> {
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    crate::observe::watcher::expect_arrival(&by, since, &Arc::new(Mutex::new(Some(tx))));
    rx
}

/// 这一条报到（序号 `n`、目录 `cwd`）是不是要等的那一条：`since` 之后报到的，sid 相同 · 或目录相同（末尾的 `/` 不算）。
pub(crate) fn hit(by: &ArriveBy, since: u64, sid: &str, n: u64, cwd: Option<&str>) -> bool {
    let trim = |p: &str| -> String {
        if p.len() > 1 {
            p.trim_end_matches('/').to_string()
        } else {
            p.to_string()
        }
    };
    n > since
        && match by {
            ArriveBy::Sid { sid: want } => want == sid,
            ArriveBy::Cwd { cwd: want } => cwd.is_some_and(|c| trim(c) == trim(want)),
        }
}

/// 交回 sid（别的 watcher 已经交过 ⇒ 什么都不做）。
fn tell(t: &Tell, sid: &str) {
    if let Some(tx) = t.lock().unwrap_or_else(|e| e.into_inner()).take() {
        // 等的那一头可能已经到期限撤了：没人要这一声。
        let _ = tx.send(sid.to_string());
    }
}

/// 这一件还有人等（没交过、等的那一头没撤）。
fn awaited(t: &Tell) -> bool {
    t.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|tx| !tx.is_closed())
}

/// 一份 watcher 上的那一本（见模块头注）。
#[derive(Debug, Default)]
pub(crate) struct Arrivals {
    live: HashMap<String, (u64, Option<String>)>,
    waiting: Vec<(ArriveBy, u64, Tell)>,
}

impl Arrivals {
    /// 一帧要发出去之前：活会话来了 / 走了 ⇒ 记账，来了的交给在等它的。
    pub(crate) fn on_frame(&mut self, f: &Frame) {
        match f {
            Frame::SessionAdded(crate::wire::SessionAdded { sid, cwd, .. }) => {
                if !self.live.contains_key(sid) {
                    self.arrive(sid, cwd.clone());
                }
            }
            Frame::SessionRemoved(crate::wire::SessionRemoved { sid, .. }) => {
                self.live.remove(sid);
            }
            _ => {}
        }
    }

    /// 活着的 `sid` 由一个新进程接上了（同一个会话不再宣告第二次）⇒ 记一次新报到。
    pub(crate) fn again(&mut self, sid: &str) {
        let cwd = self.live.get(sid).and_then(|(_, c)| c.clone());
        self.arrive(sid, cwd);
    }

    /// 记下一次报到（新的序号），交给在等它的。
    fn arrive(&mut self, sid: &str, cwd: Option<String>) {
        let n = crate::common::arrival_seq::next();
        self.waiting.retain(|(by, since, t)| {
            if hit(by, *since, sid, n, cwd.as_deref()) {
                tell(t, sid);
                false
            } else {
                awaited(t)
            }
        });
        self.live.insert(sid.to_string(), (n, cwd));
    }

    /// 交一件「等它」：已经报到过的当场交，否则等着。
    pub(crate) fn expect(&mut self, by: ArriveBy, since: u64, t: Tell) {
        let early = self
            .live
            .iter()
            .find(|(sid, (n, cwd))| hit(&by, since, sid, *n, cwd.as_deref()));
        if let Some((sid, _)) = early {
            tell(&t, sid);
            return;
        }
        self.waiting.retain(|(_, _, w)| awaited(w));
        self.waiting.push((by, since, t));
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/arrivals_tests.rs"]
mod tests;
