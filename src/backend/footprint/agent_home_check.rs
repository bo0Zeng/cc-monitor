//! 设置里换 agent 家目录（界面上的「Claude 目录」）存之前那一问（帧面 `agent-home-check`）：那个目录在不在 · 是不是目录 ·
//! 像不像那一家的家目录（里面有没有它的记录树，布局住 `agents/` 那一家：[`crate::agents::records_root`]）。
//! 只读：stat 两次，回一个码，说法归界面（按码取文案）。

use crate::common::contract::malformed;
use crate::platform::fs::Ro;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// `agent-home-check` 的入参。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentHomeCheckReq {
    /// 要查的目录：绝对路径，或 `~/` 开头（按这台家目录展开）。
    pub(crate) path: String,
}

/// `agent-home-check` 的回包。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct AgentHomeCheck {
    pub(crate) state: Verdict,
}

/// 判出来的那一态（线上 `state`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "AgentHomeState"
    )
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Verdict {
    /// 在、是目录、里面有记录树（那一家没有记录树 ⇒ 在、是目录就算）。
    Ok,
    /// 不在（或读不动）。
    Missing,
    /// 在，但不是目录。
    NotDir,
    /// 是目录，但里面没有记录树 ⇒ 读不出一条会话。
    NoRecords,
}

/// 判一个目录。
pub(crate) fn verdict(dir: &Path) -> Verdict {
    match Ro::metadata(dir) {
        Err(_) => Verdict::Missing,
        Ok(m) if !m.is_dir() => Verdict::NotDir,
        Ok(_) => match crate::agents::records_root(dir) {
            None => Verdict::Ok,
            Some(tree) => match Ro::metadata(tree) {
                Ok(p) if p.is_dir() => Verdict::Ok,
                _ => Verdict::NoRecords,
            },
        },
    }
}

/// 帧面入口：`{path}`（绝对路径，或 `~/` 开头按这台家目录展开）⇒ `{state}`。
pub(crate) fn answer_check(
    req: &AgentHomeCheckReq,
) -> Result<AgentHomeCheck, (&'static str, String)> {
    answer_check_with(req, crate::platform::paths::home_dir())
}

/// [`answer_check`] 的本体：家目录是参数（判据拿临时目录喂）。
pub(crate) fn answer_check_with(
    req: &AgentHomeCheckReq,
    home: Option<PathBuf>,
) -> Result<AgentHomeCheck, (&'static str, String)> {
    let raw = req.path.as_str();
    if raw.is_empty() {
        return Err(("bad_args", malformed("`path` must be a non-empty string")));
    }
    let path = if raw == "~" || raw.starts_with("~/") {
        let home = home.ok_or_else(|| ("bad_args", malformed("home directory unresolvable")))?;
        if raw == "~" {
            home
        } else {
            home.join(&raw[2..])
        }
    } else {
        PathBuf::from(raw)
    };
    if !path.is_absolute() {
        return Err((
            "bad_args",
            malformed(&format!("`path` is not absolute: {raw}")),
        ));
    }
    Ok(AgentHomeCheck {
        state: verdict(&path),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/agent_home_check_tests.rs"]
mod tests;
