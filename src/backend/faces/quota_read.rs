//! **`quota-read` 的成品**（有类型的一份）：每个出过数的号一条 ＝ 那一条账的原数 ＋ 显示态 ＋ 写好的几行 · 「5h 那一格」· 开窗那一判；
//! 没出过数的号一条 ＝ 种类 · 登录 · 写好的几行 · 开窗那一判；顶上 ＝ 读答三态 · 那一句 · 号名 / 位名表。
//!
//! 时刻字逐格按一把钟写（[`TextClock`]）：生产里是这台的本地钟，金样按它自己的偏移。账上的原数（`reading` · `windowsSeen`）
//! 是透传的一团，旁边的时刻字由同一把钟按 `common::time::with_texts` 那个写法添。
//! 每号几行与开窗那一判照「不含这几格的那一截」的线上样子算（`faces/quota_rows.rs`，唯一的行模型）。

use crate::accounts::quota::decide::Kind;
use crate::accounts::quota::name_words::{names, Names};
use crate::accounts::quota::show::{LoginState, QuotaShow};
use crate::common::cells::Words;
use crate::common::time::TextClock;
use crate::faces::quota_rows::{self, Rows, Warm};
use serde::Serialize;
use serde_json::Value;

/// `quota-read` 的应答（各格的意思见注册表那一条）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuotaRead {
    pub(crate) state: &'static str,
    pub(crate) reason: Value,
    pub(crate) detail: Value,
    pub(crate) path: Option<String>,
    pub(crate) now: u64,
    pub(crate) accounts: Vec<QuotaAccount>,
    pub(crate) unseen: Vec<QuotaUnseen>,
    pub(crate) usable_now: Vec<String>,
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
#[serde(rename_all = "camelCase")]
pub(crate) struct QuotaAccount {
    #[serde(flatten)]
    pub(crate) head: AccountHead,
    pub(crate) rows: Rows,
    /// 「5h 那一格」（恢复菜单 · 新会话框每个号后面那一格）；按量号 ⇒ `null`。
    pub(crate) five_hour: Option<Words>,
    pub(crate) warm: Warm,
}

/// 出过数的一个号除写好的几行之外的那一截（行与开窗那一判照它的线上样子算）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountHead {
    pub(crate) agent: String,
    pub(crate) account: String,
    pub(crate) seen_at: u64,
    pub(crate) seen_at_text: Words,
    /// 别台的钟走在前面时才有。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) seen_at_rel_text: Option<Words>,
    /// 那一条账的额度快照（原数，透传；时刻旁边的字已添）。
    pub(crate) reading: Value,
    /// 各窗口几点、从哪看到的（原数，透传；账上没记 ⇒ 缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) windows_seen: Option<Value>,
    #[serde(flatten)]
    pub(crate) show: QuotaShow,
}

/// 没出过数的一个号。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuotaUnseen {
    #[serde(flatten)]
    pub(crate) head: UnseenHead,
    pub(crate) rows: Rows,
    /// 没出过数 ⇒ 恒 `null`（这一格只给出过数的订阅号）。
    pub(crate) five_hour: Option<Words>,
    pub(crate) warm: Warm,
}

/// 没出过数的一个号除写好的几行之外的那一截。
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnseenHead {
    pub(crate) agent: String,
    pub(crate) account: String,
    pub(crate) kind: Kind,
    pub(crate) login: LoginState,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) sub_id: Option<String>,
}

/// 被拒 / 超额在兜的号里最早回来的那个。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EarliestReturn {
    pub(crate) account: String,
    pub(crate) at: u64,
    pub(crate) at_text: Words,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) at_rel_text: Option<Words>,
}

/// 出过数的一个号交进出口的样子（时刻字还没写）。
pub(crate) struct Seen {
    pub(crate) agent: String,
    pub(crate) account: String,
    pub(crate) seen_at: u64,
    pub(crate) reading: Value,
    pub(crate) windows_seen: Option<Value>,
    pub(crate) show: QuotaShow,
}

/// 顶上那几格交进出口的样子。
pub(crate) struct Base {
    pub(crate) state: &'static str,
    pub(crate) reason: Value,
    pub(crate) detail: Value,
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
    let mut reading = s.reading;
    clock.on(&mut reading);
    let windows_seen = s.windows_seen.map(|mut w| {
        clock.on(&mut w);
        w
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
    let v = serde_json::to_value(&head).unwrap_or(Value::Null);
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
    let v = serde_json::to_value(&head).unwrap_or(Value::Null);
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
        base.reason.as_str(),
    );
    QuotaRead {
        text,
        five_hour: quota_rows::head_five_hour(base.state),
        state: base.state,
        reason: base.reason,
        detail: base.detail,
        path: base.path,
        now: base.now,
        accounts: seen.into_iter().map(|s| account_of(s, clock)).collect(),
        unseen: unseen.into_iter().map(|u| unseen_of(u, clock)).collect(),
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
    use crate::accounts::quota::show::{QuotaState, SlotShow, WindowShow};
    use crate::common::cells::Tone;
    let clock = TextClock {
        now: 1_000,
        tz_min: 0,
    };
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
            at_line: true,
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
    };
    let mut r = reply_of(
        Base {
            state: "present",
            reason: Value::String("r".into()),
            detail: Value::String("d".into()),
            path: Some("/q".into()),
            now: 1_000,
            usable_now: vec!["a".into()],
            earliest: Some(("a".into(), 2_000)),
        },
        vec![Seen {
            agent: "agent".into(),
            account: "a".into(),
            seen_at: 2_000,
            // 原数是透传的一团（目录记它 `object`、不往里分格）。
            reading: serde_json::json!({}),
            windows_seen: Some(serde_json::json!({})),
            show,
        }],
        vec![UnseenHead {
            agent: "agent".into(),
            account: "b".into(),
            kind: Kind::Sub,
            login: LoginState::NeedsLogin,
            sub_id: Some("s".into()),
        }],
        &clock,
    );
    // 真出口在这一份里给不出的三格（读不出 · 一个号都没有时才有的那一句与那一格；没出过数的号那一格恒空）：样本照类型填上。
    r.text = Some(Words("t".into()));
    r.five_hour = Some(Words("f".into()));
    r.unseen[0].five_hour = Some(Words("f".into()));
    r
}
