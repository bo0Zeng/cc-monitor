//! **远端流断线重连之后，旁路快照从手上拿到的那一行接着拉，不再从尾段整份重拉。**
//!
//! 本进程按 `(origin, sid)` 存那台上一次 `history-tail` 回的 `held`（[`Held`]）：快照做完立一份，之后实时行**连续**到达、
//! 而且带着自己的末端字节时往前推（宁可少记，不许多记 —— 多记 ＝ 续传时跳过一段真没拿到的行）。
//! 重连后快照把它原样交回 `history-tail`：续不续得上（换了路径 · 变短 · 被改写 · 缺得比留存多）全由后端判一次、连第一页正文一起回。
//! 这里只存、只推、只忘。进程重启全丢（本来就是进程内软状态）⇒ 回到冷连上那一形：只读尾段。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::origin::Origin;
use cmd_wire::session::Held;

fn registry() -> &'static Mutex<HashMap<(String, String), Held>> {
    static R: OnceLock<Mutex<HashMap<(String, String), Held>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

fn key(origin: &Origin, sid: &str) -> (String, String) {
    (origin.as_wire_str().to_string(), sid.to_string())
}

/// 取某台某会话手上那一份（快照开拉之前问）。
pub(crate) fn cursor_of(origin: &Origin, sid: &str) -> Option<Held> {
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key(origin, sid))
        .cloned()
}

/// 一次快照**完整**做完：存后端回的那一份。快照读着的时候实时行已经把手上那一份推得更远（同一路径）⇒ 留推远的那一份。
pub(crate) fn note_done(origin: &Origin, sid: &str, held: Held) {
    let mut g = registry().lock().unwrap_or_else(|e| e.into_inner());
    match g.get(&key(origin, sid)) {
        Some(c) if c.path == held.path && c.next > held.next => {}
        _ => {
            g.insert(key(origin, sid), held);
        }
    }
}

/// 一批行发给前端之后：行号**恰好接上** `next`、而且带着它的末端 ⇒ 往前推（末端说不准的那一行停住，下一次从那里重读）。
pub(crate) fn note_flushed<'a>(
    origin: &Origin,
    flushed: impl IntoIterator<Item = (&'a str, u64, Option<u64>)>,
) {
    let mut g = registry().lock().unwrap_or_else(|e| e.into_inner());
    for (sid, seq, end) in flushed {
        if let (Some(c), Some(end)) = (g.get_mut(&key(origin, sid)), end) {
            if seq == c.next {
                c.next += 1;
                c.next_byte = end;
            }
        }
    }
}

/// 会话真结束（后端报 `session_removed`）⇒ 作废：再宣告时只读尾段（与冷连上同）。
pub(crate) fn forget(origin: &Origin, sid: &str) {
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&key(origin, sid));
}

/// 那台报了在看哪几个（`stream_watch`）⇒ 不在看的会话作废：它们的行不上流，手上那一份停在离开名单那一刻，
/// 重连时若照它续读就会补读一整段（中间那段界面进名单时已按 `from[]` 自己补过）。作废 ⇒ 再进名单那一次只读尾段。
pub(crate) fn keep_only(origin: &Origin, watched: &std::collections::BTreeSet<String>) {
    let wire = origin.as_wire_str();
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(o, sid), _| o != wire || watched.contains(sid));
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/snapshot_resume_tests.rs"]
mod tests;
