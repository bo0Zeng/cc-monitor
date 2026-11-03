//! **中转看见的请求标记**：每个会话（中转的流标签 ＝ 会话 id）的请求里，扩展上下文那一项带没带过。
//!
//! 写的一侧是中转的 tap 口（`stream::tap::TapHub::offer`，每个响应的 0 号那一件带着名单上那几个请求头）；认不认得出那一项
//! 由适配层在头的值里找（`agents::wide_context`）。读的一侧是会话事实定上下文上限（`recognize::facts::context_limit` 的 `relay` 那一档）。
//! 只在本进程内存里，随进程生灭；中转没看见过的会话 ⇒ 说不出（`None`）。只记布尔，不记头的值。

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

/// 至多记多少个会话（**条数**，不是字节）；超了丢最早记下的那个（它再发请求就重新记）。
pub(crate) const MARKED_SESSIONS_KEEP: usize = 4096;

#[derive(Default)]
struct Table {
    order: VecDeque<String>,
    /// 会话 ⇒ 扩展上下文那一项「带过没有」。会话在表里 ＝ 中转看见过它至少一发请求。
    by: HashMap<String, bool>,
}

fn table() -> &'static Mutex<Table> {
    static T: OnceLock<Mutex<Table>> = OnceLock::new();
    T.get_or_init(Mutex::default)
}

impl Table {
    fn note(&mut self, cap: usize, stream: &str, wide: bool) {
        if stream.is_empty() {
            return;
        }
        if let Some(with) = self.by.get_mut(stream) {
            *with |= wide;
            return;
        }
        if self.order.len() >= cap {
            if let Some(old) = self.order.pop_front() {
                self.by.remove(&old);
            }
        }
        self.order.push_back(stream.to_string());
        self.by.insert(stream.to_string(), wide);
    }

    fn seen(&self, stream: &str) -> Option<bool> {
        self.by.get(stream).copied()
    }
}

/// 这个会话的一发请求（中转带回的那几个头）。带过一次就算带过（同一会话里不带它的小请求不把它抹掉）。
pub fn note<V: AsRef<str>>(stream: &str, heads: &[(&str, V)]) {
    let wide = crate::agents::wide_context(heads);
    let mut t = table().lock().unwrap_or_else(|e| e.into_inner());
    t.note(MARKED_SESSIONS_KEEP, stream, wide);
}

/// 中转看见过这个会话的请求吗；看见过 ⇒ 扩展上下文那一项带过没有。没看见过（直接敲的、没走中转的）⇒ `None`。
pub fn seen(stream: &str) -> Option<bool> {
    table()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .seen(stream)
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/relay_marks_tests.rs"]
mod tests;
