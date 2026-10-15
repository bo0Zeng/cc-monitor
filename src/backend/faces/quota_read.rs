//! **`quota-read` 的成品**（有类型的一份）：每个出过数的号一条 ＝ 那一条账的原数 ＋ 显示态 ＋ 写好的几行 · 「5h 那一格」· 开窗那一判；
//! 没出过数的号一条 ＝ 种类 · 登录 · 写好的几行 · 开窗那一判；顶上 ＝ 读答三态 · 那一句 · 号名 / 位名表。
//!
//! 时刻字逐格按一把钟写（[`TextClock`]）：按看的那一台的时区（请求信封的 `tz`，缺了按 UTC），金样按 UTC。账上的原数（`reading` · `windowsSeen`）
//! 照原样一格不改，时刻旁边的字（`…Text` · 还没到的 `…RelText`）由同一把钟逐格添（[`ReadingShown`] · [`WindowSeenShown`]）。
//! 每号几行与开窗那一判照「不含这几格的那一截」的线上样子算（`faces/quota_rows.rs`，唯一的行模型）。

use crate::accounts::quota::facts::Kind;
use crate::accounts::quota::ledger::{ReadState, WindowSeen};
use crate::accounts::quota::name_words::{is_candidate, names};
use crate::accounts::quota::show::{QuotaShow, Stamp};
use crate::agents::{kind_of_route, QuotaOverage, QuotaReading, QuotaWindow};
use crate::common::cells::Words;
use crate::common::time::TextClock;
use crate::faces::quota_rows;
pub(crate) use cmd_wire::quota::{
    AccountHead, Candidate, EarliestReturn, OverageShown, QuotaAccount, QuotaRead, QuotaUnseen,
    ReadingShown, ResetsAt, UnseenHead, UnseenUsage, WindowReadShown, WindowSeenShown,
};
use std::collections::BTreeMap;

/// 「几点重置」那一格与旁边写好的字。
fn resets_at(t: Option<u64>, clock: &TextClock) -> ResetsAt {
    ResetsAt {
        resets_at: t,
        resets_at_text: t.map(|t| clock.text(t)),
        resets_at_rel_text: t.and_then(|t| clock.rel(t)),
    }
}

/// 账上那一条的额度快照：原数一格不改，时刻旁边按钟添字。
pub(crate) fn reading_shown(r: &QuotaReading, clock: &TextClock) -> ReadingShown {
    ReadingShown {
        status: r.status,
        refused: r.refused,
        limiting: r.limiting.clone(),
        resets_at: resets_at(r.resets_at, clock),
        windows: r
            .windows
            .iter()
            .map(|w: &QuotaWindow| WindowReadShown {
                name: w.name.clone(),
                used: w.used,
                resets_at: resets_at(w.resets_at, clock),
                warned_at: w.warned_at,
            })
            .collect(),
        overage: r.overage.as_ref().map(|o: &QuotaOverage| OverageShown {
            status: o.status,
            resets_at: resets_at(o.resets_at, clock),
            disabled: o.disabled.clone(),
            in_use: o.in_use,
        }),
    }
}

fn window_seen_shown(w: &WindowSeen, clock: &TextClock) -> WindowSeenShown {
    WindowSeenShown {
        at: w.at,
        at_text: clock.text(w.at),
        at_rel_text: clock.rel(w.at),
        from: w.from,
    }
}

/// 出过数的一个号交进出口的样子（时刻字还没写）。`agent` 是账上记的路由名，出口换成线上的 kind。
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
    let reading = reading_shown(&s.reading, clock);
    let windows_seen = (!s.windows_seen.is_empty()).then(|| {
        s.windows_seen
            .iter()
            .map(|(k, w)| (k.clone(), window_seen_shown(w, clock)))
            .collect()
    });
    let head = AccountHead {
        agent: kind_of_route(&s.agent),
        account: s.account,
        seen_at: s.seen_at,
        seen_at_text: clock.text(s.seen_at),
        seen_at_rel_text: clock.rel(s.seen_at),
        reading,
        windows_seen,
        show,
    };
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
        rows: quota_rows::seen_block(&head),
        five_hour,
        warm: quota_rows::stamped(
            quota_rows::warm_of(head.show.login, Some(&head.show), clock.now),
            clock,
        ),
        head,
    }
}

/// 没出过数的一个号 ⇒ 成品那一条。
pub(crate) fn unseen_of(head: UnseenHead, clock: &TextClock) -> QuotaUnseen {
    QuotaUnseen {
        rows: quota_rows::unseen_block(&head),
        five_hour: None,
        warm: quota_rows::stamped(quota_rows::warm_of(head.login, None, clock.now), clock),
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
        .map(|s| (kind_of_route(&s.agent), &s.account, s.show.kind))
        .chain(unseen.iter().map(|u| (u.agent.clone(), &u.account, u.kind)))
        .filter(|(_, a, _)| is_candidate(a))
        .map(|(agent, account, kind)| Candidate {
            agent,
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
            w = quota_rows::stamped(quota_rows::warm_unreadable(why, clock.now), clock);
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
    use crate::accounts::quota::ledger::Source;
    use crate::accounts::quota::show::{LoginState, QuotaState, SlotShow, WindowShow};
    use crate::agents::QuotaStatus;
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
            tokens_per_pct: Some(25_000),
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
            agent: crate::agents::AgentKind::new("agent"),
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
