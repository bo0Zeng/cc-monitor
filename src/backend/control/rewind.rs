//! 编辑已发的一句：在那条会话里替人按那一家自己的回退（同在终端里双击 Esc），退到被编辑那一句之前，再说改好的那一句。
//!
//! 只有确认那一下才到这里（改动全攒在发件那一端）：那一句随一份 [`Rewind`] 进输入队，投递那一下调 [`walk`]。
//! 按法 · 每一屏怎么认 · 「怎么退」那几项在确认页上叫什么住适配层（[`RewindFace`]）；这里只有先后与核对，不认任何一家的字。
//!
//! # 纪律（实测：同样「Esc、隔一下、Esc」一次弹了列表一次没弹，没弹时后面的 ↑ 进了输入历史、Enter 把历史里那句重发了一遍）
//!
//! - **每按一个键之前读屏**，此刻是哪一屏（输入行 · 列表 · 确认页）对得上才按；对不上一个键不按，照实回「没退成」与为什么。
//! - 列表上每往上移一行，读屏核对选中的那一行就是记录主线上该到的那一句；对不上 ⇒ 退回输入行再回。
//! - **最后那一下确认只在确认页、且选中的正是所选那一项时按**；确认页上没有所选那一项 ⇒ 退回输入行再回（不替人换一项）。
//! - 主 agent 在干活：选了等这一轮完 ⇒ 一个键不按、回「等」（输入队照排着）；选了现在停 ⇒ 先按那一家的打断键、等回到输入行。
//!   只有后台子 agent 在跑、主 agent 空着 ⇒ 屏是输入行，照常退，不碰子 agent。
//! - 输入行里有字（被打断的那一句放回来了 · 那一家把原句填回来了）⇒ 照那一家申报的删字键删掉、读屏确认空了才往下；没申报 ⇒ 不退。
//! - 送进改好的那一句之后从记录读回：主线上人说过的那几句 ＝ 被编辑那一句之前的那几句 ＋ 改好的那一句，才算成了；没读到 ⇒ 不重发，照实说。
//! - 不改 agent 的记录文件（R18），只替人按那一家自己的那几下。

pub(crate) use crate::agents::{RewindFace, RewindRow, RewindScreen};
use crate::common::said::Said;
use crate::control::terminals::no_said;
use crate::control::terminals_wire::{Sent, SentResult};
pub(crate) use cmd_wire::rewind::{
    Rewind, RewindChoiceRow, RewindHow, RewindOptions, RewindWhen, RewindWhenRow,
};
use copy_core::copy_text;

/// 按一下之后最多等几次重画（每次至多那一份 io 的一次等待）才认定「没变」。
pub(crate) const REDRAWS_PER_PRESS: usize = 4;
/// 送进改好的那一句之后，最多读回几次记录（每次之间等一次重画）。
pub(crate) const READ_BACKS: usize = 8;

/// 记录主线上人说过的一句（读回 · 核对列表用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UserLine {
    /// 记录里那一句的 id。
    pub(crate) uuid: String,
    /// 那一句的字（可能是截短的摘要）。
    pub(crate) text: String,
}

/// 回退那一串要的副作用（判据拿假 agent 的终端喂）。
pub(crate) trait RewindIo {
    /// 此刻那一屏（纯文本行）。
    fn screen(&mut self) -> Result<Vec<String>, Said>;
    /// 按一个键（`terminal-input` 键表里的名字）。
    fn press(&mut self, key: &str) -> Result<Sent, Said>;
    /// 等下一次重画（到期没变 ⇒ `false`）。
    fn redrawn(&mut self) -> bool;
    /// 隔一下（毫秒；调出菜单那几下之间）。
    fn pause(&mut self, ms: u64);
    /// 送进一句（字面字 ＋ 回车）。
    fn say(&mut self, text: &str) -> Result<Sent, Said>;
    /// 记录主线上人说过的那几句（先说的在前）。
    fn lines(&mut self) -> Result<Vec<UserLine>, Said>;
}

/// 这一趟要做的：退到哪一句之前 · 怎么退 · 什么时候退 · 改好的那一句。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Plan<'a> {
    pub(crate) rewind: &'a Rewind,
    pub(crate) text: &'a str,
}

/// 没退成 · 没做完的原因（[`Walked::said`] 写成给人看的那一句）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Why {
    /// 这一家没申报回退菜单 · 没有所选那一种退法。
    Unsupported,
    /// 记录主线上找不到被编辑的那一句。
    NoSuchLine,
    /// 输入行里有字，删不掉（没申报删字键 · 删了还在）。
    InputBusy,
    /// 此刻那一屏不是预想的那一屏（那一家升级改了样子 · 弹着别的框）。
    MenuUnlike,
    /// 按了调出菜单那几下，回退列表没出来（还在输入行）。
    MenuClosed,
    /// 列表上选中的那一行和记录主线上该到的那一句对不上。
    ListUnlike,
    /// 确认页上没有所选那一项。
    NoChoice(RewindHow),
    /// 这一家不支持「现在停」。
    CannotStop,
    /// 按了打断，没回到输入行。
    NotStopped,
    /// 送键 · 送字被拒（那一份原样：终端没了 · 不归我们 …）。
    Refused(Sent),
    /// 已退到那一句之前，改好的那一句没送进去。
    NotSent,
    /// 改好的那一句送进去了，记录里还没读到。
    NotReadBack,
}

/// 走完的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Walked {
    /// 退了、改好的那一句送进去了、记录里读回来了。
    Done,
    /// 主 agent 在干活、选的是等这一轮完：一个键没按，照排着。
    Wait,
    /// 没退成（会话没动：一个键没按，或按了又退回了输入行）。
    No(Why),
    /// 做了一部分、结局说不准（不重发）。
    Unsure(Why),
}

impl Walked {
    /// 给人看的那一句（成了 · 在等 ⇒ `None`）：投递那一下把它写进那一条的样子（未送达 · 说不准）。
    pub(crate) fn said(&self) -> Option<String> {
        match self {
            Walked::Done | Walked::Wait => None,
            Walked::No(w) | Walked::Unsure(w) => Some(said_of(w)),
        }
    }
}

fn said_of(why: &Why) -> String {
    match why {
        Why::Unsupported => copy_text("beRewind.no.unsupported", &[]),
        Why::NoSuchLine => copy_text("beRewind.no.noSuchLine", &[]),
        Why::InputBusy => copy_text("beRewind.no.inputBusy", &[]),
        Why::MenuUnlike => copy_text("beRewind.no.menuUnlike", &[]),
        Why::MenuClosed => copy_text("beRewind.no.menuClosed", &[]),
        Why::ListUnlike => copy_text("beRewind.no.listUnlike", &[]),
        Why::NoChoice(how) => copy_text("beRewind.no.noChoice", &[("choice", &how_text(*how))]),
        Why::CannotStop => copy_text("beRewind.no.cannotStop", &[]),
        Why::NotStopped => copy_text("beRewind.no.notStopped", &[]),
        Why::Refused(s) => s
            .said
            .clone()
            .or_else(|| s.why.map(no_said))
            .unwrap_or_else(|| copy_text("beRewind.no.menuUnlike", &[])),
        Why::NotSent => copy_text("beRewind.unsure.notSent", &[]),
        Why::NotReadBack => copy_text("beRewind.unsure.notReadBack", &[]),
    }
}

/// 「怎么退」那一种通用的叫法（原因句里点名用）。
fn how_text(how: RewindHow) -> String {
    match how {
        RewindHow::Conversation => copy_text("beRewind.how.conversation", &[]),
        RewindHow::CodeAndConversation => copy_text("beRewind.how.codeAndConversation", &[]),
    }
}

/// 发送那一下的选项（`session-rewind-options` 的本体）：那一家申报的几项照摆；`busy` ＝ 主 agent 此刻在干活（只有后台子 agent 在跑不算）。
/// 被编辑那一句之后的事（发送那一下判哪一项能选 · 要不要多说一句）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct After {
    /// 那一句起，主 agent 改过代码（[`code_after`]）。
    pub(crate) code: bool,
    /// 那一句之后派出、此刻还在跑的子 agent 几个（[`runs_after`]）。
    pub(crate) runs: usize,
}

/// 运行表里「`since_ms` 之后派出、还在跑」的几个：起的时刻不知道的不数；那一句的时刻不知道 ⇒ 0（不照猜的说）。
pub(crate) fn runs_after(runs: &[cmd_wire::frame::RunInfo], since_ms: Option<u64>) -> usize {
    let Some(since) = since_ms else {
        return 0;
    };
    runs.iter()
        .filter(|r| r.state == cmd_wire::frame::RunState::Running)
        .filter(|r| r.started_ms.is_some_and(|t| t >= since))
        .count()
}

/// `kind` 那一家的这几条记录（那一句起，文件序）里，主线上有没有写文件的调用：那一家的一拍里认（[`crate::agents::beat_of`]）；
/// 子运行的记录 · `off`（主线外，回退掉的那几条的 id）不算。
pub(crate) fn code_after<'a>(
    kind: &str,
    records: impl IntoIterator<Item = &'a serde_json::Value>,
    off: &[String],
) -> bool {
    records.into_iter().any(|v| {
        crate::agents::run_of_record(kind, v).is_none()
            && crate::agents::beat_of(kind, v).is_some_and(|b| {
                b.id.as_deref()
                    .is_none_or(|id| !off.iter().any(|o| o == id))
                    && b.acts
                        .iter()
                        .any(|a| matches!(a, crate::agents::beat::Act::Call { edits: Some(_), .. }))
            })
    })
}

/// 通用的几种退法（发送那一下照这个先后都列出，那一家没申报的灰着）。
const HOWS: [RewindHow; 2] = [RewindHow::Conversation, RewindHow::CodeAndConversation];

/// 一种退法这一刻能不能选：那一家没申报 ⇒ 不支持；要退代码、那一句之后没改过代码 ⇒ 没东西可退。
fn off_why(declared: bool, how: RewindHow, after: After) -> Option<String> {
    if !declared {
        return Some(copy_text("beRewind.off.unsupported", &[]));
    }
    (how == RewindHow::CodeAndConversation && !after.code)
        .then(|| copy_text("beRewind.off.noCodeChange", &[]))
}

/// 发送那一下的选项（`session-rewind-options` 的本体）：通用的几种退法都列，那一家申报的照它的字，不能选的灰着带原因；
/// `busy` ＝ 主 agent 此刻在干活（只有后台子 agent 在跑不算）；`after` ＝ 被编辑那一句之后的事。
pub(crate) fn options_of(face: Option<&RewindFace>, busy: bool, after: After) -> RewindOptions {
    let Some(face) = face.filter(|f| !f.choices.is_empty()) else {
        return RewindOptions {
            said: Some(copy_text("beRewind.no.unsupported", &[])),
            ..RewindOptions::default()
        };
    };
    let mut how: Vec<RewindChoiceRow> = HOWS
        .iter()
        .map(|&h| {
            let c = face.choices.iter().find(|c| c.how == h);
            let why = off_why(c.is_some(), h, after);
            RewindChoiceRow {
                how: h,
                text: c.map_or_else(|| how_text(h), |c| (c.text)()),
                default: why.is_none() && c.is_some_and(|c| c.default),
                enabled: why.is_none(),
                why,
            }
        })
        .collect();
    if !how.iter().any(|r| r.default) {
        if let Some(first) = how.iter_mut().find(|r| r.enabled) {
            first.default = true;
        }
    }
    let mut when = Vec::new();
    if busy {
        when.push(RewindWhenRow {
            when: RewindWhen::AfterTurn,
            text: copy_text("beRewind.when.afterTurn", &[]),
            default: true,
        });
        if face.interrupt.is_some() {
            when.push(RewindWhenRow {
                when: RewindWhen::Now,
                text: copy_text("beRewind.when.now", &[]),
                default: false,
            });
        }
    }
    let said = (after.runs > 0)
        .then(|| copy_text("beRewind.note.runsAfter", &[("n", &after.runs.to_string())]));
    RewindOptions { how, when, said }
}

/// 列表上那一行的字与记录里那一句是不是同一句：空白压成一个、去掉截短标之后，一方是另一方的开头（屏上 · 摘要都可能截短）。
pub(crate) fn same_line(shown: &str, said: &str) -> bool {
    let norm = |s: &str| {
        let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
        s.trim_end_matches('…')
            .trim_end_matches("...")
            .trim_end()
            .to_string()
    };
    let (a, b) = (norm(shown), norm(said));
    !a.is_empty() && !b.is_empty() && (a.starts_with(&b) || b.starts_with(&a))
}

/// 走一趟（见头注）。`face` ＝ 那一家的回退菜单。
pub(crate) fn walk(
    face: &RewindFace,
    plan: Plan<'_>,
    io: &mut dyn RewindIo,
) -> Result<Walked, Said> {
    let Some(choice) = face.choices.iter().find(|c| c.how == plan.rewind.how) else {
        return Ok(Walked::No(Why::Unsupported));
    };
    let before = io.lines()?;
    let Some(at) = before.iter().position(|s| s.uuid == plan.rewind.to) else {
        return Ok(Walked::No(Why::NoSuchLine));
    };
    let mut w = Walker { face, io };

    // 1 回到空着的输入行（在干活 ⇒ 等 · 先停；输入行有字 ⇒ 删）。
    match w.read()? {
        RewindScreen::Busy => {
            if plan.rewind.when == RewindWhen::AfterTurn {
                return Ok(Walked::Wait);
            }
            let Some(key) = face.interrupt else {
                return Ok(Walked::No(Why::CannotStop));
            };
            if let Some(no) = w.press(key)? {
                return Ok(Walked::No(no));
            }
            if !matches!(
                w.settle(|s| !matches!(s, RewindScreen::Busy))?,
                RewindScreen::Prompt { .. }
            ) {
                return Ok(Walked::Unsure(Why::NotStopped));
            }
        }
        RewindScreen::Prompt { .. } => {}
        _ => return Ok(Walked::No(Why::MenuUnlike)),
    }
    if let Some(no) = w.clear()? {
        return Ok(Walked::No(no));
    }

    // 2 调出回退列表：每一下之前确认还在输入行。
    for (i, key) in face.open.iter().enumerate() {
        if i > 0 {
            w.io.pause(face.open_gap_ms);
        }
        match w.read()? {
            RewindScreen::Prompt { .. } => {}
            RewindScreen::List { .. } if i > 0 => break,
            _ => return Ok(Walked::No(Why::MenuUnlike)),
        }
        if let Some(no) = w.press(key)? {
            return Ok(Walked::No(no));
        }
    }
    let rows = match w.settle(|s| !matches!(s, RewindScreen::Prompt { .. }))? {
        RewindScreen::List { rows } => rows,
        RewindScreen::Prompt { .. } => return Ok(Walked::No(Why::MenuClosed)),
        _ => return Ok(Walked::No(Why::MenuUnlike)),
    };
    // 起手选中的是「此刻」那一行。
    if !rows.iter().any(|r| r.selected && r.current) {
        return w.back_out(Why::ListUnlike);
    }

    // 3 往上移到被编辑那一句：每一步读屏核对选中的那一行。
    let ups = before.len() - at;
    let mut prev = rows_selected_text(&rows);
    for step in 1..=ups {
        if let Some(no) = w.press(face.up)? {
            return Ok(Walked::No(no));
        }
        let want = &before[before.len() - step];
        let rows = match w.settle(|s| !selected_is(s, &prev))? {
            RewindScreen::List { rows } => rows,
            _ => return w.back_out(Why::MenuUnlike),
        };
        match rows.iter().find(|r| r.selected) {
            Some(r) if !r.current && same_line(&r.text, &want.text) => prev = r.text.clone(),
            _ => return w.back_out(Why::ListUnlike),
        }
    }

    // 4 进确认页。
    if let Some(no) = w.press(face.enter)? {
        return Ok(Walked::No(no));
    }
    let items = match w.settle(|s| !matches!(s, RewindScreen::List { .. }))? {
        RewindScreen::Confirm { items } => items,
        _ => return w.back_out(Why::MenuUnlike),
    };
    let Some(want) = items.iter().position(|it| it.label == choice.label) else {
        return w.back_out(Why::NoChoice(plan.rewind.how));
    };

    // 5 移到所选那一项（每一步读屏），选中的正是它才按确认。
    let mut items = items;
    for _ in 0..items.len() {
        let Some(sel) = items.iter().position(|it| it.selected) else {
            return w.back_out(Why::MenuUnlike);
        };
        if sel == want {
            break;
        }
        let key = if sel > want { face.up } else { face.down };
        if let Some(no) = w.press(key)? {
            return Ok(Walked::No(no));
        }
        items = match w.settle(|s| !confirm_selected_is(s, sel))? {
            RewindScreen::Confirm { items } => items,
            _ => return w.back_out(Why::MenuUnlike),
        };
    }
    if items
        .get(want)
        .is_none_or(|it| !it.selected || it.label != choice.label)
    {
        return w.back_out(Why::MenuUnlike);
    }
    if let Some(no) = w.press(face.enter)? {
        return Ok(Walked::No(no));
    }

    // 6 退了：等回到输入行，有字就删，再说改好的那一句。
    if !matches!(
        w.settle(|s| !matches!(s, RewindScreen::Confirm { .. }))?,
        RewindScreen::Prompt { .. }
    ) {
        return Ok(Walked::Unsure(Why::NotSent));
    }
    if w.clear()?.is_some() {
        return Ok(Walked::Unsure(Why::NotSent));
    }
    let sent = w.io.say(plan.text)?;
    if sent.result == SentResult::Refused {
        return Ok(Walked::Unsure(Why::NotSent));
    }

    // 7 读回：主线 ＝ 被编辑那一句之前的那几句 ＋ 改好的那一句。
    for _ in 0..READ_BACKS {
        let now = w.io.lines()?;
        if read_back_ok(&before[..at], plan.text, &now) {
            return Ok(Walked::Done);
        }
        w.io.redrawn();
    }
    Ok(Walked::Unsure(Why::NotReadBack))
}

/// 主线读回对不对：前面那几句一句不差、接着恰好是改好的那一句。
pub(crate) fn read_back_ok(kept: &[UserLine], text: &str, now: &[UserLine]) -> bool {
    now.len() == kept.len() + 1
        && now.iter().zip(kept).all(|(a, b)| a.uuid == b.uuid)
        && now.last().is_some_and(|l| same_line(&l.text, text))
}

fn rows_selected_text(rows: &[RewindRow]) -> String {
    rows.iter()
        .find(|r| r.selected)
        .map(|r| r.text.clone())
        .unwrap_or_default()
}

/// 屏上还是列表、选中的还是 `text` 那一行（按了之后还没重画）。
fn selected_is(s: &RewindScreen, text: &str) -> bool {
    matches!(s, RewindScreen::List { rows } if rows.iter().any(|r| r.selected && r.text == text))
}

/// 屏上还是确认页、选中的还是第 `at` 项。
fn confirm_selected_is(s: &RewindScreen, at: usize) -> bool {
    matches!(s, RewindScreen::Confirm { items } if items.get(at).is_some_and(|it| it.selected))
}

struct Walker<'a> {
    face: &'a RewindFace,
    io: &'a mut dyn RewindIo,
}

impl Walker<'_> {
    fn read(&mut self) -> Result<RewindScreen, Said> {
        Ok((self.face.read)(&self.io.screen()?))
    }

    /// 按一下；被拒 ⇒ 原因（不再按别的）。
    fn press(&mut self, key: &str) -> Result<Option<Why>, Said> {
        let sent = self.io.press(key)?;
        Ok(match sent.result {
            SentResult::Delivered => None,
            SentResult::Unsure | SentResult::Refused => Some(Why::Refused(sent)),
        })
    }

    /// 等屏变到 `moved` 认的样子（至多 [`REDRAWS_PER_PRESS`] 次重画）；回最后读到的那一屏。
    fn settle(&mut self, moved: impl Fn(&RewindScreen) -> bool) -> Result<RewindScreen, Said> {
        let mut now = self.read()?;
        for _ in 0..REDRAWS_PER_PRESS {
            if moved(&now) {
                break;
            }
            if !self.io.redrawn() {
                break;
            }
            now = self.read()?;
        }
        Ok(now)
    }

    /// 输入行里有字 ⇒ 照申报的删字键删掉，读屏确认空了；删不掉 ⇒ 原因。
    fn clear(&mut self) -> Result<Option<Why>, Said> {
        let RewindScreen::Prompt { typed } = self.read()? else {
            return Ok(Some(Why::MenuUnlike));
        };
        if typed.is_empty() {
            return Ok(None);
        }
        let n = typed.chars().count();
        // 删几下 ＝ 屏上读到的字数（输入行在一屏之内，读到的就是全部）。
        let Some(key) = self.face.erase else {
            return Ok(Some(Why::InputBusy));
        };
        for _ in 0..n {
            if let Some(no) = self.press(key)? {
                return Ok(Some(no));
            }
        }
        match self.settle(|s| matches!(s, RewindScreen::Prompt { typed } if typed.is_empty()))? {
            RewindScreen::Prompt { typed } if typed.is_empty() => Ok(None),
            _ => Ok(Some(Why::InputBusy)),
        }
    }

    /// 中途对不上：确认页 · 列表上一层层按「退一层」退回输入行（每一下之前读屏认得出才按），回「没退成」。
    /// 退不回去（认不出那一屏 · 按了没回到输入行）⇒ 结局说不准。
    fn back_out(&mut self, why: Why) -> Result<Walked, Said> {
        for _ in 0..3 {
            match self.read()? {
                RewindScreen::Prompt { .. } => return Ok(Walked::No(why)),
                RewindScreen::List { .. } | RewindScreen::Confirm { .. } => {}
                _ => return Ok(Walked::Unsure(why)),
            }
            if self.press(self.face.back)?.is_some() {
                return Ok(Walked::Unsure(why));
            }
            self.io.redrawn();
        }
        match self.read()? {
            RewindScreen::Prompt { .. } => Ok(Walked::No(why)),
            _ => Ok(Walked::Unsure(why)),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/rewind_tests.rs"]
mod tests;
