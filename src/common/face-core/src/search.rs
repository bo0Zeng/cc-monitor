//! **几台的全文搜索结果合成一份**（[`merge`]）：两侧调同一份 —— 本机后端扇出问完各台（`faces/search_face.rs`）·
//! 手机嵌的 Rust 核心逐台直连问完（`mobcore::resident::search_all`）。线上形住 `cmd_wire::search`。
//!
//! 只并不判：每台的行由那台排好（最近优先）、写好时刻字（按问的那一方的时区）；这里只把几列按 `updatedAt` 并成一列（同一刻照台的先后）、
//! 命中数 · 读不动的份数相加、不覆盖的那几家去重、远端的行添上那台的名字；结果头那一句（搜的词 · 命中几处 · 几个会话 · 截没截）
//! 与头底下那几句（读不动几份 · 只按标题的那几家）也在这里写一次；一台都没答上 ⇒ 不写结果头那一句，两端只照它出不出。
//! 一行都没有时空态那一句也在这里写（一台都没答上 ⇒ 搜不了；答了没中 ⇒ 没有匹配），两端只照它摆。

use cmd_wire::cells::Words;
use cmd_wire::search::{HistorySearchReply, Merged, MergedRow, SearchDown};
use cmd_wire::Machine;
use copy_core::copy_text;

use crate::column::merge_by;

/// **一台 `history-search` 那一问的结局 ⇒ 答了那一份，或没答上那一格**（句里叫出那台，`name` ＝ 给人看的称呼；说法同需手动那一份，[`crate::needs::missed`]）。
/// 本机后端扇出问完与手机核心逐台问完都调这一个，再交 [`merge`]。
pub fn search_reply(
    origin: Machine,
    name: &str,
    end: cmd_wire::phone::CallEnd,
) -> Result<HistorySearchReply, SearchDown> {
    crate::needs::missed("history-search", name, end)
        .map_err(|(said, _)| SearchDown { origin, said })
}

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
            let more = row.row.more_hits;
            rows.push(MergedRow {
                reveal: false,
                delete_ask: Words::default(),
                more_text: (more > 0)
                    .then(|| copy_text("history.searchSession.more", &[("n", &more.to_string())])),
                row,
                origin: machine.clone(),
            });
        }
        per_host.push(rows);
    }
    // 几台并一列：历史页 · 手机会话屏同一条并法（各台的行由那台照最近优先排好）。
    // 一台都没答上 ⇒ 不写结果头（没问到不当「命中 0」）；两端只照这一格出不出。
    let any_answered = !per_host.is_empty();
    let lists: Vec<&[MergedRow]> = per_host.iter().map(Vec::as_slice).collect();
    let sessions: Vec<MergedRow> = merge_by(&lists, |r| r.row.row.updated_at)
        .into_iter()
        .cloned()
        .collect();
    let (n, m) = (total_hits.to_string(), sessions.len().to_string());
    let summary = any_answered.then(|| {
        if truncated {
            copy_text(
                "history.search.summaryTruncated",
                &[("q", q), ("n", &n), ("m", &m)],
            )
        } else {
            copy_text("history.search.summary", &[("q", q), ("n", &n), ("m", &m)])
        }
    });
    // 一行都没有 ⇒ 空态那一句也在这里写：没答上一台是搜不了，答了没中才是没有匹配。
    let empty = sessions.is_empty().then(|| {
        if any_answered {
            copy_text("kit.menu.noMatch", &[("q", q)])
        } else {
            copy_text("history.search.noneAnswered", &[])
        }
    });
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
        truncated,
        sessions,
        notes,
        down,
        empty,
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/face-core/search_tests.rs"]
mod tests;
