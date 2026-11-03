//! **组的待补**：改名 · 解散发给各片时没送到的那几台，这一端欠它们的那几句（ARCHITECTURE「存储层」④ 这一端自己的）。
//!
//! 由收到那一问的那台（本机后端，替这一端）记，住 `~/.cc-monitor/groups-owed.json`（声明 `relay_route_core::GROUPS_OWED_FILE`，状态）。
//! 那台再答 `groups-here` 时照它那一片判一次（[`due`]）：该补的补发（`history/groups.rs` 的 `groups-read`），补成了消掉；到期的消掉。
//!
//! - 记（[`note`]）：一台没送到 ⇒ 那台那一组一条，盖掉同一台同一组先前那条；送到了的那几台同一组先前欠的消掉；
//!   解散 ⇒ 那一组欠各台的改名都消掉（组都没了）。
//! - 到期：那台从机器表删了（[`gone`]）· 那台那一片已没有那一组 · 改名欠的那一刻不比那台那一片的名字新（那台后来又改过）⇒ 丢。

use cmd_wire::groups::GroupsHere;
use cmd_wire::Machine;
use serde::{Deserialize, Serialize};

use crate::common::said::Said;
use crate::platform::fs::Own;

/// 落盘那一份。
const DOC: crate::common::own_state::Doc<Owed> =
    crate::common::own_state::Doc::of(&relay_route_core::GROUPS_OWED_FILE);

/// 欠的是哪一句。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Owe {
    /// 改名：改成什么 · 那一刻（毫秒，与那台那一片的 `named` 比）。
    Rename { name: String, named: i64 },
    /// 解散。
    Dissolve,
}

/// 欠一台的一句：哪一组 · 哪台（这一端机器表里的叫法）· 哪一句。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Debt {
    pub id: String,
    pub machine: Machine,
    pub what: Owe,
}

/// 整份：欠的那几句（照记下的先后）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owed {
    pub owed: Vec<Debt>,
}

/// 照那台那一片判一条：补发 · 丢。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    Send,
    Drop,
}

/// 发完一句（`sent` 送到了的那几台 · `missed` 没送到的那几台）记进待补。
pub fn note(owed: &mut Owed, id: &str, sent: &[Machine], missed: &[Machine], what: Owe) {
    let dissolve = what == Owe::Dissolve;
    owed.owed.retain(|d| {
        d.id != id
            || !(sent.contains(&d.machine)
                || missed.contains(&d.machine)
                || (dissolve && matches!(d.what, Owe::Rename { .. })))
    });
    owed.owed.extend(missed.iter().map(|m| Debt {
        id: id.to_string(),
        machine: m.clone(),
        what: what.clone(),
    }));
}

/// 那台这一问答的那一片 ⇒ 这一条补不补。
pub fn due(debt: &Debt, here: &GroupsHere) -> Due {
    let record = here.groups.iter().find(|g| g.id == debt.id);
    let pointed = here.members.iter().any(|m| m.groups.contains(&debt.id));
    let send = match &debt.what {
        Owe::Rename { named, .. } => record.is_some_and(|g| g.named < *named),
        Owe::Dissolve => record.is_some() || pointed,
    };
    if send {
        Due::Send
    } else {
        Due::Drop
    }
}

/// 欠的那台已不在机器表（`listed` 答机器表里有没有这个名字）⇒ 那几条到期。
pub fn gone(owed: &Owed, listed: impl Fn(&str) -> bool) -> Vec<Debt> {
    owed.owed
        .iter()
        .filter(|d| !listed(&d.machine.to_string()))
        .cloned()
        .collect()
}

/// 补成了的一条落到那一片上（这一问合的照补好的样子）。
pub fn apply(debt: &Debt, here: &mut GroupsHere) {
    match &debt.what {
        Owe::Rename { name, named } => {
            for g in here.groups.iter_mut().filter(|g| g.id == debt.id) {
                g.name = name.clone();
                g.named = g.named.max(*named);
            }
        }
        Owe::Dissolve => {
            here.groups.retain(|g| g.id != debt.id);
            for m in &mut here.members {
                m.groups.retain(|g| *g != debt.id);
            }
        }
    }
}

/// 读这一端记着的那一份：没有 · 推不出家 ⇒ 空；读不懂 ⇒ `None`（不补、不覆盖）。
pub fn load_at(own: &Own) -> Option<Owed> {
    use crate::common::own_state::Read;
    let Ok(file) = own.at_home(DOC.rel) else {
        return Some(Owed::default());
    };
    match DOC.read(file.as_path()) {
        Read::Absent => Some(Owed::default()),
        Read::Present(o) => Some(o),
        Read::Unreadable(why) => {
            tracing::warn!(target: "groups", reason = %why.said, "groups-owed unreadable; not resending");
            None
        }
    }
}

/// 读—改—写一次（跨进程锁里；读不懂的不覆盖）。
pub fn edit_at(own: &Own, f: impl FnOnce(&mut Owed)) -> Result<(), Said> {
    use crate::common::own_state::Read;
    let dir = own.home_dir()?;
    let file = own.at_home(DOC.rel)?;
    own.ensure_dir(&dir).map_err(|e| {
        Said::with_raw(
            copy_core::copy_text(
                "reason.fs.writeFailed",
                &[
                    ("path", &dir.as_path().display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    let held = own.lock(&dir)?;
    let mut o = match DOC.read(file.as_path()) {
        Read::Absent => Owed::default(),
        Read::Present(o) => o,
        Read::Unreadable(why) => return Err(why),
    };
    f(&mut o);
    DOC.write(own, &dir, &held, &o)
}

#[cfg(test)]
#[path = "../../../tests/backend/history/groups_owed_tests.rs"]
mod tests;
