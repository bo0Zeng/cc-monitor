//! 找 pb：这台设了 pb 路径（偏好 `pbDir`，插件根目录）⇒ 只认它；没设 ⇒ 在这台装着的插件里
//! （插件根 ＋ 清单里的名字，配置根底下哪几层装插件、清单长什么样是适配层的知识）认出名字是 pb、且带着入口脚本的那一个。
//! 设之前校验一个路径能不能用（[`check`]）。

use crate::common::said::Said;
use cmd_wire::plan::PlanPb;
use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// 清单里 pb 的名字。
pub(crate) const PLUGIN_NAME: &str = "planned-build";
/// 入口脚本（相对插件根）：`python3 <它> <动词>`。
pub(crate) const ENTRY: &str = "skills/planned-build/tools/entry.py";

/// 第一个是 pb 的插件的入口脚本；一个都不是 ⇒ `None`。次序就是交进来的次序。
pub(crate) fn entry_among(plugins: &[(PathBuf, String)]) -> Option<PathBuf> {
    plugins
        .iter()
        .find_map(|(d, name)| (name == PLUGIN_NAME).then(|| entry_in(d)).flatten())
}

/// 这个插件根底下的入口脚本（在才给）。
pub(crate) fn entry_in(dir: &Path) -> Option<PathBuf> {
    let entry = dir.join(ENTRY);
    entry.is_file().then_some(entry)
}

/// 找不到 pb 的两种。
#[derive(Debug, PartialEq, Eq)]
pub enum Missing {
    /// 设了的那个路径底下没有 pb 的入口。
    SetDir(PathBuf),
    /// 没设，装着的插件里也没有。
    NotInstalled,
}

impl Missing {
    /// 给人看的那一句（一行）。
    pub fn said(&self) -> String {
        match self {
            Missing::SetDir(_) => copy_text("bePlan.face.pbDirGone", &[]),
            Missing::NotInstalled => copy_text("bePlan.face.noPb", &[]),
        }
    }

    /// 下一步（一行）；没什么可说 ⇒ `None`。
    pub fn hint(&self) -> Option<String> {
        match self {
            Missing::SetDir(d) => Some(copy_text(
                "bePlan.face.pbDirGoneHint",
                &[("dir", &d.display().to_string())],
            )),
            Missing::NotInstalled => None,
        }
    }
}

/// 那一句 ＋ 下一步连成报错那一行。
fn joined(said: String, hint: Option<String>) -> String {
    match hint {
        Some(h) => format!("{said}{}{h}", copy_text("kit.text.sep", &[])),
        None => said,
    }
}

/// pb 那一格（`plan-list` · `plan-new` 两条回包同这一份）：找到了、认得 ⇒ `ok`（版本由读到的那一份填）。
pub fn pb_ok() -> PlanPb {
    pb_state("ok", None, None)
}

/// 找不到 pb ⇒ `missing`：那一句 · 下一步分两格。
pub fn pb_missing(m: &Missing) -> PlanPb {
    pb_state("missing", Some(m.said()), m.hint())
}

/// pb 太旧（没有 `dump` · 形状不认得）⇒ `unsupported`：那一句是「要更新」，`why` 是 pb 那一半（[`super::dump::Ran::Unsupported`]）。
pub fn pb_unsupported(why: String) -> PlanPb {
    pb_state(
        "unsupported",
        Some(copy_text("bePlan.dump.tooOld", &[])),
        Some(why),
    )
}

/// pb 那一格不是 `ok` ⇒ 命令没成时交的报错（[`refusal`]）；`ok` ⇒ `None`。
pub fn refused(pb: &PlanPb) -> Option<Said> {
    (pb.state != "ok").then(|| refusal(pb))
}

/// 找不到 pb ⇒ 报错（照 [`pb_missing`] 那一格）。
pub fn no_pb(m: &Missing) -> Said {
    refusal(&pb_missing(m))
}

/// pb 太旧 ⇒ 报错（照 [`pb_unsupported`] 那一格）。
pub fn too_old(why: String) -> Said {
    refusal(&pb_unsupported(why))
}

/// pb 用不了时的报错只在这里照那一格写：码照 state（`unsupported` ⇒ `pb_unsupported`，找不到 ⇒ `no_pb`），
/// 那一行 ＝ 那一句 ＋ 下一步。
fn refusal(pb: &PlanPb) -> Said {
    let code = if pb.state == "unsupported" {
        "pb_unsupported"
    } else {
        "no_pb"
    };
    let line = joined(pb.said.clone().unwrap_or_default(), pb.hint.clone());
    crate::wire::spec::fail(code, line)
}

fn pb_state(state: &str, said: Option<String>, hint: Option<String>) -> PlanPb {
    PlanPb {
        state: state.to_string(),
        said,
        hint,
        version: None,
    }
}

/// 这台的 pb 入口：设了路径只认它，没设认装着的插件。
pub fn pick(set: Option<&Path>, plugins: &[(PathBuf, String)]) -> Result<PathBuf, Missing> {
    match set {
        Some(d) => entry_in(d).ok_or_else(|| Missing::SetDir(d.to_path_buf())),
        None => entry_among(plugins).ok_or(Missing::NotInstalled),
    }
}

/// 校验一个路径：`{state: ok | missing | no_entry | not_runnable, said}`（以人的身份起一次 `pb help`：入口在、解释器在、
/// pb 自己的几个模块都载得进来才算能用）。不写盘。
pub fn check(dir: &Path) -> cmd_wire::plan::PlanPbCheck {
    let shown = dir.display().to_string();
    let out = |state: &str, said: String| cmd_wire::plan::PlanPbCheck {
        state: state.to_string(),
        said,
    };
    if !dir.is_dir() {
        return out(
            "missing",
            copy_text("bePlan.pbDir.missing", &[("dir", &shown)]),
        );
    }
    let Some(entry) = entry_in(dir) else {
        return out(
            "no_entry",
            copy_text("bePlan.pbDir.noEntry", &[("dir", &shown)]),
        );
    };
    match super::command::run_as(&entry, dir, &["help"], false) {
        Ok(_) => out("ok", copy_text("bePlan.pbDir.ok", &[("dir", &shown)])),
        Err(s) => out(
            "not_runnable",
            copy_text(
                "bePlan.pbDir.notRunnable",
                &[("dir", &shown), ("why", &s.said)],
            ),
        ),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/locate_tests.rs"]
mod tests;
