//! 帧面 `rotation-try`（typed）：拿一份策略文件的原文（＋ 可选的设置 · 会话状态 · 时刻）照此刻的事实判一次、往后推演，不写盘不换号。
//! skill 试跑与测试台用它。`轮换-v5-分层.md` §1.9。

use crate::accounts::quota::rules::decide::{self, Cause, Pick, Sigma};
use crate::accounts::quota::rules::script;
use crate::accounts::quota::rules::settings::{Rules, Settings};
use crate::accounts::quota::rules::simulate::{self, LaneKind};
use crate::accounts::quota::rules::Acct;
use crate::stream::inbound::typed::{Blocking, Command, Cx};

/// `rotation-try` 的入参。
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationTryReq {
    /// 一份策略文件的原文（头 ＋ 代码）。
    pub(crate) source: String,
    /// 换掉头里的设置（缺 ⇒ 用头里的）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) settings: Option<Settings>,
    /// 开会话用的号（缺 ⇒ 号列表里第一个点名的号）。
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
    /// 试跑「现在用这个号」：能用 ⇒ 判的是放进临时条目最前、换过去之后的那一判；用不了 ⇒ `useNowRefused` 带几点恢复。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) use_now: Option<String>,
    /// 名字 · 灰字写哪种语言（`en` ⇒ 英文，其余 ⇒ 中文）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) lang: Option<String>,
    /// 哪一家（缺 ⇒ 有账号库的那一家）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) agent: Option<String>,
}

/// 候选次序里的一行。
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotTryRow {
    pub(crate) account: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) score: Option<f64>,
    pub(crate) why: String,
    pub(crate) from_temp: bool,
    pub(crate) ok: bool,
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
    /// `stay` · `unusable` · `back` · `delay` · `hold`。
    pub(crate) cause: String,
    pub(crate) order: Vec<RotTryRow>,
    pub(crate) skipped: Vec<String>,
    pub(crate) temp: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) next_node: Option<u64>,
    /// 策略这一判出错（所有号当没给分、按列表）：原话。
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
    pub(crate) guessed: bool,
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
    /// `usable` · `atCap` · `off` · `blocked`。
    pub(crate) kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) left: Option<f64>,
    pub(crate) reset: bool,
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
    pub(crate) account: String,
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
}

/// `rotation-try` 的应答。
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationTryReply {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) now: u64,
    /// 这份策略的名字 · 下拉里那一行灰字（照 `lang`）。
    pub(crate) name: String,
    pub(crate) line: String,
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

crate::stream::inbound::typed::codes! {
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

impl Blocking for RotationTry {
    fn run(req: RotationTryReq, cx: &Cx) -> Result<RotationTryReply, RotationTryErr> {
        let ctx = crate::faces::rotation_face::Ctx::here();
        let agent = req
            .agent
            .as_deref()
            .unwrap_or(crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT);
        let row = |a: &str| (ctx.rows)(agent, a);
        let now = crate::accounts::quota::now_unix();
        let facts: Vec<Acct> = ctx
            .hop
            .facts(agent, &row, now, &cx.tz)
            .iter()
            .map(Acct::of_fact)
            .collect();
        try_with(&req, &facts, now)
    }
}

fn cause_word(c: &Cause) -> &'static str {
    match c {
        Cause::Stay => "stay",
        Cause::Unusable => "unusable",
        Cause::Back => "back",
        Cause::Delay => "delay",
        Cause::Hold => "hold",
    }
}

fn lane_word(k: LaneKind) -> &'static str {
    match k {
        LaneKind::Usable => "usable",
        LaneKind::AtCap => "atCap",
        LaneKind::Off => "off",
        LaneKind::Blocked => "blocked",
    }
}

/// 本体（测试给假的事实与钟）。
pub(crate) fn try_with(
    req: &RotationTryReq,
    facts: &[Acct],
    now: u64,
) -> Result<RotationTryReply, RotationTryErr> {
    let file = script::parse(&req.source).map_err(|f| {
        let at = f.line.map(|l| format!("line {l}: ")).unwrap_or_default();
        RotationTryErr::BadStrategy(crate::common::contract::malformed(&format!(
            "{at}{}",
            f.msg
        )))
    })?;
    let tz = file
        .head
        .tz
        .as_deref()
        .and_then(crate::Tz::iana)
        .unwrap_or_default();
    let rules = Rules {
        s: req
            .settings
            .clone()
            .unwrap_or_else(|| file.head.settings.clone()),
        tz,
    };
    let start = req
        .start
        .clone()
        .or_else(|| rules.s.accounts.iter().find_map(|r| r.id.clone()))
        .unwrap_or_default();
    let sigma = Sigma {
        current: req.current.clone().unwrap_or_else(|| start.clone()),
        start,
        temp: req.temp.clone(),
        since: Default::default(),
    };
    let (d, use_now_refused) = match req.use_now.as_deref() {
        Some(a) => match decide::use_now(&sigma, facts, &rules, &file, now, a) {
            Ok(d) => (d, None),
            Err(at) => (
                decide::decide(&sigma, facts, &rules, &file, now, &[]),
                Some(at),
            ),
        },
        None => (decide::decide(&sigma, facts, &rules, &file, now, &[]), None),
    };
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
    let en = req.lang.as_deref() == Some("en");
    let hours = u64::from(req.hours.unwrap_or(12).clamp(1, 48));
    let budget = script::Budgeted::new(&file);
    let tl = simulate::simulate(&sigma, facts, &rules, &budget, now, now + hours * 3600);
    let (pick, hold_until, hold_account) = match &d.pick {
        Pick::Use(a) => (Some(a.clone()), None, None),
        Pick::Hold { until, account } => (None, *until, account.clone()),
    };
    Ok(RotationTryReply {
        now,
        name: file.head.name.pick(en).to_string(),
        line: file.head.line.pick(en).to_string(),
        source,
        use_now_refused,
        decision: RotTryDecision {
            pick,
            hold_until,
            hold_account,
            cause: cause_word(&d.cause).to_string(),
            order: d
                .order
                .iter()
                .map(|r| RotTryRow {
                    account: r.account.clone(),
                    score: r.score,
                    why: r.why.clone(),
                    from_temp: r.from_temp,
                    ok: r.ok,
                })
                .collect(),
            skipped: d.skipped.clone(),
            temp: d.sigma.temp.clone(),
            next_node: d.tn,
            strategy_error: d.strategy_error.as_ref().map(|e| e.msg.clone()),
        },
        timeline: RotTryTimeline {
            switches: tl
                .switches()
                .into_iter()
                .map(|(at, account)| RotTrySwitch { at, account })
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
                    guessed: u.guessed,
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
                            kind: lane_word(s.kind).to_string(),
                            left: s.left,
                            reset: s.reset,
                        })
                        .collect(),
                })
                .collect(),
            truncated: tl.truncated,
        },
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/rotation_try_face_tests.rs"]
mod tests;
