//! 隐藏的判定（`control/session_hidden.rs`）在历史这一层的口：观测侧与帧面经这里拿注解里单个那一格、叫主会话什么。

use super::history_annotations::Table;
pub(crate) use crate::control::session_hidden::Here;
use cmd_wire::session::Hidden;

/// 隐藏的规则那一侧住的两份文件（这台偏好 · 会话血缘）：动了就该重判（观测侧挂耳朵）。说不出位置的那一份不在里面。
pub(crate) fn rule_files() -> Vec<std::path::PathBuf> {
    [
        crate::control::machine_prefs::path_now(),
        crate::platform::paths::data_home().map(|d| d.join(crate::lineage::FILE_NAME)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// 这条会话隐不隐藏（`annotations` ＝ 这台的注解：单个那一格从它取；`bg` ＝ 是不是分身会话；`title_of` ＝ 主会话叫什么）。
pub(crate) fn hidden_here(
    here: &Here,
    annotations: &Table,
    sid: &str,
    bg: bool,
    title_of: &dyn Fn(&str) -> String,
) -> Option<Hidden> {
    here.hidden(annotations.get(sid).and_then(|e| e.hide), sid, bg, title_of)
}

/// 叫这条会话什么（这台的）：改过的名 ＞ 记录里的标题 · 第一句（与历史清单 · 标签页同一条规则）；记录找不到 ⇒ 会话号的头八位。
pub(crate) fn title_here(sid: &str, annotations: &Table) -> String {
    if let Some(t) = annotations
        .get(sid)
        .and_then(|e| e.custom_title.as_deref())
        .filter(|t| !t.trim().is_empty())
    {
        return t.to_string();
    }
    match crate::agents::find_session(&super::listing::agent_home(), sid) {
        Ok(at) => super::listing::session_title_of(&at.file),
        Err(_) => sid.chars().take(8).collect(),
    }
}
