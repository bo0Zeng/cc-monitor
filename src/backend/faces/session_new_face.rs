//! 帧面 `session-new` · `session-new-facts` · `session-new-dir` 的宿主壳：起新会话框要的这台事实（最近用过的目录 · 有没有 tmux ·
//! 这台能起哪几家 · 分叉源会话的三格）住观测层，起会话要的那几样（tmux 名单 · 铸名 · 起 ccm · 号）同批量起那一份，
//! 写分支记录交 `control/fork_write`（本体与 `session-fork` 同一份）。

use crate::control::session_new;
use crate::platform::fs::Ro;
use serde_json::Value;
use std::collections::BTreeMap;

/// 「最近用过的目录」最多列几条。
pub(crate) const RECENT_MAX: usize = 8;

/// `session-new`：起一个新会话（全产品一个框、一个请求）。阻塞档；写自家状态（起之前写好打开哪一份轮换配置）⇒ 写句柄由分派那一层递。
pub(crate) struct SessionNew;

impl crate::wire::typed::Command for SessionNew {
    const NAME: &'static str = "session-new";
    type Req = session_new::NewRequest;
    type Reply = session_new::SessionNew;
    type Err = session_new::SessionNewErr;
}

impl crate::wire::typed::WithOwn for SessionNew {
    fn run(
        req: session_new::NewRequest,
        _cx: &crate::wire::typed::Cx,
        files: crate::wire::typed::Files,
        own: &crate::platform::Own,
    ) -> Result<session_new::SessionNew, session_new::SessionNewErr> {
        // 带 `forkFrom` 时写分支记录：同 `session-fork`，在这里领一只只会新建的手。
        answer(req, files, own, &crate::platform::fs::NewFile::issue())
            .map_err(session_new::SessionNewErr)
    }
}

/// `session-new` 的生产那一份事实。
fn answer(
    req: session_new::NewRequest,
    files: super::launch_face::Files,
    own: &crate::platform::fs::Own,
    nf: &crate::platform::fs::NewFile,
) -> Result<session_new::SessionNew, session_new::Failed> {
    let home = crate::history::listing::agent_home();
    let fork =
        |sid: &str, uuid: &str| crate::control::fork_write::fork_for_launch(nf, &home, sid, uuid);
    let user_home = crate::platform::paths::home_dir();
    let ctx = super::rotation_face::Ctx::here();
    let sid = crate::accounts::quota::rotation::new_session_id;
    let write = |sid: &str, kind: &str, rule: &str| {
        super::rotation_face::preset_with(
            &ctx,
            own,
            sid,
            kind,
            rule,
            crate::accounts::quota::now_unix(),
        )
    };
    let forget = |sid: &str| super::rotation_face::forget_preset_with(&ctx, own, sid);
    let pre = session_new::PreRotation {
        sid: &sid,
        write: &write,
        forget: &forget,
    };
    let agent = req.agent.clone();
    super::session_batch_face::with_deps_as(None, &agent, Some(files), |d| {
        session_new::answer(req, d, &fork, &pre, user_home.as_deref())
    })
}

/// `session-new-dir`：起新会话前核一个目录（在不在 · 这台此刻会给它铸的终端名）。阻塞档：起一次 `tmux` 列会话、铸名。
pub(crate) struct SessionNewDir;

impl crate::wire::typed::Command for SessionNewDir {
    const NAME: &'static str = "session-new-dir";
    type Req = session_new::SessionNewDirReq;
    type Reply = session_new::DirChecked;
    type Err = session_new::SessionNewDirErr;
}

impl crate::wire::typed::Blocking for SessionNewDir {
    fn run(
        req: session_new::SessionNewDirReq,
        _cx: &crate::wire::typed::Cx,
    ) -> Result<session_new::DirChecked, session_new::SessionNewDirErr> {
        let user_home = crate::platform::paths::home_dir();
        super::session_batch_face::with_deps_as(None, "", None, |d| {
            session_new::dir_answer(&req, d, user_home.as_deref())
        })
    }
}

/// 默认启动器在起会话那个 shell 的 `PATH` 上找得到 —— 与足迹里「装没装」、`ccm` 起它那一下问同一个函数（`platform::shell::find_session_command`）。
fn launcher_found(name: &str) -> bool {
    launcher_found_in(
        name,
        crate::platform::shell::session_shell_path().as_deref(),
        cfg!(windows),
    )
}

/// [`launcher_found`] 的本体（那份 `PATH` 与平台是参数）。问不出那个 shell 的 `PATH` ⇒ 不藏（判不了不当成没装）。
pub(crate) fn launcher_found_in(name: &str, session_path: Option<&str>, windows: bool) -> bool {
    !matches!(
        crate::platform::shell::find_session_command(name, session_path, windows, &|p| p.is_file()),
        crate::platform::shell::SessionCommand::Absent
    )
}

/// 这台能起的几家（注册表里由我们起的、且默认启动器在这台找得到的；注册表序）。
pub(crate) fn launchable_here(found: &dyn Fn(&str) -> bool) -> Vec<&'static str> {
    crate::agents::launchable_kinds()
        .into_iter()
        .filter(|k| crate::agents::pick_kind(Some(k)).is_ok_and(|(_, f)| found(f.default_launcher)))
        .collect()
}

/// `session-new-facts` 的应答与它引到的几样：线上形住 `cmd-wire::session`。
pub(crate) use cmd_wire::session::{AgentFacts, ForkFacts, NewFacts, Pick, RecentDir};

/// 一家的起会话事实（名字 · 可选的模型与权限档）；`kind` 认不出 ⇒ 名字照 kind、两张表空。
pub(crate) fn agent_facts(kind: &'static str) -> AgentFacts {
    let face = crate::agents::pick_kind(Some(kind)).ok().map(|(_, f)| f);
    let picks = |cs: &[crate::agents::Choice]| {
        cs.iter()
            .map(|c| Pick {
                value: c.value.to_string(),
                text: (c.text)(),
            })
            .collect()
    };
    AgentFacts {
        kind: kind.to_string(),
        text: face.map_or(kind, |f| f.display_name).to_string(),
        models: face.and_then(|f| f.models).map(picks).unwrap_or_default(),
        model_default: face.and_then(|f| f.model_default).map(|c| Pick {
            value: c.value.to_string(),
            text: (c.text)(),
        }),
        model_note: face
            .and_then(|f| f.model_switch)
            .and_then(|m| m.note)
            .map(|n| n()),
        permission_modes: face
            .and_then(|f| f.permission)
            .map(|p| picks(p.modes))
            .unwrap_or_default(),
    }
}

/// 各家记录里的工作目录 ⇒ 最近用过的那几个（新的在前，同一个目录只出一次；不出会话的那个目录不算）。
pub(crate) fn recent_dirs(
    items: &[(String, i64)],
    hidden: &dyn Fn(&str) -> bool,
    clock: &crate::common::time::TextClock,
) -> Vec<RecentDir> {
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
        .map(|(cwd, ms)| RecentDir {
            cwd: cwd.to_string(),
            last_ms: ms,
            last_text: clock
                .text(u64::try_from(ms.div_euclid(1000)).unwrap_or(0))
                .0,
        })
        .collect()
}

/// 记录树那一家的项目 ＋ 合成历史那几家的会话 ⇒ `(工作目录, 最近一次)`。
fn record_dirs(home: &std::path::Path) -> Vec<(String, i64)> {
    let mut buf: Vec<u8> = Vec::new();
    // 读不出记录树 ⇒ 记一行、少这一家的目录（框里照常起，只是「最近的」少几项）。
    if let Err(e) = crate::history::listing::list_projects_into(home, &mut buf) {
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

/// `session-new-facts` 的入参。
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct NewFactsReq {
    /// 可缺：分叉源会话 sid。
    #[serde(default, rename = "forkOf")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) fork_of: Option<String>,
    /// 可缺（随 `forkOf`）：从哪条消息处分叉（框顶说第几轮、几点）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) at: Option<String>,
}

crate::wire::typed::codes! {
    /// `session-new-facts` 的码。
    pub(crate) enum NewFactsErr {
        /// 分叉源会话的记录找不着 / 记录树读不出。
        ForkFailed = "fork_failed",
    }
}

/// `session-new-facts`：起新会话那个框要的事实（只读，不写分支记录）。阻塞档：扫各家记录目录 · 问一次 tmux。
pub(crate) struct SessionNewFacts;

impl crate::wire::typed::Command for SessionNewFacts {
    const NAME: &'static str = "session-new-facts";
    type Req = NewFactsReq;
    type Reply = NewFacts;
    type Err = NewFactsErr;
}

impl crate::wire::typed::Blocking for SessionNewFacts {
    fn run(req: NewFactsReq, cx: &crate::wire::typed::Cx) -> Result<NewFacts, NewFactsErr> {
        facts(&req, &cx.tz).map_err(|(code, said)| match code {
            "bad_args" => NewFactsErr::BadArgs(said),
            _ => {
                debug_assert_eq!(code, "fork_failed");
                NewFactsErr::ForkFailed(said)
            }
        })
    }
}

/// `session-new-facts` 的本体：`{forkOf?, at?}` ⇒ `{recent, tmux, agents, fork}`。
/// `tz` ＝ 看的那一台的时区：分叉那一轮的起点钟面按它写。
pub(crate) fn facts(req: &NewFactsReq, tz: &crate::Tz) -> Result<NewFacts, (&'static str, String)> {
    let fork_of = match req.fork_of.as_deref() {
        None => None,
        Some(s) if shell_quote_core::session_id_ok(s) => Some(s),
        Some(_) => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("`forkOf` must be a session id"),
            ))
        }
    };
    let at = match req.at.as_deref() {
        None => None,
        Some(s) if shell_quote_core::session_id_ok(s) && fork_of.is_some() => Some(s),
        Some(_) => {
            return Err((
                "bad_args",
                crate::common::contract::malformed(
                    "`at` must be a message id, given with `forkOf`",
                ),
            ))
        }
    };
    let home = crate::history::listing::agent_home();
    let fork = match fork_of {
        None => None,
        Some(sid) => {
            let root = crate::agents::records_root(&home).ok_or((
                "fork_failed",
                copy_core::copy_text("beSessionNew.fork.noTree", &[]),
            ))?;
            let source =
                crate::agents::find_session_file(&root, sid).map_err(|m| ("fork_failed", m))?;
            let turn = at.and_then(|uuid| {
                let f = Ro::open(&source).ok()?;
                crate::observe::turns::turn_at(std::io::BufReader::new(f), uuid)
            });
            Some(ForkFacts {
                agent: crate::agents::record_kind_of(&source)
                    .unwrap_or_default()
                    .to_string(),
                launch: super::fork_face::launch_of(&home, &source, sid),
                turn: turn.as_ref().map(|t| t.0),
                start_text: turn.and_then(|t| crate::common::time::iso_hm(&t.1, tz)),
            })
        }
    };
    let tmux = !matches!(super::session_batch_face::tmux_rows(), Ok(None));
    Ok(NewFacts {
        recent: recent_dirs(
            &record_dirs(&home),
            &crate::history::listing::hidden_cwd,
            &crate::common::time::TextClock::new(
                u64::try_from(crate::common::time::now_secs()).unwrap_or(0),
                tz,
            ),
        ),
        tmux,
        agents: launchable_here(&launcher_found)
            .into_iter()
            .map(agent_facts)
            .collect(),
        fork,
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_new_face_tests.rs"]
mod tests;
