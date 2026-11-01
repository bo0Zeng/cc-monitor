//! **一批会话合好的那一句**：几台各自答回的逐条结局 ⇒ 汇总句 · 没成 · 跳过的逐条 · 复制详情。
//! 桌面放本机后端、手机放嵌进去的核心，链的是这同一份；称呼里套机器名（`tabBatch.line.remote`）由调用方照自己的机器表先写进 `title`。

use cmd_wire::batch::{BatchOutcome, ItemState, SessionOutcome, SessionState, SessionsBatchReply};
use copy_core::copy_text;

use crate::said_of;

/// 一批做的那一样：动作那个词（原形，汇总句用）＋ 只有一项时点名的两句（成了 · 没成，参数是称呼）。
#[derive(Clone, Debug)]
pub struct Verb {
    pub action: String,
    pub one_done: fn(&str) -> String,
    pub one_failed: fn(&str) -> String,
}

/// 一条会话的结局（合句只看这几格）。
#[derive(Clone, Copy, Debug)]
pub struct Row<'a> {
    pub title: &'a str,
    pub state: SessionState,
    /// 那台写好的那一句（成了 ⇒ `None`）。
    pub said: Option<&'a str>,
    /// 复制详情（没有 ⇒ 空串）。
    pub detail: &'a str,
}

/// 合好的那几格。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub said: String,
    pub failed: bool,
    pub lines: Vec<String>,
    pub copy_detail: String,
}

/// 合那几格：
/// - `said`：一项成了 · 一项没成 ⇒ 点名那一句；别的 ⇒ 同文件批量那一句（`said_of`）；
/// - `lines`：没成的在前、跳过的在后，各一行（「失败 · 称呼 · 那一句」/「跳过 · …」）；
/// - `copy_detail`：没成且带详情的各一段（那一句 ＋ 详情），段间空一行；都成了 ⇒ 空串。
pub fn summary_of(verb: &Verb, rows: &[Row]) -> Summary {
    let failed = rows.iter().any(|r| r.state == SessionState::Failed);
    let said = match rows {
        [only] if only.state == SessionState::Done => (verb.one_done)(only.title),
        [only] if only.state == SessionState::Failed => (verb.one_failed)(only.title),
        _ => {
            let outcomes: Vec<BatchOutcome> = rows
                .iter()
                .enumerate()
                .map(|(i, r)| BatchOutcome::bare(i.to_string(), item_state(r.state)))
                .collect();
            said_of(&verb.action, &outcomes, None)
        }
    };
    let line = |r: &Row| {
        let why = r.said.unwrap_or_default();
        match r.state {
            SessionState::Failed => copy_text(
                "tabBatch.result.failedLine",
                &[("title", r.title), ("why", why)],
            ),
            _ => copy_text(
                "tabBatch.result.skippedLine",
                &[("title", r.title), ("why", why)],
            ),
        }
    };
    let of = |s: SessionState| rows.iter().filter(move |r| r.state == s);
    let lines = of(SessionState::Failed)
        .chain(of(SessionState::Skipped))
        .map(line)
        .collect();
    let copy_detail = of(SessionState::Failed)
        .filter(|r| !r.detail.trim().is_empty())
        .map(|r| format!("{}\n{}", r.said.unwrap_or_default(), r.detail))
        .collect::<Vec<_>>()
        .join("\n\n");
    Summary {
        said,
        failed,
        lines,
        copy_detail,
    }
}

fn item_state(s: SessionState) -> ItemState {
    match s {
        SessionState::Done => ItemState::Done,
        SessionState::Skipped => ItemState::Skipped,
        SessionState::Failed => ItemState::Failed,
    }
}

/// 几台答回的逐条结局（照入参原序，称呼已套好机器名）⇒ 合好的回包。
pub fn sessions_reply(verb: &Verb, results: Vec<SessionOutcome>) -> SessionsBatchReply {
    let rows: Vec<Row> = results
        .iter()
        .map(|o| Row {
            title: &o.title,
            state: o.outcome,
            said: o.said.as_deref(),
            detail: &o.copy_detail,
        })
        .collect();
    let Summary {
        said,
        failed,
        lines,
        copy_detail,
    } = summary_of(verb, &rows);
    SessionsBatchReply {
        results,
        said,
        failed,
        lines,
        copy_detail,
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/batch-core/sessions_tests.rs"]
mod tests;
