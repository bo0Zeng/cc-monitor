//! **几台的全文搜索结果合成一份**（[`merge`]）：两侧调同一份 —— 本机后端扇出问完各台（`faces/search_face.rs`）·
//! 手机嵌的 Rust 核心逐台直连问完（`mobcore::search_merge_json`）。线上形住 `cmd_wire::search`。
//!
//! 只并不判：每台的行由那台排好（最近优先）、写好时刻字（按问的那一方的时区）；这里只把几列按 `updatedAt` 并成一列（同一刻照台的先后）、
//! 命中数 · 读不动的份数相加、不覆盖的那几家去重、远端的行添上那台的名字；结果头那一句（搜的词 · 命中几处 · 几个会话 · 截没截）
//! 与头底下那几句（读不动几份 · 只按标题的那几家）也在这里写一次。

use cmd_wire::cells::Words;
use cmd_wire::search::{HistorySearchReply, Merged, MergedRow, SearchDown};
use cmd_wire::Machine;
use copy_core::copy_text;

use crate::column::merge_by;

/// `q` 是搜的那个词（结果头那一句里说它），`answered` 照台的先后（每台 `(名字, 它答回来的那一份)`），
/// `down` 是没答上的那几台（原样带上）。纯计算：不读盘、不起进程、不看钟。
/// 每行的 `reveal` · `deleteAsk` 只放缺省，本机后端合完照哪台答的补（`history_asks`）。
pub fn merge(
    q: &str,
    answered: Vec<(Machine, HistorySearchReply)>,
    down: Vec<SearchDown>,
) -> Merged {
    let mut per_host: Vec<Vec<MergedRow>> = Vec::new();
    let mut total_hits: u64 = 0;
    let mut truncated = false;
    let mut unreadable = 0usize;
    let mut skipped: Vec<String> = Vec::new();
    for (machine, reply) in answered {
        unreadable = unreadable.saturating_add(reply.unreadable);
        for s in reply.skipped {
            if !skipped.contains(&s) {
                skipped.push(s);
            }
        }
        let mut rows: Vec<MergedRow> = Vec::with_capacity(reply.sessions.len());
        for row in reply.sessions {
            total_hits = total_hits
                .saturating_add(row.row.hits.len() as u64)
                .saturating_add(u64::from(row.row.more_hits));
            truncated |= row.row.hits_truncated;
            rows.push(MergedRow {
                reveal: false,
                delete_ask: Words::default(),
                row,
                origin: machine.clone(),
            });
        }
        per_host.push(rows);
    }
    // 几台并一列：历史页 · 手机会话屏同一条并法（各台的行由那台照最近优先排好）。
    let lists: Vec<&[MergedRow]> = per_host.iter().map(Vec::as_slice).collect();
    let sessions: Vec<MergedRow> = merge_by(&lists, |r| r.row.row.updated_at)
        .into_iter()
        .cloned()
        .collect();
    let session_count = sessions.len();
    let (n, m) = (total_hits.to_string(), session_count.to_string());
    let summary = if truncated {
        copy_text(
            "history.search.summaryTruncated",
            &[("q", q), ("n", &n), ("m", &m)],
        )
    } else {
        copy_text("history.search.summary", &[("q", q), ("n", &n), ("m", &m)])
    };
    let notes = (unreadable > 0)
        .then(|| {
            copy_text(
                "history.search.unreadable",
                &[("n", &unreadable.to_string())],
            )
        })
        .into_iter()
        .chain(
            skipped
                .iter()
                .map(|a| copy_text("history.search.skipped", &[("agent", a)])),
        )
        .collect();
    Merged {
        summary,
        total_hits,
        session_count,
        truncated,
        sessions,
        notes,
        down,
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/face-core/search_tests.rs"]
mod tests;
