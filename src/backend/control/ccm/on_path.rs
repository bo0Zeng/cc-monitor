//! 终端里敲 `ccm` 走到的是不是这台的后端落点那一份。帧 `ccm-probe` 的 `onPath` 与待办「终端里的 ccm 是旧的一份」只经这里判。

use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// 终端里敲 `ccm` 走到哪。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Reach {
    /// 终端的 `PATH` 问不出 —— 判不了，不说成「没有」。
    Unknown,
    /// `PATH` 上没有 `ccm`。
    Absent,
    /// 走到的就是落点那一份（比解过链接的文件）。
    Ours,
    /// 走到别的文件。
    Other(PathBuf),
}

/// `ccm-probe` 那一格的判词。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub(crate) enum OnPathVerdict {
    Ours,
    Other,
    Absent,
    Unknown,
}

/// 终端里敲 `ccm` 走到哪 ＋ 给人读的那一句（走到的就是落点那一份 ⇒ 空串）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct OnPath {
    pub(crate) verdict: OnPathVerdict,
    pub(crate) said: String,
}

/// `PATH` 上敲 `ccm` 会起来的那个文件（找法只住 `platform::shell::find_session_command`）与落点那一份比：解过链接是同一个文件 ⇒ 是它。
pub(crate) fn reach(
    path_env: Option<&str>,
    landing: &Path,
    windows: bool,
    is_file: &dyn Fn(&Path) -> bool,
) -> Reach {
    use crate::platform::fs::Ro;
    use crate::platform::shell::SessionCommand;
    match crate::platform::shell::find_session_command("ccm", path_env, windows, is_file) {
        SessionCommand::Unknown => Reach::Unknown,
        SessionCommand::Absent => Reach::Absent,
        SessionCommand::Found { file, .. } => {
            let canon = |p: &Path| Ro::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
            if canon(&file) == canon(landing) {
                Reach::Ours
            } else {
                Reach::Other(file)
            }
        }
    }
}

/// 判词 ＋ 那一句。`shown` = 落点那一份在界面上的写法（[`landing_shown`]）。
pub(crate) fn on_path(r: &Reach, shown: &str) -> OnPath {
    let (verdict, said) = match r {
        Reach::Ours => (OnPathVerdict::Ours, String::new()),
        Reach::Other(at) => (
            OnPathVerdict::Other,
            copy_text(
                "beCcmProbe.onPath.other",
                &[("at", &at.display().to_string()), ("where", shown)],
            ),
        ),
        Reach::Absent => (
            OnPathVerdict::Absent,
            copy_text("beCcmProbe.onPath.absent", &[("where", shown)]),
        ),
        Reach::Unknown => (
            OnPathVerdict::Unknown,
            copy_text("beCcmProbe.onPath.unknown", &[]),
        ),
    };
    OnPath { verdict, said }
}

/// 落点那一份怎么写给人看：POSIX 写 `$HOME/…`（与生成进命令里的那一形同）；Windows 写绝对路径。
pub(crate) fn landing_shown(landing: &Path, windows: bool) -> String {
    if windows {
        landing.display().to_string()
    } else {
        crate::platform::shell::posix::home_path_shown(relay_route_core::BACKEND_LANDING_REL)
    }
}

/// 这台此刻：现问新开终端的 `PATH`（不缓存），与这台的落点比。家目录说不出 ⇒ 判不了。
pub(crate) fn here() -> OnPath {
    let windows = cfg!(windows);
    let Some(landing) = crate::platform::paths::installed_ccm_entry().map(PathBuf::from) else {
        return on_path(&Reach::Unknown, "");
    };
    let path = crate::platform::shell::fresh_terminal_path();
    let r = reach(path.as_deref(), &landing, windows, &|p| p.is_file());
    on_path(&r, &landing_shown(&landing, windows))
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/ccm/on_path_tests.rs"]
mod tests;
