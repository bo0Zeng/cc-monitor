//! 问图（`plan-ask`）的帧面宿主：计划页［问这一块…］开框前问这台一次 —— 开口那句由这台拼
//! （别名合下来的开口那句 `say` ＋「看 {编号}「{标题}」：」），框顶那一句 · 起在哪那一句（`runs`）也由这台写，别名预选这台记着的「问图用的」那条。
//!
//! - 这台没有 tmux ⇒ `no_tmux`（开口那句只跟 tmux）：不开框，界面照 hello 里的「做不到」也能先灰。
//! - 给了 `alias` ⇒ 那条要在这台的配置文件里、合得下来，记成这台问图用的（[`crate::plan::review`] 的 `ask`，后端自己的状态）；
//!   没给 ⇒ 用记着的那条，那条已经没了 / 合不下来 ⇒ 不预选（`alias: null`），只给后一半。
//! - 只读计划（手上上一次读好的那一份；还没读过 ⇒ 现读），计划仓一个字节不写；起会话照旧走 `session-open`
//!   （框里那一句当第一句，盖过别名的）。问图不补足：每问一次是一件新事。

use crate::assets::aliases::profile::{self, Book};
use crate::ccm_line::argv::{flag, Parsed};
use crate::common::said::Said;
use crate::plan::needs;
use crate::wire::typed::{self, Command, Cx, Handle};
use cmd_wire::plan::PlanRead;
use copy_core::copy_text;

type Answer = Result<PlanAsk, Said>;

/// 记下这台问图用的那条（生产 ＝ [`crate::plan::review::ask_at`]）。
pub(crate) type Keep<'a> = &'a dyn Fn(&str) -> Result<(), Said>;

/// `plan-ask` 的本体：`doc` 是这台手上的那一份计划；`book` 是这台的配置文件；`kept` 是记着的那条。
pub(crate) fn ask_with(
    req: &PlanAskReq,
    doc: &PlanRead,
    book: &Book,
    kept: Option<&str>,
    tmux: Option<bool>,
    keep: Keep,
) -> Answer {
    // 开口那句只跟 tmux（`session-open` 的 `prompt` 只随 `run: tmux`）：这台没有 ⇒ 不开框；探不出 ⇒ 照做，起的那一步再说。
    if tmux == Some(false) {
        return Err(crate::wire::spec::fail(
            crate::NO_TMUX,
            copy_text("bePlan.ask.noTmux", &[]),
        ));
    }
    let no_cell =
        || crate::wire::spec::fail("no_such_cell", copy_text("bePlan.review.noSuchCell", &[]));
    let sl = doc
        .slices
        .iter()
        .find(|s| s.name == req.slice)
        .ok_or_else(no_cell)?;
    let cell = needs::target(sl, &req.id).ok_or_else(no_cell)?;
    let named = cell.title.as_deref().filter(|t| !t.is_empty());
    let part = match named {
        Some(title) => copy_text("bePlan.ask.line", &[("id", &req.id), ("title", title)]),
        None => copy_text("bePlan.ask.lineBare", &[("id", &req.id)]),
    };
    let title = copy_text("bePlan.ask.title", &[("title", named.unwrap_or(&req.id))]);
    let (alias, said) = match req.alias.as_deref().map(str::trim) {
        Some(a) => {
            let said = lines_of(book, a).map_err(|why| {
                crate::wire::spec::fail(
                    "no_such_alias",
                    copy_text("bePlan.ask.noSuchAlias", &[("alias", a), ("why", &why)]),
                )
            })?;
            keep(a)?;
            (Some(a.to_string()), Some(said))
        }
        None => match kept.and_then(|a| lines_of(book, a).ok().map(|s| (a, s))) {
            Some((a, said)) => (Some(a.to_string()), Some(said)),
            None => (None, None),
        },
    };
    let say = said.as_ref().map_or("", |s| s.say.as_str());
    let line = if say.trim().is_empty() {
        part
    } else {
        format!("{say}\n{part}")
    };
    Ok(PlanAsk {
        title,
        alias,
        line,
        runs: said.map(|s| s.runs),
    })
}

/// 一条别名合下来、问图要的两样：开口那句（没写 ⇒ 空）· 起在哪那一句。
struct AliasLines {
    say: String,
    runs: String,
}

/// 这条别名合下来的那两样；不在 / 合不下来 ⇒ 那一句。
/// 起在哪：「tmux 里」恒在（问图只跟 tmux）· 号那一项照合下来的 `--account` / `--base` 和它来自哪一层 ·
/// 进分组照 `--tab-group`；没写的那一段不出。机器不写：这台不知道看的那一端管它叫什么，框本来就锁在这台。
fn lines_of(book: &Book, name: &str) -> Result<AliasLines, String> {
    let r = profile::resolve(book, name, &[])?;
    let Parsed::Opts(o) = r.parsed else {
        return Err(copy_text("beSessionSet.alias.missing", &[("alias", name)]));
    };
    let from = |f: &str| {
        r.origins
            .iter()
            .rev()
            .find(|g| g.flag == f)
            .map_or(name, |g| g.layer.as_str())
            .to_string()
    };
    let mut parts = vec![copy_text("bePlan.ask.runsTmux", &[])];
    if !o.account.is_empty() {
        parts.push(copy_text(
            "bePlan.ask.runsAccount",
            &[("account", &o.account), ("from", &from(flag::ACCOUNT))],
        ));
    } else if o.use_base {
        parts.push(copy_text(
            "bePlan.ask.runsBase",
            &[("from", &from(flag::BASE))],
        ));
    }
    if !o.tab_group.is_empty() {
        parts.push(copy_text(
            "bePlan.ask.runsGroup",
            &[("group", &o.tab_group)],
        ));
    }
    Ok(AliasLines {
        say: o.say.clone(),
        runs: parts.join(&copy_text("bePlan.ask.runsSep", &[])),
    })
}

/// 生产那一趟：计划用这台手上上一次读好的那一份（没有 ⇒ 现读）；配置文件经这台的文件管理面读；记着的那条读自家状态。
fn ask_here(
    req: &PlanAskReq,
    files: super::launch_account_face::Files,
    own: &crate::platform::fs::Own,
    tz: &crate::Tz,
) -> Answer {
    let doc = match crate::plan::book::book().last(&req.workspace) {
        Some(d) => d,
        None => super::plan_face::read_fresh(&req.workspace, tz)?,
    };
    let store =
        crate::assets::aliases::load(files).map_err(|m| crate::wire::spec::fail("refused", m))?;
    let review = crate::plan::review::current(crate::plan::review::review_path().as_deref());
    let keep = |a: &str| crate::plan::review::ask_at(own, a);
    ask_with(
        req,
        &doc,
        &store.book,
        review.ask(),
        crate::control::terminals::tmux_here(),
        &keep,
    )
}

typed::codes! {
    /// `plan-ask` 的码：读计划（手上没有 ⇒ 起 pb）· 读这台的配置文件 · 判格与别名 · 记问图用的那条。
    pub(crate) enum PlanAskErr {
        ChildTimedOut = "child_timed_out",
        Failed = "failed",
        IoFailed = "io_failed",
        NoPb = "no_pb",
        NoSuchAlias = "no_such_alias",
        NoSuchCell = "no_such_cell",
        /// 这台没有 tmux（开口那句只跟 tmux）。
        NoTmux = "no_tmux",
        NotWorkspace = "not_workspace",
        PbUnsupported = "pb_unsupported",
        Refused = "refused",
        ReviewUnreadable = "review_unreadable",
    }
}

/// `plan-ask`：问图开框前那一问（读计划可能起 pb · 读配置文件 · 写自家状态 ⇒ 阻塞档，带文件面与自家写口）。
pub(crate) struct PlanAskCmd;

impl Command for PlanAskCmd {
    const NAME: &'static str = "plan-ask";
    type Req = PlanAskReq;
    type Reply = PlanAsk;
    type Err = PlanAskErr;
}

impl Handle for PlanAskCmd {
    type Needs = typed::needs::FilesOwn;
    fn run(
        req: PlanAskReq,
        cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<PlanAsk, PlanAskErr>> + Send + 'static {
        let (cx, needs) = cx.split();
        async move {
            let (files, own) = (needs.0, &needs.1);
            ask_here(&req, files, own, &cx.tz).map_err(|f| typed::of_fail(f, PlanAskErr::Failed))
        }
    }
}

/// `plan-ask` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanAskReq {
    /// 工作区根。
    pub(crate) workspace: String,
    /// 片名。
    pub(crate) slice: String,
    /// 格的编号。
    pub(crate) id: String,
    /// 改选的那条别名（记成这台问图用的）；缺 ⇒ 用记着的。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) alias: Option<String>,
}

/// `plan-ask` 的回包。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlanAsk {
    /// 框顶那一句（「问图 · {标题}」）。
    pub(crate) title: String,
    /// 预选的别名（这一问给的，或记着的那条）；没有 ⇒ `null`（框里那一格空着，选了才能起）。
    pub(crate) alias: Option<String>,
    /// 开口那句：别名的那句 ＋ 换行 ＋「看 {编号}「{标题}」这一格：」；没有别名 / 别名没写 ⇒ 只有后一半。
    pub(crate) line: String,
    /// 起在哪那一句（「tmux 里 · 号 lab（来自 look）· 进分组「看图」」，照别名合下来的写）；没有别名 ⇒ `null`。
    pub(crate) runs: Option<String>,
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/plan_ask_face_tests.rs"]
mod tests;
