//! Claude 记录的文本知识：`message.content` 里正文块 / 工具内容怎么抽 · **一条 user 记录是谁说的**（[`user_text`]，
//! `INVARIANTS §20`：CLI 注入的、agent 发来的都不算用户说的话）。
//!
//! 「谁说的」只有这一份：解析成品（`schema.rs::with_user_text` 填 `userText`）· 全局搜索 · 会话内查找 · 历史摘录 ·
//! 用户输入列表 · 骨架索引 · 子运行的收场都读它；通用层（`observe/`）经注册表 `agents::TextFace` 够它，不按名字够。
//!
//! 认法：先看记录级字段（`origin.kind` · `isCompactSummary` · `isMeta` · 子 agent 的首条）；没有字段的（排队消息 · 老版本）
//! 再认**具名**的框与固定句。认不出的归人 —— 宁可漏判，不许吃掉人话；也不拿「以 `<` 开头」当判据（人真会这么打字）。

use serde_json::Value;

// ── 文本抽取 ─────────────────────────────────────────────────────────────

/// 抽 `content` 里所有 text block（或裸字符串）。user 正文 / assistant 正文都用它。
pub(crate) fn extract_text_blocks(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let mut out = String::new();
            for b in arr {
                if b.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(s) = b.get("text").and_then(Value::as_str) {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(s);
                    }
                }
            }
            out
        }
        _ => String::new(),
    }
}

/// 抽 tool 相关内容（可选搜索）。
/// - assistant：`tool_use`（name + input JSON）+ `thinking`
/// - user：`tool_result` 的 content
pub(crate) fn extract_tool_text(content: &Value, is_assistant: bool) -> String {
    let Value::Array(arr) = content else {
        return String::new();
    };
    let mut out = String::new();
    let mut push = |s: &str| {
        if s.is_empty() {
            return;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(s);
    };
    for b in arr {
        match b.get("type").and_then(Value::as_str) {
            Some("tool_use") if is_assistant => {
                if let Some(name) = b.get("name").and_then(Value::as_str) {
                    push(name);
                }
                if let Some(input) = b.get("input") {
                    push(&stringify_json(input));
                }
            }
            Some("thinking") if is_assistant => {
                if let Some(t) = b.get("thinking").and_then(Value::as_str) {
                    push(t);
                }
            }
            Some("tool_result") if !is_assistant => {
                if let Some(c) = b.get("content") {
                    push(&stringify_json(c));
                }
            }
            _ => {}
        }
    }
    out
}

/// 把 JSON 值压成可搜索的纯文本（string 直接取；array/object 取其中字符串叶子）。
pub(crate) fn stringify_json(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let mut out = String::new();
            for item in arr {
                // `tool_result.content` 常是 `[{type:"text", text:"..."}]`
                let s = if let Some(t) = item.get("text").and_then(Value::as_str) {
                    t.to_string()
                } else {
                    stringify_json(item)
                };
                if !s.is_empty() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&s);
                }
            }
            out
        }
        Value::Object(_) => v.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
    }
}

use crate::agents::{Pasted, Speaker, UserText};

/// 剥掉的 CLI 注入包装（人话里夹着它们时只剥它们，人话留着）。
const INJECTED_WRAPPERS: [&str; 5] = [
    "task-notification",
    "system-reminder",
    "local-command-caveat",
    "local-command-stdout",
    "local-command-stderr",
];

/// 整行就是它、不分大小写（句号可省）⇒ CLI 续跑样板，整行剥。
const BOILERPLATE_LINES: [&str; 2] = ["continue from where you left off", "no response requested"];

/// 框前面的固定前导行（行首匹配）：后台通知的那四句 · 子 agent 来话的那一句。跟着框一起归到非人。
const LEAD_LINES: [&str; 5] = [
    NOTICE_MARK,
    "This is an automated background-task event",
    "Do NOT interpret this as user acknowledgement",
    "No human input has been received since the last genuine user message",
    "Another Claude session sent a message",
];
/// 后台通知的标记行：它在就是通知（不论后面跟没跟框）。
const NOTICE_MARK: &str = "[SYSTEM NOTIFICATION - NOT USER INPUT]";
pub(crate) const NOTICE_OPEN: &str = "<task-notification>";
const NOTICE_CLOSE: &str = "</task-notification>";
/// 子 agent 那一侧：主会话后来发给它的话以这一行起头。
const COORDINATOR_LEAD: &str = "The coordinator sent a message while you were working:";
/// 那段话后面 CLI 附的一句收尾（不是主会话说的）。
const COORDINATOR_TAIL: &str = "Address this before completing your current task.";
/// 压缩摘要的开头（老版本没有 `isCompactSummary` 字段时靠它）。
const COMPACT_LEAD: &str = "This session is being continued from a previous conversation";
/// 额度恢复后系统替人排进来的续跑话。
const LIMIT_RESET_LEAD: &str = "Your claude.ai usage limit has reset.";
/// 分叉出来的子 agent 收到的那段派活说明。
const FORK_OPEN: &str = "<fork-boilerplate>";
const PASTE_OPEN: &str = "<pasted_content";
const PASTE_CLOSE: &str = "</pasted_content";

/// 判「谁说的」要看的几格（解析成品与通用层的已解析 JSON 各取一份，判定是同一个函数）。
pub(crate) struct Facts<'a> {
    pub(crate) content: &'a Value,
    pub(crate) is_meta: bool,
    pub(crate) is_sidechain: bool,
    pub(crate) is_compact_summary: bool,
    pub(crate) has_parent: bool,
    /// 记录级的 `origin`（新版 CLI 写：`kind` ∈ human / peer / task-notification / coordinator / auto-continuation）。
    pub(crate) origin: Option<&'a Value>,
}

impl<'a> Facts<'a> {
    fn of(v: &'a Value) -> Option<Self> {
        if v.get("type").and_then(Value::as_str) != Some("user") {
            return None;
        }
        let flag = |k: &str| v.get(k).and_then(Value::as_bool) == Some(true);
        Some(Self {
            content: v
                .get("message")
                .and_then(|m| m.get("content"))
                .unwrap_or(&Value::Null),
            is_meta: flag("isMeta"),
            is_sidechain: flag("isSidechain"),
            is_compact_summary: flag("isCompactSummary"),
            has_parent: v
                .get("parentUuid")
                .and_then(Value::as_str)
                .is_some_and(|p| !p.is_empty()),
            origin: v.get("origin").filter(|o| o.is_object()),
        })
    }
}

/// 注册表 `TextFace.user`：一条已解析的记录 ⇒ 它是谁说的；不是 user 记录 ⇒ `None`。
pub(crate) fn user_text_of_record(v: &Value) -> Option<UserText> {
    Facts::of(v).map(|f| user_text(&f))
}

/// **一条 user 记录是谁说的**（规则只此一份）。
pub(crate) fn user_text(f: &Facts) -> UserText {
    let raw = extract_text_blocks(f.content);
    let has_result = f.content.as_array().is_some_and(|a| {
        a.iter()
            .any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
    });
    if has_result && raw.trim().is_empty() {
        return UserText::of(Speaker::ToolResult);
    }
    if f.is_compact_summary {
        return shown(Speaker::CompactSummary, &raw);
    }
    match f.origin.and_then(|o| o.get("kind")).and_then(Value::as_str) {
        Some("human") if !f.is_meta => return spoken(&raw),
        Some("peer") => return UserText::of(peer(f.origin, &raw)),
        Some("task-notification") => return UserText::of(notice(&raw)),
        Some("coordinator") => {
            return UserText::of(Speaker::Coordinator {
                body: origin_body(f.origin).or_else(|| coordinator_body(&raw)),
            })
        }
        Some("auto-continuation") => return UserText::of(Speaker::System),
        _ => {}
    }
    if let Some(t) = framed(&raw, f.is_sidechain) {
        return t;
    }
    if f.is_meta {
        return UserText::of(Speaker::System);
    }
    if f.is_sidechain && !f.has_parent && f.origin.is_none() {
        return shown(Speaker::AgentTask, &raw);
    }
    spoken(&raw)
}

/// 排队消息（`queue-operation` 的 `content`）是谁说的：它没有记录级字段，只认具名框与固定句，认不出归人。
pub(crate) fn queued_text(content: &str) -> UserText {
    framed(content, false).unwrap_or_else(|| spoken(content))
}

/// 整条是不是 ESC 中断标记（子运行被叫停的收场认它）。
pub(crate) fn is_interrupt_content(content: &Value) -> bool {
    is_interrupt(&strip_injected(&extract_text_blocks(content)))
}

/// 后台任务收场通知里的几格（第一个框；值都排在正文之前）。
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Notice {
    pub(crate) task_id: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) tool_use_id: Option<String>,
}

/// 一段正文是后台任务的收场通知（前导句可有可无，框必须在）⇒ 它的几格；否则 `None`。
pub(crate) fn task_notice(text: &str) -> Option<Notice> {
    let body = after_lead(text).strip_prefix(NOTICE_OPEN)?;
    let body = &body[..body.find(NOTICE_CLOSE).unwrap_or(body.len())];
    let get = |name: &str| {
        inner(body, name)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    Some(Notice {
        task_id: get("task-id"),
        status: get("status"),
        summary: get("summary"),
        tool_use_id: get("tool-use-id"),
    })
}

fn notice(raw: &str) -> Speaker {
    let n = task_notice(raw).unwrap_or_default();
    Speaker::TaskNotification {
        task_id: n.task_id,
        status: n.status,
        summary: n.summary,
        tool_use_id: n.tool_use_id,
    }
}

/// 记录级说是 agent 来的话（`origin.kind = peer`）：字段从 `origin` 取，缺的再看框上的 `from`。
fn peer(origin: Option<&Value>, raw: &str) -> Speaker {
    let field = |k: &str| {
        origin
            .and_then(|o| o.get(k))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let t = after_lead(raw);
    if opens(t, "cross-session-message") {
        return Speaker::PeerSession {
            from: field("from").or_else(|| attr(t, "from")),
            body: origin_body(origin).or_else(|| frame_body(t, "cross-session-message")),
        };
    }
    Speaker::AgentMessage {
        from: field("from").or_else(|| attr(t, "from")),
        name: field("name"),
        handback: origin
            .and_then(|o| o.get("handback"))
            .and_then(Value::as_bool)
            == Some(true),
        body: origin_body(origin).or_else(|| frame_body(t, "agent-message")),
    }
}

/// 没有记录级字段时认具名框与固定句（前导固定行跟着框走）。认不出 ⇒ `None`。
fn framed(raw: &str, sidechain: bool) -> Option<UserText> {
    let t = raw.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    let body = after_lead(t);
    if t.starts_with(NOTICE_MARK) || (body.starts_with(NOTICE_OPEN) && body.contains(NOTICE_CLOSE))
    {
        return Some(UserText::of(notice(t)));
    }
    if framed_by(body, "agent-message") {
        return Some(UserText::of(Speaker::AgentMessage {
            from: attr(body, "from"),
            name: None,
            handback: false,
            body: frame_body(body, "agent-message"),
        }));
    }
    if framed_by(body, "cross-session-message") {
        return Some(UserText::of(Speaker::PeerSession {
            from: attr(body, "from"),
            body: frame_body(body, "cross-session-message"),
        }));
    }
    if t.lines().next().map(str::trim_end) == Some(COORDINATOR_LEAD) {
        return Some(UserText::of(Speaker::Coordinator {
            body: coordinator_body(t),
        }));
    }
    if t.starts_with(LIMIT_RESET_LEAD) {
        return Some(UserText::of(Speaker::System));
    }
    if t.starts_with(COMPACT_LEAD) {
        return Some(shown(Speaker::CompactSummary, t));
    }
    if sidechain && t.starts_with(FORK_OPEN) {
        return Some(shown(Speaker::AgentTask, t));
    }
    None
}

/// 人那一路：剥注入 · 中断标记 · 斜杠命令 · `!` 输入 / 输出 · 本地命令输出；剩下的是人说的话。
fn spoken(raw: &str) -> UserText {
    if only_tags(raw, &["local-command-stdout", "local-command-stderr"]).is_some() {
        return UserText::of(Speaker::CommandOutput);
    }
    let clean = strip_injected(raw);
    if clean.is_empty() {
        // 本来就没字（只有图片之类）⇒ 仍是人；字全是注入 ⇒ 系统。
        return if raw.trim().is_empty() {
            human(String::new())
        } else {
            UserText::of(Speaker::System)
        };
    }
    if is_interrupt(&clean) {
        return UserText::of(Speaker::Interrupt);
    }
    if let Some(s) = slash_command(&clean) {
        return UserText::of(s);
    }
    if let Some(s) = bash_input(&clean) {
        return UserText::of(s);
    }
    if let Some(parts) = only_tags(&clean, &["bash-stdout", "bash-stderr"]) {
        let pick = |n: &str| {
            unescape(
                &parts
                    .iter()
                    .filter(|(t, _)| *t == n)
                    .map(|(_, c)| *c)
                    .collect::<String>(),
            )
        };
        return UserText::of(Speaker::BashOutput {
            stdout: pick("bash-stdout"),
            stderr: pick("bash-stderr"),
        });
    }
    human(clean)
}

fn human(text: String) -> UserText {
    UserText {
        speaker: Speaker::Human,
        pasted: pasted_spans(&text),
        text,
    }
}

/// 要显示正文的那几种非人来源（压缩摘要 · 派的活）：正文同样剥注入。
fn shown(speaker: Speaker, raw: &str) -> UserText {
    UserText {
        speaker,
        text: strip_injected(raw),
        pasted: Vec::new(),
    }
}

/// 五种包装全剥 · 两句样板整行剥 · trim。
fn strip_injected(s: &str) -> String {
    let mut out = s.to_string();
    for tag in INJECTED_WRAPPERS {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        while let (Some(i), Some(j)) = (out.find(&open), out.find(&close)) {
            if j > i {
                out.replace_range(i..j + close.len(), "");
            } else {
                break;
            }
        }
    }
    let kept: Vec<&str> = out
        .split('\n')
        .map(|l| {
            let t = l.trim_end_matches('\r');
            let bare = t.strip_suffix('.').unwrap_or(t).to_ascii_lowercase();
            if BOILERPLATE_LINES.contains(&bare.as_str()) {
                ""
            } else {
                l
            }
        })
        .collect();
    kept.join("\n").trim().to_string()
}

/// 整条恰是中断标记（标记后面跟着真话的不算）。
fn is_interrupt(trimmed: &str) -> bool {
    trimmed
        .strip_prefix("[Request interrupted by user")
        .and_then(|rest| rest.find(']').map(|i| (rest, i)))
        .is_some_and(|(rest, i)| !rest[..i].contains('\n') && rest[i + 1..].trim().is_empty())
}

/// 跳过开头的固定前导行（与它们之间的空行）。
fn after_lead(text: &str) -> &str {
    let mut t = text.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    while LEAD_LINES.iter().any(|l| t.starts_with(l)) {
        t = match t.find('\n') {
            Some(i) => t[i + 1..].trim_start(),
            None => "",
        };
    }
    t
}

/// 记录级 `origin.body`（新版 CLI 把来话正文单放一格）；空 ⇒ `None`。
fn origin_body(origin: Option<&Value>) -> Option<String> {
    origin
        .and_then(|o| o.get("body"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .map(str::to_string)
}

/// 具名框里的那段（开头标签之后、第一个收尾之前；trim 过）；空 ⇒ `None`。
fn frame_body(t: &str, name: &str) -> Option<String> {
    let from = t.find('>')? + 1;
    let close = format!("</{name}>");
    let len = t[from..].find(&close)?;
    Some(t[from..from + len].trim().to_string()).filter(|b| !b.is_empty())
}

/// 主会话后来发给子 agent 的话：固定前导行之后那段。
fn coordinator_body(raw: &str) -> Option<String> {
    let t = raw.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    let rest = t.strip_prefix(COORDINATOR_LEAD)?.trim();
    let rest = rest.strip_suffix(COORDINATOR_TAIL).unwrap_or(rest);
    Some(rest.trim().to_string()).filter(|b| !b.is_empty())
}

/// `t` 以 `<name` 起头，且标签名到此为止（后面是空白或 `>`）。
fn opens(t: &str, name: &str) -> bool {
    t.strip_prefix('<')
        .and_then(|r| r.strip_prefix(name))
        .and_then(|r| r.chars().next())
        .is_some_and(|c| c == '>' || c.is_whitespace())
}

/// 一个具名框：以 `<name` 起头、后面有它的收尾。
fn framed_by(t: &str, name: &str) -> bool {
    opens(t, name) && t.contains(&format!("</{name}>"))
}

/// 开头那个标签上 `name="…"` 的值。
fn attr(t: &str, name: &str) -> Option<String> {
    let tag = &t[..t.find('>')?];
    let key = format!(" {name}=\"");
    let from = tag.find(&key)? + key.len();
    let len = tag[from..].find('"')?;
    Some(tag[from..from + len].to_string()).filter(|v| !v.is_empty())
}

/// 一段 `<name>值</name>` 的值（第一次出现的那个，trim 过）。
fn inner<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let from = text.find(&open)? + open.len();
    let len = text[from..].find(&close)?;
    Some(text[from..from + len].trim())
}

/// 整段只由 `names` 里这几种 `<tag>…</tag>` 段组成（顺序不论，中间只许空白）⇒ 逐段 `(标签, 内容)`；否则 `None`。
fn only_tags<'a>(text: &'a str, names: &[&'static str]) -> Option<Vec<(&'static str, &'a str)>> {
    let mut rest = text.trim();
    if rest.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    while !rest.is_empty() {
        let (name, content, after) = names.iter().find_map(|n| {
            let open = format!("<{n}>");
            let close = format!("</{n}>");
            let body = rest.strip_prefix(open.as_str())?;
            let i = body.find(&close)?;
            Some((*n, &body[..i], &body[i + close.len()..]))
        })?;
        out.push((name, content));
        rest = after.trim();
    }
    Some(out)
}

/// 斜杠命令：三个标签（`command-name` 必有，`command-message` / `command-args` 可缺，顺序随版本漂）各剥一次，剩下必须是空白。
fn slash_command(text: &str) -> Option<Speaker> {
    let take = |s: &str, name: &str| -> Option<(String, String)> {
        let open = format!("<{name}>");
        let close = format!("</{name}>");
        let i = s.find(&open)?;
        let len = s[i + open.len()..].find(&close)?;
        let value = s[i + open.len()..i + open.len() + len].to_string();
        let mut rest = s.to_string();
        rest.replace_range(i..i + open.len() + len + close.len(), "");
        Some((value, rest))
    };
    let (name, rest) = take(text, "command-name")?;
    let rest = take(&rest, "command-message").map_or(rest, |(_, r)| r);
    let (args, rest) = take(&rest, "command-args").unwrap_or((String::new(), rest));
    if !rest.trim().is_empty() {
        return None;
    }
    let name = unescape(&name).trim().to_string();
    (!name.is_empty()).then(|| Speaker::SlashCommand {
        name,
        args: unescape(&args).trim().to_string(),
    })
}

/// `!` 输入：整段恰是一个 `<bash-input>…</bash-input>`，命令非空。
fn bash_input(text: &str) -> Option<Speaker> {
    let inner = text
        .trim()
        .strip_prefix("<bash-input>")?
        .strip_suffix("</bash-input>")?;
    if inner.contains("</bash-input>") {
        return None;
    }
    let command = unescape(inner).trim().to_string();
    (!command.is_empty()).then_some(Speaker::BashInput { command })
}

/// CLI 写标签内容时转义的几个实体，单趟解（`&amp;lt;` 不会被解两次）。
fn unescape(s: &str) -> String {
    const MAP: [(&str, char); 5] = [
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&quot;", '"'),
        ("&#39;", '\''),
        ("&amp;", '&'),
    ];
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        match MAP.iter().find(|(e, _)| rest.starts_with(e)) {
            Some((e, c)) => {
                out.push(*c);
                rest = &rest[e.len()..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// 人粘贴进来的块：`<pasted_content id="…">…</pasted_content id="…">`（下标是 UTF-16 的，界面按它切）。
fn pasted_spans(text: &str) -> Vec<Pasted> {
    let utf16 = |i: usize| u32::try_from(text[..i].encode_utf16().count()).unwrap_or(u32::MAX);
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(k) = text[from..].find(PASTE_OPEN) {
        let start = from + k;
        let tail = &text[start + PASTE_OPEN.len()..];
        let Some(gt) = tail.find('>') else { break };
        if !(gt == 0 || tail.starts_with(' ')) {
            from = start + PASTE_OPEN.len();
            continue;
        }
        let id = attr(&text[start..], "id");
        let body = start + PASTE_OPEN.len() + gt + 1;
        let close = match &id {
            Some(id) => format!("{PASTE_CLOSE} id=\"{id}\">"),
            None => format!("{PASTE_CLOSE}>"),
        };
        let Some(j) = text[body..].find(&close) else {
            break;
        };
        let end = body + j + close.len();
        let inner = text[body..body + j].trim_matches(['\n', '\r']);
        out.push(Pasted {
            id,
            start: utf16(start),
            end: utf16(end),
            body_start: utf16(body),
            body_end: utf16(body + j),
            lines: u32::try_from(inner.lines().count()).unwrap_or(u32::MAX),
        });
        from = end;
    }
    out
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/text_tests.rs"]
mod tests;
