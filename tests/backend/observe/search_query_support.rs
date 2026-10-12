//! 全文搜索几份判据共用的两样：照从前命令行那几个选项词造一份选项（冻结金样里的查询按这几个词记）· 每命中会话一行写进缓冲。

use super::{search_rules, SearchOpts, SessionHits};

/// 判据用：照从前命令行那几个选项词（`--include-tools` · `--scope X` · `--after-ms N` · `--limit N`）造一份选项。
/// 冻结金样里的查询是按这几个词记的；帧面早就直接从类型收（`faces/search_face.rs`），生产里没有这一层。
pub(crate) fn opts_from_words(rest: &[String]) -> SearchOpts {
    let mut opts = SearchOpts {
        include_tools: false,
        scope: None,
        after_ms: 0,
        limit: search_rules::DEFAULT_LIMIT,
        titles: false,
    };
    let mut it = rest.iter();
    while let Some(w) = it.next() {
        match w.as_str() {
            "--include-tools" => opts.include_tools = true,
            "--scope" => {
                opts.scope = match it.next().map(String::as_str) {
                    Some("user") => Some("user"),
                    Some("assistant") => Some("assistant"),
                    Some("report") => Some("report"),
                    _ => None,
                }
            }
            "--after-ms" => opts.after_ms = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--limit" => opts.limit = SearchOpts::limit_of(it.next().and_then(|v| v.parse().ok())),
            _ => {}
        }
    }
    opts
}

/// 判据用：每命中会话一行写进 `buf`（键按字母排的那一份 JSON，同从前逐行那一形；金样逐字节比的就是它）。
pub(crate) fn lines_into(buf: &mut Vec<u8>) -> impl FnMut(SessionHits) -> Result<(), String> + '_ {
    move |row| {
        use std::io::Write as _;
        let v = serde_json::to_value(&row).map_err(|e| e.to_string())?;
        writeln!(buf, "{v}").map_err(|e| e.to_string())
    }
}
