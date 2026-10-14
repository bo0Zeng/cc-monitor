//! Codex 的**历史清单那一面**：这台机器上有哪些 Codex 会话、各自的首条真用户话。
//!
//! # 住哪
//!
//! Codex 的格式知识只许住 `agents/codex/`（`agent_locality_guard` 判据①）⇒ 枚举与摘录在这里；
//! 通用层（`history_list.rs`）经注册表 `Adapter.history` 那一格够到它（不增 `ADAPTER_CALL_SITES`）。
//!
//! # 口径
//!
//! - 会话 = `<codex home>/sessions/**/rollout-<ts>-<uuid>.jsonl`；sid = 文件名末尾那个 UUID（`parse::codex_sid_from_path`，不像就跳过）。
//! - cwd = 开头那条 `session_meta.cwd`（[`project_dir`]；缺 / 坏 ⇒ 空串，归「(codex)」组）；修改时刻 = 文件 mtime（毫秒）。
//! - 首条真用户话 = 前 200 行里第一条人说的 `response_item.message`（`role == "user"`，「谁说的」用
//!   `record::message_said`，与渲染那一侧同一份）；取前 200 个字符。
//!
//! Codex 没有 pidfile ⇒ 判活答不了（「不知道」，不是「没活」）；Codex 的会话目录里也没有项目概念（按 cwd 分组是通用层的事）。

use crate::agents::UserTextExt as _;
use crate::platform::fs::Ro;
use serde_json::Value;
use std::io::BufRead;
use std::path::Path;

use super::parse;

// 「谁说的」只住 [`super::record`] 一份（与渲染那一侧同一个判定）。
use super::record::message_said;

/// 会话的项目目录 ＝ 开头那条 `session_meta` 的 `cwd`（注册表 `RecordFace.project_dir`；只读开头、有上界）。
pub(crate) fn project_dir(p: &Path) -> Option<String> {
    crate::agents::first_in_head(p, |v| parse::session_meta_cwd(v).map(str::to_string))
}

/// 这台机器上的 Codex 会话（没装 Codex / 没有会话目录 ⇒ 空）。
pub(crate) fn sessions() -> Vec<crate::agents::SynthSession> {
    match super::home() {
        Some(h) => sessions_under(&h),
        None => Vec::new(),
    }
}

/// 这台机器上 Codex 会话记录的根（`<codex home>/sessions`；说不出 home ⇒ `None`）。与 [`sessions`] 扫的是同一个根。
pub(crate) fn records_root() -> Option<std::path::PathBuf> {
    super::home().map(|h| parse::sessions_root(&h))
}

/// [`sessions`] 的本体，Codex home 是参数（判据喂临时目录）。
/// 一份 Codex 会话记录的文件形态：后缀 `.jsonl`（sid 另由文件名判，见 `parse::codex_sid_from_path`）。
pub(crate) fn is_session_file(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()) == Some("jsonl")
}

pub(crate) fn sessions_under(codex_home: &Path) -> Vec<crate::agents::SynthSession> {
    let root = parse::sessions_root(codex_home);
    if !root.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for ent in walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_map(Result::ok)
    {
        let p = ent.path();
        if !p.is_file() || !is_session_file(&p) {
            continue;
        }
        let Some(sid) = parse::codex_sid_from_path(p) else {
            continue;
        };
        out.push(crate::agents::SynthSession {
            sid,
            path: p.to_path_buf(),
            cwd: project_dir(p).unwrap_or_default(),
            mtime_ms: crate::platform::fs::Ro::mtime_ms(p),
        });
    }
    out
}

/// 一份 Codex 会话的首条真用户话（列表摘要）。前 200 行里找；找不到 ⇒ 空串。
pub(crate) fn first_user_excerpt(path: &Path) -> String {
    let Ok(f) = Ro::open(path) else {
        return String::new();
    };
    for line in std::io::BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .take(200)
    {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let Some((top, payload)) = parse::unwrap_envelope(&v) else {
            continue;
        };
        if top != "response_item"
            || payload.get("type").and_then(Value::as_str) != Some("message")
            || payload.get("role").and_then(Value::as_str) != Some("user")
        {
            continue;
        }
        if let Some(t) = message_said(payload).and_then(|s| s.speech()) {
            return t.chars().take(200).collect();
        }
    }
    String::new()
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/history_tests.rs"]
mod tests;
