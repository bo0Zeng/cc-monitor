//! **会话事实的续算表**：常驻后端记下最近出过的几份累加器，续传令牌因此只是 `{end, rev}`（[`super::recognize::facts`] 头注）。
//!
//! - 一份 ＝ `(rev, path, 那一份累加器)`；至多 [`FACTS_CACHE_KEEP`] 份，满了挤掉最久没用的那一份。
//! - 续上了 ⇒ 那一份换成续算出来的新一份、铸新 `rev`（每个调用方只占一格）；读不成（截断 · 不在行边界）⇒ 原来那一份留着。
//! - `rev` 从进程起来那一刻的钟往上数：后端重启过，旧令牌撞不上新进程的号。
//! - 一次性 CLI 进程里这张表恒空 ⇒ 带令牌恒回 `stale_prior`（照实说没续上）。
//! - 只在内存里，不落盘；丢了只是多一次冷扫。

use super::recognize::facts::SessionFacts;
use std::collections::VecDeque;
use std::sync::Mutex;

/// 续算表至多留几份：每个开着的 tab（桌面）/ 会话页（手机）占一格；一份最大约 140 KB（改动文件 1000 条封顶），64 份封顶约 9 MB。
pub(crate) const FACTS_CACHE_KEEP: usize = 64;

/// 调用方交回的续传令牌：上一份应答里的 `end` 与 `rev`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct FactsToken {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) end: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) rev: u64,
}

struct Table {
    next: u64,
    /// 最近用过的在末尾。
    rows: VecDeque<(u64, String, SessionFacts)>,
}

static TABLE: Mutex<Option<Table>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut Table) -> T) -> T {
    let mut g = TABLE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let t = g.get_or_insert_with(|| Table {
        // 毫秒 × 1000：到 2255 年都还在 2^53 之内（界面那一侧是 JS 的数）。
        next: crate::common::time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| {
                u64::try_from(d.as_millis())
                    .unwrap_or(1)
                    .saturating_mul(1000)
            }),
        rows: VecDeque::new(),
    });
    f(t)
}

/// 令牌对得上（同 `path` · 同 `rev` · 同 `end`）⇒ 那一份累加器的副本；对不上 / 挤掉了 ⇒ `None`。不摘：续算成了才由 [`keep`] 换掉。
pub(crate) fn find(path: &str, token: FactsToken) -> Option<SessionFacts> {
    with(|t| {
        t.rows
            .iter()
            .find(|(rev, p, f)| *rev == token.rev && p == path && f.end == token.end)
            .map(|(_, _, f)| f.clone())
    })
}

/// 记下新算出来的一份（`replaces` ＝ 它是从哪一份续来的，那一份同时摘掉），铸新 `rev` 写进 `facts.rev` 并回它。
pub(crate) fn keep(path: &str, facts: &mut SessionFacts, replaces: Option<u64>) -> u64 {
    with(|t| {
        if let Some(old) = replaces {
            t.rows.retain(|(rev, _, _)| *rev != old);
        }
        t.next += 1;
        let rev = t.next;
        facts.rev = rev;
        while t.rows.len() >= FACTS_CACHE_KEEP {
            t.rows.pop_front();
        }
        t.rows.push_back((rev, path.to_string(), facts.clone()));
        rev
    })
}
