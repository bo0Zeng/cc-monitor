//! **`quota-read` 的成品**（有类型的一份）：每个出过数的号一条 ＝ 那一条账的原数 ＋ 显示态 ＋ 写好的几行 · 「5h 那一格」· 开窗那一判；
//! 没出过数的号一条 ＝ 种类 · 登录 · 写好的几行 · 开窗那一判；顶上 ＝ 读答三态 · 那一句 · 号名 / 位名表。
//!
//! 时刻字逐格按一把钟写（[`TextClock`]）：按看的那一台的时区（请求信封的 `tz`，缺了按 UTC），金样按 UTC。账上的原数（`reading` · `windowsSeen`）
//! 照原样一格不改，时刻旁边的字（`…Text` · 还没到的 `…RelText`）由同一把钟逐格添（[`ReadingShown`] · [`WindowSeenShown`]）。
//! 每号几行与开窗那一判照「不含这几格的那一截」的线上样子算（`faces/quota_rows.rs`，唯一的行模型）。

use crate::accounts::quota::facts::Kind;
use crate::accounts::quota::ledger::{ReadState, Source, WindowSeen};
use crate::accounts::quota::name_words::{is_candidate, names, Names};
use crate::accounts::quota::show::{LoginState, QuotaShow};
use crate::agents::{QuotaOverage, QuotaReading, QuotaStatus, QuotaWindow};
use crate::common::cells::Words;
use crate::common::time::TextClock;
use crate::faces::quota_rows::{self, Rows, Warm};
use serde::Serialize;
use std::collections::BTreeMap;

/// `quota-read` 的应答：这台的额度账。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuotaRead {
    /// 那份文件读到的三态。
    pub(crate) state: ReadState,
    /// 只在 `unreadable` 时有：为什么读不出来（那一句不带原话）。
    pub(crate) reason: Option<String>,
    /// 只在 `unreadable` 时有：复制详情（时刻 · 机器 · 命令 · 码 · 原话；排法同失败应答）。
    pub(crate) detail: Option<String>,
    /// 那份文件的绝对路径（家推不出来时 `null`）。
    pub(crate) path: Option<String>,
    /// 这台此刻的 unix 秒（界面算「几分钟前看到的」「还有多久重置」都按这台的钟）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) now: u64,
    /// 每个出过数的号一条，按 `(agent, account)` 排。
    pub(crate) accounts: Vec<QuotaAccount>,
    /// 账号库里有、额度账上从没出过数的号。
    pub(crate) unseen: Vec<QuotaUnseen>,
    /// 此刻发得出去的号（路由第 2 段）：登录拿得到、不是被拒 / 超额在兜。
    pub(crate) usable_now: Vec<String>,
    /// 换号 · 加号那几处的候选：出过数的在前、没出过的在后，`_` 不在里面（[`is_candidate`]）。界面只照它画，不另拼、不另滤。
    pub(crate) candidates: Vec<Candidate>,
    /// 被拒 / 超额在兜的号里最早回来的那个；没有、或都说不出时刻 ⇒ `null`。
    pub(crate) earliest_return: Option<EarliestReturn>,
    /// 读不出 / 一个号都没有 ⇒ 那一句（`--text` 拼字时放最前）；否则 `null`（[`quota_rows::head_text`]）。
    pub(crate) text: Option<Words>,
    /// 读不出 ⇒「5h 那一格」写好的字（`5h 读不到`，[`quota_rows::head_five_hour`]）；否则 `null`（每号自己那一格）。
    pub(crate) five_hour: Option<Words>,
    /// 号名 / 位名表（出口画号名 · 位名只照它）。
    pub(crate) names: Names,
}

/// 出过数的一个号：账上那一条的原数 ＋ 显示态（[`AccountHead`]）＋ 照它写好的几格。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuotaAccount {
    #[serde(flatten)]
    #[cfg_attr(test, ts(flatten))]
    pub(crate) head: AccountHead,
    pub(crate) rows: Rows,
    /// 「5h 那一格」（恢复菜单 · 新会话框每个号后面那一格）；按量号 ⇒ `null`。
    pub(crate) five_hour: Option<Words>,
    pub(crate) warm: Warm,
}

/// 出过数的一个号除写好的几行之外的那一截（行与开窗那一判照它的线上样子算）。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "QuotaAccountHead"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountHead {
    pub(crate) agent: String,
    pub(crate) account: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) seen_at: u64,
    pub(crate) seen_at_text: Words,
    /// 别台的钟走在前面时才有。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) seen_at_rel_text: Option<Words>,
    /// 那一条账的额度快照（原数照原样；时刻旁边的字已添）。
    pub(crate) reading: ReadingShown,
    /// 各窗口几点、从哪看到的（原数照原样；账上没记 ⇒ 缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) windows_seen: Option<BTreeMap<String, WindowSeenShown>>,
    #[serde(flatten)]
    #[cfg_attr(test, ts(flatten))]
    pub(crate) show: QuotaShow,
}

/// 账上那一条的额度快照（[`QuotaReading`]）＋ 时刻旁边写好的字（`TextClock::text` · `rel`：有时刻才添、`…RelText` 只给还没到的）。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "QuotaReadingShown"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadingShown {
    /// 限流器的结论；回包里没说 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) status: Option<QuotaStatus>,
    /// 那一发被上游拒了（没被服务）。
    pub(crate) refused: bool,
    /// 此刻卡着的那个窗口名。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) limiting: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(test, ts(flatten))]
    pub(crate) resets_at: ResetsAt,
    pub(crate) windows: Vec<WindowReadShown>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) overage: Option<OverageShown>,
}

/// 「几点重置」那一格与它旁边写好的字（三格都可缺：没有时刻 ⇒ 三格都缺；过了 ⇒ 没有 `resetsAtRelText`）。
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "QuotaResetsAt"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResetsAt {
    /// 几点重置（unix 秒）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) resets_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) resets_at_text: Option<Words>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) resets_at_rel_text: Option<Words>,
}

impl ResetsAt {
    fn of(t: Option<u64>, clock: &TextClock) -> ResetsAt {
        ResetsAt {
            resets_at: t,
            resets_at_text: t.map(|t| clock.text(t)),
            resets_at_rel_text: t.and_then(|t| clock.rel(t)),
        }
    }
}

/// 快照里的一个窗口（[`QuotaWindow`]）＋ 重置时刻旁边写好的字。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "QuotaWindowRead"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WindowReadShown {
    pub(crate) name: String,
    /// 已用比例（通常 0–1，可以超过 1）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) used: Option<f64>,
    #[serde(flatten)]
    #[cfg_attr(test, ts(flatten))]
    pub(crate) resets_at: ResetsAt,
    /// 越过了哪条预警线（比例）；没越过就没有。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) warned_at: Option<f64>,
}

/// 超额那一档（[`QuotaOverage`]）＋ 重置时刻旁边写好的字。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "QuotaOverageShown"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OverageShown {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) status: Option<QuotaStatus>,
    #[serde(flatten)]
    #[cfg_attr(test, ts(flatten))]
    pub(crate) resets_at: ResetsAt,
    /// 为什么不可用（那一家的原值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) disabled: Option<String>,
    /// 这一发正在用超额。
    pub(crate) in_use: bool,
}

/// 一个窗口几点 · 从哪看到的（[`WindowSeen`]）＋ 那一刻旁边写好的字。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "QuotaWindowSeen"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WindowSeenShown {
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) at: u64,
    pub(crate) at_text: Words,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) at_rel_text: Option<Words>,
    pub(crate) from: Source,
}

impl ReadingShown {
    /// 原数一格不改，时刻旁边按钟添字。
    pub(crate) fn of(r: &QuotaReading, clock: &TextClock) -> ReadingShown {
        ReadingShown {
            status: r.status,
            refused: r.refused,
            limiting: r.limiting.clone(),
            resets_at: ResetsAt::of(r.resets_at, clock),
            windows: r
                .windows
                .iter()
                .map(|w: &QuotaWindow| WindowReadShown {
                    name: w.name.clone(),
                    used: w.used,
                    resets_at: ResetsAt::of(w.resets_at, clock),
                    warned_at: w.warned_at,
                })
                .collect(),
            overage: r.overage.as_ref().map(|o: &QuotaOverage| OverageShown {
                status: o.status,
                resets_at: ResetsAt::of(o.resets_at, clock),
                disabled: o.disabled.clone(),
                in_use: o.in_use,
            }),
        }
    }
}

impl WindowSeenShown {
    fn of(w: &WindowSeen, clock: &TextClock) -> WindowSeenShown {
        WindowSeenShown {
            at: w.at,
            at_text: clock.text(w.at),
            at_rel_text: clock.rel(w.at),
            from: w.from,
        }
    }
}

/// 没出过数的一个号。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuotaUnseen {
    #[serde(flatten)]
    #[cfg_attr(test, ts(flatten))]
    pub(crate) head: UnseenHead,
    pub(crate) rows: Rows,
    /// 没出过数 ⇒ 恒 `null`（这一格只给出过数的订阅号）。
    pub(crate) five_hour: Option<Words>,
    pub(crate) warm: Warm,
}

/// 没出过数的一个号除写好的几行之外的那一截。
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "QuotaUnseenHead"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnseenHead {
    pub(crate) agent: String,
    pub(crate) account: String,
    pub(crate) kind: Kind,
    pub(crate) login: LoginState,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) sub_id: Option<String>,
    /// 用量那一格（没出过数 ⇒ `5h —` / `按量`；同出过数的号那一格一个写法，只是没有重置那一格）。
    pub(crate) usage: UnseenUsage,
}

/// 没出过数的号用量那一格：[`Usage`](crate::accounts::quota::show::Usage) 去掉重置那一格（没出过数就说不出几点重置）。
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "QuotaUnseenUsage"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnseenUsage {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) slot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) window: Option<Words>,
    pub(crate) value: Words,
    pub(crate) text: Words,
    pub(crate) tone: crate::common::cells::Tone,
}

impl From<crate::accounts::quota::show::Usage> for UnseenUsage {
    fn from(u: crate::accounts::quota::show::Usage) -> Self {
        UnseenUsage {
            slot: u.slot,
            window: u.window,
            value: u.value,
            text: u.text,
            tone: u.tone,
        }
    }
}

/// 一个候选号：哪一家 · 哪个号 · 种类（加号那一处要分按量号）。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "QuotaCandidate"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Candidate {
    pub(crate) agent: String,
    pub(crate) account: String,
    pub(crate) kind: Kind,
}

/// 被拒 / 超额在兜的号里最早回来的那个。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "QuotaEarliestReturn"
    )
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EarliestReturn {
    pub(crate) account: String,
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) at: u64,
    pub(crate) at_text: Words,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) at_rel_text: Option<Words>,
}

/// 出过数的一个号交进出口的样子（时刻字还没写）。
pub(crate) struct Seen {
    pub(crate) agent: String,
    pub(crate) account: String,
    pub(crate) seen_at: u64,
    pub(crate) reading: QuotaReading,
    /// 账上没记 ⇒ 空。
    pub(crate) windows_seen: BTreeMap<String, WindowSeen>,
    pub(crate) show: QuotaShow,
}

/// 顶上那几格交进出口的样子。
pub(crate) struct Base {
    pub(crate) state: ReadState,
    pub(crate) reason: Option<String>,
    pub(crate) detail: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) now: u64,
    pub(crate) usable_now: Vec<String>,
    /// （号, 几点回来）。
    pub(crate) earliest: Option<(String, u64)>,
}

/// 出过数的一个号 ⇒ 成品那一条。
pub(crate) fn account_of(s: Seen, clock: &TextClock) -> QuotaAccount {
    let mut show = s.show;
    show.stamp(clock);
    let reading = ReadingShown::of(&s.reading, clock);
    let windows_seen = (!s.windows_seen.is_empty()).then(|| {
        s.windows_seen
            .iter()
            .map(|(k, w)| (k.clone(), WindowSeenShown::of(w, clock)))
            .collect()
    });
    let head = AccountHead {
        agent: s.agent,
        account: s.account,
        seen_at: s.seen_at,
        seen_at_text: clock.text(s.seen_at),
        seen_at_rel_text: clock.rel(s.seen_at),
        reading,
        windows_seen,
        show,
    };
    let v = serde_json::to_value(&head).unwrap_or_default();
    let five_hour = (head.show.kind != Kind::Api).then(|| {
        let none = copy_core::copy_text("acct.val.none", &[]);
        let text = head
            .show
            .slots
            .iter()
            .find(|x| x.slot == "5h")
            .map_or(none, |x| x.text.0.clone());
        Words(quota_rows::five_hour(&text))
    });
    QuotaAccount {
        rows: quota_rows::seen_block(&v),
        five_hour,
        warm: quota_rows::warm_of(&v, true, clock.now).stamp(clock),
        head,
    }
}

/// 没出过数的一个号 ⇒ 成品那一条。
pub(crate) fn unseen_of(head: UnseenHead, clock: &TextClock) -> QuotaUnseen {
    let v = serde_json::to_value(&head).unwrap_or_default();
    QuotaUnseen {
        rows: quota_rows::unseen_block(&v),
        five_hour: None,
        warm: quota_rows::warm_of(&v, false, clock.now).stamp(clock),
        head,
    }
}

/// ★ 出口：交进来的原数与显示态 ⇒ `quota-read` 的成品（唯一一处；生产与金样都走它）。
pub(crate) fn reply_of(
    base: Base,
    seen: Vec<Seen>,
    unseen: Vec<UnseenHead>,
    clock: &TextClock,
) -> QuotaRead {
    let text = quota_rows::head_text(
        base.state,
        seen.is_empty() && unseen.is_empty(),
        base.reason.as_deref(),
    );
    let candidates = seen
        .iter()
        .map(|s| (&s.agent, &s.account, s.show.kind))
        .chain(unseen.iter().map(|u| (&u.agent, &u.account, u.kind)))
        .filter(|(_, a, _)| is_candidate(a))
        .map(|(agent, account, kind)| Candidate {
            agent: agent.clone(),
            account: account.clone(),
            kind,
        })
        .collect();
    // 账读不懂 ⇒ 每个号都不发（判在 `warm_unreadable`），那一句带上顶上那一句（哪一份 · 为什么）。
    let unreadable = (base.state == ReadState::Unreadable)
        .then(|| text.as_ref().map(|w| w.0.clone()))
        .flatten();
    let warm_over = |mut w: quota_rows::Warm| {
        if let Some(why) = &unreadable {
            w = quota_rows::warm_unreadable(why, clock.now).stamp(clock);
        }
        w
    };
    QuotaRead {
        candidates,
        text,
        five_hour: quota_rows::head_five_hour(base.state),
        state: base.state,
        reason: base.reason,
        detail: base.detail,
        path: base.path,
        now: base.now,
        accounts: seen
            .into_iter()
            .map(|s| account_of(s, clock))
            .map(|mut a| {
                a.warm = warm_over(a.warm);
                a
            })
            .collect(),
        unseen: unseen
            .into_iter()
            .map(|u| unseen_of(u, clock))
            .map(|mut u| {
                u.warm = warm_over(u.warm);
                u
            })
            .collect(),
        usable_now: base.usable_now,
        earliest_return: base.earliest.map(|(account, at)| EarliestReturn {
            account,
            at,
            at_text: clock.text(at),
            at_rel_text: clock.rel(at),
        }),
        names: names(),
    }
}

/// 格目录与出参对拍的样本：用真出口造（每个可缺的格都填上、每个列表都不空）。
pub(crate) fn specimen() -> QuotaRead {
    use crate::accounts::quota::show::{QuotaState, SlotShow, WindowShow};
    use crate::common::cells::Tone;
    let clock = TextClock::new(1_000, &crate::Tz::default());
    let show = QuotaShow {
        kind: Kind::Sub,
        state: QuotaState::Ok,
        stale: false,
        limiting: Some("5h".into()),
        slots: vec![SlotShow {
            slot: "5h".into(),
            pct: Some(1),
            resets_at: Some(2_000),
            resets_at_text: None,
            resets_at_rel_text: None,
            full: true,
            text: Words("1%".into()),
            tone: Tone::Plain,
        }],
        login: LoginState::Ok,
        sub_id: Some("s".into()),
        windows: vec![WindowShow {
            name: "five_hour".into(),
            key: Some("5h".into()),
            pct: Some(1),
            resets_at: Some(2_000),
            resets_at_text: None,
            resets_at_rel_text: None,
            seen_at: 2_000,
            seen_at_text: None,
            seen_at_rel_text: None,
            from: Source::Headers,
            reset_since_seen: true,
        }],
        usage: crate::accounts::quota::show::Usage {
            slot: Some("5h".into()),
            window: Some(Words("5h".into())),
            value: Words("1%".into()),
            reset: None,
            text: Words("5h 1%".into()),
            tone: Tone::Plain,
            resets_at: Some(2_000),
        },
    };
    let mut r = reply_of(
        Base {
            state: ReadState::Present,
            reason: Some("r".into()),
            detail: Some("d".into()),
            path: Some("/q".into()),
            now: 1_000,
            usable_now: vec!["a".into()],
            earliest: Some(("a".into(), 2_000)),
        },
        vec![Seen {
            agent: "agent".into(),
            account: "a".into(),
            seen_at: 2_000,
            reading: QuotaReading {
                status: Some(QuotaStatus::Allowed),
                refused: false,
                limiting: Some("five_hour".into()),
                resets_at: Some(2_000),
                windows: vec![QuotaWindow {
                    name: "five_hour".into(),
                    used: Some(0.01),
                    resets_at: Some(2_000),
                    warned_at: Some(0.5),
                }],
                overage: Some(QuotaOverage {
                    status: Some(QuotaStatus::Rejected),
                    resets_at: Some(2_000),
                    disabled: Some("off".into()),
                    in_use: false,
                }),
            },
            windows_seen: [(
                "five_hour".to_string(),
                WindowSeen {
                    at: 2_000,
                    from: Source::Headers,
                },
            )]
            .into(),
            show,
        }],
        vec![UnseenHead {
            agent: "agent".into(),
            account: "b".into(),
            kind: Kind::Sub,
            login: LoginState::NeedsLogin,
            sub_id: Some("s".into()),
            usage: UnseenUsage {
                slot: Some("5h".into()),
                window: Some(Words("5h".into())),
                value: Words("v".into()),
                text: Words("5h v".into()),
                tone: Tone::Plain,
            },
        }],
        &clock,
    );
    // 真出口在这一份里给不出的三格（读不出 · 一个号都没有时才有的那一句与那一格；没出过数的号那一格恒空）：样本照类型填上。
    r.text = Some(Words("t".into()));
    r.five_hour = Some(Words("f".into()));
    r.unseen[0].five_hour = Some(Words("f".into()));
    r
}
