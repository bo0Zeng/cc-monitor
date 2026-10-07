//! 设置里换 agent 家目录（界面上的「Claude 目录」）存之前那一问（帧面 `agent-home-check`）：那个目录在不在 · 是不是目录 ·
//! 像不像那一家的家目录（里面有没有它的记录树，布局住 `agents/` 那一家：[`crate::agents::records_root`]）。
//! 只读：stat 两次，回一个码，说法归界面（按码取文案）。

use crate::common::contract::malformed;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// 判出来的那一态（线上 `state`）。
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

impl Verdict {
    fn wire(self) -> &'static str {
        match self {
            Verdict::Ok => "ok",
            Verdict::Missing => "missing",
            Verdict::NotDir => "not_dir",
            Verdict::NoRecords => "no_records",
        }
    }
}

/// 判一个目录。
pub(crate) fn verdict(dir: &Path) -> Verdict {
    match std::fs::metadata(dir) {
        Err(_) => Verdict::Missing,
        Ok(m) if !m.is_dir() => Verdict::NotDir,
        Ok(_) => match crate::agents::records_root(dir) {
            None => Verdict::Ok,
            Some(tree) => match std::fs::metadata(tree) {
                Ok(p) if p.is_dir() => Verdict::Ok,
                _ => Verdict::NoRecords,
            },
        },
    }
}

/// 帧面入口：`{path}`（绝对路径，或 `~/` 开头按这台家目录展开）⇒ `{state}`。
pub(crate) fn answer_check(args: &Value) -> Result<Value, (&'static str, String)> {
    answer_check_with(args, crate::platform::paths::home_dir())
}

/// [`answer_check`] 的本体：家目录是参数（判据拿临时目录喂）。
pub(crate) fn answer_check_with(
    args: &Value,
    home: Option<PathBuf>,
) -> Result<Value, (&'static str, String)> {
    let raw = args
        .get("path")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or(("bad_args", malformed("`path` must be a non-empty string")))?;
    let path = if raw == "~" || raw.starts_with("~/") {
        let home = home.ok_or(("bad_args", malformed("home directory unresolvable")))?;
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
    Ok(json!({ "state": verdict(&path).wire() }))
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/agent_home_check_tests.rs"]
mod tests;
