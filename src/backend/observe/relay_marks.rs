//! **中转看见的请求标记**：每个会话（中转的流标签 ＝ 会话 id）的请求里，名单上那几项（如扩展上下文那一项）带没带过。
//!
//! 写的一侧是中转的 tap 口（`stream::tap::TapHub::note_marks`，每发请求一次）；读的一侧是会话事实定上下文上限
//! （`facts_query::context_limit` 的 `relay` 那一档）。只在本进程内存里，随进程生灭；中转没看见过的会话 ⇒ 说不出（`None`）。
//! 只记布尔，不记头的值。

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

/// 至多记多少个会话（**条数**，不是字节）；超了丢最早记下的那个（它再发请求就重新记）。
pub(crate) const MARKED_SESSIONS_KEEP: usize = 4096;

type Mark = (&'static str, &'static str);

#[derive(Default)]
struct Table {
    order: VecDeque<String>,
    /// 会话 ⇒ 每一项「带过没有」。会话在表里 ＝ 中转看见过它至少一发请求。
    by: HashMap<String, Vec<(Mark, bool)>>,
}

fn table() -> &'static Mutex<Table> {
    static T: OnceLock<Mutex<Table>> = OnceLock::new();
    T.get_or_init(Mutex::default)
}

impl Table {
    fn note(&mut self, cap: usize, stream: &str, marks: &[(Mark, bool)]) {
        if stream.is_empty() {
            return;
        }
        if !self.by.contains_key(stream) {
            if self.order.len() >= cap {
                if let Some(old) = self.order.pop_front() {
                    self.by.remove(&old);
                }
            }
            self.order.push_back(stream.to_string());
            self.by.insert(stream.to_string(), Vec::new());
        }
        let Some(seen) = self.by.get_mut(stream) else {
            return;
        };
        for &(m, present) in marks {
            match seen.iter_mut().find(|(k, _)| *k == m) {
                Some((_, with)) => *with |= present,
                None => seen.push((m, present)),
            }
        }
    }

    fn seen(&self, stream: &str, mark: Mark) -> Option<bool> {
        let marks = self.by.get(stream)?;
        Some(marks.iter().any(|(k, with)| *k == mark && *with))
    }
}

/// 这个会话的一发请求里，各项在不在。带过一次就算带过（同一会话里不带它的小请求不把它抹掉）。
pub(crate) fn note(stream: &str, marks: &[(Mark, bool)]) {
    let mut t = table().lock().unwrap_or_else(|e| e.into_inner());
    t.note(MARKED_SESSIONS_KEEP, stream, marks);
}

/// 中转看见过这个会话的请求吗；看见过 ⇒ 那一项带过没有。没看见过（直接敲的、没走中转的）⇒ `None`。
pub(crate) fn seen(stream: &str, mark: Mark) -> Option<bool> {
    table()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .seen(stream, mark)
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/relay_marks_tests.rs"]
mod tests;
