//! 设置里换 agent 家目录（界面上的「Claude 目录」）存之前那一问（帧面 `agent-home-check`）：那个目录在不在 · 是不是目录 ·
//! 像不像哪一家的家目录（里面有没有哪一家的记录树，布局住 `agents/` 那一家：[`crate::agents::records_root_among`]）。
//! 只读：stat 两次，回写好的那一句（能用 ⇒ `null`），界面照抄。

use crate::common::contract::malformed;
use crate::common::said::Said;
use crate::platform::fs::Ro;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// `agent-home-check` 的入参。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct AgentHomeCheckReq {
    /// 要查的目录：绝对路径，或 `~` 打头（`~` · `~/…` · `~\…`，按这台家目录展开）。
    pub(crate) path: String,
}

/// `agent-home-check` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct AgentHomeCheck {
    /// 不能用 ⇒ 写好的那一句（不在 · 不是目录 · 没有会话记录，带用户交来的原样路径）；能用 ⇒ `null`。
    pub(crate) said: Option<crate::common::cells::Words>,
}

/// 判出来的那一态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// 判一个目录：住记录树的是注入的 `registry` 里那几家（生产入口交 [`crate::agents::REGISTRY`]，判据拿替身家喂）。
pub(crate) fn verdict(registry: &[crate::agents::Adapter], dir: &Path) -> Verdict {
    match Ro::metadata(dir) {
        Err(_) => Verdict::Missing,
        Ok(m) if !m.is_dir() => Verdict::NotDir,
        Ok(_) => {
            // 住记录树的每一家各问一遍：哪一家的树在这个目录下 ⇒ 能用；哪一家都不住记录树 ⇒ 在、是目录就算。
            let trees: Vec<PathBuf> = registry
                .iter()
                .filter_map(|a| crate::agents::records_root_among(registry, a.kind, dir))
                .collect();
            if trees.is_empty()
                || trees
                    .iter()
                    .any(|t| Ro::metadata(t).is_ok_and(|p| p.is_dir()))
            {
                Verdict::Ok
            } else {
                Verdict::NoRecords
            }
        }
    }
}

/// 帧面入口：`{path}`（绝对路径，或 `~` 打头按这台家目录展开）⇒ `{said}`（能用 ⇒ `null`）。
pub fn answer_check(req: &AgentHomeCheckReq) -> Result<AgentHomeCheck, Said> {
    answer_check_with(
        req,
        crate::platform::paths::home_dir(),
        crate::agents::REGISTRY,
    )
}

/// [`answer_check`] 的本体：家目录与注册表是参数（判据拿临时目录与替身家喂）。
pub(crate) fn answer_check_with(
    req: &AgentHomeCheckReq,
    home: Option<PathBuf>,
    registry: &[crate::agents::Adapter],
) -> Result<AgentHomeCheck, Said> {
    let raw = req.path.as_str();
    if raw.is_empty() {
        return Err(Said::new(
            "bad_args",
            malformed("`path` must be a non-empty string"),
        ));
    }
    // 家目录说不出 ⇒ 原样，落到下面「不是绝对路径」那一条。
    let path = crate::platform::paths::expand_tilde_from(raw, home.as_deref());
    if !path.is_absolute() {
        return Err(Said::new(
            "bad_args",
            malformed(&format!("`path` is not absolute: {raw}")),
        ));
    }
    let said = match verdict(registry, &path) {
        Verdict::Ok => return Ok(AgentHomeCheck { said: None }),
        Verdict::Missing => copy_core::copy_text("bePath.dir.missing", &[("path", raw)]),
        Verdict::NotDir => copy_core::copy_text("bePath.dir.notDir", &[("path", raw)]),
        Verdict::NoRecords => copy_core::copy_text("bePath.dir.noRecords", &[("path", raw)]),
    };
    Ok(AgentHomeCheck {
        said: Some(crate::common::cells::Words(said)),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/agent_home_check_tests.rs"]
mod tests;
