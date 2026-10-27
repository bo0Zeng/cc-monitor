//! 每个建了的权力范围的接手会话此刻在哪几组（成品 `blocks[].groups`）。pb 不知道有组：组是 cc-monitor 后端的数据（[`crate::history::groups`]），
//! 这里只照接手人对到的那条会话（子 agent ⇒ 父会话）去组模块并好的那一份里找它，不另判组员关系。
//!
//! 组会变而计划没变 ⇒ 不进本子、不进 `rev`：帧面每次答 `plan-read` 之前现标一次。

use super::wire::{PlanBlock, PlanBlockGroups, PlanGroup, PlanRead};
use crate::history::groups::GroupsRead;

/// 问组模块的口：生产是这台那一半并好的那一份（[`here_ties`]），判据喂一份合成的。
pub(crate) type GroupsPort<'a> = &'a dyn Fn() -> Result<GroupsRead, String>;

/// 给成品里每个建了的权力范围标上 `groups`。有接手会话的块才问组模块，整份只问一次。
pub(crate) fn annotate(doc: &mut PlanRead, port: GroupsPort) {
    let asks = doc
        .slices
        .iter()
        .flat_map(|s| &s.blocks)
        .any(|b| sid_of(b).is_some());
    let got = asks.then(port);
    for b in doc.slices.iter_mut().flat_map(|s| &mut s.blocks) {
        b.groups = match (sid_of(b), &got) {
            (Some(sid), Some(Ok(read))) => of_groups(of_sid(read, sid)),
            (Some(_), Some(Err(why))) => PlanBlockGroups {
                unread: Some(copy_core::copy_text(
                    "bePlan.groups.unread",
                    &[("why", why)],
                )),
                ..nobody()
            },
            _ => nobody(),
        };
    }
}

/// 没有接手会话那一格（成品刚造出来、还没标的时候也是它）。
pub(crate) fn nobody() -> PlanBlockGroups {
    of_groups(Vec::new())
}

/// 读到的那几组 ⇒ 那一格：组名照排，一组都没有 ⇒「—」。
fn of_groups(groups: Vec<PlanGroup>) -> PlanBlockGroups {
    let text = if groups.is_empty() {
        copy_core::copy_text("plan.page.none", &[])
    } else {
        let titles: Vec<&str> = groups.iter().map(|g| g.title.as_str()).collect();
        titles.join(&copy_core::copy_text("kit.text.sep", &[]))
    };
    PlanBlockGroups {
        groups,
        text,
        unread: None,
    }
}

/// 生产的口：这台那份注解文件里的组与组员关系（不读清单、不画样子）照组模块的 [`merge`](crate::history::groups::merge) 并一次。
/// 推不出家 · 读不懂 ⇒ 那一句。
pub(crate) fn here_ties() -> Result<GroupsRead, String> {
    use crate::history::groups::{here_of, merge, unlisted, Half};
    let (table, book) = crate::history::history_annotations::load_all().unwrap_or_else(|| {
        Err(copy_core::copy_text(
            "beHistoryAnnotations.needPath.unknown",
            &[("env", creds_core::store::DATA_DIR_ENV)],
        ))
    })?;
    let half = here_of(&table, &book, |_| {
        unlisted(None, cmd_wire::listing::RowLife::Ended)
    });
    Ok(merge(
        &[(cmd_wire::Machine(cmd_wire::LOCAL.into()), Half::Now(half))],
        Vec::new(),
    ))
}

/// 接手人对到的会话（子 agent ⇒ 父会话，`PlanWho.sid` 已是它）。
fn sid_of(b: &PlanBlock) -> Option<&str> {
    b.owner.as_ref().and_then(|w| w.sid.as_deref())
}

/// 那条会话在哪几组（照组模块排的先后）。
fn of_sid(read: &GroupsRead, sid: &str) -> Vec<PlanGroup> {
    read.groups
        .iter()
        .filter(|g| g.members.iter().any(|m| m.sid == sid))
        .map(|g| PlanGroup {
            id: g.id.clone(),
            title: g.title.0.clone(),
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/groups_tests.rs"]
mod tests;
