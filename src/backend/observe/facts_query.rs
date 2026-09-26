//! 〔STC · `设计/90 §4` 阶段 C · `设计/10 §2.2`〕**会话事实**：分叉血缘 · 改动文件集 · agent 列表 · 最新 usage。
//!
//! # 它顶掉了什么
//!
//! 这四样从前是活 tab 在 `onLine` 旁路上一条一条攒的（前端 `tab-session-facts.ts` 那四个抽取器，已删）：
//! **到达序不是对话序**（重放是尾块先到）、**不完整**（重放缓冲每个会话只留尾部 `REPLAY_TAIL_KEEP` 条 ⇒
//! F5 之后长会话的分叉血缘看不见、agent / 改动文件只剩尾巴那一截）、每个 tab 各攒一份（`10 §2.2` 那三个病）。
//! 今天它们由本文件读一遍文件算出来，经帧命令 `history-facts`（宿主 `read_face.rs`）出**成品**，
//! 界面经通道直接问、按形状收（`src/session-reads.ts`），**本机与远端同一条路**。
//!
//! # 续传令牌就是上一份成品（后端零状态）
//!
//! 事实要跟着会话长，而大会话（本机实测 120 MB）每批整份重扫是撞墙的。选的形状：调用方把**上一次的应答原样**
//! 交回来（`prior`），本文件从 `prior.end` 接着扫、把新的一截累加在它上面 —— 判定与累加都只在这里，
//! 前端不读、不改、不合并那一份成品；后端不留任何状态（`设计/05 §15.1` 对 `history-lines` 的同一条取舍）。
//! 续点的两道校验（截断 · 不在行边界）在 `history_query::open_facts_at`。理由全文 `调研/第四波记录/STC.md §1.2`。
//!
//! # 口径（逐格，四样各一个住址）
//!
//! | 格 | 口径 |
//! |---|---|
//! | `forkedFrom` | `history_query::fork_origin`（与历史会话行同一个函数）；首条命中即锁定 |
//! | `touchedFiles` | `assistant` 记录里写类工具（[`EDIT_TOOL_PATH_KEYS`]）的路径，去重、**近因序**（再碰一次移到末尾），至多 [`TOUCHED_FILES_KEEP`] 条 |
//! | `agents` | `assistant` 里 agent 工具（[`AGENT_TOOLS`]）的 `tool_use` ⇒ `running`；`user` 里命中的 `tool_result` ⇒ `done`；超 [`AGENTS_SOFT_KEEP`] 从最老删非 running，再超 [`AGENTS_HARD_KEEP`] 删最老 |
//! | `usage` | `assistant` 记录的 `message.usage` 三项 prompt token 之和 > 0 ⇒ `{promptTokens, model}`，文件序最后一条胜 |
//!
//! 「中止」不在这里：它是「会话落到不忙那一刻」这个**事件**的反应（`10 §2.2`「刚刚发生了什么留在流上」），住前端。
//!
//! # 快路
//!
//! 一行要不要解析，先按字节看它有没有可能改动事实（[`could_matter`]）：分叉已锁 ⇒ 不找 `"forkedFrom"`；
//! 没有 running 的 agent ⇒ 工具结果那一大类（常是整份文件内容）连解析都不做。**只省时间、不改结果**：
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
// ⚠ agent 工具名今天有两个 Rust 住址：monitor `adapter.rs::CLAUDE_AGENT_TOOLS`（经生成物
// `src/generated/agent-profile-table.ts` 喂前端**渲染** agent 卡）与这里（喂**会话事实**）。两半不许在编译期互咬
// （`设计/90 §0` 的 `C2`），今天没有能放它的共享 crate ⇒ 由一条异源对拍钉住（本文件测试：这里 == 生成物里 claude 那一行
// 的 `agentTools`）。登记在「判定 → 唯一住址」那张表（DUP1，`tests/judgment-single-home.vitest.ts` 的 J19，`open`：收成一份等主会话拍），见 `调研/第四波记录/STC.md §1.3`。
// 写类工具表**只有这一份**（前端 `panorama/session-files.ts` 整份随搬家删了）。

/// 展开 = 子会话的工具（Claude Code 的 `Task`，新版改名 `Agent`，两个都认）。
pub(crate) const AGENT_TOOLS: &[&str] = &["Agent", "Task"];

/// 写类工具 → 取路径的键。Edit / Write / MultiEdit 用 `file_path`；NotebookEdit 用 `notebook_path`。
/// 与渲染那边的「写类」（`adapter.rs::CLAUDE_DIFF_TOOLS`，行级 diff）**不是同一个问题**：
/// 这里多一个 NotebookEdit（它改文件、但不按行 diff 渲染）。
pub(crate) const EDIT_TOOL_PATH_KEYS: &[(&str, &str)] = &[
    ("Edit", "file_path"),
    ("Write", "file_path"),
    ("MultiEdit", "file_path"),
    ("NotebookEdit", "notebook_path"),
];

fn is_agent_tool(name: &str) -> bool {
    AGENT_TOOLS.contains(&name)
}

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

/// agent 列表的软上界（逐字搬自前端旧口径）：超过就从最老的开始删**非 running** 的。
pub(crate) const AGENTS_SOFT_KEEP: usize = 30;

/// agent 列表的硬上界：全是 running 仍超过 ⇒ 删最老的（同上，为回传那 1 MiB）。旧口径没有这一道。
pub(crate) const AGENTS_HARD_KEEP: usize = 200;

/// `label` 在没有 `description` 时取 `prompt` 首行的前多少个字（Unicode 标量）。
pub(crate) const LABEL_PROMPT_CHARS: usize = 80;

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
    /// 插入序（同一个 id 再来一次原位替换）。
    pub(crate) agents: Vec<AgentFact>,
    /// 最后一条带有效 usage 的 assistant 记录；一条都没有 ⇒ `null`。
    pub(crate) usage: Option<UsageFact>,
}

/// 一次 agent 调用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentFact {
    /// `tool_use` 的 id（配对 `tool_result` 用）。
    pub(crate) id: String,
    /// 显示用：`description` ‖ `prompt` 首行前 [`LABEL_PROMPT_CHARS`] 字 ‖ 工具名。
    pub(crate) label: String,
    /// `subagent_type`；没有 ⇒ `null`。
    pub(crate) agent_type: Option<String>,
    pub(crate) status: AgentStatus,
    /// 产出这次调用的那条记录的 `timestamp`（点进去看子 agent 记录时定位用）；没有 ⇒ 空串。
    pub(crate) timestamp: String,
    /// trim 后的 `description`（按它**精确串等**找子 agent 记录，所以不能用 `label`）。
    pub(crate) desc: String,
}

/// jsonl 看得出来的两态。「中止」是前端对事件的反应，不在这里（见头注）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AgentStatus {
    Running,
    Done,
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
    const TOP: &[&str] = &["agents", "end", "forkedFrom", "touchedFiles", "usage"];
    const AGENT: &[&str] = &["agentType", "desc", "id", "label", "status", "timestamp"];
    const USAGE: &[&str] = &["model", "promptTokens"];
    exact_keys(v, TOP, "prior")?;
    for a in v["agents"]
        .as_array()
        .ok_or("`prior.agents` must be an array")?
    {
        exact_keys(a, AGENT, "prior.agents[]")?;
    }
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
        || (facts
            .agents
            .iter()
            .any(|a| a.status == AgentStatus::Running)
            && contains(line, b"\"tool_result\""))
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// 一条**已解析**的记录累加到 `f` 上。四格口径的唯一住址（见头注那张表）。
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
                let timestamp = v.get("timestamp").and_then(Value::as_str).unwrap_or("");
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
                    if is_agent_tool(name) {
                        if let Some(id) = b.get("id").and_then(Value::as_str) {
                            upsert_agent(f, agent_of(id, name, b.get("input"), timestamp));
                        }
                    }
                }
                cap_agents(f);
            }
            note_usage(f, v);
        }
        Some("user") => {
            for b in blocks.into_iter().flatten() {
                if b.get("type").and_then(Value::as_str) != Some("tool_result") {
                    continue;
                }
                let Some(id) = b.get("tool_use_id").and_then(Value::as_str) else {
                    continue;
                };
                if let Some(a) = f
                    .agents
                    .iter_mut()
                    .find(|a| a.id == id && a.status == AgentStatus::Running)
                {
                    a.status = AgentStatus::Done;
                }
            }
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

/// trim：Unicode 空白 ＋ BOM（旧口径是 JS `String.prototype.trim`；两个集合只差 U+0085，那个字不会出现在工具描述里）。
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

fn agent_of(id: &str, name: &str, input: Option<&Value>, timestamp: &str) -> AgentFact {
    let field = |k: &str| input.and_then(|i| i.get(k)).and_then(Value::as_str);
    let desc = js_trim(field("description").unwrap_or("")).to_string();
    let prompt_head: String = field("prompt")
        .unwrap_or("")
        .split('\n')
        .next()
        .unwrap_or("")
        .chars()
        .take(LABEL_PROMPT_CHARS)
        .collect();
    let label = if !desc.is_empty() {
        desc.clone()
    } else if !prompt_head.is_empty() {
        prompt_head
    } else {
        name.to_string()
    };
    AgentFact {
        id: id.to_string(),
        label,
        agent_type: field("subagent_type").map(str::to_string),
        status: AgentStatus::Running,
        timestamp: timestamp.to_string(),
        desc,
    }
}

/// 同一个 id 再来一次 ⇒ 原位替换（旧口径 `Map.set` 的语义：位置不变、内容与状态换新）。
fn upsert_agent(f: &mut SessionFacts, a: AgentFact) {
    match f.agents.iter_mut().find(|x| x.id == a.id) {
        Some(slot) => *slot = a,
        None => f.agents.push(a),
    }
}

/// 软上界：从最老的开始删非 running 的，删到不超为止；硬上界：仍超 ⇒ 删最老的。
/// 旧口径每条带内容数组的 assistant 记录都走一次（不只是这条记录加了 agent 的时候）—— 照搬。
fn cap_agents(f: &mut SessionFacts) {
    let mut i = 0;
    while f.agents.len() > AGENTS_SOFT_KEEP && i < f.agents.len() {
        if f.agents[i].status == AgentStatus::Running {
            i += 1;
        } else {
            f.agents.remove(i);
        }
    }
    while f.agents.len() > AGENTS_HARD_KEEP {
        f.agents.remove(0);
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
