//! 计划这一侧「要你看」的判定：纯函数，跑在 [`super::product::make`] 之后（吃成品，不吃 pb 的原样），不读盘、不起进程。
//!
//! 只收四种（人只是 project 的上一级：划出的权力范围交回由上一级 agent 收，都不进这里）：
//!
//! | 种 | 什么时候出 | 键（带条目版本：版本换了，认可就失效、再出） |
//! |---|---|---|
//! | `top` | 顶层算活的节点全签收了（顶层进度 done ＝ of ＞ 0） | `top:<全签收那一次的 rev>` |
//! | `red` | `check.red`（校验不通过）每条一项 | `red:<规则>@<权力范围>` |
//! | `ended` | 某个权力范围的负责人对到的会话已结束、它的根节点没签收（子 agent 看父会话：父会话也停了才算） | `ended:<权力范围>@<负责人 id>` |
//! | `ask` | 某个权力范围的负责人对到的会话此刻在等你 | `ask:<权力范围>@<负责人 id>`（不收认可：答了自己消失；计数由会话那一侧算） |
//!
//! 另有「另开会话接手」那一格（[`takeover`]，与 `ended` 读同一处 [`owner_now`]）· 「退回」落地的判法（[`landing`]）·
//! 送进会话的那一行（[`line`]）· 退回的对象（[`target`]：一格，或 `project`）。

use super::wire::{
    PlanBlock, PlanCell, PlanNeed, PlanRead, PlanReturned, PlanReturnedChild, PlanSlice,
    PlanStatusCode, PlanTakeover, PlanWho,
};
use copy_core::copy_text;

/// 一个 id 对到的会话（`PlanWho`）此刻活不活：活着 ⇒ `Some(在干什么)`；不活 · 认不出 ⇒ `None`。只读那一格的整份脸。
pub(crate) fn live_of(who: &PlanWho) -> Option<Option<crate::agents::SessionActivity>> {
    match who.face.life {
        crate::wire::WireLife::Live { activity } => Some(activity),
        _ => None,
    }
}

/// 一个权力范围的负责人那条会话此刻怎样。`ended` 那一种与「另开会话接手」那一格（[`takeover`]）读的是这一处。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OwnerNow {
    /// 活着：在干什么（说不清 ⇒ `None`）。
    Live(Option<crate::agents::SessionActivity>),
    /// 对到这台的一个会话，它停了（子 agent 看父会话）。
    Stopped,
    /// 对不上这台的任何会话：说不清停没停。
    Unsure,
}

/// 负责人 ⇒ 它那条会话此刻怎样。
pub(crate) fn owner_now(o: &PlanWho) -> OwnerNow {
    if o.sid.is_none() {
        return OwnerNow::Unsure;
    }
    live_of(o).map_or(OwnerNow::Stopped, OwnerNow::Live)
}

/// 一个建了的权力范围给不给「另开会话接手」：划出那一项删了 ⇒ 不出；负责人活着 ⇒ 不出；负责人认不出 ⇒ 说不清、不给；
/// 没有负责人或它停了 ⇒ 出，pb 给了交接说明就是它，没给 ⇒ 写为什么。
pub(crate) fn takeover(b: &PlanBlock) -> PlanTakeover {
    if !b.listed {
        return PlanTakeover::Merging;
    }
    match b.owner.as_ref().map(owner_now) {
        Some(OwnerNow::Live(_)) => PlanTakeover::Running,
        Some(OwnerNow::Unsure) => PlanTakeover::Unsure,
        None | Some(OwnerNow::Stopped) if b.handoff.is_some() => PlanTakeover::Open,
        None | Some(OwnerNow::Stopped) => PlanTakeover::NoLine,
    }
}

/// 最上面那个权力范围的 id（pb 的规矩：叫 project）。
pub(crate) const TOP_BLOCK: &str = "project";

/// 不收认可的那一种（答了自己消失）。
const ASK: &str = "ask";

/// 一次「退回」记下的那一刻：送给谁 · 送没送到 · 那一格当时的子格集合与正文摘要（判落地用）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Returned {
    /// 送出的时刻（epoch ms）。
    pub(crate) at: u64,
    /// 送到的会话 sid。
    pub(crate) to: String,
    /// `delivered` · `unsure`（送达未知）。
    pub(crate) result: String,
    /// 当时那一格的子格。
    pub(crate) children: Vec<String>,
    /// 当时那一格正文的摘要（[`body_digest`]）。
    pub(crate) body: String,
}

fn block<'a>(sl: &'a PlanSlice, id: &str) -> Option<&'a PlanBlock> {
    sl.blocks.iter().find(|b| b.id == id)
}

fn cell<'a>(sl: &'a PlanSlice, id: &str) -> Option<&'a PlanCell> {
    sl.cells.iter().find(|c| c.id == id)
}

/// 顶层算活的节点全签收了（有收的值、顶层至少一个算活的）。
pub(crate) fn top_done(sl: &PlanSlice) -> bool {
    sl.progress
        .as_ref()
        .is_some_and(|p| p.of > 0 && p.done == p.of)
}

/// 一个权力范围收没收尾：project 看顶层全签收没有；别的看它的根节点是不是都签收了（找不到那一格 · 不算活 ⇒ 算没收尾）。
fn block_finished(sl: &PlanSlice, b: &PlanBlock) -> bool {
    if b.id == TOP_BLOCK {
        return top_done(sl);
    }
    b.cells
        .iter()
        .all(|id| cell(sl, id).and_then(|c| c.status_code) == Some(PlanStatusCode::Done))
}

/// 一块的根格（`cells` 第一项）。
fn root_of(b: &PlanBlock) -> Option<String> {
    b.cells.first().cloned()
}

fn need(kind: &str, key: String, block: Option<String>, cell: Option<String>) -> PlanNeed {
    PlanNeed {
        key,
        kind: kind.to_string(),
        block,
        cell,
        sid: None,
        red: None,
        acked: false,
    }
}

/// 一片（成品）的要你看。`entered` ＝ 顶层全签收那一次的 rev（不知道 ⇒ 不出 `top` 那一条）。
pub(crate) fn of_slice(sl: &PlanSlice, entered: Option<&str>) -> Vec<PlanNeed> {
    let mut out = Vec::new();
    if let (true, Some(rev)) = (top_done(sl), entered) {
        let root = block(sl, TOP_BLOCK).and_then(root_of);
        out.push(need(
            "top",
            format!("top:{rev}"),
            Some(TOP_BLOCK.to_string()),
            root,
        ));
    }
    for (i, r) in sl.check.red.iter().enumerate() {
        let rule = r.rule.as_deref().unwrap_or_default();
        let b = r.block.as_deref();
        let cell = b.and_then(|b| block(sl, b)).and_then(root_of);
        let mut n = need(
            "red",
            format!("red:{rule}@{}", b.unwrap_or_default()),
            r.block.clone(),
            cell,
        );
        n.red = Some(u32::try_from(i).unwrap_or(u32::MAX));
        out.push(n);
    }
    for b in &sl.blocks {
        let Some(o) = &b.owner else { continue };
        // 认不出 ⇒ 说不清停没停、问没问。
        let Some(sid) = &o.sid else { continue };
        let now = owner_now(o);
        if now == OwnerNow::Stopped {
            if !block_finished(sl, b) {
                let mut n = need(
                    "ended",
                    format!("ended:{}@{}", b.id, o.id),
                    Some(b.id.clone()),
                    root_of(b),
                );
                n.sid = Some(sid.clone());
                out.push(n);
            }
        } else if now == OwnerNow::Live(Some(crate::agents::SessionActivity::NeedsYou)) {
            let mut n = need(
                ASK,
                format!("ask:{}@{}", b.id, o.id),
                Some(b.id.clone()),
                b.at.clone().or_else(|| root_of(b)),
            );
            n.sid = Some(sid.clone());
            out.push(n);
        }
    }
    out
}

/// 照认可记录给每片的 `needs` 标 `acked`（`acked(片, 键)`）。`ask` 那一种不收认可。
pub(crate) fn mark_acked(doc: &mut PlanRead, acked: &dyn Fn(&str, &str) -> bool) {
    for sl in &mut doc.slices {
        for x in &mut sl.needs {
            x.acked = x.kind != ASK && acked(&sl.name, &x.key);
        }
    }
}

/// 一片里此刻的键（认可只留这些，别的是过了版本的）。
pub(crate) fn keys_of(sl: &PlanSlice) -> Vec<String> {
    sl.needs.iter().map(|x| x.key.clone()).collect()
}

/// 这个键此刻在不在、能不能认可：不在 ⇒ `None`；在、是问人那一种 ⇒ `Some(false)`。
pub(crate) fn ackable(sl: &PlanSlice, key: &str) -> Option<bool> {
    sl.needs
        .iter()
        .find(|x| x.key == key)
        .map(|x| x.kind != ASK)
}

/// 一片要你看的数：没认可的、不算问人那一种（那一条由会话那一侧数，只算一次）。
pub(crate) fn count_slice(sl: &PlanSlice) -> u32 {
    let n = sl
        .needs
        .iter()
        .filter(|x| x.kind != ASK && !x.acked)
        .count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// 退回 / 说给负责的那一个对象：一格，或顶块 `project`（pb 里与格同样可寻址：`pb see project`）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Target {
    pub(crate) id: String,
    pub(crate) title: Option<String>,
    pub(crate) children: Vec<String>,
    pub(crate) body: Option<String>,
    /// 在长它的。
    pub(crate) owner: Option<PlanWho>,
    /// 签它的。
    pub(crate) signer: Option<PlanWho>,
}

impl Target {
    /// 一格。
    pub(crate) fn of_cell(c: &PlanCell) -> Self {
        Self {
            id: c.id.clone(),
            title: c.title.clone(),
            children: c.children.clone(),
            body: c.body.clone(),
            owner: c.owner.clone(),
            signer: c.signer.clone(),
        }
    }

    /// `owner` · `signer` 那一位；别的名字 ⇒ `None`。
    pub(crate) fn who(&self, role: &str) -> Option<&PlanWho> {
        match role {
            "owner" => self.owner.as_ref(),
            "signer" => self.signer.as_ref(),
            _ => None,
        }
    }
}

/// 退回的对象：一格 ⇒ 那一格；`project`（顶块）⇒ 标题是片名 · 子是顶层那几格 · 在长它的是顶块接手 · 没有正文 · 没有签它的（project 谁都签不了）。
/// 落地照格的判法（[`landing`]）：顶层多出退回之后新建的一格。这一片没有这一格 ⇒ `None`。
pub(crate) fn target(sl: &PlanSlice, id: &str) -> Option<Target> {
    if id == TOP_BLOCK {
        return Some(Target {
            id: TOP_BLOCK.to_string(),
            title: Some(sl.name.clone()),
            children: sl.top.clone(),
            body: None,
            owner: block(sl, TOP_BLOCK)?.owner.clone(),
            signer: None,
        });
    }
    cell(sl, id).map(Target::of_cell)
}

/// 一格正文的摘要（只用来分辨「改没改」）。
pub(crate) fn body_digest(body: Option<&str>) -> String {
    super::book::rev_of(body.unwrap_or_default().as_bytes())
}

/// 一次退回此刻落没落地：那一格底下多出了退回时没有的子格（带它的标题）· 或它的正文改了。
/// 没落地 ⇒ `state` 照送的结局：`returned`（送到了）· `unsure`（送达未知）。`cells` 是那一片此刻的全部格（取新格的标题）。
/// `to` 是那一格此刻在长它的 / 签它的里头会话对得上记下那个 sid 的那一位；都对不上 ⇒ `None`。
pub(crate) fn landing(r: &Returned, t: &Target, cells: &[PlanCell]) -> PlanReturned {
    let state = if r.result == "unsure" {
        "unsure"
    } else {
        "returned"
    };
    let to = [&t.owner, &t.signer]
        .into_iter()
        .flatten()
        .find(|w| w.sid.as_deref() == Some(r.to.as_str()))
        .cloned();
    let mut out = PlanReturned {
        at: r.at,
        at_text: None,
        to,
        state: state.to_string(),
        by: None,
        child: None,
    };
    if let Some(new) = t.children.iter().find(|c| !r.children.contains(c)) {
        out.state = "landed".to_string();
        out.by = Some("child".to_string());
        out.child = Some(PlanReturnedChild {
            id: new.clone(),
            title: cells
                .iter()
                .find(|c| c.id == *new)
                .and_then(|c| c.title.clone()),
        });
    } else if body_digest(t.body.as_deref()) != r.body {
        out.state = "landed".to_string();
        out.by = Some("body".to_string());
    }
    out
}

/// 送进会话的那一行：只带事实（人 · 编号 标题：原话），收到人的话怎么办 pb 已经教过 agent。原话里的换行与连串空白并成一个空格（一行送出）。
pub(crate) fn line(id: &str, title: &str, text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    copy_text(
        "bePlan.return.line",
        &[("id", id), ("title", title), ("text", &text)],
    )
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/needs_tests.rs"]
mod tests;
