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
//!   `record::message_said`，与渲染那一侧同一份）；交原文，截 · 压空白在核心一处。
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

/// 注册表 `HistoryFace.sessions` 那一格：这一家各号的家（账号库交来，账号 0 是默认家）各列一遍，
/// 没装 Codex / 没有会话目录的家 ⇒ 不出。
pub(crate) fn sessions(homes: &[crate::agents::AccountHome]) -> Vec<crate::agents::SynthSession> {
    homes.iter().flat_map(|h| sessions_under(&h.home)).collect()
}

/// 注册表 `HistoryFace.roots` 那一格：每个 Codex 家两个根 —— 日期树 `<家>/sessions` 与平铺的归档根 `<家>/archived_sessions`
/// （[`parse::archived_root`]）。与 [`sessions`] 扫的是同样两处。
pub(crate) fn records_roots(homes: &[crate::agents::AccountHome]) -> Vec<std::path::PathBuf> {
    homes
        .iter()
        .flat_map(|h| [parse::sessions_root(&h.home), parse::archived_root(&h.home)])
        .collect()
}

/// 在一个会话根（[`records_roots`] 那几个之一）下按 sid 找那份 rollout（注册表 `RecordFace.find_session` 那一格）。
/// sid 形状不合法 ⇒ 先于任何 IO 就拒；只认文件名末尾那个 UUID 是它的；符号链接不算命中（不跟随）；按名字次序走，同一棵树两次答同一份。
pub(crate) fn find_session(root: &Path, sid: &str) -> Result<std::path::PathBuf, String> {
    if !shell_quote_core::session_id_ok(sid) {
        return Err(format!("invalid session id {sid:?}"));
    }
    find_session_in(root, DATED_DEPTH, None, sid, &mut |_| {})
        .ok_or_else(|| format!("session {sid} not found under {}", root.display()))
}

/// 「这一份是不是 sid 那个会话」只住这一处：在 `dir` 底下至多 `depth` 层里按名字次序找；`skip` 那个目录整棵不进；
/// 每走过一个目录调一次 `seen`（子线程认父先找同一天、再扫整根时用它跳过已找过的那一天，判据用它数走了几个目录）。
/// sid 不成形 ⇒ 先于任何 IO 答 `None`。
pub(crate) fn find_session_in(
    dir: &Path,
    depth: usize,
    skip: Option<&Path>,
    sid: &str,
    seen: &mut dyn FnMut(&Path),
) -> Option<std::path::PathBuf> {
    if !shell_quote_core::session_id_ok(sid) {
        return None;
    }
    walkdir::WalkDir::new(dir)
        .max_depth(depth)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| Some(e.path()) != skip)
        .filter_map(Result::ok)
        .inspect(|e| {
            if e.file_type().is_dir() {
                seen(e.path());
            }
        })
        .find(|e| {
            e.file_type().is_file()
                && is_session_file(e.path())
                && parse::codex_sid_from_path(e.path()).as_deref() == Some(sid)
        })
        .map(walkdir::DirEntry::into_path)
}

/// 会话根往下几层到 rollout：年 / 月 / 日 / 文件。
pub(crate) const DATED_DEPTH: usize = 4;

/// 一份 Codex 会话记录的文件形态：后缀 `.jsonl`（sid 另由文件名判，见 `parse::codex_sid_from_path`）。
pub(crate) fn is_session_file(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()) == Some("jsonl")
}

/// 压缩存的 rollout 的后缀（Codex 那个本地线程存储压缩的特性，默认关）：这里读不了 ⇒ 不当会话，登漂移账。
const COMPRESSED: &str = "jsonl.zst";

/// [`sessions`] 的本体，Codex home 是参数（判据喂临时目录）。日期树 ＋ 归档目录（平铺一层）：归档了的也列，归没归档由归档面那一判说。
pub(crate) fn sessions_under(codex_home: &Path) -> Vec<crate::agents::SynthSession> {
    let root = parse::sessions_root(codex_home);
    if !root.is_dir() {
        return Vec::new();
    }
    let shelf = parse::archived_root(codex_home);
    let mut out = Vec::new();
    for ent in walkdir::WalkDir::new(&root)
        .into_iter()
        .chain(walkdir::WalkDir::new(&shelf).max_depth(1))
        .filter_map(Result::ok)
    {
        let p = ent.path();
        if !p.is_file() {
            continue;
        }
        if p.to_str().is_some_and(|s| s.ends_with(COMPRESSED)) {
            super::record::drift(super::record::DriftFace::CompressedRollout, COMPRESSED);
            continue;
        }
        if !is_session_file(p) {
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

/// 一份 Codex 会话的首条真用户话原文（截 · 压空白是核心的事，`history::search_rules::excerpt_of`）。前 200 行里找；找不到 ⇒ 空串。
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
        if let Some(t) = message_said(payload).and_then(|s| s.who.speech()) {
            return t;
        }
    }
    String::new()
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/history_tests.rs"]
mod tests;
