//! 一步结果那一句（唯一一处，[`said`]）与一步用时的配对表：工具调用与它的结果在两条记录上，用时是跨记录的 ——
//! 调用那一条记下 id 与时刻，结果那一条来了配上，连用时一起把那一句写进结果那一格。
//!
//! 通用层，不认哪一家：只读通用记录的 `at_ms` · 回复里的 `Block::ToolUse` · 带结果那一条的 `Block::ToolResult` 与 `results`。
//! 适配层只交数与码（`text` 空、[`StepResult::read`]），字都在这里写。
//! 只记没结果的调用，至多 [`PENDING_KEEP`] 条（与会话事实里没结果的调用同一个数），满了丢最早的。
//! 配不上（没记下 · 已丢 · 哪一头没有时刻 · 结果早于调用）⇒ 照没有用时的那一形写：读不到就不写，不猜。
//! 调用方（出页 · 实时那条）各持一份，按记录的先后先 `note` 再 `time`，每条记录只过一次。时刻取 `at_ms`；出口还没写它（实时那条在出口之前配）⇒ 照 `at` 现算（[`at_ms_of`]）。

use super::facts::PENDING_KEEP;
use crate::agents::record::{Block, Body, Record};
use crate::agents::{Answer, StepResult};
use crate::common::cells::Words;
use crate::common::line_diff;
use std::collections::{HashMap, VecDeque};

/// 没结果的调用：`(调用 id, 调用那一条的 at_ms)`，记下的先后。
#[derive(Debug, Default, Clone)]
pub(crate) struct StepClock {
    calls: VecDeque<(String, i64)>,
}

impl StepClock {
    /// 记下这条回复里每个工具调用的 id 与时刻（这条没有时刻 ⇒ 不记：配上了也算不出用时）。
    /// 同一 id 又来一次 ⇒ 照后一次的时刻。
    pub(crate) fn note(&mut self, r: &Record) {
        let (Some(at), Body::Reply { blocks, .. }) = (at_ms_of(r), &r.body) else {
            return;
        };
        for b in blocks {
            if let Block::ToolUse { id, .. } = b {
                self.seed(id, at);
            }
        }
    }

    /// 记下一个没结果的调用（id · 调用那一条的 `at_ms`）：`note` 读到的与起步时会话事实里的 `pending` 都从这里进。
    /// 同一 id 又来一次 ⇒ 照后一次的时刻；满了丢最早的。
    pub(crate) fn seed(&mut self, id: &str, at: i64) {
        self.calls.retain(|(c, _)| c != id);
        self.calls.push_back((id.to_string(), at));
        if self.calls.len() > PENDING_KEEP {
            self.calls.pop_front();
        }
    }

    /// 给这条里每个工具结果配上调用的时刻、写好那一句（[`said`]）：配上了 ⇒ 带用时；有结果的调用都从表里摘掉。
    pub(crate) fn time(&mut self, r: &mut Record) {
        let at = at_ms_of(r);
        let Body::Said {
            blocks, results, ..
        } = &mut r.body
        else {
            return;
        };
        let mut timed: HashMap<&str, u64> = HashMap::new();
        for b in blocks.iter() {
            let Block::ToolResult { of, .. } = b else {
                continue;
            };
            let Some(i) = self.calls.iter().position(|(c, _)| c == of) else {
                continue;
            };
            let (_, called) = self.calls.remove(i).unwrap_or_default();
            if let Some(ms) = at
                .and_then(|a| a.checked_sub(called))
                .and_then(|d| u64::try_from(d).ok())
            {
                timed.insert(of.as_str(), ms);
            }
        }
        for (of, cell) in results.iter_mut() {
            *cell = said(std::mem::take(cell), timed.get(of.as_str()).copied());
        }
    }

    /// 表里此刻记着几条。
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.calls.len()
    }
}

/// ★ 一步结果那一句与报错尾巴（唯一一处；适配层交来的数与码 ＋ 配上的用时 ⇒ 写好的字）。
/// - 报错尾巴：失败了（没批准的不算）⇒ 有退出码写退出码，否则「出错」。
/// - 右侧那一句：未批准 · 失败（配上了 ⇒「失败 · 用时」）· 提问 / 计划答了什么 · 改动「+N −M」（[`line_diff::stat`]）·
///   读了几行（只读文件那一类，[`StepResult::read`]）· 几个文件；都说不上 ⇒ 配上了只写用时，否则空串。
pub(crate) fn said(mut r: StepResult, ms: Option<u64>) -> StepResult {
    use copy_core::copy_text;
    r.error = (!r.ok && !r.rejected).then(|| {
        Words(match r.exit_code {
            Some(code) => copy_text("stream.result.exitCode", &[("n", &code.to_string())]),
            None => copy_text("stream.result.error", &[]),
        })
    });
    let dur = ms.map(copy_core::short_duration);
    let diff = line_diff::stat(r.added.unwrap_or(0), r.removed.unwrap_or(0));
    let text = if r.rejected {
        copy_text("stream.step.rejected", &[])
    } else if !r.ok {
        match &dur {
            Some(d) => copy_text("stream.step.failedFor", &[("dur", d)]),
            None => copy_text("runs.state.failed", &[]),
        }
    } else if let Some(a) = &r.answer {
        // 提问 / 计划答了什么（卡片底行那一句）。
        match a {
            Answer::Approved => copy_text("interactive.done.approved", &[]),
            Answer::Picked { options } => copy_text(
                "interactive.done.picked",
                &[("option", &options.join(" / "))],
            ),
        }
    } else if !diff.is_empty() {
        diff
    } else if let Some(n) = r.lines.filter(|_| r.read) {
        copy_text("stream.step.lines", &[("n", &n.to_string())])
    } else if let Some(n) = r.files {
        copy_text("stream.step.files", &[("n", &n.to_string())])
    } else {
        dur.unwrap_or_default()
    };
    r.text = Words(text);
    r
}

/// 这条记录的时刻（毫秒）：出口写过的 `at_ms`；还没写 ⇒ `at` 现算（与出口写它同一个解析，时刻与时区无关）。
fn at_ms_of(r: &Record) -> Option<i64> {
    r.at_ms.or_else(|| {
        r.at.as_deref()
            .and_then(crate::common::time::parse_iso8601_ms)
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/observe/recognize/step_clock_tests.rs"]
mod tests;
