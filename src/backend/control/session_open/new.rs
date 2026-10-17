//! 新起那一支要的几样：分叉自哪 · 轮换用哪一份 · 起之前定 sid 那几下 · 失败信封的 `data` · 目录那一问（`session-open-dir`）。

use super::super::launch_account as la;
use super::super::session_batch::{carriers, Deps, NameBase, Standing};
use serde::{Deserialize, Serialize};

/// 分叉自：源会话 ＋ 从哪条消息处。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct ForkFrom {
    /// 源会话 id。
    pub(crate) sid: String,
    /// 从哪条消息处分叉。
    pub(crate) uuid: String,
}

/// 轮换：用哪一份策略。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct RotationPick {
    /// 策略的 id（策略目录里的文件名去掉 `.rhai`，非空）；`null` ⇒ 这个会话不轮换（新会话用的那份不管它）。
    #[cfg_attr(test, ts(type = "string | null"))]
    pub(crate) strategy: Option<String>,
}

/// 哪一格不行（`data.field`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub enum OpenField {
    Agent,
    Command,
    Cwd,
    Account,
    Place,
    TmuxName,
}

/// 失败信封的 `data`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct OpenRefusal {
    /// 不行的那一格；整体的（起不来 · 分叉写不成 · 渲不出）⇒ `null`。
    pub field: Option<OpenField>,
    /// 选不了号（码 `account_unavailable`）⇒ 那一形；别的 ⇒ `null`。
    pub unavailable: Option<la::AccountUnavailable>,
}

/// 起之前定 sid 那几下（宿主给）：铸一个 sid（那一家认先定 sid 的新起都铸）· 选了策略（或「不轮换」，id 空）时起之前把那个会话写成用那一份（策略不在 ⇒ `no_strategy`）· 起不成撤掉。
pub(crate) struct PreRotation<'a> {
    pub(crate) sid: &'a dyn Fn() -> String,
    pub(crate) write: &'a dyn Fn(&str, &str, &str) -> Result<(), (&'static str, String)>,
    pub(crate) forget: &'a dyn Fn(&str),
}

/// 写分支记录：`(源 sid, 消息 uuid)` ⇒ 新 sid；不成 ⇒ `(码, 那一句)`。
pub(crate) type ForkWrite<'a> = &'a dyn Fn(&str, &str) -> Result<String, (&'static str, String)>;

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

/// 那一家起的时候收不收第一句（[`crate::agents::LaunchFace::first_prompt_arg`]）：不收 ⇒ 那句不行的话。
pub(crate) fn first_prompt_refused(face: &crate::agents::LaunchFace) -> Option<&'static str> {
    (!face.first_prompt_arg).then_some("this agent cannot take a first prompt at launch")
}

/// `session-open-dir` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct OpenDirReq {
    /// 工作目录（开头的 `~` 按这台的家目录读）。
    pub(crate) cwd: String,
    /// 分叉出来的那一条：源会话 sid（铸名按它此刻所在的终端名派生 `<…>-fork-cc`）。
    #[serde(default, rename = "forkOf")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) fork_of: Option<String>,
}

/// `session-open-dir` 的应答。
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct DirChecked {
    /// 这个目录在不在。
    pub(crate) exists: bool,
    /// 这台此刻会给它铸的终端名（没 tmux ⇒ `null`）。
    pub(crate) tmux_name: Option<String>,
}

crate::wire::typed::codes! {
    /// `session-open-dir` 自己没有别的码（`forkOf` 不是会话 id ⇒ `bad_args`）。
    pub(crate) enum OpenDirErr {}
}

/// `session-open-dir` 的本体：目录在不在 · 这台此刻会给它铸的终端名（没 tmux ⇒ `null`）。
pub(crate) fn dir_answer(
    req: &OpenDirReq,
    deps: &Deps,
    home: Option<&std::path::Path>,
) -> Result<DirChecked, OpenDirErr> {
    let fork_of = match req.fork_of.as_deref() {
        None => None,
        Some(s) if shell_quote_core::session_id_ok(s) => Some(s),
        Some(_) => {
            return Err(OpenDirErr::BadArgs(
                crate::common::contract::malformed("`forkOf` must be a session id"),
            ))
        }
    };
    let cwd = expand_home(req.cwd.trim(), home);
    let exists = !cwd.is_empty() && (deps.host.is_dir)(&cwd);
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
    Ok(DirChecked { exists, tmux_name })
}

