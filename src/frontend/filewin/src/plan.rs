//! 文件窗口反查：目录落在某一片的仓库里 ⇒ 列表多一列「格」· 预览头多一块「归属」· 右键多一项「在计划里看」。
//!
//! 判定全在那台的后端（`plan-files`：这个目录归哪一片、每份文件归哪一格、谁也声明了它、哪几份无主）；回包按线上类型
//! [`cmd_wire::plan::PlanFiles`] 解（不手解），这里只把它摊成按名字查的一张表（[`PlanDir::claim`]）、排版。「编号 标题」后端拼好（`label`）。
//! 别处的目录（不在任何一片里）回包 `slice: null` ⇒ 什么都不多。无主照回包的 `unowned` 画；判不了（`unowned: null`）⇒ 没主的那几份写「—」、悬停 pb 那一句。
//! 「在计划里看」经通道交给 monitor（[`filewin_contract::PLAN_OPEN_OP`]）：把主窗口拉到前面、切到计划页、选中那一格。
//! 计划变了（那台推 `changed/plan`，[`PlanWatch`]）⇒ 每个标签页重问一次 `plan-files`：只换「格」一列与预览头那一块，不重列、不重排、不丢选中。

use cmd_wire::plan::{PlanFileState, PlanFiles, PlanStatusCode};
use copy_core::copy_text;
use std::collections::{BTreeMap, BTreeSet};

/// 一份文件归的那一格（线上那一份原样）。
pub use cmd_wire::plan::PlanFileOwner as Owner;

/// 那条线上命令的名字（后端 `plan` 族）。
pub const CMD_PLAN_FILES: &str = "plan-files";

/// 那台的计划变了那条流：后端的 `changed` 帧、主题 `plan`（monitor 按主题原样扇出）。格体 `{key, rev, body}` 窗口不读 ——
/// 变了就重问 `plan-files`，归谁、什么状态照旧只由那一问答。
pub const PLAN_CHANGED_KIND: &str = "changed/plan";

/// 那台的计划变过几次（整扇窗一份，每个标签页拿同一份；[`PLAN_CHANGED_KIND`] 那条流写进来）。
#[derive(Clone, Default)]
pub struct PlanWatch(std::sync::Arc<std::sync::Mutex<PlanWatchInner>>);

#[derive(Default)]
struct PlanWatchInner {
    /// 流上来过几格（丢了几格那一种也算一次：丢的那几格里可能有变）。
    ticks: u64,
    /// 已经起了那条订阅。
    watching: bool,
}

impl PlanWatch {
    fn lock(&self) -> std::sync::MutexGuard<'_, PlanWatchInner> {
        match self.0.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    /// 变过几次。
    pub fn ticks(&self) -> u64 {
        self.lock().ticks
    }

    /// 起那条订阅（整扇窗一次；流关了就停：之后列目录那一趟照旧顺带问一次）。
    pub fn watch(
        &self,
        rt: &tokio::runtime::Handle,
        line: super::source::Line,
        origin: super::source::Origin,
        ctx: Option<egui::Context>,
    ) {
        {
            let mut g = self.lock();
            if g.watching {
                return;
            }
            g.watching = true;
        }
        let me = self.clone();
        rt.spawn(async move {
            super::source::watch_plan(&line, &origin, || {
                me.lock().ticks += 1;
                if let Some(c) = &ctx {
                    c.request_repaint();
                }
            })
            .await;
        });
    }
}

/// 目录里一项在这一片里是什么。
#[derive(Debug, PartialEq, Eq)]
pub enum Claim<'a> {
    /// 有一格声明它。
    Owner(&'a Owner),
    /// pb 说没有一格声明它。
    Unowned,
    /// 判不了无主（仓库读不出）：pb 那一句。
    Untold(&'a str),
    /// 什么都不画：文件夹 · 那一片读不成 · 回包里没提到它。
    Unknown,
}

/// 一个目录的反查结果（落在某一片的仓库里才有）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanDir {
    pub workspace: String,
    pub slice: String,
    /// 那一片此刻读不成的那一句 ⇒ 「格」一列整列写「—」。
    pub unreadable: Option<String>,
    /// 文件名 ⇒ 归的那一格。
    owners: BTreeMap<String, Owner>,
    /// 无主的那几份；判不了 ⇒ `None`。
    unowned: Option<BTreeSet<String>>,
    unowned_why: Option<String>,
}

impl PlanDir {
    /// `plan-files` 的回包 → 一张按名字查的表；不在任何一片里（`slice: null`）⇒ `None`。
    pub fn of(f: PlanFiles) -> Option<Self> {
        Some(Self {
            workspace: f.workspace.unwrap_or_default(),
            slice: f.slice?,
            unreadable: f.unreadable,
            owners: f.entries.into_iter().map(|e| (e.name.clone(), e)).collect(),
            unowned: f.unowned.map(|u| u.into_iter().collect()),
            unowned_why: f.unowned_why,
        })
    }

    /// 这一项归哪一格（没人声明的 ⇒ `None`）。
    pub fn owner(&self, name: &str) -> Option<&Owner> {
        self.owners.get(name)
    }

    /// 这一项在这一片里是什么（有主 · 无主 · 判不了 · 不画）。
    pub fn claim(&self, name: &str) -> Claim<'_> {
        if let Some(o) = self.owners.get(name) {
            return Claim::Owner(o);
        }
        match (&self.unowned, &self.unowned_why) {
            (Some(u), _) if u.contains(name) => Claim::Unowned,
            (None, Some(why)) => Claim::Untold(why),
            _ => Claim::Unknown,
        }
    }
}

/// 对账「坏」时 pb 给的原因（在 · 缺 · 空 ⇒ `None`）。
pub fn broken(o: &Owner) -> Option<&str> {
    (o.file_state == Some(PlanFileState::Broken))
        .then(|| o.file_note.as_deref().unwrap_or_default())
}

/// 状态的图标（同计划页：已签收 ✓ · 没签收 ○ · 不算活的节点 ▤）。
pub fn status_icon(st: Option<PlanStatusCode>) -> &'static str {
    match st {
        Some(PlanStatusCode::Done) => egui_phosphor::regular::CHECK_CIRCLE,
        Some(PlanStatusCode::Open) => egui_phosphor::regular::CIRCLE,
        None => egui_phosphor::regular::NOTE,
    }
}

/// 状态那一道色：已签收 ⇒ `done`，其余 ⇒ `rest`（同计划页：不算活的与没签收的同色、靠图标分）。
pub fn status_tint(
    st: Option<PlanStatusCode>,
    done: egui::Color32,
    rest: egui::Color32,
) -> egui::Color32 {
    if st == Some(PlanStatusCode::Done) {
        done
    } else {
        rest
    }
}

/// 无主那一份「格」一列写的字。
pub fn unowned_mark() -> String {
    copy_text("rsFilewinPlan.cell.unowned", &[])
}

/// 列名。
pub fn column_label() -> String {
    copy_text("rsFilewinPlan.col.title", &[])
}

/// 读不成那一片时「格」一列每一格写的字。
pub fn unreadable_mark() -> String {
    copy_text("rsFilewinPlan.cell.unreadable", &[])
}

/// 也声明它的那一句（悬停 · 预览头那一行红字）。
pub fn dup_text(o: &Owner) -> String {
    copy_text(
        "rsFilewinPlan.cell.dup",
        &[(
            "title",
            &o.dup
                .iter()
                .map(|d| d.label.as_str())
                .collect::<Vec<_>>()
                .join(&copy_text("rsFilewinPlan.cell.listSep", &[])),
        )],
    )
}

/// 预览头「归属」那一块的第二行：仓库 · 权力范围「…」（住最上面那一个 ⇒ 不写那一段）。
pub fn owner_line(slice: &str, o: &Owner) -> String {
    let mut parts = vec![slice.to_string()];
    if let Some(b) = &o.block {
        parts.push(copy_text("rsFilewinPlan.owner.block", &[("block", b)]));
    }
    parts.join(&copy_text("rsFilewinRows.tip.sep", &[]))
}

/// 右键菜单那一项的字。
pub fn menu_label(o: &Owner) -> String {
    copy_text("rsFilewinPlan.menu.open", &[("title", &o.label)])
}

/// 预览头那颗按钮的字。
pub fn open_label() -> String {
    copy_text("rsFilewinPlan.owner.open", &[])
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/plan_tests.rs"]
mod tests;
