//! **一条会话隐不隐藏** —— 判定只住这里（[`judge`]，纯函数）。「隐藏」全产品一个意思：不占桌面标签页、不进历史默认清单；
//! 会话总览 · 需手动 · 通知照常。线上那一格是 [`Hidden`]（`{by, said}`，不隐藏 ⇒ 不上线）。
//!
//! 三处带它、都调 [`judge`]：会话宣告那一帧（`observe/watcher.rs`，经 `history/session_hidden.rs`）· `history-list` 每一行
//! （`history/history_list.rs`）· `sessions-needs` 每一行（`faces/history_face.rs`）。这台读一次的那几样事实在 [`Here`]；
//! 那一台的清单带它自己的（`history-list` 远端那一支）。单个那一格是注解（住 history），调用方从自己手里那一份取了交进来。
//!
//! 五档，先有先算：
//! 1. 单个那一格（注解 `hide`：`on` ⇒ 隐藏、`off` ⇒ 不隐藏）；
//! 2. 起会话时那一格（启动台账里记的 `--tab`，别名里写的也落在这里）；
//! 3. 分身会话 ⇒ 这台偏好「分身会话」；
//! 4. 有主会话（会话血缘）⇒ 这台偏好「子会话」；
//! 5. 都没有 ⇒ 不隐藏。

use crate::control::launch_account::LaunchTab;
use cmd_wire::session::{Hidden, HiddenBy, Hide, Tab};
use copy_core::copy_text;

/// 这台偏好里那两格（线上形与缺省住 `cmd-wire`）。
pub(crate) use cmd_wire::session::Rules;

/// 判一条会话要的事实。
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Facts<'a> {
    /// 单个那一格（注解）。
    pub(crate) pin: Option<Hide>,
    /// 起会话时那一格（启动台账）。
    pub(crate) launch: Option<&'a LaunchTab>,
    /// 是不是分身会话。
    pub(crate) bg: bool,
    /// 主会话叫什么（有主会话才有；叫法由调用方定，[`title_here`] 那一种）。
    pub(crate) parent: Option<&'a str>,
    /// 这台偏好。
    pub(crate) rules: Rules,
}

/// **唯一的判定**（五档见头注）。不隐藏 ⇒ `None`。
///
/// 〔继承表〕标签页那一格：命令行或别名写了 `--tab` 照它；没写 ⇒ 不随调用方，照这台偏好（分身 · 有主会话各一格）。
pub(crate) fn judge(f: &Facts) -> Option<Hidden> {
    let hide = |by: HiddenBy, said: String| Some(Hidden { by, said });
    match f.pin {
        Some(Hide::On) => return hide(HiddenBy::Own, copy_text("beSession.hidden.self", &[])),
        Some(Hide::Off) => return None,
        None => {}
    }
    if let Some(l) = f.launch {
        return match (l.tab, l.alias.as_deref()) {
            (Tab::Show, _) => None,
            (Tab::Hide, Some(name)) => hide(
                HiddenBy::Alias,
                copy_text("beSession.hidden.alias", &[("name", name)]),
            ),
            (Tab::Hide, None) => hide(HiddenBy::Launch, copy_text("beSession.hidden.launch", &[])),
        };
    }
    if f.bg {
        return match f.rules.bg_tab {
            Tab::Show => None,
            Tab::Hide => hide(HiddenBy::Bg, copy_text("beSession.hidden.bg", &[])),
        };
    }
    match (f.parent, f.rules.child_tab) {
        (Some(parent), Tab::Hide) => hide(
            HiddenBy::Child,
            copy_text("beSession.hidden.child", &[("parent", parent)]),
        ),
        _ => None,
    }
}

/// 这台读一次的那几样（启动台账 · 会话血缘 · 偏好；单个那一格是注解，调用方手里那一份）：一问里要判好几条时读一次、逐条判。读不到的那一样当没有。
#[derive(Debug, Clone, Default)]
pub(crate) struct Here {
    pub(crate) ledger: crate::control::launch_account::Book,
    pub(crate) lineage: crate::lineage::Book,
    pub(crate) rules: Rules,
}

impl Here {
    /// 现读这台那几份（家说不出 ⇒ 空的）。
    pub(crate) fn read_now() -> Here {
        Here::read_in(
            crate::platform::paths::data_home().as_deref(),
            crate::control::machine_prefs::read_now().prefs().rules(),
        )
    }

    /// [`Here::read_now`] 的可喂夹具那一半：数据目录 · 偏好由调用方给。
    pub(crate) fn read_in(data: Option<&std::path::Path>, rules: Rules) -> Here {
        let ledger = data
            .and_then(|d| crate::control::launch_account::read_book(d).ok())
            .unwrap_or_default();
        let lineage = data
            .map(
                |d| match crate::lineage::read_at(&d.join(crate::lineage::FILE_NAME)) {
                    crate::common::own_state::Read::Present(b) => b,
                    _ => crate::lineage::Book::default(),
                },
            )
            .unwrap_or_default();
        Here {
            ledger,
            lineage,
            rules,
        }
    }

    /// 这条会话的主会话（会话血缘）。
    pub(crate) fn parent_of(&self, sid: &str) -> Option<&str> {
        self.lineage.parent_of(sid)
    }

    /// 判这条会话（`pin` ＝ 注解里单个那一格；`bg` ＝ 是不是分身会话；`title_of` ＝ 主会话叫什么）。
    pub(crate) fn hidden(
        &self,
        pin: Option<Hide>,
        sid: &str,
        bg: bool,
        title_of: &dyn Fn(&str) -> String,
    ) -> Option<Hidden> {
        let parent = self.parent_of(sid).map(title_of);
        judge(&Facts {
            pin,
            launch: self.ledger.tabs.get(sid),
            bg,
            parent: parent.as_deref(),
            rules: self.rules,
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/session_hidden_tests.rs"]
mod tests;
