//! **各台在等人的合成一份**（`sessions-needs-merge` 的回包 · 手机核心逐台问完合的那一份，同一处）：
//! 每台交它答回来的 `sessions-needs` 整份原样，这里分成待答请求 · 需手动两列（[`column_of`] 一处判）、各照 `rank` 排、各数各的，
//! 带上会外读不出的那几台与没问到的那几台，写好两列头那一句 · 窗口标题 · 读不出那一行（出口照摆，不带参数取字）。
//! 先后只看 `rank`（那台写好的：会话照 `needs.rank`，计划 · 会外各占会话之后的一档）；纯计算。线上那几形住 `cmd_wire::needs`。

use cmd_wire::cells::Words;
use cmd_wire::needs::{
    NeedsAt, NeedsColumn, NeedsDown, NeedsEnded, NeedsItem, NeedsList, NeedsMerged, NeedsUnreadable,
};
use cmd_wire::phone::CallEnd;
use cmd_wire::Machine;
use copy_core::copy_text;

/// 那台答了一个错、句里已叫出那台的那几个码（照原样，不再套「<那台> · …」）：两端对不上 · 不认这条命令 ·
/// 那台常驻不在（那一句说那台的名字）。
const NAMED: [&str; 3] = [
    cmd_wire::SHAPE_DIFFERS,
    cmd_wire::UNKNOWN_COMMAND,
    cmd_wire::phone::CALL_ABSENT,
];

/// **一问的结局 ⇒ 答了那一份，或没答上那一句**（`cmd` 是问的那条命令，进读不懂那一份的详情；`name` ＝ 给人看的那台的称呼）。
/// 需手动 · 搜索没答上那几台那一句只在这里写：断着 ⇒ 离线那一句；到期 · 撤了 ⇒ 核心写好的那一句原样（已叫出那台）；
/// 那台答了一个错 ⇒「<那台> · <那一句>」（[`NAMED`] 那几个码照原样）；回包照类型解不开 ⇒ 读不懂那一句。
/// `Err` ＝ `(那一句, 复制详情)`。
pub fn missed<T: serde::de::DeserializeOwned>(
    cmd: &str,
    name: &str,
    end: CallEnd,
) -> Result<T, (String, String)> {
    match end {
        CallEnd::Ok { data, .. } => {
            let raw = data.to_string();
            serde_json::from_value(data).map_err(|e| {
                (
                    copy_core::reply_unreadable(name),
                    format!("command: {cmd}\nreply: {raw}\n{e}"),
                )
            })
        }
        CallEnd::Failed {
            code, said, detail, ..
        } => Err(if NAMED.contains(&code.as_str()) {
            (said, detail)
        } else {
            (
                copy_text(
                    "kit.text.machineSaid",
                    &[("machine", name), ("said", &said)],
                ),
                detail,
            )
        }),
        CallEnd::Timeout { said, detail, .. } | CallEnd::Cancelled { said, detail, .. } => {
            Err((said, detail))
        }
        CallEnd::Down { .. } => Err((copy_core::offline(name), String::new())),
        CallEnd::Unreadable { detail, .. } => Err((copy_core::reply_unreadable(name), detail)),
    }
}

/// **一台 `sessions-needs` 那一问的结局 ⇒ 答了那一份，或没问到那一格**（句里叫出那台，见 [`missed`]）。
/// 本机后端（`sessions-needs-merge`）与手机核心逐台问完都调这一个；没问到的那一格交进 [`merge_needs`] 就让数说不准（断着也算）。
pub fn needs_reply(origin: Machine, name: &str, end: CallEnd) -> Result<NeedsList, NeedsDown> {
    missed("sessions-needs", name, end).map_err(|(said, detail)| NeedsDown {
        origin,
        said,
        detail,
    })
}

/// 一项的先后（三臂各带的那一格；一台里排 · 几台合都只按它）。
pub fn rank_of(item: &NeedsItem) -> u64 {
    match item {
        NeedsItem::Session { rank, .. }
        | NeedsItem::Plan { rank, .. }
        | NeedsItem::Manual { rank, .. } => *rank,
    }
}

/// **各台在等人的合成一份、分两列**（`answered` ＝ 每台 `(名字, 它答回来的 sessions-needs)`，`down` ＝ 没问到的那几台原样带上，句子已叫出那台）：
/// 每项照 [`column_of`] 落待答请求或需手动；两列各照各项的 `rank` 排，并列 ⇒ 本机在前、再照给的台序、台内原来的先后。
/// 各台的 `rank` 是同一回问回来的、已等多久各在那台的钟上量，跨台照样比。
/// 数说得准只在这里判：有哪台还在读（`reading`）· 有哪台没问到（`down` 非空，断着也算）⇒ 两列都说不准；有哪台会外读不出 ⇒ 只有需手动那一列说不准。
/// 两列头那一句 · 窗口标题 · 读不出那一行在这里写好（`name` ＝ 给人看的那台的称呼），出口照摆。
pub fn merge_needs(
    mut answered: Vec<(Machine, NeedsList)>,
    down: Vec<NeedsDown>,
    reading: bool,
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
    let ended: Vec<NeedsEnded> = answered
        .iter()
        .flat_map(|(origin, list)| {
            list.ended.iter().map(move |e| NeedsEnded {
                origin: origin.clone(),
                sid: e.sid.clone(),
                end: e.end.clone(),
            })
        })
        .collect();
    let asks_sure = !reading && down.is_empty();
    let manual_sure = asks_sure && unreadable.is_empty();
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
    let (asks, manual): (Vec<NeedsAt>, Vec<NeedsAt>) = items
        .into_iter()
        .partition(|x| column_of(&x.item) == Column::Answerable);
    let asks = column(asks, asks_sure, asks_count, || {
        copy_text("needs.asks.title", &[])
    });
    let manual = column(manual, manual_sure, manual_count, || {
        copy_text("beSession.needs.unknown", &[])
    });
    // 窗口标题：非零那几列的「待答请求 n」「需手动 n」（待答请求在前：此刻就能动手的先说）；只看数、不看准不准。
    let counts: Vec<String> = [
        (&asks, asks_count as fn(u32) -> String),
        (&manual, manual_count),
    ]
    .into_iter()
    .filter(|(c, _)| c.total > 0)
    .map(|(c, count)| count(c.total))
    .collect();
    NeedsMerged {
        window_title: Words(if counts.is_empty() {
            copy_text("needs.windowTitle.base", &[])
        } else {
            copy_text(
                "needs.windowTitle.count",
                &[("counts", &counts.join(&copy_text("kit.text.sep", &[])))],
            )
        }),
        asks,
        manual,
        unreadable,
        down,
        ended,
    }
}

/// 合好的一项落哪一列。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    /// 待答请求：界面答得了。
    Answerable,
    /// 需手动：别的一律。
    Manual,
}

/// **一项落哪一列（唯一一处）**：会话且那台交了答得了的请求（`needs.ask` 在；答不答得了由那台后端 `Asked::manual` 判、落成交不交 `ask`）
/// ⇒ 待答请求；答不了的会话 · 计划要看的格 · 会话外的 ⇒ 需手动。
pub fn column_of(item: &NeedsItem) -> Column {
    match item {
        NeedsItem::Session { session, .. } if session.needs.ask().is_some() => Column::Answerable,
        _ => Column::Manual,
    }
}

/// 「待答请求 n」（待答请求那一列准时的头 · 窗口标题那一段）。
fn asks_count(n: u32) -> String {
    copy_text("beGroups.count.asks", &[("n", &n.to_string())])
}

/// 「需手动 n」（需手动那一列准时的头 · 窗口标题那一段）。
fn manual_count(n: u32) -> String {
    copy_text("beGroups.count.needs", &[("n", &n.to_string())])
}

/// 排好的一列 ⇒ 那几项 · 几项 · 头那一句（准 ⇒ `count` 带数；不准 ⇒ `unsure` 不带数）· 准不准。
fn column(
    items: Vec<NeedsAt>,
    sure: bool,
    count: fn(u32) -> String,
    unsure: impl Fn() -> String,
) -> NeedsColumn {
    let total = u32::try_from(items.len()).unwrap_or(u32::MAX);
    NeedsColumn {
        head: Words(if sure { count(total) } else { unsure() }),
        items,
        total,
        sure,
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/face-core/needs_tests.rs"]
mod tests;
