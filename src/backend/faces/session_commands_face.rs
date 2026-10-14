//! 帧面 `session-commands` 的宿主壳：一条会话能用的斜杠命令与 Skill（［+］面板 · 输入框打 `/` 那一份）。
//! 内置几条由适配层声明；住文件里的按那条会话的项目目录与它用的号的配置目录现扫（目录名 · 认法住适配层）；每格的字由这里写好。

use crate::agents::AssetFace;
use crate::common::cells::Words;
use crate::common::contract::malformed;
use crate::wire::spec::Fail;
use crate::wire::typed::{self, Command};
use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// `session-commands` 的入参。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionCommandsReq {
    /// 会话 id。
    pub(crate) sid: String,
}

/// `session-commands` 的回包。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct SessionCommands {
    /// 内置的在前，再本项目、用户级（各按名字排）。
    pub(crate) commands: Vec<SessionCommand>,
    /// 本项目的在前，再用户级。
    pub(crate) skills: Vec<SessionSkill>,
    /// 读不出来的那几份（一句一份；不整条失败）。
    pub(crate) problems: Vec<String>,
}

/// 一条斜杠命令。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionCommand {
    /// 命令名（不带 `/`；界面填进输入框的是 `/name `）。
    pub(crate) name: String,
    /// 简介；文件里没写 ⇒ `null`。
    pub(crate) text: Option<String>,
    /// 住哪。
    pub(crate) scope: CommandScope,
    /// 同一格写给人看的字。
    #[cfg_attr(test, ts(type = "string"))]
    pub(crate) scope_text: Words,
    /// 要跟参数（填进去停住等补）。
    pub(crate) needs_args: bool,
    /// 发之前先确认。
    pub(crate) confirm: bool,
}

/// 命令住哪。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommandScope {
    /// 那一家内置。
    Builtin,
    /// 本项目（`<项目>/.claude/commands`）。
    Project,
    /// 这条会话用的号的配置目录里。
    User,
}

/// 一个 Skill。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct SessionSkill {
    /// 名字。
    pub(crate) name: String,
    /// 简介；没写 ⇒ `null`。
    pub(crate) text: Option<String>,
    /// 点它时发进会话的那一句（后端写好，经 `session-say` 发）。
    #[cfg_attr(test, ts(type = "string"))]
    pub(crate) say: Words,
}

typed::codes! {
    /// `session-commands` 的码。
    pub(crate) enum SessionCommandsErr {
        /// 这台找不到这条会话的记录。
        NoSuchSession = "no_such_session",
    }
}

/// `session-commands`：一条会话能用的命令与 Skill。阻塞档（读记录开头 · 扫几个目录）。
pub(crate) struct SessionCommandsCmd;

impl Command for SessionCommandsCmd {
    const NAME: &'static str = "session-commands";
    type Req = SessionCommandsReq;
    type Reply = SessionCommands;
    type Err = SessionCommandsErr;
}

impl crate::wire::typed::Handle for SessionCommandsCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: SessionCommandsReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<SessionCommands, SessionCommandsErr>> + Send + 'static
    {
        async move { answer(&req).map_err(|f| typed::of_fail(f, SessionCommandsErr::NoSuchSession)) }
    }
}

fn answer(req: &SessionCommandsReq) -> Result<SessionCommands, Fail> {
    if !shell_quote_core::session_id_ok(&req.sid) {
        return Err(Fail::new(
            "bad_args",
            malformed("`sid` must be a session id"),
        ));
    }
    let home = crate::history::listing::agent_home();
    let record = crate::observe::history_query::session_record(&home, &req.sid).map_err(|_| {
        Fail::new(
            "no_such_session",
            copy_text("beSessionCommands.session.noRecord", &[]),
        )
    })?;
    let kind = crate::agents::record_kind_of(&record).unwrap_or_default();
    let Some(face) = crate::agents::asset_face(kind) else {
        return Ok(SessionCommands {
            commands: Vec::new(),
            skills: Vec::new(),
            problems: Vec::new(),
        });
    };
    let project = crate::agents::project_dir_of(&record).map(PathBuf::from);
    let root = config_root_of(
        super::launch_face::last_of(&req.sid).as_deref(),
        &super::launch_face::library_of(kind),
        crate::agents::home_of_kind(kind).as_deref(),
    );
    Ok(match root {
        Some(root) => commands_of(&face, project.as_deref(), &root),
        None => commands_of(&face, project.as_deref(), Path::new("")),
    })
}

/// 这条会话用哪个号的配置目录（同起会话「跟随」那一序）：上次用的号 → 这台默认号 → 账号 0（那一家自己的家，`zero`）。
/// 号在库里却没有配置目录 ⇒ 往下一级退。
pub(crate) fn config_root_of(
    last: Option<&str>,
    lib: &crate::control::launch_account::Library,
    zero: Option<&Path>,
) -> Option<PathBuf> {
    let dir_of = |name: &str| {
        lib.accounts
            .iter()
            .find(|a| a.name == name)
            .and_then(|a| a.config_dir.as_deref())
            .map(PathBuf::from)
    };
    last.and_then(dir_of)
        .or_else(|| lib.default.as_deref().and_then(dir_of))
        .or_else(|| zero.map(Path::to_path_buf))
}

/// 本体：内置 ＋ 那两处现扫的，写好字。
pub(crate) fn commands_of(
    face: &AssetFace,
    project: Option<&Path>,
    root: &Path,
) -> SessionCommands {
    let seen = (face.session_scan)(project, root);
    let scope_text = |s: CommandScope| {
        Words(match s {
            CommandScope::Builtin => copy_text("beSessionCommands.scope.builtin", &[]),
            CommandScope::Project => copy_text("beSessionCommands.scope.project", &[]),
            CommandScope::User => copy_text("beSessionCommands.scope.user", &[]),
        })
    };
    let builtin = face.builtin_commands.iter().map(|b| SessionCommand {
        name: b.name.to_string(),
        text: Some((b.text)()),
        scope: CommandScope::Builtin,
        scope_text: scope_text(CommandScope::Builtin),
        needs_args: b.needs_args,
        confirm: b.confirm,
    });
    let files = seen.commands.into_iter().map(|c| {
        let scope = if c.project {
            CommandScope::Project
        } else {
            CommandScope::User
        };
        SessionCommand {
            name: c.name,
            text: c.description,
            scope,
            scope_text: scope_text(scope),
            needs_args: c.needs_args,
            confirm: false,
        }
    });
    SessionCommands {
        commands: builtin.chain(files).collect(),
        skills: seen
            .skills
            .into_iter()
            .map(|s| SessionSkill {
                say: Words(copy_text(
                    "beSessionCommands.skill.say",
                    &[("name", &s.name)],
                )),
                name: s.name,
                text: s.description,
            })
            .collect(),
        problems: seen.problems,
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_commands_face_tests.rs"]
mod tests;
