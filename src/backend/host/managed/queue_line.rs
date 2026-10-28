//! 瘦客户端底行「排队 N 条」那一句（A98-4-5）：瘦客户端、托管终端与会话同在一台（托管口是本机套接字），这台就是队的主人。
//!
//! 会话 id 由后端打标写进 `hosts/<id>/facts.json`（A3-3-4）；队在 `session-queue/<sid>.json`。两处都经 [`crate::platform::watch_file`] 盯：
//! 打标那一下 · 队动了 ⇒ 现读事实里的 sid、照 `session-queue-read` 在这台那一半读名单，那一句由队那一侧写
//! （[`crate::history::session_queue::waiting_said`]），变了才说。瘦客户端只照说来的那一句摆（`attach.rs`）。

use super::home;
use crate::history::session_queue as q;
use crate::platform::fs::Own;
use crate::platform::watch_file::Watching;

/// 此刻那一句：没打标 · 没在等的 · 读不出 ⇒ `None`。
pub(crate) fn now(own: &Own, id: &str) -> Option<String> {
    let dir = home::dir_of(own, id).ok()?;
    let sid = home::read_facts(dir.as_path())?.sid;
    if sid.is_empty() {
        return None;
    }
    let list = q::read(own, &sid, crate::common::time::now_ms()).ok()?;
    q::waiting_said(&list)
}

/// 盯 `id` 那个托管终端那条会话的队：起来那一下先说一次，之后那一句变了才说（`tell`）。交回的那一份丢了 ⇒ 不再盯。
pub(crate) fn ears(
    own: &Own,
    id: &str,
    tell: impl Fn(Option<String>) + Send + 'static,
) -> Result<Watching, String> {
    let host = home::dir_of(own, id).map_err(|s| s.logged())?;
    let queue = q::made_dir(own).map_err(|s| s.logged())?;
    let (host, queue) = (host.as_path().to_path_buf(), queue.as_path().to_path_buf());
    let dirs = [(host.clone(), false), (queue.clone(), false)];
    let concerns = move |p: &std::path::Path| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        (p.parent() == Some(host.as_path()) && name == home::FACTS)
            || (p.parent() == Some(queue.as_path()) && name.ends_with(".json"))
    };
    let first = now(own, id);
    tell(first.clone());
    let (own, id) = (own.clone(), id.to_string());
    let mut last = first;
    crate::platform::watch_file::watch(&dirs, concerns, "queue-line", move |_| {
        let line = now(&own, &id);
        if line != last {
            tell(line.clone());
            last = line;
        }
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/host/queue_line_tests.rs"]
mod tests;
