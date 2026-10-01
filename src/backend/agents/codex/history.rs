//! Codex 的**历史清单那一面**：这台机器上有哪些 Codex 会话、各自的首条真用户话。
//!
//! # 从哪来
//!
//! 本机历史浏览器里的 Codex 合成项目 / 会话，此前由 monitor 进程内自己枚举（`history·rs::enumerate_codex_sessions`〔散文墓碑〕
//! 与 `codex_first_user_excerpt`〔散文墓碑〕）。「codex 合成的项目与会话一起进后端（join 只一个家）」⇒
//! 枚举与摘录搬进这里（Codex 的格式知识只许住 `agents/codex/`，`agent_locality_guard` 判据①），
//! 通用层（`history_join.rs`）经注册表 `Adapter.history` 那一格够到它（不增 `ADAPTER_CALL_SITES`）。
//!
//! # 口径
//!
//! - 会话 = `<codex home>/sessions/**/rollout-<ts>-<uuid>.jsonl`；sid = 文件名末尾那个 UUID（`parse::codex_sid_from_path`，不像就跳过）。
//! - cwd = 开头那条 `session_meta.cwd`（[`project_dir`]；缺 / 坏 ⇒ 空串，归「(codex)」组）；修改时刻 = 文件 mtime（毫秒）。
//! - 首条真用户话 = 前 200 行里第一条 `response_item.message` 且 `role == "user"`，文本拍平后去掉**注入的上下文**
//!   （`record::is_injected_context`，与渲染那一侧同一份）；取前 200 个字符。
//!
//! Codex 没有 pidfile ⇒ 判活答不了（「不知道」，不是「没活」）；Codex 的会话目录里也没有项目概念（按 cwd 分组是通用层的事）。

use serde_json::Value;
use std::io::BufRead;
use std::path::Path;

use super::parse;

// 「注入的上下文」那张表与数组文本拍平只住 [`super::record`] 一份（渲染那一侧从 monitor 搬进来之后，
//   同一个模块里没有理由再留第二份、再靠判据对拍）。
use super::record::{flatten_text, is_injected_context};

/// 会话的项目目录 ＝ 开头那条 `session_meta` 的 `cwd`（注册表 `RecordFace.project_dir`；只读开头、有上界）。
pub(crate) fn project_dir(p: &Path) -> Option<String> {
    crate::agents::first_in_head(p, |v| parse::session_meta_cwd(v).map(str::to_string))
}

fn mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
        if !p.is_file() || p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(sid) = parse::codex_sid_from_path(p) else {
            continue;
        };
        out.push(crate::agents::SynthSession {
            sid,
            path: p.to_path_buf(),
            cwd: project_dir(p).unwrap_or_default(),
            mtime_ms: mtime_ms(p),
        });
    }
    out
}

/// 一份 Codex 会话的首条真用户话（列表摘要）。前 200 行里找；找不到 ⇒ 空串。
pub(crate) fn first_user_excerpt(path: &Path) -> String {
    let Ok(f) = std::fs::File::open(path) else {
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
        let text = payload.get("content").map(flatten_text).unwrap_or_default();
        let t = text.trim();
        if !t.is_empty() && !is_injected_context(t) {
            return t.chars().take(200).collect();
        }
    }
    String::new()
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/history_tests.rs"]
mod tests;
