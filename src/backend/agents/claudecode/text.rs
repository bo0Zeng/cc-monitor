//! 要求住址：`4d-lanes.md ## 发版后四路 ### P1` 第 2 件「Claude 记录文本知识 → `agents/claudecode/text.rs`（通用层经 `RecordFace` 够）」·
//! `INVARIANTS §20`（CLI 注入的非真用户输入不算用户说的话）· `设计/10 §2.2b ⑤`（注入噪声那条规则只有一份）。
//!
//! 〔P1〕原共享 crate `search-core` 的**Claude 记录文本那一半**：`message.content` 里正文块 / 工具内容怎么抽 ·
//! user 正文里 CLI 注入的五种包装与两句样板怎么剥、整条是不是 ESC 中断标记。渲染（[`super::schema::UserText::of`]）·
//! 全局搜索 · 会话内查找 · 历史摘录 · 用户输入列表共用这一份；通用层（`observe/`）经注册表 `agents::TextFace` 够它，不按名字够。

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

/// CLI 注入的 prompt 包装 —— 剥掉它们，搜索只命中真内容（INVARIANT § 20 同一意图，从宽）。
const INJECTED_WRAPPERS: [&str; 5] = [
    "task-notification",
    "system-reminder",
    "local-command-caveat",
    "local-command-stdout",
    "local-command-stderr",
];

/// 〔RENDER2 · J10〕整行就是它、不分大小写（句号可省）⇒ CLI 续跑样板，整行剥。
const BOILERPLATE_LINES: [&str; 2] = ["continue from where you left off", "no response requested"];

/// 判一条 user 正文（`设计/10 §2.2b ⑤` 那条不等价的根：规则从此只有这一份）：
/// 五种包装全剥 · 两句样板整行剥 · 剩下的**整条**恰是中断标记才归零（标记后面跟着真话就留着） · trim。
pub(crate) fn user_text(s: &str) -> super::schema::UserText {
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
    let joined = kept.join("\n");
    let trimmed = joined.trim();
    let interrupt = trimmed
        .strip_prefix("[Request interrupted by user")
        .and_then(|rest| rest.find(']').map(|i| (rest, i)))
        .is_some_and(|(rest, i)| !rest[..i].contains('\n') && rest[i + 1..].trim().is_empty());
    super::schema::UserText {
        clean: if interrupt {
            String::new()
        } else {
            trimmed.to_string()
        },
        interrupt,
    }
}

/// 去掉 CLI 注入的 prompt 包装、样板行与 ESC 中断标记（[`user_text`] 的 `clean`）。
pub(crate) fn clean_user_text(s: &str) -> String {
    user_text(s).clean
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/text_tests.rs"]
mod tests;
