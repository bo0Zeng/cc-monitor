//! 问图（`plan-ask`）的帧面宿主：计划页［问这一块…］开框前问这台一次 —— 开口那句由这台拼
//! （别名合下来的开口那句 `say` ＋「看 {编号}「{标题}」这一格：」），别名预选这台记着的「问图用的」那条。
//!
//! - 给了 `alias` ⇒ 那条要在这台的配置文件里、合得下来，记成这台问图用的（[`crate::plan::review`] 的 `ask`，后端自己的状态）；
//!   没给 ⇒ 用记着的那条，那条已经没了 / 合不下来 ⇒ 不预选（`alias: null`），只给后一半。
//! - 只读计划（手上上一次读好的那一份；还没读过 ⇒ 现读），计划仓一个字节不写；起会话照旧走 `session-open`
//!   （框里那一句当第一句，盖过别名的）。问图不补足：每问一次是一件新事。

use crate::assets::aliases::profile::{self, Book};
use crate::ccm_line::argv::Parsed;
use crate::plan::needs;
use crate::plan::wire::PlanRead;
use crate::wire::spec::Fail;
use crate::wire::typed::{self, Command, Cx, Handle};
use copy_core::copy_text;

type Answer = Result<PlanAsk, Fail>;

/// 记下这台问图用的那条（生产 ＝ [`crate::plan::review::ask_at`]）。
pub(crate) type Keep<'a> = &'a dyn Fn(&str) -> Result<(), Fail>;

/// `plan-ask` 的本体：`doc` 是这台手上的那一份计划；`book` 是这台的配置文件；`kept` 是记着的那条。
pub(crate) fn ask_with(
    req: &PlanAskReq,
    doc: &PlanRead,
    book: &Book,
    kept: Option<&str>,
    keep: Keep,
) -> Answer {
    let no_cell = || Fail::new("no_such_cell", copy_text("bePlan.review.noSuchCell", &[]));
    let sl = doc
        .slices
        .iter()
        .find(|s| s.name == req.slice)
        .ok_or_else(no_cell)?;
    let cell = needs::target(sl, &req.id).ok_or_else(no_cell)?;
    let part = match cell.title.as_deref().filter(|t| !t.is_empty()) {
        Some(title) => copy_text("bePlan.ask.line", &[("id", &req.id), ("title", title)]),
        None => copy_text("bePlan.ask.lineBare", &[("id", &req.id)]),
    };
    let (alias, say) = match req.alias.as_deref().map(str::trim) {
        Some(a) => {
            let say = say_of(book, a).map_err(|why| {
                Fail::new(
                    "no_such_alias",
                    copy_text("bePlan.ask.noSuchAlias", &[("alias", a), ("why", &why)]),
                )
            })?;
            keep(a)?;
            (Some(a.to_string()), say)
        }
        None => match kept.and_then(|a| say_of(book, a).ok().map(|s| (a, s))) {
            Some((a, say)) => (Some(a.to_string()), say),
            None => (None, String::new()),
        },
    };
    let line = if say.trim().is_empty() {
        part
    } else {
        format!("{say}\n{part}")
    };
    Ok(PlanAsk { alias, line })
}

/// 这条别名合下来的开口那句（没写 ⇒ 空）；不在 / 合不下来 ⇒ 那一句。
fn say_of(book: &Book, name: &str) -> Result<String, String> {
    match profile::resolve(book, name, &[])?.parsed {
        Parsed::Opts(o) => Ok(o.say.clone()),
        Parsed::Early(_) => Err(copy_text("beSessionSet.alias.missing", &[("alias", name)])),
    }
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
    let store = crate::assets::aliases::load(files).map_err(|m| Fail::new("refused", m))?;
    let review = crate::plan::review::current(crate::plan::review::review_path().as_deref());
    let keep = |a: &str| crate::plan::review::ask_at(own, a);
    ask_with(req, &doc, &store.book, review.ask(), &keep)
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
    /// 预选的别名（这一问给的，或记着的那条）；没有 ⇒ `null`（框里那一格空着，选了才能起）。
    pub(crate) alias: Option<String>,
    /// 开口那句：别名的那句 ＋ 换行 ＋「看 {编号}「{标题}」这一格：」；没有别名 / 别名没写 ⇒ 只有后一半。
    pub(crate) line: String,
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/plan_ask_face_tests.rs"]
mod tests;
