//! **`quota-read` 的成品**（有类型的一份）：每个出过数的号一条 ＝ 那一条账的原数 ＋ 显示态 ＋ 写好的几行 · 「5h 那一格」· 开窗那一判；
//! 没出过数的号一条 ＝ 种类 · 登录 · 写好的几行 · 开窗那一判；顶上 ＝ 读答三态 · 那一句 · 号名 / 位名表。
//!
//! 时刻字逐格按一把钟写（[`TextClock`]）：按看的那一台的时区（请求信封的 `tz`，缺了按 UTC），金样按 UTC。
//! 账上的原数只在后端里用来写行（不上线）；每号几行与开窗那一判照显示态算（`faces/quota_rows.rs`，唯一的行模型）。

use crate::accounts::quota::facts::Kind;
use crate::accounts::quota::ledger::ReadState;
use crate::accounts::quota::name_words::{account_text, is_candidate, names};
use crate::accounts::quota::rules::decide::{WarmVerdict, WarmWhy};
use crate::accounts::quota::show::QuotaShow;
use crate::agents::{kind_of_route, QuotaReading};
use crate::common::cells::{Tone, Words};
use crate::common::time::TextClock;
use crate::faces::quota_rows::{self, Stamp};
pub(crate) use cmd_wire::quota::{
    AccountHead, Candidate, QuotaAccount, QuotaRead, QuotaUnseen, ResetsAt, UnseenHead,
    UnseenUsage, Verdict, SLOT_5H,
};

/// 「几点重置」那一格与旁边写好的字。
fn resets_at(t: Option<u64>, clock: &TextClock) -> ResetsAt {
    ResetsAt {
        resets_at: t,
        resets_at_text: t.map(|t| clock.text(t)),
        resets_at_rel_text: t.and_then(|t| clock.rel(t)),
    }
}

/// 出过数的一个号交进出口的样子（时刻字还没写）。`agent` 是账上记的路由名，出口换成线上的 kind。
pub struct Seen {
    pub agent: String,
    pub account: String,
    pub seen_at: u64,
    pub reading: QuotaReading,
    pub show: QuotaShow,
    /// 开窗那一判（accounts 照这个号的事实判好的，`rules::decide::warm_verdict`）。
    pub warm: WarmVerdict,
}

/// 没出过数的一个号交进出口的样子：那一截 ＋ 开窗那一判。
pub(crate) struct Unseen {
    pub(crate) head: UnseenHead,
    pub(crate) warm: WarmVerdict,
}

/// 顶上那几格交进出口的样子。
pub(crate) struct Base {
    pub(crate) state: ReadState,
    pub(crate) reason: Option<String>,
    pub(crate) detail: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) now: u64,
    pub(crate) usable: Vec<String>,
    /// （号, 几点回来）。
    pub(crate) earliest: Option<(String, u64)>,
}

/// 出过数的一个号 ⇒ 成品那一条。
pub fn account_of(s: Seen, clock: &TextClock) -> QuotaAccount {
    let mut show = s.show;
    quota_rows::written(&mut show, s.reading.resets_at);
    show.stamp(clock);
    let head = AccountHead {
        agent: kind_of_route(&s.agent),
        account: s.account,
        seen_at: s.seen_at,
        seen_at_text: clock.text(s.seen_at),
        resets_at: resets_at(s.reading.resets_at, clock),
        show,
    };
    QuotaAccount {
        rows: quota_rows::seen_block(&head),
        warm: quota_rows::warm_of(s.warm, Some(&head.show), clock),
        head,
    }
}

/// 没出过数的一个号 ⇒ 成品那一条。
pub(crate) fn unseen_of(mut u: Unseen, clock: &TextClock) -> QuotaUnseen {
    u.head.usage = quota_rows::unseen_usage(u.head.kind);
    QuotaUnseen {
        rows: quota_rows::unseen_block(&u.head),
        warm: quota_rows::warm_of(u.warm, None, clock),
        head: u.head,
    }
}

/// ★「现在还能不能发」那两行（唯一写法）：有号发得出去 ⇒「可发 · n 个号」＋ 哪几个；一个都发不出去 ⇒「均不可发」＋ 最早哪个几点回来（说不出 ⇒ 缺）。
fn verdict_of(usable: &[String], earliest: Option<&(String, u64)>, clock: &TextClock) -> Verdict {
    if !usable.is_empty() {
        let sep = copy_core::copy_text("kit.text.sep", &[]);
        let names: Vec<String> = usable.iter().map(|a| account_text(a)).collect();
        return Verdict {
            text: Words(copy_core::copy_text(
                "acct.verdict.can",
                &[("n", &usable.len().to_string())],
            )),
            sub: Some(Words(names.join(&sep))),
            tone: Tone::Plain,
        };
    }
    let sub = earliest.map(|(account, at)| {
        let account = account_text(account);
        let at_text = clock.text(*at).0;
        Words(match clock.rel(*at) {
            Some(rel) => copy_core::copy_text(
                "acct.verdict.earliest",
                &[("account", &account), ("at", &at_text), ("rel", &rel.0)],
            ),
            None => copy_core::copy_text(
                "acct.verdict.earliestAt",
                &[("account", &account), ("at", &at_text)],
            ),
        })
    });
    Verdict {
        text: Words(copy_core::copy_text("acct.verdict.none", &[])),
        sub,
        tone: Tone::Fail,
    }
}

/// ★ 出口：交进来的原数与显示态 ⇒ `quota-read` 的成品（唯一一处；生产与金样都走它）。
pub(crate) fn reply_of(
    base: Base,
    seen: Vec<Seen>,
    unseen: Vec<Unseen>,
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
        .chain(
            unseen
                .iter()
                .map(|u| (u.head.agent.clone(), &u.head.account, u.head.kind)),
        )
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
    let verdict = (base.state == ReadState::Present && !(seen.is_empty() && unseen.is_empty()))
        .then(|| verdict_of(&base.usable, base.earliest.as_ref(), clock));
    QuotaRead {
        verdict,
        candidates,
        text,
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
        names: names(),
    }
}

/// 格目录与出参对拍的样本：用真出口造（每个可缺的格都填上、每个列表都不空）。
pub(crate) fn specimen() -> QuotaRead {
    use crate::accounts::quota::ledger::Source;
    use crate::accounts::quota::show::{LoginState, QuotaState, SlotShow, WindowShow};
    use crate::agents::QuotaStatus;
    let clock = TextClock::new(1_000, &crate::common::time::Tz::default());
    let show = QuotaShow {
        kind: Kind::Sub,
        state: QuotaState::Refused,
        stale: false,
        limiting: Some(SLOT_5H.into()),
        slots: vec![SlotShow {
            slot: SLOT_5H.into(),
            pct: Some(1),
            resets_at: Some(2_000),
            resets_at_text: None,
            resets_at_rel_text: None,
            reset_text: Some(Words("r".into())),
            full: true,
            text: Words("1%".into()),
            tone: Tone::Plain,
            per_pct_text: Some(Words("p".into())),
            window: "five_hour".into(),
        }],
        login: LoginState::Ok,
        windows: vec![WindowShow {
            name: "five_hour".into(),
            key: Some(SLOT_5H.into()),
            pct: Some(1),
            resets_at: Some(2_000),
            resets_at_text: None,
            resets_at_rel_text: None,
            seen_at: 2_000,
            from: Source::Headers,
            tokens_per_pct: Some(25_000),
            per_pct_text: Some(Words("p".into())),
            reset_since_seen: true,
            tone: Tone::Plain,
        }],
        usage: crate::accounts::quota::show::Usage {
            slot: Some(SLOT_5H.into()),
            window: Some(Words(SLOT_5H.into())),
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
            usable: vec!["a".into()],
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
                windows: vec![crate::agents::QuotaWindow {
                    name: "five_hour".into(),
                    used: Some(0.01),
                    resets_at: Some(2_000),
                    warned_at: Some(0.5),
                }],
                overage: Some(crate::agents::QuotaOverage {
                    status: Some(QuotaStatus::Rejected),
                    resets_at: Some(2_000),
                    disabled: Some("off".into()),
                    in_use: false,
                }),
            },
            show,
            warm: WarmVerdict::Wait {
                until: Some(2_000),
                why: WarmWhy::Ticking,
            },
        }],
        vec![Unseen {
            head: UnseenHead {
                agent: crate::agents::AgentKind::new("agent"),
                account: "b".into(),
                kind: Kind::Sub,
                login: LoginState::NeedsLogin,
                usage: UnseenUsage {
                    slot: Some(SLOT_5H.into()),
                    window: Some(Words(SLOT_5H.into())),
                    value: Words("v".into()),
                    text: Words("5h v".into()),
                    tone: Tone::Plain,
                },
            },
            warm: WarmVerdict::Wait {
                until: None,
                why: WarmWhy::Login,
            },
        }],
        &clock,
    );
    // 真出口在这一份里给不出的那一格（读不出 · 一个号都没有时才有的那一句）：样本照类型填上。
    r.text = Some(Words("t".into()));
    r
}
