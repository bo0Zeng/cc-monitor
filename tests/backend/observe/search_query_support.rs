//! 全文搜索几份判据共用的几样：照冻结金样记查询用的那几个选项词造一份选项 · 每命中会话一行写进缓冲 ·
//! 会话内查找的夹具与现扫（本包的判据与主 crate 挂的帧面那几条经 `test-support` 共用）。

use super::{scan_session_find, search_rules, with_hit_text, FindPage, SearchOpts, SessionHits};
use serde_json::Value;

/// 会话内查找的夹具里那个词。
pub const Q: &str = "NeEdLe";

/// 每行一个形状，`uuid` 自带「该不该中」：`hit-*` 不看工具也中 · `tool-*` 只在带工具时中 · `miss-*` 永不中。
pub fn fixture() -> Vec<String> {
    vec![
        r#"{"type":"user","uuid":"hit-user-str","message":{"content":"find the needle here"}}"#.into(),
        r#"{"type":"assistant","uuid":"miss-plain","message":{"content":[{"type":"text","text":"nothing to see"}]}}"#.into(),
        r#"{"type":"assistant","uuid":"hit-asst-upper","message":{"content":[{"type":"text","text":"a NEEDLE in caps"}]}}"#.into(),
        // 注入的包装在 user 正文里被剥掉（只搜人说的话）⇒ 只在包装里出现的不算
        r#"{"type":"user","uuid":"miss-wrapped","message":{"content":"<system-reminder>needle</system-reminder>real words"}}"#.into(),
        r#"{"type":"assistant","uuid":"tool-use-input","message":{"content":[{"type":"tool_use","name":"Grep","input":{"pattern":"needle"}}]}}"#.into(),
        r#"{"type":"user","uuid":"tool-result","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":"line with needle"}]}}"#.into(),
        // 没有 uuid / 空 uuid ⇒ 跳不过去 ⇒ 不列
        r#"{"type":"user","message":{"content":"needle without uuid"}}"#.into(),
        r#"{"type":"user","uuid":"","message":{"content":"needle with empty uuid"}}"#.into(),
        // 不是 user/assistant ⇒ 不搜
        r#"{"type":"system","uuid":"miss-system","content":"needle in system"}"#.into(),
        r#"{"type":"ai-title","uuid":"miss-title","aiTitle":"needle title"}"#.into(),
        // 非 JSON
        r#"{"type":"user","uuid":"miss-broken","message":{"content":"needle"#.into(),
        "".into(),
        "\u{feff}{\"type\":\"user\",\"uuid\":\"hit-bom\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"x needle y\"}]}}".into(),
        // 子 agent 记录的首条是主会话派给它的活，不是人说的 ⇒ 不算 user 命中
        r#"{"type":"user","uuid":"miss-sidechain-task","isSidechain":true,"message":{"content":"sub needle"}}"#.into(),
        // agent 发来的话是 agent 回报（种类 `report` 单列）；后台通知不是谁说的话 ⇒ 不搜
        r#"{"type":"user","uuid":"hit-agent-report","isMeta":true,"origin":{"kind":"peer","from":"a1"},"message":{"content":"<agent-message from=\"a1\">needle</agent-message>"}}"#.into(),
        r#"{"type":"user","uuid":"miss-notice","origin":{"kind":"task-notification"},"message":{"content":"<task-notification><summary>needle</summary></task-notification>"}}"#.into(),
        // 代理那一侧自动写的应答（型号 `<synthetic>`）流里不画 ⇒ 不搜；同一型号的 API 报错那条画成报错卡 ⇒ 照搜
        r#"{"type":"assistant","uuid":"miss-synthetic","message":{"model":"<synthetic>","content":[{"type":"text","text":"needle auto reply"}]}}"#.into(),
        r#"{"type":"assistant","uuid":"hit-api-error","isApiErrorMessage":true,"message":{"model":"<synthetic>","content":[{"type":"text","text":"needle api error"}]}}"#.into(),
    ]
}

/// 每行后补一个换行，拼成一份记录的字节。
pub fn bytes_of(lines: &[String]) -> Vec<u8> {
    let mut v = Vec::new();
    for l in lines {
        v.extend_from_slice(l.as_bytes());
        v.push(b'\n');
    }
    v
}

/// 现扫一遍（帧面 `history-find` 不归常驻索引管时跑的那一个），摊成 `[{}, 每条命中…, {count, total}]`。
pub fn find(data: &[u8], q: &str, tools: bool, limit: usize) -> Vec<Value> {
    let mut v = vec![serde_json::json!({})];
    let page = FindPage { skip: 0, limit };
    let (count, total) = scan_session_find(
        crate::agents::claudecode::AGENT_KIND,
        data,
        q,
        tools,
        None,
        page,
        |hit| {
            v.push(serde_json::to_value(with_hit_text(hit, &Default::default())).unwrap());
            Ok(())
        },
    )
    .expect("find ok");
    v.push(serde_json::json!({ "count": count, "total": total }));
    assert_eq!(
        v[v.len() - 1]["count"].as_u64().unwrap() as usize,
        v.len() - 2,
        "尾行条数 == 实到条数"
    );
    v
}

/// [`find`] 摊出来的那几条命中的 uuid（去掉头尾两格）。
pub fn uuids(v: &[Value]) -> Vec<String> {
    v[1..v.len() - 1]
        .iter()
        .map(|r| r["uuid"].as_str().unwrap().to_string())
        .collect()
}

/// 常驻索引上一问读盘的账（整份重读 · 追加读 · 没读，各几份）：`home` 那棵记录树没建过索引 ⇒ `None`。
pub fn resident_reads(home: &std::path::Path) -> Option<(usize, usize, usize)> {
    let root =
        crate::common::fence::Fence::at(&crate::agents::claudecode::paths::projects_root(home))
            .ok()?
            .root()
            .to_path_buf();
    let r = super::resident().get(&root).map(|i| i.last)?;
    Some((r.full, r.appended, r.reused))
}

/// 判据用：照那几个选项词（`--include-tools` · `--scope X` · `--after-ms N` · `--limit N`）造一份选项。
/// 冻结金样里的查询是按这几个词记的；帧面直接从类型收（`faces/search_face.rs`），生产里没有这一层。
pub fn opts_from_words(rest: &[String]) -> SearchOpts {
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

/// 判据用：每命中会话一行写进 `buf`（键按字母排的那一份 JSON，一行一份；金样逐字节比的就是它）。
pub fn lines_into(buf: &mut Vec<u8>) -> impl FnMut(SessionHits) -> Result<(), String> + '_ {
    move |row| {
        use std::io::Write as _;
        let v = serde_json::to_value(&row).map_err(|e| e.to_string())?;
        writeln!(buf, "{v}").map_err(|e| e.to_string())
    }
}
