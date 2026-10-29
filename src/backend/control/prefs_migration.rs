//! **`prefs.json` 第 1 版 ⇒ 第 2 版**（[`reshape_prefs_in`]）：恢复那一格从开关 `resumeInTmux` 换成三选 `resumeWhere`（开 ⇒ tmux，关 ⇒ 托管）；
//! 别的几格原样。只由本机常驻后端在起来时换一次（[`migrate_here`]）；读不懂的一个字节不动。读口只认第 2 版（旧键不留兼容）。

use crate::common::machine_prefs::{self, write as write_prefs};
use crate::platform::Own;
use serde_json::Value;
use std::sync::OnceLock;

/// 旧开关 `resumeInTmux` 读成三选：开 ⇒ tmux；关 / 没有 ⇒ 托管（关着时今天就落托管）。
fn resume_where_of(old: Option<&Value>) -> cmd_wire::session::ResumeWhere {
    use cmd_wire::session::ResumeWhere;
    if old.and_then(Value::as_bool) == Some(true) {
        ResumeWhere::Tmux
    } else {
        ResumeWhere::Managed
    }
}

/// 换第 1 版 `prefs.json` 那一次的结局（判据按它说；生产只记一行日志）。
#[derive(Debug, PartialEq)]
pub(crate) enum Reshaped {
    /// 不在，或已不是第 1 版：什么都不做。
    Nothing,
    /// 第 1 版读不懂（那一句）：一个字节不写。
    Unreadable(String),
    /// 换成了第 2 版。
    Done,
    /// 写没成（那一句）：下次起来再试。
    WriteFailed(String),
}

/// 盘上第 1 版的 `prefs.json`（恢复那一格是开关 `resumeInTmux`）⇒ 第 2 版（三选 `resumeWhere`，[`resume_where_of`]），别的几格原样。
/// 换完照第 2 版的值域核一遍再写；核不过 ⇒ 一个字节不写（读不懂的不当空的换）。
pub(crate) fn reshape_prefs_in(own: &Own) -> Reshaped {
    use crate::common::own_state::Read;
    let Ok(at) = own.at_home(machine_prefs::DOC.rel) else {
        return Reshaped::Nothing;
    };
    // 同一份声明按原样读（换形要看旧键；读不懂 ⇒ 不动）。
    let mut v = match machine_prefs::DOC.read_as::<Value>(at.as_path()) {
        Read::Absent => return Reshaped::Nothing,
        Read::Unreadable(why) => return Reshaped::Unreadable(why.said),
        Read::Present(v) => v,
    };
    let Some(o) = v.as_object_mut() else {
        return Reshaped::Unreadable(format!("{} is not an object", machine_prefs::FILE_NAME));
    };
    if o.get("version").and_then(Value::as_u64) != Some(1) {
        return Reshaped::Nothing;
    }
    let old = o.remove("resumeInTmux");
    let w = resume_where_of(old.as_ref());
    o.insert("resumeWhere".into(), serde_json::json!(w));
    o.insert("version".into(), serde_json::json!(machine_prefs::VERSION));
    let p = match serde_json::from_value::<machine_prefs::OnDisk>(v) {
        Ok(d) => d.prefs,
        Err(e) => return Reshaped::Unreadable(e.to_string()),
    };
    if let Err(why) = machine_prefs::check(&p) {
        return Reshaped::Unreadable(why);
    }
    match write_prefs(own, &p) {
        Ok(()) => Reshaped::Done,
        Err(e) => Reshaped::WriteFailed(e.said),
    }
}

/// 换过一次了（[`migrate_here`] 填）：一个进程只换一次。
static RAN: OnceLock<()> = OnceLock::new();

/// 本机常驻后端起来时调一次（只在拿到常驻门牌的那个进程里）。
pub fn migrate_here(own: &Own) {
    if RAN.set(()).is_err() {
        tracing::warn!("换形那一段被叫了第二次（只该在常驻那一位起来时叫一次）：这一次不换");
        return;
    }
    run_in(own);
}

/// 起来那一下做的事（句柄是参数）。
pub(crate) fn run_in(own: &Own) {
    match reshape_prefs_in(own) {
        Reshaped::Nothing => {}
        Reshaped::Done => tracing::info!("prefs.json 换成了第 2 版（恢复时放在哪改三选）"),
        Reshaped::Unreadable(why) => tracing::warn!("prefs.json 第 1 版读不懂，没换：{why}"),
        Reshaped::WriteFailed(why) => tracing::warn!("prefs.json 没换成，下次起来再试：{why}"),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/prefs_migration_tests.rs"]
mod tests;
