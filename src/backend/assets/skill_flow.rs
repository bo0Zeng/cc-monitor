//! 〔D 组〕skill「装到这台」与「卸」的写那一半：帧面 `skill-install-apply`；卸由 `ext-uninstall-apply` 调 [`answer_uninstall`]。
//!
//! 从 monitor `skill_install.rs` 搬来（从前 monitor 请这台判「写哪几个」、再逐个经这台 `files-put` / `files-delete` 写、最后交装记录）。
//! 今天判（`skill_install::answer_plan_with` · `answer_uninstall_plan_at`）、写（本进程文件管理面 [`Door`]）、记（`skill-install-record`，
//! 写口由 `stream/inbound/` 递进来 —— `readonly_guard` 第四层只许那一扇门）都在被写的这一台。
//! 🔴 写**不重读重算**：用户确认的是他看到的那份差异；看过之后变了 ⇒ `stale` 就停，说清前面写了 / 删了哪几个。

use super::door::{self, Door, Refused};
use crate::assets::mcp_sync::Facts;
use copy_core::copy_text;
use serde_json::{json, Map, Value};

type Answer = Result<Value, (&'static str, String)>;

/// 装记录的写口（生产 = `skill_ledger::answer_record`，由 `stream/inbound/` 递进来）。
pub(crate) type Record<'a> = &'a dyn Fn(&Value) -> Answer;

fn texts_by_path(
    v: Option<&Value>,
    key: &str,
) -> Result<Map<String, Value>, (&'static str, String)> {
    let arr = v.and_then(Value::as_array).ok_or((
        "bad_args",
        crate::common::contract::malformed(&format!(
            "missing `{key}` (an array of {{path, text}})"
        )),
    ))?;
    let mut out = Map::new();
    for f in arr {
        let p = f.get("path").and_then(Value::as_str).ok_or((
            "bad_args",
            crate::common::contract::malformed(&format!("`{key}[].path` must be a string")),
        ))?;
        out.insert(p.to_string(), f.clone());
    }
    Ok(out)
}

fn str_of(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn names(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|n| n.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// 停在哪一个、为什么、前面做了哪几个（key 逐字写成字面量：文案表判据按字面量对账）。
enum Doing {
    Install,
    Uninstall,
}

fn stopped_said(doing: Doing, at: &str, why: &str, done: &[String]) -> String {
    let list = done.join(&copy_text("beSkillFlow.stopped.listSep", &[]));
    let done = match (&doing, done.is_empty()) {
        (Doing::Install, true) => copy_text("beSkillFlow.install.noneWritten", &[]),
        (Doing::Install, false) => copy_text("beSkillFlow.install.someWritten", &[("list", &list)]),
        (Doing::Uninstall, true) => copy_text("beSkillFlow.uninstall.noneDeleted", &[]),
        (Doing::Uninstall, false) => {
            copy_text("beSkillFlow.uninstall.someDeleted", &[("list", &list)])
        }
    };
    match doing {
        Doing::Install => copy_text(
            "beSkillFlow.install.stoppedAt",
            &[("at", at), ("why", why), ("done", &done)],
        ),
        Doing::Uninstall => copy_text(
            "beSkillFlow.uninstall.stoppedAt",
            &[("at", at), ("why", why), ("done", &done)],
        ),
    }
}

/// `skill-install-apply {name, source, target, take, overwrite}`：`source` = 来源那台 `skill-read` 的 `files`（原样），
/// `target` = 看差异时这台回的那几份原文（CAS 期望）。回 `{dir, written, chmodFailed, recordFailed}`。
pub(crate) fn answer_install(
    d: &dyn Door,
    facts: &dyn Facts,
    root: Option<&std::path::Path>,
    record: Record,
    args: &Value,
) -> Answer {
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let source = texts_by_path(args.get("source"), "source")?;
    let target = texts_by_path(args.get("target"), "target")?;
    let wire_source: Vec<Value> = source
        .values()
        .map(|f| json!({ "path": f["path"], "text": f["text"], "exec": f.get("exec").cloned().unwrap_or(json!(false)) }))
        .collect();
    let plan = crate::assets::skill_install::answer_plan_with(
        facts,
        root,
        &json!({ "name": name, "source": wire_source, "take": args.get("take"), "overwrite": args.get("overwrite"), "project": args.get("project") }),
    )?;
    let (dir, base, prefix) = (
        str_of(&plan, "dir"),
        str_of(&plan, "base"),
        str_of(&plan, "prefix"),
    );
    let ledger = plan
        .get("ledger")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut written = Vec::new();
    let mut chmod_failed = Vec::new();
    let mut stopped: Option<String> = None;
    for path in names(&plan, "write") {
        let Some(text) = source
            .get(&path)
            .and_then(|f| f.get("text"))
            .and_then(Value::as_str)
        else {
            stopped = Some(copy_text(
                "beSkillFlow.install.noOriginal",
                &[("path", &path)],
            ));
            break;
        };
        let expect = target
            .get(&path)
            .and_then(|t| t.get("text"))
            .and_then(Value::as_str);
        let rel = format!("{prefix}/{path}");
        match door::put(d, &base, &rel, text, expect, false, true) {
            Ok(_) => written.push(path.clone()),
            Err(e) => {
                let why = match &e {
                    Refused::Stale(_) => copy_text("beSkillFlow.install.stale", &[]),
                    _ => e.clone().said(),
                };
                stopped = Some(stopped_said(Doing::Install, &path, &why, &written));
                break;
            }
        }
        let exec = source
            .get(&path)
            .and_then(|f| f.get("exec"))
            .and_then(Value::as_bool)
            == Some(true);
        if exec && door::chmod(d, &base, &rel, 0o755).is_err() {
            chmod_failed.push(path.clone());
        }
    }
    let record_failed = if written.is_empty() {
        None
    } else {
        let files: Map<String, Value> = written
            .iter()
            .filter_map(|p| ledger.get(p).map(|v| (p.clone(), v.clone())))
            .collect();
        let mut rec = json!({ "op": "add", "name": name, "files": files });
        if let Some(p) = args.get("project").filter(|p| p.is_string()) {
            rec["project"] = p.clone();
        }
        record(&rec)
            .err()
            .map(|(_, e)| copy_text("beSkillFlow.install.recordFailed", &[("e", &e)]))
    };
    if let Some(why) = stopped {
        let why = match record_failed {
            None => why,
            Some(r) => format!("{why}{r}"),
        };
        return Err(("stale", why));
    }
    Ok(
        json!({ "dir": dir, "written": written, "chmodFailed": chmod_failed, "recordFailed": record_failed }),
    )
}

/// 卸 `{dir, seen, take, confirm}`：`seen` = 看的时候这台那几份现有原文（CAS 期望）。
/// 回 `{dir, deleted, recordFailed, dirRemoved, dirFailed}`。
pub(crate) fn answer_uninstall(
    d: &dyn Door,
    ledger: &std::path::Path,
    record: Record,
    args: &Value,
) -> Answer {
    let dir = args.get("dir").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `dir`"),
    ))?;
    let seen = texts_by_path(args.get("seen"), "seen")?;
    let ask = json!({ "dir": dir, "take": args.get("take"), "confirm": args.get("confirm") });
    let plan = crate::assets::skill_install::answer_uninstall_plan_at(ledger, &ask)?;
    let (delete, forget) = (names(&plan, "delete"), names(&plan, "forget"));
    let recorded: Vec<String> = delete.iter().chain(forget.iter()).cloned().collect();
    let mut deleted = Vec::new();
    let mut stopped: Option<String> = None;
    for path in &delete {
        let Some(expect) = seen
            .get(path)
            .and_then(|t| t.get("text"))
            .and_then(Value::as_str)
        else {
            stopped = Some(copy_text("beSkillFlow.uninstall.noSeen", &[("path", path)]));
            break;
        };
        match door::delete(d, dir, path, expect) {
            Ok(()) => deleted.push(path.clone()),
            Err(e) => {
                let why = match &e {
                    Refused::Stale(_) => copy_text("beSkillFlow.uninstall.stale", &[]),
                    _ => e.clone().said(),
                };
                stopped = Some(stopped_said(Doing::Uninstall, path, &why, &deleted));
                break;
            }
        }
    }
    let mut drop = deleted.clone();
    drop.extend(forget);
    let record_failed = if drop.is_empty() {
        None
    } else {
        record(&json!({ "op": "drop", "dir": dir, "paths": drop }))
            .err()
            .map(|(_, e)| copy_text("beSkillFlow.uninstall.dropFailed", &[("e", &e)]))
    };
    if let Some(why) = stopped {
        let why = match record_failed {
            None => why,
            Some(r) => format!("{why}{r}"),
        };
        return Err(("stale", why));
    }
    let (dir_removed, dir_failed) = remove_emptied_dirs(d, dir, &recorded);
    Ok(
        json!({ "dir": dir, "deleted": deleted, "recordFailed": record_failed, "dirRemoved": dir_removed, "dirFailed": dir_failed }),
    )
}

/// 删完文件之后收掉装时 `parents` 建出来、此刻已空的子目录与 skill 目录本身（只删空目录那一形；不空 / 已不在 ⇒ 留着）。
fn remove_emptied_dirs(d: &dyn Door, dir: &str, files: &[String]) -> (bool, Option<String>) {
    let mut subs: std::collections::BTreeSet<String> = Default::default();
    for f in files {
        let mut at = std::path::Path::new(f).parent();
        while let Some(p) = at.filter(|p| !p.as_os_str().is_empty()) {
            subs.insert(p.to_string_lossy().replace('\\', "/"));
            at = p.parent();
        }
    }
    let mut order: Vec<String> = subs.into_iter().collect();
    order.sort_by_key(|s| std::cmp::Reverse(s.matches('/').count()));
    let failed = |at: &str, e: Refused| {
        copy_text(
            "beSkillFlow.uninstall.dirFailed",
            &[("path", at), ("e", &e.said())],
        )
    };
    for sub in &order {
        match door::delete_empty_dir(d, dir, sub) {
            Ok(()) | Err(Refused::Stale(_)) => {}
            Err(e) => return (false, Some(failed(sub, e))),
        }
    }
    let here = std::path::Path::new(dir);
    let (Some(parent), Some(name)) = (here.parent(), here.file_name()) else {
        return (false, None);
    };
    match door::delete_empty_dir(d, &parent.to_string_lossy(), &name.to_string_lossy()) {
        Ok(()) => (true, None),
        Err(Refused::Stale(_)) => (false, None),
        Err(e) => (false, Some(failed(dir, e))),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/skill_flow_tests.rs"]
mod tests;
