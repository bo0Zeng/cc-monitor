//! 出口那一下写记录的钟面那几格（推 `line` 帧 · 回按页读的那几条）：按看的那一台的时区写 `timeText` · `atMs`，
//! 顺手写好照钟面拼的那两句（重试那一行 · 收场通知那一行）。适配层与解析那一层不写它：同一份记录可能推给几个时区不同的看的人。
//!
//! 进来的是后端的行（[`BackRecord`]：线上那一形 ＋ 只给后端看的那几格），出去的是线上那一形：私有那一半在这里用完就丢。
//!
//! 形状在 `cmd-wire` · `agents`（孤儿规则不许在别的包的类型上加固有方法 ⇒ 扩展特征）。

use crate::agents::record::{reason_word, BackRecord, Body, Record};
use crate::common::cells::Words;
use crate::common::time::{iso_hm, parse_iso8601_ms, Tz};

/// 后端写记录钟面那一侧。
pub trait RecordExt {
    /// 按看的那一台的时区写钟面，交出线上那一形：`timeText` ＝ `at` 的 `HH:MM`；`at` 解不出 ⇒ 不写。
    /// 带着重试那几格（[`BackRecord::retry`]）⇒ 写重试那一行；不带 ⇒ 那一行原样（已写好的不再写一遍）。
    fn stamp(self, tz: &Tz) -> Record;
}

impl RecordExt for BackRecord {
    fn stamp(self, tz: &Tz) -> Record {
        let BackRecord { rec: mut r, retry } = self;
        r.at_ms = r.at.as_deref().and_then(parse_iso8601_ms);
        r.time_text = r.at.as_deref().and_then(|a| iso_hm(a, tz)).map(Words);
        let time = r.time_text.as_ref().map_or("", |w| w.0.as_str());
        if let (Body::Retry { text }, Some(f)) = (&mut r.body, retry) {
            *text = Some(Words(retry_line(f.reason, f.attempt, f.max, time)));
        }
        if let Body::Said { who, .. } = &mut r.body {
            if let cmd_wire::record::Speaker::CompactSummary { title_text } = &mut who.speaker {
                // 摘要多少字：照显示的那一份正文数（按字，不按 UTF-16）。
                let n = copy_core::count_text(who.text.chars().count() as u64);
                *title_text = Some(Words(copy_core::copy_text(
                    "compact.summary.title",
                    &[("n", &n), ("time", time)],
                )));
            }
            if let cmd_wire::record::Speaker::TaskNotification {
                task_id,
                summary,
                end,
                row,
                ..
            } = &mut who.speaker
            {
                *row = Some(Words(notice_row(
                    summary.as_deref().or(task_id.as_deref()).unwrap_or("?"),
                    *end,
                    time,
                )));
            }
        }
        r
    }
}

/// 收场通知那一行（「后台任务 {做的事} {怎么收场} · {钟面}」）：只在出口写这一处。
fn notice_row(what: &str, end: Option<cmd_wire::record::TaskEnd>, time: &str) -> String {
    use cmd_wire::record::TaskEnd;
    let state = match end {
        Some(TaskEnd::Done) => copy_core::copy_text("runs.state.done", &[]),
        Some(TaskEnd::Failed) => copy_core::copy_text("runs.state.failed", &[]),
        Some(TaskEnd::Stopped) => copy_core::copy_text("runs.state.stopped", &[]),
        None => copy_core::copy_text("runs.state.unknown", &[]),
    };
    copy_core::copy_text(
        "speaker.notice.row",
        &[("what", what), ("state", &state), ("time", time)],
    )
}

/// 还在重试的那一行（「{原因} · 重试 a/b · {钟面} 起」；次数缺一样 ⇒ 不带那一截）：记录出口与会话事实那一串都经它。
pub fn retry_line(
    reason: crate::agents::ApiReason,
    attempt: Option<u32>,
    max: Option<u32>,
    since: &str,
) -> String {
    let retry = match (attempt, max) {
        (Some(a), Some(b)) => copy_core::copy_text(
            "apiError.retry.count",
            &[
                ("retryAttempt", &a.to_string()),
                ("maxRetries", &b.to_string()),
            ],
        ),
        _ => String::new(),
    };
    copy_core::copy_text(
        "apiError.retry.line",
        &[
            ("reason", &reason_word(reason)),
            ("retry", &retry),
            ("time", since),
        ],
    )
}

#[cfg(test)]
#[path = "../../../tests/backend/wire/stamp_tests.rs"]
mod tests;
