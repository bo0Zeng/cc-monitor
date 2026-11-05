//! **各台的「需手动」合成一份**（`sessions-needs-merge` 的回包 · 手机核心逐台问完合的那一份，同一处）：
//! 每台交它答回来的 `sessions-needs` 整份原样，这里照各项的 `rank` 排成一列、数总数、带上会外读不出的那几台与没问到的那几台，
//! 写好头那一句 · 读不出那一行（出口照摆，不带参数取字）。
//! 先后只看 `rank`（那台写好的：会话照 `needs.rank`，计划 · 会外各占会话之后的一档）；纯计算。线上那几形住 `cmd_wire::needs`。

use cmd_wire::cells::Words;
use cmd_wire::needs::{NeedsAt, NeedsDown, NeedsItem, NeedsList, NeedsMerged, NeedsUnreadable};
use cmd_wire::Machine;
use copy_core::copy_text;

/// 一项的先后（三臂各带的那一格；一台里排 · 几台合都只按它）。
pub fn rank_of(item: &NeedsItem) -> u64 {
    match item {
        NeedsItem::Session { rank, .. }
        | NeedsItem::Plan { rank, .. }
        | NeedsItem::Manual { rank, .. } => *rank,
    }
}

/// **各台的「需手动」合成一份**（`answered` ＝ 每台 `(名字, 它答回来的 sessions-needs)`，`down` ＝ 没问到的那几台原样带上，句子已叫出那台）：
/// 照各项的 `rank` 排；并列 ⇒ 本机在前、再照给的台序、台内原来的先后。
/// 各台的 `rank` 是同一回问回来的、已等多久各在那台的钟上量，跨台照样比。
/// `unsure` ＝ 调用方知道有哪台的数说不准（还在读 · 答失败；断着不算）；会外读不出的那几台这里自己算进去。
/// 头那一句 · 读不出那一行在这里写好（`name` ＝ 给人看的那台的称呼），出口照摆。
pub fn merge_needs(
    mut answered: Vec<(Machine, NeedsList)>,
    down: Vec<NeedsDown>,
    unsure: bool,
    name: impl Fn(&Machine) -> String,
) -> NeedsMerged {
    answered.sort_by_key(|(origin, _)| !origin.is_local());
    let unreadable: Vec<NeedsUnreadable> = answered
        .iter()
        .filter_map(|(origin, list)| {
            list.manual_unreadable.clone().map(|said| NeedsUnreadable {
                origin: origin.clone(),
                said,
                line: Words(copy_text(
                    "needs.panel.unreadable",
                    &[("machine", &name(origin))],
                )),
            })
        })
        .collect();
    let sure = !unsure && unreadable.is_empty();
    let mut items: Vec<NeedsAt> = answered
        .into_iter()
        .flat_map(|(origin, list)| {
            list.items.into_iter().map(move |item| NeedsAt {
                origin: origin.clone(),
                item,
            })
        })
        .collect();
    items.sort_by_key(|x| rank_of(&x.item));
    let total = items.len() as u32;
    NeedsMerged {
        head: Words(if sure {
            copy_text("beGroups.count.needs", &[("n", &total.to_string())])
        } else {
            copy_text("beSession.needs.unknown", &[])
        }),
        sure,
        total,
        items,
        unreadable,
        down,
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/face-core/needs_tests.rs"]
mod tests;
