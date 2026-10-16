//! `ccm-user-path`：`ccm` 所在目录在不在这台的**用户级** `PATH` 上（cmd · Git Bash 也敲得到 `ccm` 靠它）· 加 · 撤。
//! 只 Windows 有这一档；别处照实答「这台不是 Windows」。读写注册表那几段 PowerShell 住 `platform::shell::powershell`，
//! 这里只出成品：状态 · 那一句 · 详情 · 给人看的那段命令（与开关跑的同一份字节）。

use crate::common::said::Said;
use crate::platform::shell::powershell::{user_path_add, user_path_remove};
use copy_core::copy_text;

/// 要做的那一下。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub(crate) enum UserPathOp {
    Read,
    Add,
    Remove,
}

/// `ccm-user-path` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct UserPathReq {
    pub(crate) op: UserPathOp,
}

/// 现问出来的状态。问不出 ⇒ `unknown`，不说成「不在」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub(crate) enum UserPathState {
    On,
    Off,
    Unknown,
    NotWindows,
}

/// `ccm-user-path` 的应答：做完那一下之后现问的状态 ＋ 给人读的那一句（出了岔子时就是那一岔）＋ 复制详情 ＋ 那段命令。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct UserPath {
    pub(crate) state: UserPathState,
    pub(crate) said: String,
    /// 问不出 / 改没成时的复制详情（原话在里面）；顺利 ⇒ `null`。
    pub(crate) detail: Option<String>,
    /// 在 ⇒ 撤那一段；不在 ⇒ 加那一段；判不了 ⇒ `null`。
    pub(crate) command: Option<String>,
}

/// 成品。`dir` = 这台 `ccm` 所在目录（家目录说不出 ⇒ `None`）；`holds` / `set` 是问与改那两下（只 Windows 上会叫）。
pub(crate) fn answer(
    op: UserPathOp,
    windows: bool,
    dir: Option<&str>,
    holds: &dyn Fn(&str) -> Result<bool, Said>,
    set: &dyn Fn(&str, bool) -> Result<(), Said>,
) -> UserPath {
    let plain = |state, said| UserPath {
        state,
        said,
        detail: None,
        command: None,
    };
    if !windows {
        return plain(
            UserPathState::NotWindows,
            copy_text("beUserPath.notWindows", &[]),
        );
    }
    let Some(dir) = dir else {
        return plain(UserPathState::Unknown, copy_text("beUserPath.noHome", &[]));
    };
    let wrote = match op {
        UserPathOp::Read => Ok(()),
        UserPathOp::Add => set(dir, true).map_err(|s| ("beUserPath.addFailed", s)),
        UserPathOp::Remove => set(dir, false).map_err(|s| ("beUserPath.removeFailed", s)),
    };
    let (state, said, failed) = match holds(dir) {
        Ok(true) => (UserPathState::On, copy_text("beUserPath.on", &[]), None),
        Ok(false) => (
            UserPathState::Off,
            copy_text("beUserPath.off", &[("dir", dir)]),
            None,
        ),
        Err(s) => (
            UserPathState::Unknown,
            copy_text("beUserPath.unknown", &[("why", &s.said)]),
            Some(("unreadable", s.raw)),
        ),
    };
    let (said, failed) = match wrote {
        Ok(()) => (said, failed),
        Err((key, s)) => (
            copy_text(key, &[("why", &s.said)]),
            Some(("write_failed", s.raw)),
        ),
    };
    UserPath {
        state,
        said,
        detail: failed.map(|(code, raw)| crate::wire::detail::of(Some(NAME), code, raw.as_deref())),
        command: match state {
            UserPathState::On => Some(user_path_remove(dir)),
            UserPathState::Off => Some(user_path_add(dir)),
            UserPathState::Unknown | UserPathState::NotWindows => None,
        },
    }
}

const NAME: &str = "ccm-user-path";

/// 整条命令总期限的上限（改一发 ＋ 再问一发）。
pub(crate) const USER_PATH_CAP: crate::platform::child::Deadline =
    crate::platform::child::Deadline::secs(28);

/// 这台 `ccm` 所在目录（后端落点的上一级）；家目录说不出 ⇒ `None`。
fn ccm_dir() -> Option<String> {
    let entry = crate::platform::paths::installed_ccm_entry()?;
    std::path::Path::new(&entry)
        .parent()
        .map(|d| d.display().to_string())
}

/// 这台此刻（起 PowerShell ⇒ 阻塞档）。
pub(crate) fn here(op: UserPathOp) -> UserPath {
    use crate::platform::shell::powershell::{user_path_holds, user_path_set};
    let absent = || Said::from(copy_text("reason.spawn.notInstalled", &[]));
    answer(
        op,
        crate::platform::shell::speaks_powershell(),
        ccm_dir().as_deref(),
        &|d| user_path_holds(d).unwrap_or_else(|| Err(absent())),
        &|d, on| user_path_set(d, on).unwrap_or_else(|| Err(absent())),
    )
}

crate::wire::typed::codes! {
    /// `ccm-user-path` 没有别的码：问不出 · 改没成都在成品里（状态 ＋ 那一句 ＋ 详情）。
    pub(crate) enum UserPathErr {}
}

/// `ccm-user-path`：用户级 `PATH` 那一格（读 / 加 / 撤，做完现问）。
pub(crate) struct CcmUserPath;

impl crate::wire::typed::Command for CcmUserPath {
    const NAME: &'static str = NAME;
    type Req = UserPathReq;
    type Reply = UserPath;
    type Err = UserPathErr;
}

impl crate::wire::typed::Handle for CcmUserPath {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: UserPathReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<UserPath, UserPathErr>> + Send + 'static {
        async move { Ok(here(req.op)) }
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/ccm/user_path_tests.rs"]
mod tests;
