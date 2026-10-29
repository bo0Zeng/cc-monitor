//! 帧面 `rotation-strategies` · `rotation-view`（typed，配置编辑层）：轮换的两条读；事实 · 一判 · 推演问基础层，成品在这里拼。
//! 都只回成品：策略目录里每一份（名字 · 灰字 · 版本 · 读不懂的原因 · 哪几个会话在用）；一个会话（照它载入的副本）或设置页上的一份策略
//! （照文件此刻的）此刻的设置 · 每个号的事实 · 顺序 · 这一判 · 推演 · 提示。界面照画，不判、不排、不算时刻。

use crate::accounts::quota::rules::book::{self, Book};
use crate::accounts::quota::rules::decide::Pick;
use crate::accounts::quota::rules::{standing, Adopted};
use crate::accounts::quota::setup::{self, form, script, shelf, uses};
use crate::accounts::quota::strategy::FailWhy;
use crate::common::cells::Words;
use crate::faces::rotation_act_face::{judged, lineup_of, plain_lineup, Judged};
use crate::faces::rotation_face::Ctx;
use crate::faces::rotation_try_face::{decision_of, timeline_of};
use crate::wire::typed::Command;
use cmd_wire::cells::Tone;
pub(crate) use cmd_wire::rotation::{
    RotAccount, RotBadCell, RotBoard, RotCell, RotChoice, RotDaySeg, RotGroup, RotHint, RotLine,
    RotLogRow, RotMark, RotNote, RotOldFile, RotPlate, RotRefused, RotRow, RotStrategyError,
    RotStrategyItem, RotTarget, RotationStrategiesReply, RotationStrategiesReq, RotationViewReply,
    RotationViewReq,
};
use copy_core::copy_text;
use serde_json::Value;

crate::wire::typed::codes! {
    /// `rotation-strategies` 自己的码（只有入参不对那一种）。
    pub(crate) enum RotationStrategiesErr {}
}

/// `rotation-strategies`：读策略目录与 `rotation.json` ⇒ 阻塞档。
pub(crate) struct RotationStrategies;

impl Command for RotationStrategies {
    crate::wire::typed::by_call!(RotationStrategiesReq);
    type Err = RotationStrategiesErr;
}

impl crate::wire::typed::Handle for RotationStrategies {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: RotationStrategiesReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotationStrategiesReply, RotationStrategiesErr>>
           + Send
           + 'static {
        async move {
            let ctx = Ctx::here();
            let live = (ctx.hop.live)();
            Ok(strategies_with(&ctx, &req, &|sid| live.contains(sid)))
        }
    }
}

/// 一份写成线上那一项。
pub(crate) fn item_of(
    s: &shelf::Shelved,
    book: &Book,
    for_new: &str,
    live: &dyn Fn(&str) -> bool,
) -> RotStrategyItem {
    let head = s.file.as_ref().ok().map(|f| &f.head);
    let description = s
        .file
        .as_ref()
        .map_or_else(|_| String::new(), |f| f.about.clone());
    RotStrategyItem {
        id: s.id.clone(),
        summary: summary_of(&description),
        description,
        rev: s.rev.clone(),
        error: s.file.as_ref().err().map(error_of),
        in_use: uses::in_use(book, &s.id, None)
            .into_iter()
            .filter(|sid| live(sid))
            .collect(),
        for_new: for_new == s.id,
        words_bad: head.is_some_and(|h| form::words_bad(&h.form)),
        tz_unknown: head.is_some_and(|h| h.tz.is_none() || h.tz_unknown),
        hand: s.file.as_ref().is_ok_and(|f| !setup::editor::made_by(f)),
    }
}

/// 说明的第一行（不空的那一行）。
fn summary_of(description: &str) -> String {
    description
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
}

/// 本体（测试给假的账本 · 判活）。
pub(crate) fn strategies_with(
    ctx: &Ctx,
    _req: &RotationStrategiesReq,
    live: &dyn Fn(&str) -> bool,
) -> RotationStrategiesReply {
    let book = ctx.hop.store.now();
    let for_new = ctx.setup.uses.now().for_new;
    let dir = ctx.setup.dir();
    RotationStrategiesReply {
        strategies: shelf::all(dir.as_deref())
            .iter()
            .map(|s| item_of(s, &book, &for_new, live))
            .collect(),
        for_new,
        old_scripts: shelf::old_count(dir.as_deref()),
    }
}

/// 一个号的号牌：号色照账号库（库里没有 ⇒ 按名补的那一格）· 号名 · 号牌字（与 `accounts-list` 那一行同一处写法）。
fn plate_of(id: &str, colors: &std::collections::BTreeMap<String, u8>) -> RotPlate {
    use crate::accounts::quota::name_words::{account_text, plate_text};
    RotPlate {
        color: colors
            .get(id)
            .copied()
            .unwrap_or_else(|| crate::accounts::manage::model::color_fallback(id)),
        label: Words(account_text(id)),
        text: Words(plate_text(id)),
    }
}

/// 视图里的一个号：事实 · 号牌 · 用满了的那几窗（照 [`standing::used_up`] 那一处判）。
fn account_of(
    fact: cmd_wire::rotation::Fact,
    colors: &std::collections::BTreeMap<String, u8>,
) -> RotAccount {
    let full = [("5h", &fact.w5h), ("7d", &fact.w7d)]
        .into_iter()
        .filter(|(_, w)| standing::used_up(w.used))
        .map(|(k, _)| k.to_string())
        .collect();
    RotAccount {
        plate: plate_of(&fact.account, colors),
        fact,
        full,
    }
}

/// 读不懂的那一份为什么：第几行 · 原话（哪一种常见的错只拼提示那一句，不上线：[`why_text`]）。
pub(crate) fn error_of(f: &crate::accounts::quota::strategy::Fail) -> RotStrategyError {
    RotStrategyError {
        line: f.line,
        msg: f.msg.clone(),
    }
}

/// 出错的那种说法（文案表里的）。
fn why_text(why: FailWhy) -> String {
    match why {
        FailWhy::DivZero => copy_core::copy_text("beRotation.why.divZero", &[]),
        FailWhy::NoFn => copy_core::copy_text("beRotation.why.noFn", &[]),
        FailWhy::Type => copy_core::copy_text("beRotation.why.type", &[]),
        FailWhy::Limit => copy_core::copy_text("beRotation.why.limit", &[]),
        FailWhy::Timeout => copy_core::copy_text("beRotation.why.timeout", &[]),
        FailWhy::Index => copy_core::copy_text("beRotation.why.index", &[]),
        FailWhy::Gone => copy_core::copy_text("beRotation.why.gone", &[]),
        FailWhy::Other => copy_core::copy_text("beRotation.why.other", &[]),
    }
}

/// 提示写成一句（字照文案表）。`name` ＝ 那份策略的标题；`why` ＝ 出错那一条是哪一种常见的错（第几行 · 说法进那一句，原话进复制详情）。
fn note_of(hint: RotHint, name: &str, why: FailWhy) -> RotNote {
    let text = match &hint {
        RotHint::Hold {
            until_text: Some(at),
            account: Some(a),
            ..
        } => copy_core::copy_text(
            "beRotation.hint.holdUntil",
            &[("at", at), ("account", a), ("dur", "{dur}")],
        ),
        RotHint::Hold {
            until_text: Some(at),
            ..
        } => copy_core::copy_text("beRotation.hint.holdAt", &[("at", at), ("dur", "{dur}")]),
        RotHint::Hold { .. } => copy_core::copy_text("beRotation.hint.hold", &[]),
        RotHint::StrategyError { error, .. } => match error.line {
            Some(l) => copy_core::copy_text(
                "beRotation.hint.errorLine",
                &[
                    ("name", name),
                    ("line", &l.to_string()),
                    ("why", &why_text(why)),
                ],
            ),
            None => copy_core::copy_text(
                "beRotation.hint.error",
                &[("name", name), ("why", &why_text(why))],
            ),
        },
        RotHint::TzUnknown { .. } => copy_core::copy_text("beRotation.hint.tz", &[]),
        RotHint::Newer => copy_core::copy_text("beRotation.hint.newer", &[("name", name)]),
        RotHint::Older { n } => {
            copy_core::copy_text("beRotation.hint.older", &[("n", &n.to_string())])
        }
        RotHint::OldScripts { n, .. } => {
            copy_core::copy_text("beRotation.hint.old", &[("n", &n.to_string())])
        }
        RotHint::WordsBad => copy_core::copy_text("beRotation.hint.words", &[]),
        RotHint::BookUnreadable { text } => text.clone(),
    };
    let detail = match &hint {
        RotHint::StrategyError { error, .. } => Some(error.msg.clone()),
        _ => None,
    };
    let tone = match &hint {
        RotHint::StrategyError { .. } | RotHint::BookUnreadable { .. } => Tone::Fail,
        RotHint::Hold { .. } => Tone::Warn,
        RotHint::TzUnknown { .. } | RotHint::OldScripts { .. } => Tone::Need,
        RotHint::Newer | RotHint::Older { .. } | RotHint::WordsBad => Tone::Plain,
    };
    RotNote {
        hint,
        tone,
        text,
        detail,
    }
}

/// 换号记录配上为什么那一句。
fn log_rows(log: &[book::Log]) -> Vec<RotLogRow> {
    log.iter()
        .map(|h| RotLogRow {
            because: match (&h.why, &h.cause) {
                (Some(w), _) if !w.is_empty() => w.clone(),
                (_, book::Cause::Order) => copy_core::copy_text("beRotation.cause.order", &[]),
                (_, book::Cause::Unusable) => {
                    copy_core::copy_text("beRotation.cause.unusable", &[("account", &h.from)])
                }
                (_, book::Cause::Temp) => copy_core::copy_text("beRotation.cause.temp", &[]),
                (_, book::Cause::UseNow) => copy_core::copy_text("beRotation.cause.useNow", &[]),
                (_, book::Cause::Hold { .. }) => copy_core::copy_text("beRotation.cause.hold", &[]),
                (_, book::Cause::Error) => copy_core::copy_text("beRotation.cause.error", &[]),
            },
            log: h.clone(),
        })
        .collect()
}

/// 会话那一形顶栏写的名字（[`RotationViewReply::title`]）。
fn title_of(target: &RotTarget) -> Option<String> {
    let RotTarget::Session { sid } = target else {
        return None;
    };
    let annotations = match crate::history::history_annotations::load() {
        crate::history::history_annotations::Loaded::Read(t) => t,
        _ => Default::default(),
    };
    Some(crate::history::session_hidden::title_here(
        sid,
        &annotations,
    ))
}

crate::wire::typed::codes! {
    /// `rotation-view` 自己的码。
    pub(crate) enum RotationViewErr {
        /// 中转没见过这个会话。
        NoSession = "no_session",
        /// 策略目录里没有那一份。
        NoStrategy = "no_strategy",
    }
}

/// 照那份策略的声明校验一份设置：收不下 ⇒ 那几格。
pub(crate) fn checked(file: Option<&script::File>, settings: &Value) -> Result<(), RotRefused> {
    let form = file.map_or(&[][..], |f| f.head.form.as_slice());
    let bad = form::check(form, settings);
    if bad.is_empty() {
        Ok(())
    } else {
        Err(RotRefused {
            cells: bad
                .into_iter()
                .map(|b| RotBadCell {
                    cell: b.cell,
                    code: b.code.to_string(),
                })
                .collect(),
        })
    }
}

/// `rotation-view`：读额度账 · 账号库 · 策略目录、在沙箱里跑策略 ⇒ 阻塞档。
pub(crate) struct RotationView;

impl Command for RotationView {
    crate::wire::typed::by_call!(RotationViewReq);
    type Err = RotationViewErr;
}

impl crate::wire::typed::Handle for RotationView {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: RotationViewReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotationViewReply, RotationViewErr>> + Send + 'static
    {
        let (cx, ()) = cx.split();
        async move {
            let cx = &cx;

            let ctx = Ctx::here();
            let live = (ctx.hop.live)();
            view_with(
                &ctx,
                &req,
                crate::accounts::quota::now_unix(),
                &cx.tz,
                &|sid| live.contains(sid),
            )
            .map_err(|e| match (e, &req.target) {
                // 账本里没有它：上游是自设的那种（中转直通、不进账本）⇒ 照实写那一句。
                (RotationViewErr::NoSession(_), RotTarget::Session { sid }) => session_gone(
                    crate::accounts::upstream_select::session_upstream::own_upstream_here(sid),
                ),
                (e, _) => e,
            })
        }
    }
}

/// 一刻的钟面（`HH:MM`，看的那一台的时区）。
fn face(t: u64, tz: &crate::Tz) -> String {
    crate::common::time::hm(tz.local(i64::try_from(t).unwrap_or(i64::MAX)))
}

/// 会话那一形推多远，只为「下一个」那一格的钟点（不上线）。
const SESSION_HOURS: u64 = 12;

/// 顺序后面那几格是什么：会话有策略 ⇒ 接下来先用谁（可拖）；不轮换 ⇒ 只是备着的号（不可拖，不写「下一个」）。
pub(crate) enum Ahead {
    /// `next_at` ＝ 推演里第一次换号换到谁、几点（钟面；推不准 · 马上就换 ⇒ 缺）：换到的正是第二格才写钟点。
    Strategy {
        next_at: Option<(String, String)>,
    },
    Plain,
}

/// 会话的顺序成行（`line` ＝ [`lineup_of`] 排好的先后，第一格是在用的号）：哪一组 · 右格的字 · 策略那一句 · 能不能拖。
pub(crate) fn lines_of(
    line: &[String],
    off: &[String],
    sendable: &dyn Fn(&str) -> bool,
    blocked: &dyn Fn(&str) -> Option<String>,
    why: &[(String, String)],
    ahead: Ahead,
) -> Vec<RotLine> {
    let is_off = |a: &str| off.iter().any(|x| x == a);
    line.iter()
        .enumerate()
        .map(|(i, a)| {
            let group = match () {
                () if i == 0 => RotGroup::Use,
                () if is_off(a) => RotGroup::Off,
                () if !sendable(a) => RotGroup::Out,
                () => RotGroup::Next,
            };
            let state = match group {
                RotGroup::Use if is_off(a) => copy_text("beRotation.line.offUse", &[]),
                RotGroup::Use => copy_text("beRotation.line.use", &[]),
                RotGroup::Next if i == 1 && matches!(ahead, Ahead::Strategy { .. }) => match &ahead
                {
                    Ahead::Strategy {
                        next_at: Some((to, at)),
                    } if to == a => copy_text("beRotation.line.nextAt", &[("at", at)]),
                    _ => copy_text("beRotation.line.next", &[]),
                },
                RotGroup::Next => copy_text("beRotation.line.spare", &[]),
                RotGroup::Out => {
                    blocked(a).unwrap_or_else(|| copy_text("beRotation.line.out", &[]))
                }
                RotGroup::Off => copy_text("beRotation.line.off", &[]),
            };
            let stay = match group {
                RotGroup::Out => Some(copy_text("beRotation.edit.unusable", &[("acct", a)])),
                RotGroup::Off => Some(copy_text("beRotation.edit.offNow", &[("acct", a)])),
                RotGroup::Use | RotGroup::Next => None,
            };
            RotLine {
                account: a.clone(),
                group,
                state,
                why: why
                    .iter()
                    .find(|(x, _)| x == a)
                    .map(|(_, w)| w.clone())
                    .filter(|w| !w.is_empty()),
                movable: group == RotGroup::Next && matches!(ahead, Ahead::Strategy { .. }),
                stay,
            }
        })
        .collect()
}

/// 中转账本里没有这个会话（`no_session`）那一句：上游是自设的 ⇒ 额度看不到 · 换号不适用；不是 ⇒ 后端没见过它。
pub(crate) fn session_gone(own_upstream: bool) -> RotationViewErr {
    RotationViewErr::NoSession(if own_upstream {
        copy_core::copy_text("acct.own.line", &[])
    } else {
        copy_core::copy_text("beRotation.session.gone", &[])
    })
}

/// 本体（测试给假的账本 · 额度账 · 钟 · 判活）。
pub(crate) fn view_with(
    ctx: &Ctx,
    req: &RotationViewReq,
    now: u64,
    tz: &crate::Tz,
    live: &dyn Fn(&str) -> bool,
) -> Result<RotationViewReply, RotationViewErr> {
    let base = ctx.hop.store.now();
    let for_new = ctx.setup.uses.now().for_new;
    let dir = ctx.setup.dir();
    let all = shelf::all(dir.as_deref());
    // 会话 ⇒ 照它载入的副本；设置页 ⇒ 照文件此刻的。
    let (session, plan, shelved, form, agent) = match &req.target {
        RotTarget::Session { sid } => {
            let s = base
                .sessions
                .get(sid)
                .cloned()
                .ok_or_else(|| session_gone(false))?;
            let shelved = s.uses.as_ref().and_then(|c| shelf::find(&all, &c.id));
            let form = s.uses.as_ref().map(setup::form_of).unwrap_or_default();
            let agent = s.agent.clone();
            let plan = Adopted::of(s.uses.as_ref());
            (s, plan, shelved, form, agent)
        }
        RotTarget::Strategy { strategy } => {
            let shelved = shelf::find(&all, strategy).ok_or_else(|| {
                RotationViewErr::NoStrategy(copy_core::copy_text(
                    "beRotation.strategy.gone",
                    &[("name", strategy)],
                ))
            })?;
            let (plan, form) = match &shelved.file {
                Ok(f) => (
                    setup::adopted_of_file(strategy, &shelved.rev, f, None),
                    f.head.form.clone(),
                ),
                Err(e) => (
                    Adopted {
                        id: strategy.clone(),
                        rev: shelved.rev.clone(),
                        code: Err(e.clone()),
                        ..Adopted::of(None)
                    },
                    Vec::new(),
                ),
            };
            let zero = crate::accounts::manage::model::ACCOUNT_ZERO;
            let s = book::Session::fresh(crate::faces::rotation_face::LIBRARY_AGENT, zero, now);
            let agent = crate::faces::rotation_face::LIBRARY_AGENT.to_string();
            (s, plan, Some(shelved), form, agent)
        }
    };
    let unreadable = || {
        ctx.hop
            .store
            .unreadable()
            .or_else(|| ctx.setup.uses.unreadable())
    };
    if plan.id.is_empty() {
        let mut hints = Vec::new();
        if let Some(e) = unreadable() {
            hints.push(note_of(
                RotHint::BookUnreadable { text: e.said },
                "",
                FailWhy::Other,
            ));
        }
        // 不轮换 ＝ 不自动换：会话那一形照样一号一行（手动换号照常），不判、不推演；号都不在号列表里。
        let (lineup, ids) = match &req.target {
            RotTarget::Session { sid } => {
                let row = ctx.row(&agent);
                let facts = ctx
                    .hop
                    .accts(sid, &session, &base, &plan.settings, &row, now);
                let line = plain_lineup(&session, &facts, now);
                let sendable = |a: &str| {
                    facts
                        .iter()
                        .find(|x| x.id == a)
                        .is_some_and(|x| standing::sendable(x, now))
                };
                let all = ctx.hop.facts(&agent, &row, now, tz);
                let blocked = |a: &str| {
                    all.iter()
                        .find(|f| f.account == a)
                        .and_then(|f| f.blocked.as_ref())
                        .map(|b| b.text.clone())
                };
                let lines = lines_of(&line, &session.off, &sendable, &blocked, &[], Ahead::Plain);
                (lines, line)
            }
            RotTarget::Strategy { .. } => (Vec::new(), vec![session.start.clone()]),
        };
        let lib = ctx.hop.library();
        let row = ctx.row(&agent);
        let colors = crate::observe::accounts_query::account_colors();
        return Ok(RotationViewReply {
            now,
            now_text: face(now, tz),
            title: title_of(&req.target),
            strategy: None,
            board: RotBoard::default(),
            start: session.start.clone(),
            current: session.current.clone(),
            lineup,
            manual: false,
            off: session.off.clone(),
            accounts: ids
                .iter()
                .filter(|id| !id.is_empty())
                .map(|id| account_of(ctx.hop.fact_one(&agent, &lib, id, &row, now, tz), &colors))
                .collect(),
            listed: 0,
            decision: None,
            timeline: None,
            log: log_rows(&session.log),
            hints,
        });
    }
    let row = ctx.row(&agent);
    let sid = match &req.target {
        RotTarget::Session { sid } => sid.as_str(),
        RotTarget::Strategy { .. } => "",
    };
    let hours = match &req.target {
        RotTarget::Session { .. } => SESSION_HOURS,
        RotTarget::Strategy { .. } => u64::from(req.hours.unwrap_or(12).clamp(1, 48)),
    };
    let Judged { facts, d, tl } = judged(ctx, sid, &session, &plan, &row, now, now + hours * 3600);
    let item = match &shelved {
        Some(s) => item_of(s, &base, &for_new, live),
        None => RotStrategyItem {
            id: plan.id.clone(),
            description: String::new(),
            summary: String::new(),
            rev: String::new(),
            error: None,
            in_use: Vec::new(),
            for_new: for_new == plan.id,
            words_bad: false,
            tz_unknown: false,
            hand: false,
        },
    };
    let mut hints = Vec::new();
    if let Pick::Hold { until, account } = &d.pick {
        hints.push(RotHint::Hold {
            until: *until,
            account: account.clone(),
            until_text: until.map(|t| {
                crate::common::time::fmt_at(
                    i64::try_from(t).unwrap_or(i64::MAX),
                    i64::try_from(now).unwrap_or(i64::MAX),
                    tz,
                )
            }),
        });
    }
    let why = d.strategy_error.as_ref().map_or(FailWhy::Other, |e| e.why);
    if let Some(e) = &d.strategy_error {
        hints.push(RotHint::StrategyError {
            error: error_of(e),
            path: shelved
                .as_ref()
                .and(dir.as_deref())
                .map(|d| shelf::path_of(d, &plan.id).display().to_string()),
        });
    }
    match &req.target {
        // 文件在它载入之后改过（还读得懂）⇒ 这个会话还在用旧版。
        RotTarget::Session { .. } => {
            if item.error.is_none() && !item.rev.is_empty() && item.rev != plan.rev {
                hints.push(RotHint::Newer);
            }
        }
        RotTarget::Strategy { strategy } => {
            let n = uses::in_use(&base, strategy, Some(&item.rev))
                .into_iter()
                .filter(|s| live(s))
                .count();
            if n > 0 {
                hints.push(RotHint::Older { n });
            }
        }
    }
    if item.tz_unknown {
        hints.push(RotHint::TzUnknown {
            zone: tz.viewer_zone().unwrap_or_else(|| "UTC".into()),
        });
    }
    if item.words_bad {
        hints.push(RotHint::WordsBad);
    }
    if let Some(e) = unreadable() {
        hints.push(RotHint::BookUnreadable { text: e.said });
    }
    let old = shelf::old_files(dir.as_deref());
    if !old.is_empty() {
        hints.push(RotHint::OldScripts {
            n: old.len(),
            files: old
                .iter()
                .map(|p| RotOldFile {
                    name: p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    path: p.display().to_string(),
                })
                .collect(),
        });
    }
    let all_facts = ctx.hop.facts(&agent, &row, now, tz);
    // 设置行：号列表里一个号一行（同号只认头一次，同判那一处），各带它在列表里的行号；这台账号库没有的号照出。
    let mut rows: Vec<(usize, String)> = Vec::new();
    for (i, id) in crate::accounts::quota::rules::listed_ids(&plan.settings, &session.start)
        .into_iter()
        .enumerate()
    {
        if !rows.iter().any(|(_, x)| x == id) {
            rows.push((i, id.to_string()));
        }
    }
    let listed = rows.len();
    let ids: Vec<String> = rows
        .iter()
        .map(|(_, id)| id.clone())
        .chain(
            facts
                .iter()
                .map(|a| a.id.clone())
                .filter(|id| !rows.iter().any(|(_, x)| x == id)),
        )
        .collect();
    let lib = ctx.hop.library();
    let colors = crate::observe::accounts_query::account_colors();
    let accounts: Vec<RotAccount> = ids
        .iter()
        .map(|id| {
            account_of(
                all_facts
                    .iter()
                    .find(|f| f.account == *id)
                    .cloned()
                    .unwrap_or_else(|| ctx.hop.fact_one(&agent, &lib, id, &row, now, tz)),
                &colors,
            )
        })
        .collect();
    let (lineup, timeline, log) = match &req.target {
        RotTarget::Session { .. } => {
            // 推演里第一次换号：在用的号推不准几点到上限 ⇒ 不写钟点；马上就换 ⇒ 也不写（接着此刻那段）。
            let sure = tl.uses.first().is_some_and(|u| !u.cap_unknown);
            let next_at = tl
                .switches()
                .into_iter()
                .next()
                .filter(|(at, _)| sure && *at > now)
                .map(|(at, a)| (a, face(at, tz)));
            let blocked = |a: &str| {
                accounts
                    .iter()
                    .find(|x| x.fact.account == a)
                    .and_then(|x| x.fact.blocked.as_ref())
                    .map(|b| b.text.clone())
            };
            let sendable = |a: &str| {
                facts
                    .iter()
                    .find(|x| x.id == a)
                    .is_some_and(|x| standing::sendable(x, now))
            };
            (
                lines_of(
                    &lineup_of(&session, &d, &facts, now),
                    &session.off,
                    &sendable,
                    &blocked,
                    &d.why,
                    Ahead::Strategy { next_at },
                ),
                None,
                log_rows(&session.log),
            )
        }
        RotTarget::Strategy { .. } => (
            Vec::new(),
            Some(timeline_of(&tl, now, now + hours * 3600, tz)),
            Vec::new(),
        ),
    };
    let name = item.id.clone();
    Ok(RotationViewReply {
        now,
        now_text: face(now, tz),
        title: title_of(&req.target),
        strategy: Some(item),
        board: board_of(
            &form,
            &plan.settings,
            &rows
                .iter()
                .zip(&accounts)
                .map(|((i, id), a)| (*i, a, facts.iter().any(|f| f.id == *id)))
                .collect::<Vec<_>>(),
        ),
        start: session.start.clone(),
        current: session.current.clone(),
        lineup,
        manual: !session.temp.is_empty(),
        off: session.off.clone(),
        accounts,
        listed,
        decision: Some(decision_of(&d)),
        timeline,
        log,
        hints: hints.into_iter().map(|h| note_of(h, &name, why)).collect(),
    })
}

/// 策略写的字（界面只有中文 ⇒ 取 `zh`）。
fn text_of(t: &script::Text) -> String {
    match t {
        script::Text::One(s) => s.clone(),
        script::Text::Langs { zh, .. } => zh.clone(),
    }
}

/// 一天里的分钟 ⇒ `HH:MM`（一整天的末尾写成 24:00）。
fn day_clock(min: u64) -> String {
    if min >= form::DAY_MIN {
        copy_text("rot.day.end", &[])
    } else {
        format!("{:02}:{:02}", min / 60, min % 60)
    }
}

/// 24 小时条上的一段配上字（数照它的样子写：整数不带小数点）。
fn day_seg((from, to, v): (u64, u64, f64)) -> RotDaySeg {
    let n = format!("{v}");
    RotDaySeg {
        from,
        to,
        from_text: day_clock(from),
        to_text: day_clock(to),
        v,
        text: if v == 0.0 {
            copy_text("rot.day.zero", &[])
        } else {
            copy_text("rot.day.v", &[("v", &n)])
        },
        tip: copy_text(
            "rot.day.seg",
            &[
                ("from", &day_clock(from)),
                ("to", &day_clock(to)),
                ("v", &n),
            ],
        ),
    }
}

/// 一格（24 小时条 ⇒ 每窗一格）照声明解好。
fn cells_of(c: &form::Cell, settings: &Value, row: Option<usize>) -> Vec<RotCell> {
    let label = text_of(&c.label);
    let base = RotCell {
        key: c.key.clone(),
        kind: c.kind,
        window: None,
        label: label.clone(),
        help: c.help.as_ref().map(text_of).filter(|h| !h.is_empty()),
        v: Value::Null,
        num: None,
        from: form::RotFrom::Default,
        nullable: c.nullable.as_ref().map(text_of),
        choices: c
            .choices
            .iter()
            .map(|x| RotChoice {
                v: x.v.clone(),
                label: text_of(&x.label),
            })
            .collect(),
        min: c.min,
        max: c.max,
        segs: Vec::new(),
        lead: c.lead.as_ref().map(text_of),
        under: c.show_if.as_ref().map(|w| w.key.clone()),
        same: None,
    };
    if c.kind == form::Kind::DaySegments {
        let win = |w: &str| match w {
            "5h" => copy_text("rot.win.fiveHour", &[]),
            "7d" => copy_text("rot.win.sevenDay", &[]),
            _ => copy_text("rot.win.other", &[("w", w)]),
        };
        let first = c.windows.first().map(|w| win(w)).unwrap_or_default();
        return c
            .windows
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let (segs, from) = form::day_of(c, settings, row, w);
                RotCell {
                    window: Some(w.clone()),
                    label: copy_text("rot.cell.win", &[("label", &label), ("win", &win(w))]),
                    from,
                    segs: segs.into_iter().map(day_seg).collect(),
                    same: (c.follow && i > 0)
                        .then(|| copy_text("rot.cell.same", &[("win", &first)])),
                    ..base.clone()
                }
            })
            .collect();
    }
    let (v, from) = form::value_of(c, settings, row);
    let num = matches!(c.kind, form::Kind::Percent | form::Kind::Minutes).then(|| {
        v.as_f64()
            .or_else(|| c.default.as_f64())
            .or(c.min)
            .unwrap_or(0.0)
    });
    vec![RotCell {
        v,
        num,
        from,
        ..base
    }]
}

/// 一格写成一句（只看的规则摘要）：名字 · 此刻的值（选项名 · 可空的说法 · 24 小时条每段那一句）。
fn rule_of(c: &RotCell) -> String {
    let sep = copy_text("kit.text.sep", &[]);
    let v = match c.kind {
        form::Kind::Toggle if c.v == Value::Bool(true) => copy_text("rot.order.on", &[]),
        form::Kind::Toggle => copy_text("rot.order.off", &[]),
        form::Kind::OneOf => c
            .choices
            .iter()
            .find(|x| Value::String(x.v.clone()) == c.v)
            .map(|x| x.label.clone())
            .unwrap_or_default(),
        form::Kind::DaySegments if c.from == form::RotFrom::Same => {
            c.same.clone().unwrap_or_default()
        }
        form::Kind::DaySegments => c
            .segs
            .iter()
            .map(|g| g.tip.clone())
            .collect::<Vec<_>>()
            .join(&sep),
        form::Kind::Percent | form::Kind::Minutes => match (&c.v, &c.nullable) {
            (Value::Null, Some(n)) => n.clone(),
            _ if c.kind == form::Kind::Percent => {
                format!(
                    "{}{}",
                    c.num.unwrap_or(0.0),
                    copy_text("rot.unit.percent", &[])
                )
            }
            _ => format!(
                "{}{}",
                c.num.unwrap_or(0.0),
                copy_text("rot.unit.minutes", &[])
            ),
        },
    };
    format!("{}{sep}{v}", c.label)
}

/// 设置那几块：总体（`scope: all`）· 号列表每一行（字 ＋ `scope: each` 那几格 ＋ 写成的句子）。
/// `listed` 一项一行：它在号列表里的行号（格照这一行画）· 号牌 · 这台账号库里有没有这个号（没有 ⇒ 头一句照实说）。
fn board_of(
    form: &[form::Cell],
    settings: &Value,
    listed: &[(usize, &RotAccount, bool)],
) -> RotBoard {
    let of = |scope: form::Scope, row: Option<usize>| -> Vec<RotCell> {
        form.iter()
            .filter(|c| c.scope == scope && form::shown(form, c, settings, row))
            .flat_map(|c| cells_of(c, settings, row))
            .collect()
    };
    // 号名旁的记号：那一行每个几选一此刻取的那一项带的记号。
    let marks = |row: usize| -> Vec<RotMark> {
        form.iter()
            .filter(|c| c.scope == form::Scope::Each && c.kind == form::Kind::OneOf)
            .filter_map(|c| {
                let v = form::value_of(c, settings, Some(row)).0;
                let o = c.choices.iter().find(|o| v == o.v.as_str())?;
                let m = o.mark.as_ref()?;
                Some(RotMark {
                    glyph: m.glyph.clone(),
                    dashed: m.dashed,
                    tip: copy_text(
                        "rot.mark.tip",
                        &[("label", &text_of(&c.label)), ("v", &text_of(&o.label))],
                    ),
                })
            })
            .collect()
    };
    let pct = |w: &crate::accounts::quota::facts::WinFact| format!("{}", w.used.round());
    RotBoard {
        all: of(form::Scope::All, None),
        rows: listed
            .iter()
            .map(|&(i, RotAccount { fact: f, .. }, here)| {
                let (u5, u7) = (pct(&f.w5h), pct(&f.w7d));
                let mut line = copy_text("rot.acct.line", &[("u5", &u5), ("u7", &u7)]);
                if let Some(at) = &f.w5h.resets_text {
                    line.push_str(&copy_text("kit.text.sep", &[]));
                    line.push_str(&copy_text("rot.acct.reset", &[("at", at)]));
                }
                let hover = copy_text("rot.card.hover", &[("u5", &u5), ("u7", &u7)]);
                let (line, hover) = if here {
                    (line, hover)
                } else {
                    let missing = copy_text("rot.acct.missing", &[]);
                    (missing.clone(), missing)
                };
                let cells = of(form::Scope::Each, Some(i));
                RotRow {
                    account: f.account.clone(),
                    row: i,
                    line,
                    hover,
                    rules: cells.iter().map(rule_of).collect(),
                    cells,
                    marks: marks(i),
                }
            })
            .collect(),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/rotation_view_face_tests.rs"]
pub(crate) mod tests;
