//! 帧 `ccm-probe` 的 `onPath` 那一格：终端里敲 `ccm` 走到哪（判法只住 `platform::shell::reach`）＋ 给人读的那一句。

use crate::platform::shell::{reach, Reach};
use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// `ccm-probe` 那一格的判词。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub(crate) enum OnPathVerdict {
    Ours,
    Other,
    Absent,
    Unknown,
}

/// 终端里敲 `ccm` 走到哪 ＋ 给人读的那一句（走到的就是落点那一份 ⇒ 空串）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub(crate) struct OnPath {
    pub(crate) verdict: OnPathVerdict,
    pub(crate) said: String,
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
