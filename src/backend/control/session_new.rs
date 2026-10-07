//! **起一个新会话 —— 全产品一个请求**（帧命令 `session-new`）：`{agent, cwd, account?, place, tmuxName?, command?, forkFrom?}`。
//!
//! 这台自己判：那一家认不认得 · 启动命令能不能拼 · 目录在不在 · 用哪个号（选不了 ⇒ 不起、带替代号）· 有没有 tmux ·
//! 终端名（给了就核不占用，没给就铸）· 分叉（只在这一步写分支记录，前面几格全过了才写）· 起。
//!
//! 回：`started`（在这台 tmux 里后台起好了，带会话名）· `open`（要开一个终端跑那一行：带 `cmd`，窗口由 monitor 开）。
//! 某一格不行 ⇒ 失败信封，`data.field` 说是哪一格（`agent` · `command` · `cwd` · `account` · `place` · `tmuxName`；整体的 ⇒ `null`），
//! 选不了号 ⇒ `data.unavailable` 是那一形（带替代号）。会话报没报到由界面等那台的会话流（不在这里等）。
//!
//! 要动 tmux / 起 ccm / 写分支记录的几样由入口经 [`Deps`] 交进来（control 不引用 observe），判据交替身。

use super::launch_account::{self as la, AccountAsk, LaunchedAccount, Settled};
use super::launch_render::{local, wire};
use super::session_batch::{carriers, Deps, NameBase, Standing};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// 失败：码 ＋ 那一句 ＋ `data`（[`Refusal`]）。
pub(crate) type Failed = (&'static str, String, Option<Value>);

/// 放在哪：这台 tmux 里后台起（关终端不断）· 开一个新的终端窗口直接跑。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Place {
    Tmux,
    Window,
}

/// 分叉自：源会话 ＋ 从哪条消息处。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ForkFrom {
    pub(crate) sid: String,
    pub(crate) uuid: String,
}

/// `session-new` 的入参（`deny_unknown_fields`：多送一格 ⇒ 拒）。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct NewRequest {
    /// 哪一家（线上的 kind）。必填。
    pub(crate) agent: String,
    pub(crate) cwd: String,
    /// 缺席 ＝ 跟随（分叉跟随源会话上次的号；新起的 ⇒ 这台的默认号）。
    #[serde(default)]
    pub(crate) account: Option<AccountAsk>,
    pub(crate) place: Place,
    /// 用户改过的终端名；缺席 ⇒ 这台铸（分叉不收这一格：名字从源会话的终端名铸）。
    #[serde(default)]
    pub(crate) tmux_name: Option<String>,
    /// 启动命令（空 / 缺席 ⇒ 那一家的默认启动器）。
    #[serde(default)]
    pub(crate) command: Option<String>,
    #[serde(default)]
    pub(crate) fork_from: Option<ForkFrom>,
    /// 这台的模型偏好表（号 → 模型，用户设置的原值）。
    #[serde(default)]
    pub(crate) models: BTreeMap<String, String>,
    /// 发请求的界面就在这台上（开窗那一形本机与远端渲法不同）。
    pub(crate) local: bool,
}

/// 哪一格不行（`data.field`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub enum SessionNewField {
    Agent,
    Command,
    Cwd,
    Account,
    Place,
    TmuxName,
}

/// 失败信封的 `data`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SessionNewRefusal {
    /// 不行的那一格；整体的（起不来 · 分叉写不成 · 渲不出）⇒ `null`。
    pub field: Option<SessionNewField>,
    /// 选不了号（码 `account_unavailable`）⇒ 那一形；别的 ⇒ `null`。
    pub unavailable: Option<la::AccountUnavailable>,
}

/// 结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub enum SessionNewOutcome {
    /// 在这台 tmux 里后台起好了（`session` 是会话名）。
    Started,
    /// 要开一个终端窗口跑 `cmd`（窗口由 monitor 开）。
    Open,
}

/// `session-new` 的成品。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct SessionNew {
    pub outcome: SessionNewOutcome,
    /// tmux 会话名（`started` 那一形；开窗那一形 ⇒ `null`）。
    pub session: Option<String>,
    /// 分叉出来的新会话 sid（新起的 ⇒ `null`：报到之前说不出）。
    pub sid: Option<String>,
    /// 开窗那一形要跑的那一行（`started` ⇒ `null`）。
    pub cmd: Option<String>,
    /// 实际用的号（账号 0 / 不指定 ⇒ `null`）。
    pub account: Option<LaunchedAccount>,
    /// 起的是哪一家。
    pub agent: String,
    /// 起在哪个目录（`~` 已按这台的家目录展开：认报到的会话按它）。
    pub cwd: String,
}

/// 写分支记录：`(源 sid, 消息 uuid)` ⇒ 新 sid；不成 ⇒ `(码, 那一句)`。
pub(crate) type ForkWrite<'a> = &'a dyn Fn(&str, &str) -> Result<String, (&'static str, String)>;

fn fail(code: &'static str, said: String, field: Option<SessionNewField>) -> Failed {
    let data = SessionNewRefusal {
        field,
        unavailable: None,
    };
    (code, said, serde_json::to_value(data).ok())
}

/// `~` / `~/…` ⇒ 这台的家目录下（界面显示的目录常写成 `~/…`）。家说不出 ⇒ 原样。
pub(crate) fn expand_home(cwd: &str, home: Option<&std::path::Path>) -> String {
    let rest = match cwd.strip_prefix('~') {
        Some(r) if r.is_empty() || r.starts_with('/') => r,
        _ => return cwd.to_string(),
    };
    match home {
        Some(h) => format!("{}{rest}", h.to_string_lossy().trim_end_matches('/')),
        None => cwd.to_string(),
    }
}

/// `session-new` 的本体。`home` ＝ 这台的家目录（展开 `~`）。
pub(crate) fn answer(
    args: &Value,
    deps: &Deps,
    fork: ForkWrite,
    home: Option<&std::path::Path>,
) -> Result<Value, Failed> {
    let req: NewRequest = serde_json::from_value(args.clone()).map_err(|e| {
        (
            "bad_args",
            crate::common::contract::malformed(&e.to_string()),
            None,
        )
    })?;
    // 没说是哪一家就不起（不落默认那一家：默认是界面读画像时的事）。
    if req.agent.trim().is_empty() {
        return Err(fail(
            "unknown_agent",
            copy_core::copy_text("beSessionNew.agent.missing", &[]),
            Some(SessionNewField::Agent),
        ));
    }
    let (kind, face) = crate::agents::pick_kind(Some(&req.agent))
        .map_err(|m| fail("unknown_agent", m, Some(SessionNewField::Agent)))?;
    // 启动命令：拼进那一行之前过全仓那一张命令片段白名单。
    let command = req
        .command
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    if let Some(c) = command.and_then(shell_quote_core::launcher_refused_char) {
        return Err(fail(
            "bad_command",
            copy_core::copy_text(
                "rsHistory.launcher.badChars",
                &[
                    ("launcher", &format!("{:?}", command.unwrap_or_default())),
                    ("c", &format!("{c:?}")),
                ],
            ),
            Some(SessionNewField::Command),
        ));
    }
    let launcher = command.unwrap_or(face.default_launcher).to_string();
    let cwd = expand_home(req.cwd.trim(), home);
    if cwd.is_empty() || !(deps.local_facts.is_dir)(&cwd) {
        return Err(fail(
            "no_dir",
            copy_core::copy_text("beSessionNew.cwd.missing", &[("cwd", req.cwd.trim())]),
            Some(SessionNewField::Cwd),
        ));
    }
    if let Some(f) = &req.fork_from {
        for id in [&f.sid, &f.uuid] {
            if !shell_quote_core::session_id_ok(id) {
                return Err(fail(
                    "bad_args",
                    crate::common::contract::malformed(
                        "`forkFrom` needs a session id and a message id",
                    ),
                    None,
                ));
            }
        }
        if req.tmux_name.is_some() {
            return Err(fail(
                "bad_args",
                crate::common::contract::malformed(
                    "`tmuxName` does not go with `forkFrom` (the fork mints its own)",
                ),
                None,
            ));
        }
    }
    // 号：跟随时分叉跟源会话上次那个号。
    let asked = req.account.clone().unwrap_or(AccountAsk::Follow);
    let source = req.fork_from.as_ref().map(|f| f.sid.as_str());
    let account = la::settle(&asked, source, &req.models, deps.accounts).map_err(|u| {
        let said = la::unavailable_said(&u);
        let data = SessionNewRefusal {
            field: Some(SessionNewField::Account),
            unavailable: Some(u),
        };
        ("account_unavailable", said, serde_json::to_value(data).ok())
    })?;
    // 终端名（只 tmux 那一形）：给了 ⇒ 核写法与不占用；没给 ⇒ 这台铸（分叉从源会话此刻的终端名铸）。
    let name = match req.place {
        Place::Window => None,
        Place::Tmux => Some(tmux_name(&req, &cwd, deps)?),
    };
    // 前面几格全过了才写分支记录（取消 / 某格不行都不留东西）。
    let sid = match &req.fork_from {
        Some(f) => Some(fork(&f.sid, &f.uuid).map_err(|(c, m)| fail(c, m, None))?),
        None => None,
    };
    let launched = match &account {
        Settled::Account(a) => Some(a.clone()),
        _ => None,
    };
    let reply = |outcome, session, cmd| SessionNew {
        outcome,
        session,
        sid: sid.clone(),
        cmd,
        account: launched.clone(),
        agent: kind.to_string(),
        cwd: cwd.clone(),
    };
    let out = match name {
        Some(name) => {
            let argv = own_entry(deps).and_then(|entry| {
                wire::ccm_launch_argv(
                    &wire_req(
                        kind,
                        &launcher,
                        face.default_launcher,
                        sid.as_deref(),
                        Some(&name),
                        &cwd,
                        &asked,
                        &req.models,
                    ),
                    &account,
                    deps.caps,
                    &entry,
                    true,
                )
            });
            let argv = argv.map_err(|m| fail("refused", m, None))?;
            match (deps.run_ccm)(&argv) {
                Ok((0, _, _)) => reply(SessionNewOutcome::Started, Some(name), None),
                // ccm 的退出码 3 = 会话名被占（它响亮失败，不接回别人的会话）。
                Ok((3, _, _)) => {
                    return Err(fail(
                        "tmux_taken",
                        copy_core::copy_text("beSessionNew.tmux.taken", &[("name", &name)]),
                        Some(SessionNewField::TmuxName),
                    ))
                }
                Ok((_, _, err)) => return Err(fail("start_failed", err.trim().to_string(), None)),
                Err((c, m)) => return Err(fail(c, m, None)),
            }
        }
        None if req.local => {
            let lreq = local::LocalLaunchRequest {
                agent: kind.to_string(),
                action: match &sid {
                    Some(s) => local::LocalAction::Resume { sid: s.clone() },
                    None => local::LocalAction::New,
                },
                cwd: Some(cwd.clone()),
                launcher: Some(launcher.clone()).filter(|l| l != face.default_launcher),
                account: Some(asked.clone()),
                tmux_name: None,
                default_launcher: face.default_launcher.to_string(),
            };
            let cmd = local::plan(&lreq, &account, &deps.local_facts)
                .map_err(|m| fail("refused", m, None))?;
            reply(SessionNewOutcome::Open, None, Some(cmd))
        }
        None => {
            let cmd = own_entry(deps).and_then(|entry| {
                wire::render_ccm_launch_with(
                    &wire_req(
                        kind,
                        &launcher,
                        face.default_launcher,
                        sid.as_deref(),
                        None,
                        &cwd,
                        &asked,
                        &req.models,
                    ),
                    &account,
                    deps.caps,
                    &entry,
                )
            });
            reply(
                SessionNewOutcome::Open,
                None,
                Some(cmd.map_err(|m| fail("refused", m, None))?),
            )
        }
    };
    serde_json::to_value(out).map_err(|e| ("bad_args", e.to_string(), None))
}

/// tmux 那一形的会话名。这台没 tmux ⇒ `place` 那一格不行；看不见名单 ⇒ 整体不行（不拿空名单铸：那是不避让）。
fn tmux_name(req: &NewRequest, cwd: &str, deps: &Deps) -> Result<String, Failed> {
    let rows = match (deps.list)() {
        Ok(Some(rows)) => rows,
        Ok(None) => {
            return Err(fail(
                "place_unavailable",
                copy_core::copy_text("beSessionNew.place.noTmux", &[]),
                Some(SessionNewField::Place),
            ))
        }
        Err(m) => return Err(fail("unobservable", m, None)),
    };
    let taken = |n: &str| rows.iter().any(|r| r.name == n);
    if let Some(n) = req
        .tmux_name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        if crate::control::gate_rules::new_tmux_name_issue(n).is_some() {
            return Err(fail(
                "bad_tmux_name",
                copy_core::copy_text("beSessionNew.tmux.badName", &[("name", n)]),
                Some(SessionNewField::TmuxName),
            ));
        }
        if taken(n) {
            return Err(fail(
                "tmux_taken",
                copy_core::copy_text("beSessionNew.tmux.taken", &[("name", n)]),
                Some(SessionNewField::TmuxName),
            ));
        }
        return Ok(n.to_string());
    }
    let base = match &req.fork_from {
        // 源会话此刻所在终端的名字（在跑的那一个；命中多个取第一个）；不在任何终端里 ⇒ 这一项的工作目录。
        Some(f) => NameBase::ForkOf(match carriers(&rows, &f.sid) {
            Standing::Running(e) => e.name.as_str(),
            Standing::Ambiguous(es) => es.first().map_or(cwd, |e| e.name.as_str()),
            Standing::Idle(_) | Standing::None => cwd,
        }),
        None => NameBase::Cwd(cwd),
    };
    let name = (deps.mint)(base).map_err(|(c, m)| fail(c, m, None))?;
    if taken(&name) {
        return Err(fail(
            "tmux_taken",
            copy_core::copy_text("beSessionNew.tmux.taken", &[("name", &name)]),
            Some(SessionNewField::TmuxName),
        ));
    }
    Ok(name)
}

/// 那一行的上线入参（与界面那一行同一份映射、同一个渲染器）：分叉 ⇒ resume 新 sid；新起 ⇒ new。
#[allow(clippy::too_many_arguments)]
fn wire_req(
    kind: &str,
    launcher: &str,
    default_launcher: &str,
    sid: Option<&str>,
    tmux: Option<&str>,
    cwd: &str,
    account: &AccountAsk,
    models: &BTreeMap<String, String>,
) -> wire::CliRenderRequest {
    wire::CliRenderRequest {
        agent: kind.to_string(),
        action: match sid {
            Some(s) => wire::WireAction::Resume { sid: s.to_string() },
            None => wire::WireAction::New,
        },
        container: match tmux {
            Some(name) => wire::WireContainer::Tmux {
                name: name.to_string(),
                send_into: false,
            },
            None => wire::WireContainer::None,
        },
        cwd: Some(cwd.to_string()),
        account: account.clone(),
        // 身份标记打在建出来的 tmux 会话上（resume 才说得出 sid）。
        ccm_sid: sid.filter(|_| tmux.is_some()).map(str::to_string),
        model: None,
        models: models.clone(),
        launcher: launcher.to_string(),
        default_launcher: default_launcher.to_string(),
    }
}

fn own_entry(deps: &Deps) -> Result<String, String> {
    (deps.local_facts.entry)()
        .ok_or_else(|| copy_core::copy_text("beLaunchRender.entry.noHome", &[]))
}

/// `session-new-dir` 的本体：`{cwd, forkOf?}` ⇒ `{exists, tmuxName}`（目录在不在 · 这台此刻会给它铸的终端名；没 tmux ⇒ `null`）。
pub(crate) fn dir_answer(
    args: &Value,
    deps: &Deps,
    home: Option<&std::path::Path>,
) -> Result<Value, (&'static str, String)> {
    let o = args.as_object().ok_or((
        "bad_args",
        crate::common::contract::malformed("args must be an object"),
    ))?;
    for k in o.keys() {
        if k != "cwd" && k != "forkOf" {
            return Err((
                "bad_args",
                crate::common::contract::malformed(&format!("unknown field `{k}`")),
            ));
        }
    }
    let raw = o.get("cwd").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing string `cwd`"),
    ))?;
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
    let cwd = expand_home(raw.trim(), home);
    let exists = !cwd.is_empty() && (deps.local_facts.is_dir)(&cwd);
    let tmux_name = match (deps.list)() {
        Ok(Some(rows)) => {
            let base = match fork_of {
                Some(sid) => NameBase::ForkOf(match carriers(&rows, sid) {
                    Standing::Running(e) => e.name.as_str(),
                    Standing::Ambiguous(es) => es.first().map_or(cwd.as_str(), |e| e.name.as_str()),
                    Standing::Idle(_) | Standing::None => cwd.as_str(),
                }),
                None => NameBase::Cwd(&cwd),
            };
            (deps.mint)(base).ok()
        }
        Ok(None) | Err(_) => None,
    };
    Ok(json!({ "exists": exists, "tmuxName": tmux_name }))
}

#[cfg(test)]
#[path = "../../../tests/backend/control/session_new_tests.rs"]
mod tests;
