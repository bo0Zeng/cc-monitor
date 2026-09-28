//! 〔MIG-3a · D 组〕skill 接入面（收件箱）：帧面 `skill-host-list` · `skill-host-read` · `skill-host-write`。
//!
//! 从 monitor `skill_host.rs` 搬来：哪几个 skill、可编辑文件是哪几份、能不能碰 —— 声明与围栏住适配层（`Adapter.assets` 那一格，
//! `agents/claudecode/skill_host.rs`）；这里只接线：读与写都经本进程文件管理面（读 `files-peek`，写 `files-put` 带 CAS）。
//! 本机远端同一条路：问的是那台机器自己的后端，`cwd` 是那台上的项目目录。

use super::door::{self, Door, Refused};
use copy_core::copy_text;
use serde_json::{json, Value};
use std::path::Path;

type Answer = Result<Value, (&'static str, String)>;

fn str_arg<'a>(args: &'a Value, k: &str) -> Result<&'a str, (&'static str, String)> {
    args.get(k).and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed(&format!("missing `{k}` (a string)")),
    ))
}

fn face() -> Result<crate::agents::AssetFace, (&'static str, String)> {
    crate::agents::skill_asset_face().ok_or(("refused", copy_text("beSkillInbox.face.none", &[])))
}

/// `skill-host-list {cwd}` → `{skills: [SkillView…]}`。
pub(crate) fn answer_list(args: &Value) -> Answer {
    let cwd = str_arg(args, "cwd")?;
    Ok(json!({ "skills": (face()?.skill_views)(Path::new(cwd)) }))
}

fn target(args: &Value) -> Result<(String, String), (&'static str, String)> {
    let (cwd, id, path) = (
        str_arg(args, "cwd")?,
        str_arg(args, "skillId")?,
        str_arg(args, "path")?,
    );
    let (root, rel) = (face()?.skill_editable)(id, Path::new(cwd), Path::new(path))
        .map_err(|m| ("refused", m))?;
    Ok((root.to_string_lossy().into_owned(), rel))
}

/// `skill-host-read {cwd, skillId, path}` → `{text}`（文件必须已存在）。
pub(crate) fn answer_read(d: &dyn Door, args: &Value) -> Answer {
    let (root, rel) = target(args)?;
    let got = door::peek(d, &root, &rel).map_err(|m| ("refused", m))?;
    let text = got.text.ok_or_else(|| {
        (
            "refused",
            copy_text(
                "rsSkillHost.read.missing",
                &[
                    ("door", &copy_text("beSkillInbox.door.here", &[])),
                    ("root", &root),
                    ("rel", &rel),
                ],
            ),
        )
    })?;
    Ok(json!({ "text": text }))
}

/// `skill-host-write {cwd, skillId, path, content, expected}` → `{path}`；`expected` = 打开时读到的那一份（CAS）。
pub(crate) fn answer_write(d: &dyn Door, args: &Value) -> Answer {
    let (root, rel) = target(args)?;
    let content = str_arg(args, "content")?;
    let expected = str_arg(args, "expected")?;
    match door::put(d, &root, &rel, content, Some(expected), false, false) {
        Ok(l) => Ok(json!({ "path": l.path })),
        Err(Refused::Stale(why)) => Err((
            "stale",
            copy_text("rsSkillHost.write.stale", &[("why", &why)]),
        )),
        Err(e) => Err(("refused", e.said())),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/skill_inbox_tests.rs"]
mod tests;
