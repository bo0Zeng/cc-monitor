//! **会话事实**：分叉血缘 · 改动文件集 · 最新 usage · 项目目录。
//! 子 agent 的列表与状态不在这里：它们是运行表（`observe::runs`，经 `session_runs` 帧），判定只有那一处。
//!
//! # 它顶掉了什么
//!
//! 这几样从前是活 tab 在 `onLine` 旁路上一条一条攒的（前端 `tab-session-facts.ts` 那四个抽取器，已删）：
//! **到达序不是对话序**（重放是尾块先到）、**不完整**（重放缓冲每个会话只留尾部 `REPLAY_TAIL_KEEP` 条 ⇒
//! F5 之后长会话的分叉血缘看不见、agent / 改动文件只剩尾巴那一截）、每个 tab 各攒一份（那三个病）。
//! 今天它们由本文件读一遍文件算出来，经帧命令 `history-facts`（宿主 `read_face.rs`）出**成品**，
//! 界面经通道直接问、按形状收（`src/frontend/ui/session-reads.ts`），**本机与远端同一条路**。
//!
//! # 续传令牌就是上一份成品（后端零状态）
//!
//! 事实要跟着会话长，而大会话（本机实测 120 MB）每批整份重扫是撞墙的。选的形状：调用方把**上一次的应答原样**
//! 交回来（`prior`），本文件从 `prior.end` 接着扫、把新的一截累加在它上面 —— 判定与累加都只在这里，
//! 前端不读、不改、不合并那一份成品；后端不留任何状态（对 `history-lines` 的同一条取舍）。
//! 续点的两道校验（截断 · 不在行边界）在 `history_query::open_facts_at`。理由。
//!
//! # 口径（逐格，三样各一个住址）
//!
//! | 格 | 口径 |
//! |---|---|
//! | `forkedFrom` | `history_query::fork_origin`（与历史会话行同一个函数）；首条命中即锁定 |
//! | `touchedFiles` | `assistant` 记录里写类工具（[`EDIT_TOOL_PATH_KEYS`]）的路径，去重、**近因序**（再碰一次移到末尾），至多 [`TOUCHED_FILES_KEEP`] 条 |
//! | `usage` | `assistant` 记录的 `message.usage` 三项 prompt token 之和 > 0 ⇒ `{promptTokens, model}`，文件序最后一条胜 |
//! | `projectDir` | 适配层 `RecordFace.project_dir`（只读记录开头）；读到即锁定，不在本文件的逐行扫描里 |
//!
//! # 快路
//!
//! 一行要不要解析，先按字节看它有没有可能改动事实（[`could_matter`]）：分叉已锁 ⇒ 不找 `"forkedFrom"`；
//! 工具结果那一大类（常是整份文件内容）连解析都不做。**只省时间、不改结果**：
//! 能改动事实的记录必然带着那几个键名（Claude Code 写 JSON 不转义 ASCII 字母），由判据逐行对拍「过滤 / 不过滤」两向相等。

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Claude 的工具词表里会话事实要认的两张 ─────────────────────────────────────────
//
// ⚠ **它们是 Claude 的记录格式知识，却住在 `observe/`** —— 与 `user_inputs.rs` 的四条口径、
// `history_query::analyze_session` 认的 `ai-title` / `sessionKind` 同一处境（`observe/mod.rs` 头注：本层今天仍是 Claude 专属的；
// `agent_locality_guard` 诚实边界第 2 条：Claude 那半今天不在它的射程里）。
// 搬进适配层 `agents/claudecode/` 就要从这里直呼它 ⇒ `agent_locality_guard::NEW_AGENT_GAP_BASELINE` 26 → 27，
// 而那是只许降的棘轮 ⇒ 不抬。收进接口（`L2`/`S6`）时这两张随本文件一起走。
//
// 写类工具表**只有这一份**（前端那份随搬家删了）。

/// 写类工具 → 取路径的键。Edit / Write / MultiEdit 用 `file_path`；NotebookEdit 用 `notebook_path`。
/// 与渲染那边的「写类」（卡型 `diff`，适配层 `agents/claudecode/cards.rs` 那张写类工具表，行级 diff）**不是同一个问题**：
/// 这里多一个 NotebookEdit（它改文件、但不按行 diff 渲染）。
pub(crate) const EDIT_TOOL_PATH_KEYS: &[(&str, &str)] = &[
    ("Edit", "file_path"),
    ("Write", "file_path"),
    ("MultiEdit", "file_path"),
    ("NotebookEdit", "notebook_path"),
];

fn edit_path_key(name: &str) -> Option<&'static str> {
    EDIT_TOOL_PATH_KEYS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, k)| *k)
}

/// 改动文件集至多留多少条（超 ⇒ 丢最久没碰的）。
///
/// 为什么要上界：成品要原样回传当续传令牌，而一条请求行 ≤ `inbound::MAX_LINE_BYTES`（1 MiB）。
/// 1000 条 × 路径长（本机 623 份会话实测最长 136 字节）≈ 140 KB；同一批会话实测最多 **39** 条（`STC.md §1.4`）。
pub(crate) const TOUCHED_FILES_KEEP: usize = 1000;

/// 一份会话的事实（**帧面成品的形状，键名一字不差**；跨语言金样 `tests/__fixtures__/session-reads.golden.json`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SessionFacts {
    /// 最后一个完整行的末字节 ＝ 下一次的续点。
    pub(crate) end: u64,
    /// 源会话 sid；不是分叉来的 ⇒ `null`。
    pub(crate) forked_from: Option<String>,
    /// 写类工具碰过的文件（原样，可能相对 / Windows 路径），近因序：最近碰的在末尾。
    pub(crate) touched_files: Vec<String>,
    /// 最后一条带有效 usage 的 assistant 记录；一条都没有 ⇒ `null`。
    pub(crate) usage: Option<UsageFact>,
    /// 会话的项目目录（会话起在哪个目录）：适配层读记录开头给（`agents::project_dir_of`），读到即锁定；
    /// 开头里还没有 ⇒ `null`，下一次再读。与会话宣告那一帧的 `project_dir` 同一个函数。
    pub(crate) project_dir: Option<String>,
}

/// 最新 usage：context 占用的原料（上限表与百分比在前端 `views/context-limit.ts`，那是排版）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UsageFact {
    /// `input_tokens + cache_creation_input_tokens + cache_read_input_tokens`。
    pub(crate) prompt_tokens: u64,
    pub(crate) model: Option<String>,
}

/// 调用方交回来的 `prior` ⇒ [`SessionFacts`]。**形状必须恰好是本文件出的那一形**：缺格 / 多格 / 类型不对 ⇒ 拒
/// （serde 对 `Option` 缺格默认读成 `None`，所以键集合先逐层核一遍 —— 不猜）。
pub(crate) fn prior_from(v: &Value) -> Result<SessionFacts, String> {
    const TOP: &[&str] = &["end", "forkedFrom", "projectDir", "touchedFiles", "usage"];
    const USAGE: &[&str] = &["model", "promptTokens"];
    exact_keys(v, TOP, "prior")?;
    if !v["usage"].is_null() {
        exact_keys(&v["usage"], USAGE, "prior.usage")?;
    }
    serde_json::from_value(v.clone()).map_err(|e| format!("`prior` is not a facts product: {e}"))
}

fn exact_keys(v: &Value, want: &[&str], what: &str) -> Result<(), String> {
    let obj = v
        .as_object()
        .ok_or_else(|| format!("`{what}` must be an object"))?;
    let mut got: Vec<&str> = obj.keys().map(String::as_str).collect();
    got.sort_unstable();
    if got != want {
        return Err(format!("`{what}` keys {got:?} != {want:?}"));
    }
    Ok(())
}

/// 读 `r`（已定位在 `facts.end`）逐行累加到 `facts` 上，回累加后的那一份（`end` 推进到最后一个完整行的末字节）。
/// **纯 I/O 泛型**，单测直接喂字节。只看完整行（`\n` 收尾）；torn 残尾不看、不计进 `end`。
pub(crate) fn scan_facts<R: std::io::BufRead>(
    mut r: R,
    mut facts: SessionFacts,
) -> std::io::Result<SessionFacts> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let read = r.read_until(b'\n', &mut buf)?;
        if read == 0 || buf.last() != Some(&b'\n') {
            break;
        }
        facts.end += read as u64;
        let line = &buf[..buf.len() - 1];
        if !could_matter(line, &facts) {
            continue;
        }
        if let Some(v) = parse_line(line) {
            note_record(&mut facts, &v);
        }
    }
    Ok(facts)
}

fn parse_line(line: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(line);
    serde_json::from_str(text.trim_start_matches('\u{feff}').trim()).ok()
}

/// 这一行有没有可能改动事实（快路，见头注）。**只许放过、不许误拦**：凡是 [`note_record`] 会动的记录，这里必为真。
fn could_matter(line: &[u8], facts: &SessionFacts) -> bool {
    (facts.forked_from.is_none() && contains(line, b"\"forkedFrom\""))
        || contains(line, b"\"usage\"")
        || contains(line, b"\"tool_use\"")
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// 一条**已解析**的记录累加到 `f` 上。三格口径的唯一住址（见头注那张表）。
fn note_record(f: &mut SessionFacts, v: &Value) {
    if f.forked_from.is_none() {
        if let Some((sid, _)) = super::history_query::fork_origin(v) {
            f.forked_from = Some(sid);
        }
    }
    let blocks = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array);
    match v.get("type").and_then(Value::as_str) {
        Some("assistant") => {
            if let Some(blocks) = blocks {
                for b in blocks {
                    if b.get("type").and_then(Value::as_str) != Some("tool_use") {
                        continue;
                    }
                    let Some(name) = b.get("name").and_then(Value::as_str) else {
                        continue;
                    };
                    if let Some(key) = edit_path_key(name) {
                        if let Some(p) = b
                            .get("input")
                            .and_then(|i| i.get(key))
                            .and_then(Value::as_str)
                            .filter(|p| !p.is_empty())
                        {
                            touch(f, p);
                        }
                    }
                }
            }
            note_usage(f, v);
        }
        _ => {}
    }
}

/// 近因序去重：再碰一次 ⇒ 移到末尾；超上界 ⇒ 丢最久没碰的。
fn touch(f: &mut SessionFacts, p: &str) {
    if let Some(i) = f.touched_files.iter().position(|x| x == p) {
        f.touched_files.remove(i);
    }
    f.touched_files.push(p.to_string());
    if f.touched_files.len() > TOUCHED_FILES_KEEP {
        f.touched_files.remove(0);
    }
}

fn note_usage(f: &mut SessionFacts, v: &Value) {
    let msg = v.get("message");
    let Some(usage) = msg.and_then(|m| m.get("usage")).filter(|u| u.is_object()) else {
        return;
    };
    let num = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
    let prompt = num("input_tokens")
        .saturating_add(num("cache_creation_input_tokens"))
        .saturating_add(num("cache_read_input_tokens"));
    if prompt == 0 {
        return;
    }
    f.usage = Some(UsageFact {
        prompt_tokens: prompt,
        model: msg
            .and_then(|m| m.get("model"))
            .and_then(Value::as_str)
            .map(str::to_string),
    });
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/facts_query_tests.rs"]
mod tests;
