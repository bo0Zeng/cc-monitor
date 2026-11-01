//! 一步用时的配对表：工具调用与它的结果在两条记录上，用时是跨记录的 —— 调用那一条记下 id 与时刻，
//! 结果那一条来了配上，把用时写进结果那一格（写字只经 [`StepResultExt::timed_by`]）。
//!
//! 通用层，不认哪一家：只读通用记录的 `at_ms` · 回复里的 `Block::ToolUse` · 带结果那一条的 `Block::ToolResult` 与 `results`。
//! 只记没结果的调用，至多 [`PENDING_KEEP`] 条（与会话事实里没结果的调用同一个数），满了丢最早的。
//! 配不上（没记下 · 已丢 · 哪一头没有时刻 · 结果早于调用）⇒ 那一格不动：读不到就不写，不猜。
//! 调用方（出页 · 实时那条）各持一份，按记录的先后先 `note` 再 `time`；`at_ms` 要在这之前写好。

use super::facts::PENDING_KEEP;
use crate::agents::record::{Block, Body, Record};
use crate::agents::StepResultExt;
use std::collections::VecDeque;

/// 没结果的调用：`(调用 id, 调用那一条的 at_ms)`，记下的先后。
#[derive(Debug, Default, Clone)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct StepClock {
    calls: VecDeque<(String, i64)>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl StepClock {
    /// 记下这条回复里每个工具调用的 id 与时刻（这条没有时刻 ⇒ 不记：配上了也算不出用时）。
    /// 同一 id 又来一次 ⇒ 照后一次的时刻。
    pub(crate) fn note(&mut self, r: &Record) {
        let (Some(at), Body::Reply { blocks, .. }) = (r.at_ms, &r.body) else {
            return;
        };
        for b in blocks {
            if let Block::ToolUse { id, .. } = b {
                self.calls.retain(|(c, _)| c != id);
                self.calls.push_back((id.clone(), at));
                if self.calls.len() > PENDING_KEEP {
                    self.calls.pop_front();
                }
            }
        }
    }

    /// 给这条里每个工具结果配上调用的时刻：配上了 ⇒ 那一格写好用时；有结果的调用都从表里摘掉。
    pub(crate) fn time(&mut self, r: &mut Record) {
        let at = r.at_ms;
        let Body::Said {
            blocks, results, ..
        } = &mut r.body
        else {
            return;
        };
        for b in blocks.iter() {
            let Block::ToolResult { of, .. } = b else {
                continue;
            };
            let Some(i) = self.calls.iter().position(|(c, _)| c == of) else {
                continue;
            };
            let (_, called) = self.calls.remove(i).unwrap_or_default();
            let ms = at
                .and_then(|a| a.checked_sub(called))
                .and_then(|d| u64::try_from(d).ok());
            if let Some(cell) = results.get_mut(of) {
                *cell = std::mem::take(cell).timed_by(ms);
            }
        }
    }

    /// 表里此刻记着几条。
    pub(crate) fn len(&self) -> usize {
        self.calls.len()
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/observe/recognize/step_clock_tests.rs"]
mod tests;
