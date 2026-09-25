//! 〔SE2 · `设计/10 §6 步 6`〕**monitor 侧的会话内查找**：实时 tab 里 Ctrl+F 问后端要命中。
//!
//! | 命令 | 跑什么 | 给谁 |
//! |---|---|---|
//! | [`find_in_session`] | `--find-in-session [--include-tools] --limit <n> --query <q> <p>` | 查找面板「搜索」那一格：`IPC-PROTOCOL.md §10.5` |
//!
//! 走 [`crate::subagent::Backend`]（本机 exec 本机后端 / 远端 ssh exec 同一个二进制）——
//! 「这条查询谁去跑」全仓只有那一处分流（与大纲 `session_outline`、骨架索引 `session_skeleton` 同一个形）。
//!
//! # 口径不在这里
//!
//! 「一条记录拿什么去搜、命中算哪一种、片段怎么切」只住后端 `observe/search_query.rs`（`--search` 与
//! `--find-in-session` 同调）＋ `search-core`。本侧**不解释**记录，只核头尾、把行搬成 [`FindHit`]。
//!
//! # 🔴 诚实降级
//!
//! 老后端不认这条子命令（stdout 0 字节、退出 2）、本机后端不在、输出被截断 ——
//! 一律回 `available: false` ＋ 原因（**不是错误**）：面板上那一行状态说清为什么。
//! **绝不**把别的输出当命中解析。
//!
//! 与大纲不同，这里**不分失败种类**：查找只在用户按 Enter 时发一次，没有「要不要自动再要」这个问题 ——
//! 用户再按一次就是再要一次。
//!
//! # 买不到
//!
//! - 远端整体 30s 超时（`run_list_query` 的 `LIST_TIMEOUT`）：弱网上超大会话可能撞上。
//! - 远端要等 `BUILD_ID` bump 后判 stale 重装才有这条子命令；今天走拨号（`frame_query::STILL_DIALED` 那一行）。

use crate::subagent::Backend;

/// 一条命中（后端那一行的形状，键名一字不差；片段三段同全局搜索的 `Hit`）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct FindHit {
    /// 命中的那条记录的 uuid —— 前端按骨架索引 `uuid → seq` 跳过去。
    pub uuid: String,
    /// `"user"` | `"assistant"` | `"tool"`
    pub kind: String,
    pub before: String,
    pub matched: String,
    pub after: String,
}

/// 查找的回包。`available == false` 时 `hits` 为空、`reason` 是给人看的原因（**不是错误**）。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct FindResult {
    pub available: bool,
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 按文件顺序（= 对话顺序），最多 [`FIND_LIMIT`] 条。
    pub hits: Vec<FindHit>,
    /// 全量命中数（≥ `hits.len()`；大于 ⇒ 被上限砍过，面板要说出来）。
    #[cfg_attr(test, ts(type = "number"))]
    pub total: u64,
}

/// 一次最多列多少条。与后端缺省（`search_query::FIND_DEFAULT_LIMIT`）同值，但**显式带上** ——
/// 不靠对面的缺省（对面换了缺省，本侧的「被砍过」提示就会对不上）。
pub(crate) const FIND_LIMIT: u64 = 500;

/// 本侧认得出的「拿不到命中」的几种样子。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FindUnavailable {
    /// 首行不是 `session_find` 头 —— 对面是不认这条子命令的老后端（0 字节退出）或回了别的东西。
    OldBackend,
    /// 有头没尾、尾行条数对不上、或中间一行坏了 —— 输出被截断，**不许当全量用**。
    Truncated { got: usize },
}

impl FindUnavailable {
    fn reason(&self) -> String {
        match self {
            Self::OldBackend => {
                "这台机器上的后端版本旧，还不能在会话里查找（重装后端之后就有）".into()
            }
            Self::Truncated { got } => {
                format!("查找结果传到一半断了（只收到 {got} 条），这次先不显示")
            }
        }
    }
}

/// 核头尾、剥出命中。**纯函数**，两条 transport 的输出（逐行、已 trim、已剔空行）都走它。
pub(crate) fn parse_find_output(lines: &[String]) -> Result<(u64, Vec<FindHit>), FindUnavailable> {
    let parse = |l: &String| serde_json::from_str::<serde_json::Value>(l).ok();
    let kind_is =
        |v: &serde_json::Value, k: &str| v.get("kind").and_then(|x| x.as_str()) == Some(k);
    if !lines
        .first()
        .and_then(parse)
        .is_some_and(|h| kind_is(&h, "session_find"))
    {
        return Err(FindUnavailable::OldBackend);
    }
    let got = lines.len().saturating_sub(2);
    let tail = lines
        .last()
        .filter(|_| lines.len() >= 2)
        .and_then(parse)
        .filter(|t| kind_is(t, "session_find_end"))
        .ok_or(FindUnavailable::Truncated { got })?;
    if tail.get("count").and_then(|v| v.as_u64()) != Some(got as u64) {
        return Err(FindUnavailable::Truncated { got });
    }
    let total = tail
        .get("total")
        .and_then(|v| v.as_u64())
        .ok_or(FindUnavailable::Truncated { got })?;
    let mut hits = Vec::with_capacity(got);
    for l in &lines[1..lines.len() - 1] {
        match serde_json::from_str::<FindHit>(l) {
            Ok(h) => hits.push(h),
            Err(_) => return Err(FindUnavailable::Truncated { got }),
        }
    }
    Ok((total, hits))
}

/// 查找那条的 argv。🔴 **选项全写在位置参数前面**（与 `session_outline::user_inputs_argv` 同一条纪律），
/// 查询串是 `--query` 的**值** —— 以 `--` 起头的查询（`--force`）不会被后端当成选项。
pub(crate) fn find_argv(jsonl_path: &str, query: &str, include_tools: bool) -> Vec<String> {
    let mut v: Vec<String> = vec!["--find-in-session".into()];
    if include_tools {
        v.push("--include-tools".into());
    }
    v.extend([
        "--limit".into(),
        FIND_LIMIT.to_string(),
        "--query".into(),
        query.into(),
        jsonl_path.into(),
    ]);
    v
}

/// 路径的廉价预检（与 `session_outline` / `session_skeleton` 同一条纪律）：
/// 真正的越权读由后端 `fence_under_projects` 兜底。
fn precheck(jsonl_path: &str) -> Result<(), String> {
    if jsonl_path.contains("..") || !jsonl_path.ends_with(".jsonl") {
        return Err(format!("非法会话路径: {jsonl_path}"));
    }
    Ok(())
}

/// 一趟查询的结果 → 回包。**纯函数**。
pub(crate) fn find_result(queried: Result<Vec<String>, crate::subagent::QueryError>) -> FindResult {
    let unavailable = |reason: String| FindResult {
        available: false,
        reason: Some(reason),
        hits: Vec::new(),
        total: 0,
    };
    let lines = match queried {
        Ok(l) => l,
        Err(e) => return unavailable(e.message),
    };
    match parse_find_output(&lines) {
        Ok((total, hits)) => FindResult {
            available: true,
            reason: None,
            hits,
            total,
        },
        Err(u) => unavailable(u.reason()),
    }
}

/// **在这一份会话里找** `query`（大小写不敏感子串；`include_tools` 同全局搜索那个勾）。
#[tauri::command]
pub async fn find_in_session(
    origin: crate::origin::Origin,
    jsonl_path: String,
    query: String,
    include_tools: bool,
) -> Result<FindResult, String> {
    let route = origin.route("find_in_session")?;
    precheck(&jsonl_path)?;
    let backend = Backend::for_origin(route)?;
    let argv = find_argv(&jsonl_path, &query, include_tools);
    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
    let res = find_result(backend.query(&argv).await);
    if !res.available {
        tracing::info!(
            "find_in_session({jsonl_path}): {} 查不了：{}",
            backend.whose(),
            res.reason.as_deref().unwrap_or("")
        );
    }
    Ok(res)
}

#[cfg(test)]
#[path = "../../../tests/bridge/session_find_tests.rs"]
mod tests;
