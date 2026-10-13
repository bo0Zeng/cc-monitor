//! 帧面 `rotation-strategies` · `rotation-view`（typed，`轮换-v5-分层.md` §6.6 第 10 条）：轮换的两条读。
//! 都只回成品：策略目录里每一份（名字 · 灰字 · 版本 · 读不懂的原因 · 哪几个会话在用）；一个会话（或设置页上的一份策略）此刻的
//! 设置 · 声明 · 每个号的事实 · 这一判 · 推演 · 提示。界面照画，不判、不排、不算时刻。

use crate::accounts::quota::facts::Fact;
use crate::accounts::quota::rules::book::{self, Uses};
use crate::accounts::quota::rules::decide::Pick;
use crate::accounts::quota::rules::{form, script, shelf, simulate};
use crate::accounts::upstream_select::turn::{self, Loaded};
use crate::faces::rotation_face::Ctx;
use crate::faces::rotation_try_face::{decision_of, timeline_of, RotTryDecision, RotTryTimeline};
use crate::stream::inbound::typed::{Blocking, Command, Cx};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 读不懂的那一份为什么：第几行（说得出才有）· 原话。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotStrategyError {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) line: Option<usize>,
    pub(crate) msg: String,
}

impl RotStrategyError {
    pub(crate) fn of(f: &crate::accounts::quota::strategy::Fail) -> Self {
        Self {
            line: f.line,
            msg: f.msg.clone(),
        }
    }
}

/// 策略目录里的一份。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotStrategyItem {
    pub(crate) id: String,
    /// 说明（文件开头那段 `//` 注释，一行一行 `\n` 接起来、空行分段；读不懂 ⇒ 空）。标题就是 `id`（文件名，§2.1）。
    pub(crate) description: String,
    pub(crate) rev: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) error: Option<RotStrategyError>,
    /// 此刻还活着、用着它的会话。
    pub(crate) in_use: Vec<String>,
    pub(crate) for_new: bool,
    /// 这份的设置说明写法不合文案规矩（设置照样能用）。
    pub(crate) words_bad: bool,
    /// 文件头里没写时区（分时段按 UTC 算）。
    pub(crate) tz_unknown: bool,
}

/// `rotation-strategies` 的入参。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationStrategiesReq {}

/// `rotation-strategies` 的应答。
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationStrategiesReply {
    /// 照 id 排。
    pub(crate) strategies: Vec<RotStrategyItem>,
    pub(crate) for_new: String,
    /// `strategies/v3/` 里没有加载的第三版脚本几份。
    pub(crate) old_scripts: usize,
}

crate::stream::inbound::typed::codes! {
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

impl Blocking for RotationStrategies {
    fn run(
        req: RotationStrategiesReq,
        _cx: &Cx,
    ) -> Result<RotationStrategiesReply, RotationStrategiesErr> {
        let ctx = Ctx::here();
        let live = (ctx.hop.live)();
        Ok(strategies_with(&ctx, &req, &|sid| live.contains(sid)))
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
        error: s.file.as_ref().err().map(RotStrategyError::of),
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

/// 提示（第一行下面那几条）：停发 · 策略出错 · 时区不明 · 旧格式没加载 · 说明写法不合规。字由第 4 刀照文案表写。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "k", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) enum RotHint {
    /// 停发：到几点 · 到时恢复的号。
    Hold {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional, type = "number"))]
        until: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        account: Option<String>,
        /// `until` 的钟面（`HH:MM`，看的那一台的时区；不在今天带日期）。
        #[serde(rename = "untilText", skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        until_text: Option<String>,
    },
    /// 策略出错（这一判走了兜底：此刻的号发得出去就照用，否则照号列表的先后）。
    StrategyError { error: RotStrategyError },
    /// 文件头里没写时区：分时段按 UTC 算。
    TzUnknown,
    /// `strategies/v3/` 里有没加载的第三版脚本。
    OldScripts { n: usize },
    /// 这份策略的设置说明写法不合文案规矩。
    WordsBad,
    /// `rotation.json` 读不懂（照上一份读懂的接着用，盘上那份不动）：那一句。
    BookUnreadable { text: String },
}

/// 一条提示：哪一种（界面照它挑颜色）· 写好的那一句（停发那一句里 `{dur}` 是「还有多久」，界面在画的那一刻照 `now` · `until` 填）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotNote {
    pub(crate) hint: RotHint,
    pub(crate) text: String,
}

/// 一条换号记录 ＋ 为什么那一句（策略写的；没写 ⇒ 后端看得见的原因）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotLogRow {
    #[serde(flatten)]
    #[cfg_attr(test, ts(flatten))]
    pub(crate) log: book::Log,
    pub(crate) because: String,
}

/// 提示写成一句（字照文案表）。`name` ＝ 那份策略的标题。
fn note_of(hint: RotHint, name: &str) -> RotNote {
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
        RotHint::StrategyError { error } => match error.line {
            Some(l) => copy_core::copy_text(
                "beRotation.hint.errorLine",
                &[
                    ("name", name),
                    ("line", &l.to_string()),
                    ("msg", &error.msg),
                ],
            ),
            None => copy_core::copy_text(
                "beRotation.hint.error",
                &[("name", name), ("msg", &error.msg)],
            ),
        },
        RotHint::TzUnknown => copy_core::copy_text("beRotation.hint.tz", &[]),
        RotHint::OldScripts { n } => {
            copy_core::copy_text("beRotation.hint.old", &[("n", &n.to_string())])
        }
        RotHint::WordsBad => copy_core::copy_text("beRotation.hint.words", &[]),
        RotHint::BookUnreadable { text } => text.clone(),
    };
    RotNote { hint, text }
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

/// `rotation-view` 的应答。
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationViewReply {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) now: u64,
    /// `now` 的钟面（`HH:MM`，看的那一台的时区）。
    pub(crate) now_text: String,
    /// 用的那份策略；会话不轮换（没选策略）⇒ `null`，这时 `decision` · `timeline` 也是 `null`、只用开会话的号。
    pub(crate) strategy: Option<RotStrategyItem>,
    /// 设置怎么画（策略的声明）。
    pub(crate) form: Vec<form::Cell>,
    /// 生效的设置（会话有草稿 ⇒ 草稿）。
    #[cfg_attr(test, ts(type = "unknown"))]
    pub(crate) settings: Value,
    /// 跟文件里的不同（会话 ⇒ 有草稿；设置页 ⇒ 交来的跟文件里的不同）。
    pub(crate) dirty: bool,
    /// 开会话用的号 · 此刻在用的号。
    pub(crate) start: String,
    pub(crate) current: String,
    /// 每个号的事实，照号列表的先后（列表里没有的接在后面）。
    pub(crate) accounts: Vec<Fact>,
    /// 号列表里有几个（`accounts` 前这么多个是列表里的）。
    pub(crate) listed: usize,
    pub(crate) decision: Option<RotTryDecision>,
    pub(crate) timeline: Option<RotTryTimeline>,
    /// 换号记录（会话才有）。
    pub(crate) log: Vec<RotLogRow>,
    pub(crate) hints: Vec<RotNote>,
}

crate::stream::inbound::typed::codes! {
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

/// 收不下的那几格（`rotation-view` · `rotation-settings` · `rotation-save` 同用）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotRefused {
    pub(crate) cells: Vec<RotBadCell>,
}

/// 一格收不下：哪一格 · 短码（`type` · `range` · `of` · `unknown` · `dup` · `account` · `overlap` · `order`）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotBadCell {
    pub(crate) cell: String,
    pub(crate) code: String,
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

impl Blocking for RotationView {
    fn run(req: RotationViewReq, _cx: &Cx) -> Result<RotationViewReply, RotationViewErr> {
        let ctx = Ctx::here();
        let live = (ctx.hop.live)();
        view_with(&ctx, &req, crate::accounts::quota::now_unix(), &|sid| {
            live.contains(sid)
        })
    }
}

/// 此刻的钟面（看的那一台的时区）。
fn now_face(now: u64) -> String {
    crate::common::time::hm(crate::Tz::default().local(i64::try_from(now).unwrap_or(i64::MAX)))
}

/// 本体（测试给假的账本 · 额度账 · 钟 · 判活）。
pub(crate) fn view_with(
    ctx: &Ctx,
    req: &RotationViewReq,
    now: u64,
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
        RotTarget::Strategy { strategy, settings } => {
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
            hints.push(note_of(RotHint::BookUnreadable { text: e.said }, ""));
        }
        return Ok(RotationViewReply {
            now,
            now_text: now_face(now),
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
    let tz = loaded.tz.clone();
    let tl = simulate::simulate(
        &turn::sigma_of(&session),
        &facts,
        &loaded.settings,
        &budget,
        &|t| crate::accounts::quota::rules::clock_at(&tz, t),
        now,
        now + hours * 3600,
    );
    let item = match &loaded.shelved {
        Some(s) => item_of(s, &book, live),
        None => RotStrategyItem {
            id: loaded.id.clone(),
            description: String::new(),
            rev: String::new(),
            error: loaded.broken().as_ref().map(RotStrategyError::of),
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
                    &crate::Tz::default(),
                )
            }),
        });
    }
    if let Some(e) = &d.strategy_error {
        hints.push(RotHint::StrategyError {
            error: RotStrategyError::of(e),
        });
    }
    if item.tz_unknown {
        hints.push(RotHint::TzUnknown);
    }
    if item.words_bad {
        hints.push(RotHint::WordsBad);
    }
    if let Some(e) = ctx.hop.store.unreadable() {
        hints.push(RotHint::BookUnreadable { text: e.said });
    }
    let old = shelf::old_count(dir.as_deref());
    if old > 0 {
        hints.push(RotHint::OldScripts { n: old });
    }
    let all_facts = ctx.hop.facts(&agent, &row, now, &crate::Tz::default());
    let listed_ids: Vec<String> = facts.iter().map(|a| a.id.clone()).collect();
    let listed = loaded
        .settings
        .get(form::ACCOUNTS)
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
        .min(listed_ids.len());
    let lib = ctx.hop.library();
    let accounts = listed_ids
        .iter()
        .map(|id| {
            all_facts
                .iter()
                .find(|f| f.account == *id)
                .cloned()
                .unwrap_or_else(|| {
                    ctx.hop
                        .fact_one(&agent, &lib, id, &row, now, &crate::Tz::default())
                })
        })
        .collect();
    let dirty = match &req.target {
        RotTarget::Session { .. } => session.strategy.draft.is_some(),
        RotTarget::Strategy { settings, .. } => {
            settings.is_some() && head.map(|h| &h.settings) != settings.as_ref()
        }
    };
    let name = item.id.clone();
    Ok(RotationViewReply {
        now,
        now_text: now_face(now),
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
            now,
            now + hours * 3600,
            &crate::Tz::default(),
        )),
        log: match &req.target {
            RotTarget::Session { .. } => log_rows(&session.log),
            RotTarget::Strategy { .. } => Vec::new(),
        },
        hints: hints.into_iter().map(|h| note_of(h, &name)).collect(),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/rotation_view_face_tests.rs"]
pub(crate) mod tests;
