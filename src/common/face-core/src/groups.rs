//! **几台的组合成一份**（[`merge`]）：两侧调同一份 —— 本机后端问完各台 `groups-here`（`history/groups.rs` 的 `groups-read`）·
//! 手机嵌的 Rust 核心逐台直连问完（`mobcore::groups_merge_json`）。线上形住 `cmd_wire::groups`。
//!
//! 合法只在这里：名字取各片里同一组 `named` 最新的那份（同一刻 ⇒ 这一问答了的那片优先，再照问的先后），`made` 取最早；
//! 哪一片都没有那一组的记录、只有组员指着它 ⇒ 照出、名字缺；组员照问的台序接；组头计数与字在这里数一次。
//! 纯计算：不读盘、不问网、不看钟。

use std::collections::BTreeMap;

use cmd_wire::cells::Words;
use cmd_wire::frame::SessionDot;
use cmd_wire::groups::{
    GroupCount, GroupKnown, GroupMember, GroupRead, GroupsHere, GroupsMachine, GroupsRead,
    HereGroup,
};
use cmd_wire::Machine;
use copy_core::copy_text;

/// 一台这一问拿到的那一片：这一问答的 · 上一次答的 · 没有。
pub enum Half {
    Now(GroupsHere),
    /// 上一次答的那一片：`at` ＝ 那一份几时读到的（写好的字）；`offline` ＝ 那台的离线那一句（组员画成看不见那一张时用；连接层写它）。
    Before {
        here: GroupsHere,
        at: Words,
        offline: Words,
    },
    None,
}

/// 合：各台那一片（照问的先后）＋ 这一问各台的结局（原样带上）⇒ 并好的一份。
pub fn merge(halves: &[(Machine, Half)], machines: Vec<GroupsMachine>) -> GroupsRead {
    // 组 id ⇒ (取到名字的那一份, 名字从哪来, 各片里最早的建组时刻)
    let mut records: BTreeMap<&str, (&HereGroup, GroupKnown, i64)> = BTreeMap::new();
    for (_, half) in halves {
        let (here, known) = match half {
            Half::Now(h) => (h, GroupKnown::Now),
            Half::Before { here, at, .. } => (here, GroupKnown::Before { at: at.clone() }),
            Half::None => continue,
        };
        for g in &here.groups {
            let fresh = known == GroupKnown::Now;
            match records.get_mut(g.id.as_str()) {
                None => {
                    records.insert(g.id.as_str(), (g, known.clone(), g.made));
                }
                Some((best, k, made)) => {
                    *made = (*made).min(g.made);
                    let best_fresh = *k == GroupKnown::Now;
                    if (g.named, fresh) > (best.named, best_fresh) {
                        *best = g;
                        *k = known.clone();
                    }
                }
            }
        }
    }
    let mut members: BTreeMap<&str, Vec<GroupMember>> = BTreeMap::new();
    let mut pinned = Vec::new();
    for (origin, half) in halves {
        let (here, offline) = match half {
            Half::Now(h) => (h, None),
            Half::Before { here, offline, .. } => (here, Some(offline)),
            Half::None => continue,
        };
        for m in &here.members {
            // 这一问答了 ⇒ 那台画的样子；上次所知 ⇒ 看不见的那一张（不冒充在跑）。
            let face = match offline {
                None => m.face.clone(),
                Some(line) => crate::book::unseen(line.clone()),
            };
            let row = GroupMember {
                origin: origin.clone(),
                sid: m.sid.clone(),
                pinned: m.pinned,
                face,
                facts: m.facts.clone(),
            };
            if m.pinned {
                pinned.push(row.clone());
            }
            for g in &m.groups {
                members.entry(g.as_str()).or_default().push(row.clone());
            }
        }
    }
    let mut ids: Vec<&str> = records.keys().copied().collect();
    ids.extend(
        members
            .keys()
            .copied()
            .filter(|id| !records.contains_key(id)),
    );
    let mut groups: Vec<GroupRead> = ids
        .into_iter()
        .map(|id| {
            let rows = members.remove(id).unwrap_or_default();
            let counts = counts_of(&rows);
            match records.get(id) {
                Some((g, known, made)) => GroupRead {
                    id: id.to_string(),
                    name: Some(g.name.clone()),
                    title: Words(g.name.clone()),
                    made: Some(*made),
                    named: Some(g.named),
                    known: known.clone(),
                    members: rows,
                    counts,
                },
                None => GroupRead {
                    id: id.to_string(),
                    name: None,
                    title: Words(copy_text("beGroups.name.unknown", &[])),
                    made: None,
                    named: None,
                    known: GroupKnown::Unknown,
                    members: rows,
                    counts,
                },
            }
        })
        .collect();
    groups.sort_by(|a, b| {
        (a.made.unwrap_or(i64::MAX), &a.id).cmp(&(b.made.unwrap_or(i64::MAX), &b.id))
    });
    GroupsRead {
        groups,
        pinned,
        machines,
    }
}

/// 组头那几颗点：需手动 · 在跑 · 后台各几条，名字照第一条那张脸上的字。
fn counts_of(rows: &[GroupMember]) -> Vec<GroupCount> {
    [
        SessionDot::NeedsYou,
        SessionDot::Running,
        SessionDot::Background,
    ]
    .into_iter()
    .filter_map(|dot| {
        let mut hit = rows.iter().filter(|r| r.face.dot == dot);
        let first = hit.next()?;
        let n = u32::try_from(1 + hit.count()).unwrap_or(u32::MAX);
        let n_text = n.to_string();
        let n_text = n_text.as_str();
        let text = match dot {
            SessionDot::NeedsYou => copy_text("beGroups.count.needs", &[("n", n_text)]),
            SessionDot::Running => copy_text("beGroups.count.running", &[("n", n_text)]),
            _ => copy_text("beGroups.count.background", &[("n", n_text)]),
        };
        Some(GroupCount {
            dot,
            n,
            word: first.face.name.clone(),
            text: Words(text),
        })
    })
    .collect()
}

#[cfg(test)]
#[path = "../../../../tests/common/face-core/groups_tests.rs"]
mod tests;
