//! 帧面 `rotation-try`（typed）：拿一份策略文件的原文（＋ 可选的设置 · 会话状态 · 时刻）照此刻的事实判一次、往后推演，不写盘不换号。
//! skill 试跑与测试台用它。`轮换-v5-分层.md` §1.9。

use crate::accounts::quota::rules::decide::{self, Pick, Sigma};
use crate::accounts::quota::rules::script;
use crate::accounts::quota::rules::simulate;
use crate::accounts::quota::rules::Acct;
use crate::wire::typed::Command;

/// `rotation-try` 的入参。
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationTryReq {
    /// 一份策略文件的原文（头 ＋ 代码）。
    pub(crate) source: String,
    /// 换掉头里的设置（形状由这份策略自己定；缺 ⇒ 用头里的）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    #[cfg_attr(test, ts(type = "unknown"))]
    pub(crate) settings: Option<serde_json::Value>,
    /// 开会话用的号（缺 ⇒ 事实里第一个号）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) start: Option<String>,
    /// 在用的号（缺 ⇒ 开会话用的号）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) current: Option<String>,
    /// 临时条目。
    #[serde(default)]
    pub(crate) temp: Vec<String>,
    /// 往后推几小时（1–48，缺 ⇒ 12）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) hours: Option<u8>,
    /// 试跑「现在用这个号」：事实上发得出去 ⇒ 判的是换过去、放进拖出来的顺序最前之后的那一判；发不出去 ⇒ `useNowRefused` 带几点恢复。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) use_now: Option<String>,
    /// 哪一家（缺 ⇒ 有账号库的那一家）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) agent: Option<String>,
}

/// 策略说的一个号：一句为什么。
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotTryRow {
    pub(crate) account: String,
    pub(crate) why: String,
}

/// 这一判。
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotTryDecision {
    /// 用哪个号（停发 ⇒ 缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) pick: Option<String>,
    /// 停发到几点 · 到时恢复的号（停发才有）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) hold_until: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) hold_account: Option<String>,
    /// 策略交的先后（此刻愿意用的号）。
    pub(crate) order: Vec<String>,
    /// 每个号一句（策略写的）。
    pub(crate) why: Vec<RotTryRow>,
    /// 这一号来自用户拖出来的顺序。
    pub(crate) from_temp: bool,
    pub(crate) skipped: Vec<String>,
    pub(crate) temp: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) next_node: Option<u64>,
    /// 策略这一判出错（走了兜底）：原话。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) strategy_error: Option<String>,
}

/// 「用哪个号」那一条的一段。
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotTryUse {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) from: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) to: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) account: Option<String>,
    pub(crate) from_temp: bool,
    /// 在用的号用到上限的时刻还不知道（额度账里量不到它的用速）：这一段只排确定的事件。
    pub(crate) cap_unknown: bool,
    pub(crate) skipped: Vec<String>,
}

/// 每个号那一行的一段。
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotTryLaneSeg {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) from: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) to: u64,
    /// 事实上发得出去吗（被拒 · 登录 · key · 超额 · 用到 100 都算发不出去）。
    pub(crate) sendable: bool,
    /// 这一段开头两窗的用量（5 小时 · 7 天）。
    pub(crate) used: [f64; 2],
    pub(crate) reset: bool,
    /// 这一段起点的钟面（`HH:MM`，看的那一台的时区；重置那段写成 `↻18:30`，字由界面照文案表配）。
    pub(crate) from_text: String,
    /// 这一段开头剩多少（事实：100 − 两窗里用得多的那个，不低于 0）。
    pub(crate) left: f64,
}

/// 一个号那一行。
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotTryLane {
    pub(crate) account: String,
    pub(crate) segs: Vec<RotTryLaneSeg>,
}

/// 一次换号。
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotTrySwitch {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) at: u64,
    /// `at` 的钟面（`HH:MM`）。
    pub(crate) at_text: String,
    pub(crate) account: String,
}

/// 时间轴上的一格刻度：整点 · 钟面。
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotTryTick {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) at: u64,
    pub(crate) text: String,
}

/// 推演。
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotTryTimeline {
    /// 将来每次换到别的号（时间轴上方那几个「16:07 work」）。
    pub(crate) switches: Vec<RotTrySwitch>,
    pub(crate) uses: Vec<RotTryUse>,
    pub(crate) lanes: Vec<RotTryLane>,
    pub(crate) truncated: bool,
    /// 此刻之后的整点刻度（推 12 小时以内每 2 小时、更长每 4 小时，看的那一台的钟）。
    pub(crate) ticks: Vec<RotTryTick>,
}

/// `rotation-try` 的应答。
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationTryReply {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) now: u64,
    /// 这份策略的说明（原文开头那段 `//` 注释，一行一行 `\n` 接起来、空行分段）。
    pub(crate) description: String,
    /// 给了 `settings` ⇒ 换上这份设置以后的整份原文（头重写、代码原样），存成文件就是它；没给 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) source: Option<String>,
    /// 试跑「现在用」碰上用不了：几点恢复（说不出 ⇒ `null`）；这时 `decision` 是不换的那一判。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number | null"))]
    pub(crate) use_now_refused: Option<Option<u64>>,
    pub(crate) decision: RotTryDecision,
    pub(crate) timeline: RotTryTimeline,
}

crate::wire::typed::codes! {
    /// `rotation-try` 自己的码。
    pub(crate) enum RotationTryErr {
        /// 那份原文读不懂（头的形状 · 编不过 · 没有 `score`）。
        BadStrategy = "bad_strategy",
    }
}

/// `rotation-try`：读额度账与账号库、在沙箱里跑策略 ⇒ 阻塞档。
pub(crate) struct RotationTry;

impl Command for RotationTry {
    const NAME: &'static str = "rotation-try";
    type Req = RotationTryReq;
    type Reply = RotationTryReply;
    type Err = RotationTryErr;
}

impl crate::wire::typed::Handle for RotationTry {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: RotationTryReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotationTryReply, RotationTryErr>> + Send + 'static
    {
        let (cx, ()) = cx.split();
        async move {
            let cx = &cx;

            let ctx = crate::faces::rotation_face::Ctx::here();
            let agent = req
                .agent
                .as_deref()
                .unwrap_or(crate::faces::rotation_face::LIBRARY_AGENT);
            let row = ctx.row(agent);
            let now = crate::accounts::quota::now_unix();
            let facts: Vec<Acct> = ctx
                .hop
                .facts(agent, &row, now, &cx.tz)
                .iter()
                .map(Acct::of_fact)
                .collect();
            try_with(&req, &facts, now, &cx.tz)
        }
    }
}

/// 本体（测试给假的事实与钟）。`tz` ＝ 看的那一台的时区（时刻的字照它写）。
pub(crate) fn try_with(
    req: &RotationTryReq,
    facts: &[Acct],
    now: u64,
    tz: &crate::Tz,
) -> Result<RotationTryReply, RotationTryErr> {
    let file = script::parse(&req.source).map_err(|f| {
        let at = f.line.map(|l| format!("line {l}: ")).unwrap_or_default();
        RotationTryErr::BadStrategy(crate::common::contract::malformed(&format!(
            "{at}{}",
            f.msg
        )))
    })?;
    // 分时段照策略头里的时区算；时刻的字照看的那一台（`tz`）写。
    let rule_tz = crate::accounts::quota::rules::tz_of(Some(&file.head));
    let clock_of = |t: u64| crate::accounts::quota::rules::clock_at(&rule_tz, t);
    let settings = req
        .settings
        .clone()
        .unwrap_or_else(|| file.head.settings.clone());
    let start = req
        .start
        .clone()
        .or_else(|| facts.first().map(|f| f.id.clone()))
        .unwrap_or_default();
    let mut sigma = Sigma {
        current: req.current.clone().unwrap_or_else(|| start.clone()),
        start,
        temp: req.temp.clone(),
        ..Sigma::default()
    };
    let mut use_now_refused = None;
    if let Some(a) = req.use_now.as_deref() {
        match decide::use_now(&sigma, facts, now, a) {
            Ok(s) => sigma = s,
            Err(at) => use_now_refused = Some(at),
        }
    }
    let d = decide::turn(&sigma, facts, &settings, &file, clock_of(now), now, &[]);
    let hours = u64::from(req.hours.unwrap_or(12).clamp(1, 48));
    let budget = script::Budgeted::new(&file);
    let tl = simulate::simulate(
        &sigma,
        facts,
        &settings,
        &budget,
        &clock_of,
        now,
        now + hours * 3600,
    );
    let source = match &req.settings {
        Some(s) => {
            let mut head = file.head.clone();
            head.settings = s.clone();
            Some(
                script::with_head(&req.source, &head)
                    .map_err(|f| RotationTryErr::BadStrategy(f.msg))?,
            )
        }
        None => None,
    };
    Ok(RotationTryReply {
        now,
        description: file.about.clone(),
        source,
        use_now_refused,
        decision: decision_of(&d),
        timeline: timeline_of(&tl, now, now, now + hours * 3600, tz),
    })
}

/// 一判写成线上的形（`rotation-try` · `rotation-view` 同用）。
pub(crate) fn decision_of(d: &decide::Decision) -> RotTryDecision {
    let (pick, hold_until, hold_account) = match &d.pick {
        Pick::Use(a) => (Some(a.clone()), None, None),
        Pick::Hold { until, account } => (None, *until, account.clone()),
    };
    RotTryDecision {
        pick,
        hold_until,
        hold_account,
        order: d.order.clone(),
        why: d
            .why
            .iter()
            .map(|(a, w)| RotTryRow {
                account: a.clone(),
                why: w.clone(),
            })
            .collect(),
        from_temp: d.from_temp,
        skipped: d.skipped.clone(),
        temp: d.sigma.temp.clone(),
        next_node: d.tn,
        strategy_error: d.strategy_error.as_ref().map(|e| e.msg.clone()),
    }
}

/// 推演写成线上的形（`rotation-try` · `rotation-view` 同用）。
/// `[now, end)` 是推演的那一截；`tz` 是看的那一台的时区（钟面都按它写）。
/// 刻度从 `ticks_from` 起（有过去那一截 ⇒ 它的开头；否则此刻），步长照往后推多久定。
pub(crate) fn timeline_of(
    tl: &simulate::Timeline,
    ticks_from: u64,
    now: u64,
    end: u64,
    tz: &crate::Tz,
) -> RotTryTimeline {
    let face = |t: u64| crate::common::time::hm(tz.local(i64::try_from(t).unwrap_or(i64::MAX)));
    RotTryTimeline {
        switches: tl
            .switches()
            .into_iter()
            .map(|(at, account)| RotTrySwitch {
                at,
                at_text: face(at),
                account,
            })
            .collect(),
        uses: tl
            .uses
            .iter()
            .map(|u| RotTryUse {
                from: u.from,
                to: u.to,
                account: match &u.pick {
                    Pick::Use(a) => Some(a.clone()),
                    Pick::Hold { .. } => None,
                },
                from_temp: u.from_temp,
                cap_unknown: u.cap_unknown,
                skipped: u.skipped.clone(),
            })
            .collect(),
        lanes: tl
            .lanes
            .iter()
            .map(|(a, segs)| RotTryLane {
                account: a.clone(),
                segs: segs
                    .iter()
                    .map(|s| RotTryLaneSeg {
                        from: s.from,
                        to: s.to,
                        sendable: s.sendable,
                        used: s.used,
                        reset: s.reset,
                        from_text: face(s.from),
                        left: (100.0 - s.used[0].max(s.used[1])).max(0.0),
                    })
                    .collect(),
            })
            .collect(),
        truncated: tl.truncated,
        ticks: ticks_of(ticks_from, end, end.saturating_sub(now) <= 12 * 3600, tz)
            .into_iter()
            .map(|at| RotTryTick { at, text: face(at) })
            .collect(),
    }
}

/// 刻度落在哪几刻：`from` 之后、`end` 以内，看的那一台的整点；推 12 小时以内（`short`）每 2 小时，更长每 4 小时。
fn ticks_of(from: u64, end: u64, short: bool, tz: &crate::Tz) -> Vec<u64> {
    let step: i64 = if short { 2 } else { 4 };
    let mut out = Vec::new();
    let Ok(mut t) = i64::try_from(from) else {
        return out;
    };
    let end = i64::try_from(end).unwrap_or(i64::MAX);
    // 往后找第一个本地整点、且本地钟点是 step 的倍数的那一刻；之后每 step 小时一格（跨夏令时照那一刻的本地钟重对齐）。
    t = t - tz.local(t).rem_euclid(3600) + 3600;
    while t <= end {
        if (tz.local(t).rem_euclid(86_400) / 3600) % step == 0 {
            out.push(u64::try_from(t).unwrap_or(0));
            t += step * 3600;
        } else {
            t += 3600;
        }
    }
    out
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/rotation_try_face_tests.rs"]
mod tests;
