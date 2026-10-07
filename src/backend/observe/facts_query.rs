//! **会话事实**：分叉血缘 · 改动文件集 · 最新 usage（连同上下文上限）· 项目目录。
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
//! | `usage` | `assistant` 记录的 `message.usage` 三项 prompt token 之和 > 0 ⇒ `{promptTokens, model}`，文件序最后一条胜；`peakPromptTokens` 取全会话最大；上限见 [`context_limit`] |
//! | `projectDir` | 适配层 `RecordFace.project_dir`（只读记录开头）；读到即锁定，不在本文件的逐行扫描里 |
//! | `writers` | 这台 pidfile 里此刻持着这条会话的活进程（`observe::accounts_query::session_writers`）；每次现查，不在逐行扫描里 |
//! | `pending` | `assistant` 记录里的工具调用，等到 `user` 记录里同 id 的 `tool_result` 才摘；你又发了一句（`user` 记录里没有 `tool_result`）⇒ 全摘。文件序，至多 [`PENDING_KEEP`] 条 |
//! | `lastSay` | 文件序最后一段 `assistant` 正文（`text` 块）的头一个非空行，至多 [`SAY_CHARS`] 字 |
//! | `needs` | 那台 pidfile 说在等（[`PidWait`]）⇒ 配上 `pending` 判种类（[`needs_of`]）；每次现查，不累加 |
//! | `handedBack` | 交回了的子运行：`user` 记录「谁说的」是 agent 交回（适配层 `agents::user_text_of` 的 `AgentMessage { handback: true }`）⇒ 它的 `from`；去重、文件序，至多 [`HANDED_BACK_KEEP`] 条。同一个子运行的收场通知（`taskNotification.taskId` ＝ 这个 id）以交回为准，界面不再另画 |
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

/// 交互工具：在等你**回答**的 · 在等你**批准计划**的。别的工具在等 ⇒ 等你批准它（`waitingFor` 说是批准框时）。
const ANSWER_TOOLS: &[&str] = &["AskUserQuestion"];
const PLAN_TOOLS: &[&str] = &["ExitPlanMode"];

/// 工具 → 「它在做什么」那一格取哪个入参（一行人话的主参数）。没登记的工具 ⇒ 不给（界面只写工具名）。
const WHAT_KEYS: &[(&str, &str)] = &[
    ("Bash", "command"),
    ("Read", "file_path"),
    ("Edit", "file_path"),
    ("Write", "file_path"),
    ("MultiEdit", "file_path"),
    ("NotebookEdit", "notebook_path"),
    ("Grep", "pattern"),
    ("Glob", "pattern"),
    ("WebFetch", "url"),
    ("WebSearch", "query"),
    ("Task", "description"),
    ("Agent", "description"),
];

/// 没结果的工具调用至多留几条（并发调用一批也就几条；超 ⇒ 丢最早的）。
pub(crate) const PENDING_KEEP: usize = 16;

/// 交回了的子运行至多留多少个（超 ⇒ 丢最早的）。成品要原样回传当续传令牌，一个 id 几十字节。
pub(crate) const HANDED_BACK_KEEP: usize = 500;
/// 一行人话至多几个字（工具主参数 · 最后一段正文的头一行）。
pub(crate) const SAY_CHARS: usize = 160;

/// 头一个非空行，压成一行、至多 [`SAY_CHARS`] 字（超 ⇒ 截断加 `…`）。空 ⇒ `None`。
fn one_line(s: &str) -> Option<String> {
    let line = s.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut out: String = line.chars().take(SAY_CHARS).collect();
    if line.chars().count() > SAY_CHARS {
        out.push('…');
    }
    Some(out)
}

/// 一个工具调用的主参数一行（[`WHAT_KEYS`]；提问工具取第一问）。
fn what_of(name: &str, input: Option<&Value>) -> Option<String> {
    let input = input?;
    if ANSWER_TOOLS.contains(&name) {
        return input
            .get("questions")
            .and_then(Value::as_array)
            .and_then(|q| q.first())
            .and_then(|q| q.get("question"))
            .and_then(Value::as_str)
            .and_then(one_line);
    }
    let key = WHAT_KEYS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, k)| *k)?;
    input.get(key).and_then(Value::as_str).and_then(one_line)
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
    /// 这份记录是哪一家的（线上的 kind，适配层按记录认：`agents::record_kind_of`）；认不出 ⇒ `null`（界面要分家的那几项灰着，不落哪一家）。
    pub(crate) agent: Option<String>,
    /// 此刻持着这条会话的活进程 pid（这台的 pidfile，判活同会话宣告那一路），升序；不累加，每次现查（`prior` 里那一份不用）。
    /// 不止一个 ⇒ 同一条会话有几个进程在同时写。不留 pidfile 的那一家恒空。
    pub(crate) writers: Vec<u32>,
    /// 还没有结果的工具调用（文件序，正在跑 / 在等批准的那几步）。
    pub(crate) pending: Vec<PendingCall>,
    /// 最后一段正文的头一行（悬停卡「它最后一句」）。
    pub(crate) last_say: Option<LastSay>,
    /// **需要你**：那台说在等、等的是什么（不累加，每次现查；`prior` 里那一份不用）。不在等 ⇒ `null`。
    pub(crate) needs: Option<Needs>,
    /// 交回了的子运行（子 agent 的 id，文件序、去重）：同一个子运行的收场通知以交回为准，只报一次。
    pub(crate) handed_back: Vec<String>,
}

/// 一个还没有结果的工具调用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PendingCall {
    /// 工具调用 id（等它的 `tool_result` 来摘）。
    pub(crate) id: String,
    /// 工具名原样。
    pub(crate) name: String,
    /// 主参数一行（[`WHAT_KEYS`]；提问取第一问）；没登记 / 入参里没有 ⇒ `null`。
    pub(crate) what: Option<String>,
    /// 那条记录的 `timestamp` 原样（从何时起在跑）；没有 ⇒ `null`。
    pub(crate) at: Option<String>,
}

/// 最后一段正文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LastSay {
    pub(crate) text: String,
    /// 那条记录的 `timestamp` 原样；没有 ⇒ `null`。
    pub(crate) at: Option<String>,
}

/// 「需要你」的种类。判不出 ⇒ `Unknown`（只说在等你，不猜）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum NeedsKind {
    /// 批准一个工具调用。
    Approve,
    /// 回答一个问题。
    Answer,
    /// 批准计划。
    Plan,
    Unknown,
}

/// **需要你**的成品：种类 · 等的那一句 · 从何时起等。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Needs {
    pub(crate) kind: NeedsKind,
    /// 等的是哪个工具调用（工具名原样）；判不出 ⇒ `null`。
    pub(crate) tool: Option<String>,
    /// 那个调用的 id（记录里 `tool_use` 的 `id`）：界面据此把过程里那一步画成「在等你批准」；判不出 ⇒ `null`。
    pub(crate) call: Option<String>,
    /// 批准：那一步的主参数；回答：问题原文头一行；计划 / 判不出 ⇒ `null`。
    pub(crate) what: Option<String>,
    /// 何时起等（那台 pidfile 的 `statusUpdatedAt`，epoch ms）；没有 ⇒ `null`。
    pub(crate) since_ms: Option<u64>,
}

/// 那台 pidfile 说「在等」（`observe::accounts_query::session_wait`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PidWait {
    /// `waitingFor` 原样（`permission prompt` · `dialog open` …）。
    pub(crate) waiting_for: Option<String>,
    pub(crate) since_ms: Option<u64>,
}

/// **「需要你」的唯一判定**：那台说在等 ＋ 记录里最早一个还没结果的调用。
/// - 那个调用是提问工具 ⇒ 回答（问题原文）；是计划工具 ⇒ 批准计划；
/// - 别的工具、且那台说是批准框（`waitingFor` 带 `permission`）⇒ 批准（那一步的主参数）；
/// - 其余（没有没结果的调用 · 说不出是哪种框）⇒ 判不出，不猜。
pub(crate) fn needs_of(pending: &[PendingCall], wait: Option<&PidWait>) -> Option<Needs> {
    let wait = wait?;
    let first = pending.first();
    let asks = pending
        .iter()
        .find(|p| ANSWER_TOOLS.contains(&p.name.as_str()));
    let plan = pending
        .iter()
        .find(|p| PLAN_TOOLS.contains(&p.name.as_str()));
    let permission = wait
        .waiting_for
        .as_deref()
        .is_some_and(|w| w.to_ascii_lowercase().contains("permission"));
    let (kind, call) = if let Some(a) = asks {
        (NeedsKind::Answer, Some(a))
    } else if let Some(p) = plan {
        (NeedsKind::Plan, Some(p))
    } else if let (Some(p), true) = (first, permission) {
        (NeedsKind::Approve, Some(p))
    } else {
        (NeedsKind::Unknown, None)
    };
    Some(Needs {
        kind,
        tool: call.map(|c| c.name.clone()),
        call: call.map(|c| c.id.clone()),
        what: call.and_then(|c| c.what.clone()),
        since_ms: wait.since_ms,
    })
}

/// 最新 usage ＋ 这份会话的上下文上限（状态栏与监控板读同一个数；百分比是排版，在前端）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UsageFact {
    /// `input_tokens + cache_creation_input_tokens + cache_read_input_tokens`。
    pub(crate) prompt_tokens: u64,
    pub(crate) model: Option<String>,
    /// 全会话最大的一轮（见过超过标准上限的 ⇒ 上限不可能是标准那一档）。
    pub(crate) peak_prompt_tokens: u64,
    /// 上下文上限（tokens），恒 ≥ `peak_prompt_tokens` ⇒ 百分比不会超过 100。
    pub(crate) limit: u64,
    pub(crate) limit_from: LimitFrom,
}

/// 上限从哪来。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum LimitFrom {
    /// 中转看见了这个会话的请求：带了扩展上下文那一项 ⇒ 1M，没带过 ⇒ 这个模型的默认（`observe::relay_marks`）。
    Relay,
    /// 设置里的上限表（按模型名子串）。
    Setting,
    /// 模型名自己带着 `[1m]`。
    Model,
    /// 这份会话见过超过标准上限的一轮。
    Observed,
    /// 判不出（`limit` 只是占位的 1M）：界面不算百分比，只写用了多少。
    Assumed,
}

/// 标准上下文（200k，模型的默认）与扩展那一档（1M）。记录里的模型名是接口回的名字、不带 `[1m]`，
/// 真来源是中转看见的请求（扩展上下文的请求带那一项）。
pub(crate) const CONTEXT_STANDARD: u64 = 200_000;
pub(crate) const CONTEXT_EXTENDED: u64 = 1_000_000;

/// 设置里的上限表：模型名子串（小写）→ 上限。调用方随请求交来（`history-facts` 的 `limits`）。
pub(crate) type ContextLimits = Vec<(String, u64)>;

/// `limits` 入参 ⇒ [`ContextLimits`]。缺席 / `null` ⇒ 空表；不是「串 → 正整数」的对象 ⇒ 拒。
pub(crate) fn limits_from(v: Option<&Value>) -> Result<ContextLimits, String> {
    let Some(v) = v.filter(|v| !v.is_null()) else {
        return Ok(Vec::new());
    };
    let obj = v
        .as_object()
        .ok_or_else(|| "`limits` must be an object".to_string())?;
    obj.iter()
        .map(|(k, n)| match n.as_u64() {
            Some(n) if n > 0 && !k.trim().is_empty() => Ok((k.trim().to_lowercase(), n)),
            _ => Err(format!("`limits.{k}` must be a positive integer")),
        })
        .collect()
}

/// **上下文上限的唯一判定**：中转看见过这个会话的请求（`relay`）⇒ 带过扩展上下文那一项是 1M、没带过是默认 200k；
/// 中转没看见过 ⇒ 设置里的上限表（最长匹配的子串胜）> 模型名带 `[1m]` > 见过超过 200k 的一轮 > 判不出（占位 1M，`Assumed`）。
/// 任何一档给出的数小于见过的最大一轮 ⇒ 那一档不对，按「见过」那一档（上限至少是 1M，再大就是见过的那么大）。
pub(crate) fn context_limit(
    model: Option<&str>,
    peak: u64,
    limits: &ContextLimits,
    relay: Option<bool>,
) -> (u64, LimitFrom) {
    let m = model.unwrap_or("").to_lowercase();
    let from_relay = relay.map(|wide| {
        let n = if wide {
            CONTEXT_EXTENDED
        } else {
            CONTEXT_STANDARD
        };
        (n, LimitFrom::Relay)
    });
    let setting = limits
        .iter()
        .filter(|(sub, _)| m.contains(sub.as_str()))
        .max_by_key(|(sub, _)| sub.len())
        .map(|(_, n)| (*n, LimitFrom::Setting));
    let chosen = from_relay.or(setting).unwrap_or(if m.contains("[1m]") {
        (CONTEXT_EXTENDED, LimitFrom::Model)
    } else if peak > CONTEXT_STANDARD {
        (CONTEXT_EXTENDED, LimitFrom::Observed)
    } else {
        (CONTEXT_EXTENDED, LimitFrom::Assumed)
    });
    if chosen.0 < peak {
        (peak.max(CONTEXT_EXTENDED), LimitFrom::Observed)
    } else {
        chosen
    }
}

/// 调用方交回来的 `prior` ⇒ [`SessionFacts`]。**形状必须恰好是本文件出的那一形**：缺格 / 多格 / 类型不对 ⇒ 拒
/// （serde 对 `Option` 缺格默认读成 `None`，所以键集合先逐层核一遍 —— 不猜）。
pub(crate) fn prior_from(v: &Value) -> Result<SessionFacts, String> {
    const TOP: &[&str] = &[
        "agent",
        "end",
        "forkedFrom",
        "handedBack",
        "lastSay",
        "needs",
        "pending",
        "projectDir",
        "touchedFiles",
        "usage",
        "writers",
    ];
    const USAGE: &[&str] = &[
        "limit",
        "limitFrom",
        "model",
        "peakPromptTokens",
        "promptTokens",
    ];
    exact_keys(v, TOP, "prior")?;
    if !v["usage"].is_null() {
        exact_keys(&v["usage"], USAGE, "prior.usage")?;
    }
    if !v["lastSay"].is_null() {
        exact_keys(&v["lastSay"], &["at", "text"], "prior.lastSay")?;
    }
    if !v["needs"].is_null() {
        exact_keys(
            &v["needs"],
            &["call", "kind", "sinceMs", "tool", "what"],
            "prior.needs",
        )?;
    }
    for p in v["pending"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
        exact_keys(p, &["at", "id", "name", "what"], "prior.pending[]")?;
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
/// 上限每次按此刻的 `limits` 与中转看见的（`relay`）重判（设置改过 / 中转看见了 ⇒ 带着上一份成品再问一次就是新的数）。
pub(crate) fn scan_facts<R: std::io::BufRead>(
    mut r: R,
    mut facts: SessionFacts,
    limits: &ContextLimits,
    relay: Option<bool>,
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
    if let Some(u) = facts.usage.as_mut() {
        (u.limit, u.limit_from) =
            context_limit(u.model.as_deref(), u.peak_prompt_tokens, limits, relay);
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
        || (contains(line, b"\"assistant\"") && contains(line, b"\"text\""))
        // 交回：记录级 `origin.handback`（键名在行里）。
        || contains(line, b"\"handback\"")
        // 有没结果的调用：它的结果（行里带着它的 id）· 你又发了一句（`user` 记录、没有工具结果）。
        // 别人的工具结果（常是整份文件内容）照旧连解析都不做。
        || (!facts.pending.is_empty()
            && contains(line, b"\"user\"")
            && (!contains(line, b"\"tool_result\"")
                || facts.pending.iter().any(|p| contains(line, p.id.as_bytes()))))
}

/// 这条是 agent 交回 ⇒ 那个子运行进 `handed_back`（谁说的由适配层判，口径同全局搜索的「agent 回报」）。
fn note_handback(f: &mut SessionFacts, v: &Value) {
    let kind = crate::agents::record_tree_kind().unwrap_or_default();
    let Some(said) = crate::agents::user_text_of(kind, v) else {
        return;
    };
    let crate::agents::Speaker::AgentMessage {
        from: Some(from),
        handback: true,
        ..
    } = said.speaker
    else {
        return;
    };
    if f.handed_back.contains(&from) {
        return;
    }
    f.handed_back.push(from);
    if f.handed_back.len() > HANDED_BACK_KEEP {
        f.handed_back.remove(0);
    }
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
    let at = v
        .get("timestamp")
        .and_then(Value::as_str)
        .map(str::to_string);
    match v.get("type").and_then(Value::as_str) {
        Some("user") => note_user(f, v),
        Some("assistant") => {
            if let Some(blocks) = blocks {
                for b in blocks {
                    if b.get("type").and_then(Value::as_str) == Some("text") {
                        if let Some(text) = b.get("text").and_then(Value::as_str).and_then(one_line)
                        {
                            f.last_say = Some(LastSay {
                                text,
                                at: at.clone(),
                            });
                        }
                        continue;
                    }
                    if b.get("type").and_then(Value::as_str) != Some("tool_use") {
                        continue;
                    }
                    let Some(name) = b.get("name").and_then(Value::as_str) else {
                        continue;
                    };
                    if let Some(id) = b
                        .get("id")
                        .and_then(Value::as_str)
                        .filter(|i| !i.is_empty())
                    {
                        f.pending.retain(|p| p.id != id);
                        f.pending.push(PendingCall {
                            id: id.to_string(),
                            name: name.to_string(),
                            what: what_of(name, b.get("input")),
                            at: at.clone(),
                        });
                        if f.pending.len() > PENDING_KEEP {
                            f.pending.remove(0);
                        }
                    }
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

/// `user` 记录：是 agent 交回 ⇒ 记下那个子运行；带着工具结果 ⇒ 摘掉结果对上的那几个调用；
/// 没有工具结果（你发的一句 · 中断）⇒ 没结果的全摘（那一轮过去了）。
fn note_user(f: &mut SessionFacts, v: &Value) {
    note_handback(f, v);
    if f.pending.is_empty() {
        return;
    }
    let content = v.get("message").and_then(|m| m.get("content"));
    let results: Vec<&str> = content
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
                .filter_map(|b| b.get("tool_use_id").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let has_result_block = content.and_then(Value::as_array).is_some_and(|a| {
        a.iter()
            .any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
    });
    if has_result_block {
        f.pending.retain(|p| !results.contains(&p.id.as_str()));
    } else {
        f.pending.clear();
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
    let peak = f
        .usage
        .as_ref()
        .map_or(0, |u| u.peak_prompt_tokens)
        .max(prompt);
    let model = msg
        .and_then(|m| m.get("model"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let (limit, limit_from) = context_limit(model.as_deref(), peak, &Vec::new(), None);
    f.usage = Some(UsageFact {
        prompt_tokens: prompt,
        model,
        peak_prompt_tokens: peak,
        limit,
        limit_from,
    });
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/facts_query_tests.rs"]
mod tests;
