//! 帧面 `rotation-strategies` · `rotation-view`（typed，`轮换-v5-分层.md` §6.6 第 10 条）：轮换的两条读。
//! 都只回成品：策略目录里每一份（名字 · 灰字 · 版本 · 读不懂的原因 · 哪几个会话在用）；一个会话（或设置页上的一份策略）此刻的
//! 设置 · 声明 · 每个号的事实 · 这一判 · 推演 · 提示。界面照画，不判、不排、不算时刻。

use crate::accounts::quota::rules::book::{self, Uses};
use crate::accounts::quota::rules::decide::Pick;
use crate::accounts::quota::rules::{form, past, script, shelf, simulate};
use crate::accounts::quota::strategy::FailWhy;
use crate::accounts::upstream_select::turn::{self, Loaded};
use crate::faces::rotation_face::Ctx;
use crate::faces::rotation_try_face::{decision_of, timeline_of};
use crate::wire::typed::Command;
use serde::Deserialize;
use serde_json::Value;
use crate::common::cells::Words;
pub(crate) use cmd_wire::rotation::{
    RotAccount, RotBadCell, RotHint, RotLogRow, RotNote, RotOldFile, RotPast, RotPastLane, RotPastSeg, RotPastUse,
    RotRefused, RotStrategyError, RotStrategyItem, RotationStrategiesReply, RotationViewReply,
};

/// `rotation-strategies` 的入参。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationStrategiesReq {}

crate::wire::typed::codes! {
    /// `rotation-strategies` 自己的码（只有入参不对那一种）。
    pub(crate) enum RotationStrategiesErr {}
}

/// `rotation-strategies`：读策略目录与 `rotation.json` ⇒ 阻塞档。
pub(crate) struct RotationStrategies;

impl Command for RotationStrategies {
    const NAME: &'static str = "rotation-strategies";
    type Req = RotationStrategiesReq;
    type Reply = RotationStrategiesReply;
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
    book: &book::Book,
    live: &dyn Fn(&str) -> bool,
) -> RotStrategyItem {
    let head = s.file.as_ref().ok().map(|f| &f.head);
    RotStrategyItem {
        id: s.id.clone(),
        description: s
            .file
            .as_ref()
            .map_or_else(|_| String::new(), |f| f.about.clone()),
        rev: s.rev.clone(),
        error: s.file.as_ref().err().map(error_of),
        in_use: book
            .in_use(&s.id)
            .into_iter()
            .filter(|sid| live(sid))
            .collect(),
        for_new: book.for_new == s.id,
        words_bad: head.is_some_and(|h| form::words_bad(&h.form)),
        tz_unknown: head.is_some_and(|h| h.tz.is_none() || h.tz_unknown),
    }
}

/// 本体（测试给假的账本 · 判活）。
pub(crate) fn strategies_with(
    ctx: &Ctx,
    _req: &RotationStrategiesReq,
    live: &dyn Fn(&str) -> bool,
) -> RotationStrategiesReply {
    let book = ctx.hop.store.now();
    let dir = ctx.hop.store.strategy_dir();
    RotationStrategiesReply {
        strategies: shelf::all(dir.as_deref())
            .iter()
            .map(|s| item_of(s, &book, live))
            .collect(),
        for_new: book.for_new.clone(),
        old_scripts: shelf::old_count(dir.as_deref()),
    }
}

/// 看哪一份：一个会话（会话面板）· 一份策略（设置页；`settings` ＝ 界面手上的草稿，缺 ⇒ 文件里的）。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) enum RotTarget {
    Session {
        sid: String,
    },
    Strategy {
        strategy: String,
        #[serde(default)]
        #[cfg_attr(test, ts(optional, type = "unknown"))]
        settings: Option<Value>,
        /// `settings` 是照哪个版本改出来的（缺 ⇒ 不比）：跟文件此刻的不同 ⇒ 提示「别处被改了」。
        #[serde(default)]
        #[cfg_attr(test, ts(optional))]
        rev: Option<String>,
    },
}

/// `rotation-view` 的入参。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationViewReq {
    pub(crate) target: RotTarget,
    /// 往后推几小时（1–48，缺 ⇒ 12）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) hours: Option<u8>,
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
        RotHint::Newer => copy_core::copy_text("beRotation.hint.newer", &[]),
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
    RotNote { hint, text, detail }
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

crate::wire::typed::codes! {
    /// `rotation-view` 自己的码。
    pub(crate) enum RotationViewErr {
        /// 中转没见过这个会话。
        NoSession = "no_session",
        /// 策略目录里没有那一份。
        NoStrategy = "no_strategy",
        /// 交来的设置不合声明：`data.cells` 是哪几格。
        Refused = "refused" => RotRefused,
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
    const NAME: &'static str = "rotation-view";
    type Req = RotationViewReq;
    type Reply = RotationViewReply;
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
        }
    }
}

/// 一刻的钟面（`HH:MM`，看的那一台的时区）。
fn face(t: u64, tz: &crate::Tz) -> String {
    crate::common::time::hm(tz.local(i64::try_from(t).unwrap_or(i64::MAX)))
}

/// 过去那一截往前推演时长的几分之一（12 小时 ⇒ 往前 2 小时，同稿的比例）。
const PAST_SHARE: u64 = 6;

/// 时间轴过去那一截：最上那条照这个会话的换号记录；每号一行照此刻的读数与记录，说不出的空着（[`past`]）。
fn past_of(
    session: &book::Session,
    facts: &[crate::accounts::quota::rules::Acct],
    lanes: &[String],
    holding: bool,
    from: u64,
    now: u64,
    tz: &crate::Tz,
) -> RotPast {
    RotPast {
        from,
        uses: past::uses(
            &session.log,
            &session.current,
            session.since,
            from,
            now,
            holding,
        )
        .into_iter()
        .map(|u| RotPastUse {
            from: u.from,
            to: u.to,
            account: u.account,
        })
        .collect(),
        lanes: lanes
            .iter()
            .filter_map(|id| facts.iter().find(|a| a.id == *id))
            .map(|a| RotPastLane {
                account: a.id.clone(),
                segs: past::lane(a, &session.log, from, now)
                    .into_iter()
                    .map(|g| RotPastSeg {
                        from: g.from,
                        to: g.to,
                        sendable: g.sendable,
                        reset: g.reset,
                        from_text: face(g.from, tz),
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// 本体（测试给假的账本 · 额度账 · 钟 · 判活）。
pub(crate) fn view_with(
    ctx: &Ctx,
    req: &RotationViewReq,
    now: u64,
    tz: &crate::Tz,
    live: &dyn Fn(&str) -> bool,
) -> Result<RotationViewReply, RotationViewErr> {
    let book = ctx.hop.store.now();
    let dir = ctx.hop.store.strategy_dir();
    let (session, agent) = match &req.target {
        RotTarget::Session { sid } => {
            let s = book.sessions.get(sid).cloned().ok_or_else(|| {
                RotationViewErr::NoSession(copy_core::copy_text("beRotation.session.gone", &[]))
            })?;
            let agent = s.agent.clone();
            (s, agent)
        }
        RotTarget::Strategy {
            strategy, settings, ..
        } => {
            let all = shelf::all(dir.as_deref());
            let shelved = shelf::find(&all, strategy).ok_or_else(|| {
                RotationViewErr::NoStrategy(copy_core::copy_text(
                    "beRotation.strategy.gone",
                    &[("name", strategy)],
                ))
            })?;
            if let Some(s) = settings {
                checked(shelved.file.as_ref().ok(), s).map_err(|r| {
                    RotationViewErr::Refused(
                        crate::common::contract::malformed("settings do not fit the form"),
                        r,
                    )
                })?;
            }
            let zero = crate::accounts::manage::model::ACCOUNT_ZERO;
            let mut s = book::Session::fresh(
                crate::faces::rotation_face::LIBRARY_AGENT,
                zero,
                now,
                strategy,
            );
            s.strategy = Uses {
                id: strategy.clone(),
                rev: shelved.rev.clone(),
                draft: settings.clone(),
            };
            (s, crate::faces::rotation_face::LIBRARY_AGENT.to_string())
        }
    };
    if session.strategy.id.is_empty() {
        let mut hints = Vec::new();
        if let Some(e) = ctx.hop.store.unreadable() {
            hints.push(note_of(RotHint::BookUnreadable { text: e.said }, "", FailWhy::Other));
        }
        return Ok(RotationViewReply {
            now,
            now_text: face(now, tz),
            strategy: None,
            form: Vec::new(),
            settings: Value::Object(Default::default()),
            dirty: false,
            start: session.start.clone(),
            current: session.current.clone(),
            accounts: Vec::new(),
            listed: 0,
            decision: None,
            timeline: None,
            past: None,
            base_rev: String::new(),
            log: log_rows(&session.log),
            hints,
        });
    }
    let loaded: Loaded = turn::load_in(dir.as_deref(), &session.strategy);
    let row = ctx.row(&agent);
    let facts = ctx.hop.accts(&session, &loaded.settings, &row, now);
    let d = ctx.hop.judge_session(&session, &loaded, &facts, now, &[]);
    let hours = u64::from(req.hours.unwrap_or(12).clamp(1, 48));
    let budget = crate::accounts::quota::rules::script::Budgeted::new(&loaded);
    let stz = loaded.tz.clone();
    let tl = simulate::simulate(
        &turn::sigma_of(&session),
        &facts,
        &loaded.settings,
        &budget,
        &|t| crate::accounts::quota::rules::clock_at(&stz, t),
        now,
        now + hours * 3600,
    );
    let item = match &loaded.shelved {
        Some(s) => item_of(s, &book, live),
        None => RotStrategyItem {
            id: loaded.id.clone(),
            description: String::new(),
            rev: String::new(),
            error: loaded.broken().as_ref().map(error_of),
            in_use: Vec::new(),
            for_new: book.for_new == loaded.id,
            words_bad: false,
            tz_unknown: false,
        },
    };
    let head = loaded.file().map(|f| &f.head);
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
            path: loaded
                .shelved
                .as_ref()
                .and(dir.as_deref())
                .map(|d| shelf::path_of(d, &loaded.id).display().to_string()),
        });
    }
    if item.tz_unknown {
        hints.push(RotHint::TzUnknown {
            zone: tz.viewer_zone().unwrap_or_else(|| "UTC".into()),
        });
    }
    if item.words_bad {
        hints.push(RotHint::WordsBad);
    }
    if let Some(e) = ctx.hop.store.unreadable() {
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
    let listed_ids: Vec<String> = facts.iter().map(|a| a.id.clone()).collect();
    let listed = loaded
        .settings
        .get(form::ACCOUNTS)
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
        .min(listed_ids.len());
    let lib = ctx.hop.library();
    let colors = crate::observe::accounts_query::account_colors();
    let accounts = listed_ids
        .iter()
        .map(|id| {
            let fact = all_facts
                .iter()
                .find(|f| f.account == *id)
                .cloned()
                .unwrap_or_else(|| ctx.hop.fact_one(&agent, &lib, id, &row, now, tz));
            RotAccount {
                color: colors
                    .get(id)
                    .copied()
                    .unwrap_or_else(|| crate::accounts::manage::model::color_fallback(id)),
                label: Words(crate::accounts::quota::name_words::account_text(id)),
                fact,
            }
        })
        .collect();
    let dirty = match &req.target {
        RotTarget::Session { .. } => session.strategy.draft.is_some(),
        RotTarget::Strategy { settings, .. } => {
            settings.is_some() && head.map(|h| &h.settings) != settings.as_ref()
        }
    };
    // 显示的这份设置照哪个版本：会话有草稿 ⇒ 草稿基于的；设置页交了草稿与它的版本 ⇒ 那个；否则文件此刻的。
    let base_rev = match &req.target {
        RotTarget::Session { .. }
            if session.strategy.draft.is_some() && !session.strategy.rev.is_empty() =>
        {
            session.strategy.rev.clone()
        }
        RotTarget::Strategy {
            settings: Some(_),
            rev: Some(r),
            ..
        } => r.clone(),
        _ => item.rev.clone(),
    };
    if !item.rev.is_empty() && base_rev != item.rev {
        hints.push(RotHint::Newer);
    }
    let past = match &req.target {
        RotTarget::Session { .. } => {
            let lanes: Vec<String> = tl.lanes.iter().map(|(a, _)| a.clone()).collect();
            let from = now.saturating_sub(hours * 3600 / PAST_SHARE);
            Some(past_of(
                &session,
                &facts,
                &lanes,
                matches!(d.pick, Pick::Hold { .. }),
                from,
                now,
                tz,
            ))
        }
        RotTarget::Strategy { .. } => None,
    };
    let name = item.id.clone();
    Ok(RotationViewReply {
        now,
        now_text: face(now, tz),
        strategy: Some(item),
        form: head.map(|h| h.form.clone()).unwrap_or_default(),
        settings: loaded.settings.clone(),
        dirty,
        start: session.start.clone(),
        current: session.current.clone(),
        accounts,
        listed,
        decision: Some(decision_of(&d)),
        timeline: Some(timeline_of(
            &tl,
            past.as_ref().map_or(now, |p| p.from),
            now,
            now + hours * 3600,
            tz,
        )),
        past,
        base_rev,
        log: match &req.target {
            RotTarget::Session { .. } => log_rows(&session.log),
            RotTarget::Strategy { .. } => Vec::new(),
        },
        hints: hints.into_iter().map(|h| note_of(h, &name, why)).collect(),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/rotation_view_face_tests.rs"]
pub(crate) mod tests;
