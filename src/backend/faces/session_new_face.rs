//! 帧面 `session-new` · `session-new-facts` · `session-new-dir` 的宿主壳：起新会话框要的这台事实（最近用过的目录 · 有没有 tmux ·
//! 这台能起哪几家 · 分叉源会话的三格）住观测层，起会话要的那几样（tmux 名单 · 铸名 · 起 ccm · 号）同批量起那一份，
//! 写分支记录交 `control/fork_write`（本体与 `session-fork` 同一份）。

use crate::control::session_new;
use serde_json::{json, Value};
use std::collections::BTreeMap;

type Answer = Result<Value, (&'static str, String)>;

/// 「最近用过的目录」最多列几条。
pub(crate) const RECENT_MAX: usize = 8;

fn agent_of(args: &Value) -> &str {
    args.get("agent")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// `session-new`。
pub(crate) fn answer(
    args: &Value,
    files: super::launch_face::Files,
) -> Result<Value, session_new::Failed> {
    let home = crate::observe::history_query::agent_home();
    let fork =
        |sid: &str, uuid: &str| crate::control::fork_write::fork_for_launch(&home, sid, uuid);
    let user_home = crate::platform::paths::home_dir();
    super::session_batch_face::with_deps_as(None, agent_of(args), Some(files), |d| {
        session_new::answer(args, d, &fork, user_home.as_deref())
    })
}

/// `session-new-dir`：`{cwd, forkOf?}` ⇒ `{exists, tmuxName}`。
pub(crate) fn dir(args: &Value) -> Answer {
    let user_home = crate::platform::paths::home_dir();
    super::session_batch_face::with_deps_as(None, "", None, |d| {
        session_new::dir_answer(args, d, user_home.as_deref())
    })
}

/// 默认启动器在起会话那个 shell 的 `PATH` 上找得到（Windows 上带 `.exe` / `.cmd` 也算）——与足迹里「装没装」同一个查法。
fn launcher_found(name: &str) -> bool {
    launcher_found_in(
        name,
        crate::platform::shell::session_shell_path().as_deref(),
    )
}

/// [`launcher_found`] 的本体（那份 `PATH` 是参数）。问不出那个 shell 的 `PATH` ⇒ 不藏（判不了不当成没装）。
pub(crate) fn launcher_found_in(name: &str, session_path: Option<&str>) -> bool {
    let exts: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd"]
    } else {
        &[""]
    };
    let runnable = |p: &str| {
        exts.iter().any(|e| {
            crate::plugin::discover::is_executable(std::path::Path::new(&format!("{p}{e}")))
        })
    };
    crate::footprint::rows::resolves_on_path(name, session_path, &runnable).unwrap_or(true)
}

/// 这台能起的几家（注册表里由我们起的、且默认启动器在这台找得到的；注册表序）。
pub(crate) fn launchable_here(found: &dyn Fn(&str) -> bool) -> Vec<&'static str> {
    crate::agents::launchable_kinds()
        .into_iter()
        .filter(|k| crate::agents::pick_kind(Some(k)).is_ok_and(|(_, f)| found(f.default_launcher)))
        .collect()
}

/// 各家记录里的工作目录 ⇒ 最近用过的那几个（新的在前，同一个目录只出一次；不出会话的那个目录不算）。
pub(crate) fn recent_dirs(items: &[(String, i64)], hidden: &dyn Fn(&str) -> bool) -> Vec<Value> {
    let mut best: BTreeMap<&str, i64> = BTreeMap::new();
    for (cwd, ms) in items {
        if cwd.is_empty() || hidden(cwd) {
            continue;
        }
        let e = best.entry(cwd.as_str()).or_insert(*ms);
        *e = (*e).max(*ms);
    }
    let mut v: Vec<(&str, i64)> = best.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    v.into_iter()
        .take(RECENT_MAX)
        .map(|(cwd, ms)| json!({ "cwd": cwd, "lastMs": ms }))
        .collect()
}

/// 记录树那一家的项目 ＋ 合成历史那几家的会话 ⇒ `(工作目录, 最近一次)`。
fn record_dirs(home: &std::path::Path) -> Vec<(String, i64)> {
    let mut buf: Vec<u8> = Vec::new();
    // 读不出记录树 ⇒ 记一行、少这一家的目录（框里照常起，只是「最近的」少几项）。
    if let Err(e) = crate::observe::history_query::list_projects_into(home, &mut buf) {
        tracing::warn!("session-new-facts：读不出记录树的项目清单：{e}");
    }
    let mut out: Vec<(String, i64)> = String::from_utf8_lossy(&buf)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| {
            Some((
                v.get("projectPath")?.as_str()?.to_string(),
                v.get("lastActivityMs")?.as_i64()?,
            ))
        })
        .collect();
    for (_, face) in crate::agents::history_faces() {
        out.extend((face.sessions)().into_iter().map(|s| (s.cwd, s.mtime_ms)));
    }
    out
}

/// `session-new-facts`：`{forkOf?, at?}` ⇒ `{recent, tmux, agents, fork}`（`at` ＝ 从哪条消息处分叉：框顶说第几轮、几点）。
pub(crate) fn facts(args: &Value) -> Answer {
    let o = args.as_object().ok_or((
        "bad_args",
        crate::common::contract::malformed("args must be an object"),
    ))?;
    if let Some(k) = o
        .keys()
        .find(|k| k.as_str() != "forkOf" && k.as_str() != "at")
    {
        return Err((
            "bad_args",
            crate::common::contract::malformed(&format!("unknown field `{k}`")),
        ));
    }
    let fork_of = match o.get("forkOf") {
        None => None,
        Some(v) => Some(
            v.as_str()
                .filter(|s| shell_quote_core::session_id_ok(s))
                .ok_or((
                    "bad_args",
                    crate::common::contract::malformed("`forkOf` must be a session id"),
                ))?,
        ),
    };
    let at = match o.get("at") {
        None => None,
        Some(v) => Some(
            v.as_str()
                .filter(|s| shell_quote_core::session_id_ok(s) && fork_of.is_some())
                .ok_or((
                    "bad_args",
                    crate::common::contract::malformed(
                        "`at` must be a message id, given with `forkOf`",
                    ),
                ))?,
        ),
    };
    let home = crate::observe::history_query::agent_home();
    let fork = match fork_of {
        None => Value::Null,
        Some(sid) => {
            let root = crate::agents::records_root(&home).ok_or((
                "fork_failed",
                copy_core::copy_text("beSessionNew.fork.noTree", &[]),
            ))?;
            let source =
                crate::agents::find_session_file(&root, sid).map_err(|m| ("fork_failed", m))?;
            let turn = at.and_then(|uuid| {
                let f = std::fs::File::open(&source).ok()?;
                crate::observe::turns::turn_at(std::io::BufReader::new(f), uuid)
            });
            json!({
                "agent": crate::agents::record_kind_of(&source).unwrap_or_default(),
                "launch": super::fork_face::launch_of(&home, &source, sid).to_json(),
                "turn": turn.as_ref().map(|t| t.0),
                "startText": turn.and_then(|t| crate::common::time::iso_hm_here(&t.1)),
            })
        }
    };
    let tmux = !matches!(super::session_batch_face::tmux_rows(), Ok(None));
    Ok(json!({
        "recent": recent_dirs(&record_dirs(&home), &crate::observe::history_query::hidden_cwd),
        "tmux": tmux,
        "agents": launchable_here(&launcher_found),
        "fork": fork,
    }))
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_new_face_tests.rs"]
mod tests;
