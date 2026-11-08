//! **额度每号几行**（`quota-read` 每号带的 `rows`）：一个号一段，首行 `名 · 类型 · 标签`，其余每行一组格（键 · 值 · ↻ · 距今，缺的格不出）。
//!
//! 唯一一处行模型：桌面悬停卡、CLI `--text`（通用拼字，见 `stream/cli.rs`）、手机、skill 都只照抄这几行，不按码取字。
//! 读的是出口写好时刻字之后的回包（`faces/quota_read.rs`，同一把钟 `TextClock`）：时刻的字 `…Text` 与距今 `…RelText` 已写好；
//! 显示态那一判在 `accounts/quota/show.rs`；判好的写成字与语气（每个语义位那一格 · 用量那一格 · 时刻旁边那几格）也在这里（[`written`] · [`Stamp`]）。
//! 「采样那一格」只写几点（数旧带「旧」）：哪台采的是出口自己的事（它知道问的是哪台、那台叫什么）。

use crate::accounts::quota::name_words::slot_text;
use crate::accounts::quota::rules::decide::{WarmVerdict, WarmWhy};
use crate::common::cells::{Tone, Words};
use crate::common::time::TextClock;
use cmd_wire::quota::{
    AccountHead, Kind, LoginState, QuotaShow, QuotaState, ResetsAt, SlotShow, UnseenHead,
    UnseenUsage, Usage, WindowShow, SLOT_5H, SLOT_7D,
};
pub(crate) use cmd_wire::quota::{Cell, Rows, Warm, WarmAct};
use copy_core::copy_text;

// ── 显示态写成字（判在 `accounts/quota/show.rs`，这里只照判好的写） ─────────────────────────────

/// ★ 一个语义位那一格写成什么字、什么语气（唯一一处：额度每号几行 · 账号面板 · 规则编辑器 · 起会话菜单都读它）。
/// `here` ＝ 它是卡人的那一格（[`QuotaShow::limiting`]）。
pub(crate) fn slot_words(
    state: QuotaState,
    here: bool,
    pct: Option<u32>,
    full: bool,
) -> (Words, Tone) {
    if here && state == QuotaState::OverageInUse {
        return (Words(copy_text("acct.row.over", &[])), Tone::Warn);
    }
    if here && matches!(state, QuotaState::ResetSinceSeen | QuotaState::Unseen) {
        return (Words(copy_text("acct.val.none", &[])), Tone::Plain);
    }
    if full {
        return (Words(copy_text("acct.val.full", &[])), Tone::Fail);
    }
    if here && state == QuotaState::Refused {
        return match pct {
            Some(p) => (
                Words(copy_text("acct.val.refusedPct", &[("pct", &p.to_string())])),
                Tone::Fail,
            ),
            None => (Words(copy_text("reason.code.refused", &[])), Tone::Fail),
        };
    }
    match pct {
        None => (Words(copy_text("acct.val.none", &[])), Tone::Plain),
        Some(p) => (
            Words(copy_text("acct.val.pct", &[("pct", &p.to_string())])),
            if here && state == QuotaState::Near {
                Tone::Warn
            } else {
                Tone::Plain
            },
        ),
    }
}

/// 用量那一格连成一句（窗口 · 值 · 重置）。
fn joined(u: &mut Usage) {
    let v = u.value.0.as_str();
    u.text = Words(match (&u.window, &u.reset) {
        (None, None) => v.to_string(),
        (None, Some(r)) => copy_text("acct.usage.wv", &[("w", v), ("v", &r.0)]),
        (Some(w), None) => copy_text("acct.usage.wv", &[("w", &w.0), ("v", v)]),
        (Some(w), Some(r)) => copy_text("acct.usage.wvr", &[("w", &w.0), ("v", v), ("r", &r.0)]),
    });
}

/// 一个号用量那一格（还没按钟写重置那一格）。`reading_reset` ＝ 回包说的几点能再用（卡着的那一格没给时刻时用它）。
/// 回包没说卡在哪一窗 ⇒ 画 5h（这一判也只在这里）。
pub(crate) fn usage_of(
    kind: Kind,
    state: QuotaState,
    limiting: Option<&str>,
    slots: &[SlotShow],
    reading_reset: Option<u64>,
) -> Usage {
    let mut u = if kind == Kind::Api {
        if state == QuotaState::Refused {
            Usage {
                value: Words(copy_text("reason.code.refused", &[])),
                tone: Tone::Fail,
                resets_at: reading_reset,
                ..Usage::default()
            }
        } else {
            Usage {
                value: Words(copy_text("acct.kind.api", &[])),
                tone: Tone::Plain,
                ..Usage::default()
            }
        }
    } else {
        let w = limiting.unwrap_or(SLOT_5H);
        let slot = slots.iter().find(|s| s.slot == w);
        let value = slot.map_or_else(
            || {
                Words(if state == QuotaState::Refused {
                    copy_text("reason.code.refused", &[])
                } else {
                    copy_text("acct.val.none", &[])
                })
            },
            |s| s.text.clone(),
        );
        let (tone, resets_at) = match state {
            QuotaState::Refused => (Tone::Fail, slot.and_then(|s| s.resets_at).or(reading_reset)),
            QuotaState::OverageInUse => (Tone::Warn, None),
            QuotaState::ResetSinceSeen | QuotaState::Unseen => (Tone::Plain, None),
            _ if slot.is_some_and(|s| s.full) => (Tone::Fail, None),
            QuotaState::Near if slot.is_some_and(|s| s.pct.is_some()) => (Tone::Warn, None),
            _ => (Tone::Plain, None),
        };
        Usage {
            slot: Some(w.to_string()),
            window: Some(Words(slot_text(w))),
            value,
            tone,
            resets_at,
            ..Usage::default()
        }
    };
    joined(&mut u);
    u
}

/// 没出过数的号那一格用量（只看种类）。
pub(crate) fn unseen_usage(kind: Kind) -> UnseenUsage {
    usage_of(kind, QuotaState::Unseen, None, &[], None).into()
}

/// 「1% ≈ 多少用量」那一格的字（唯一写法）。
fn per_pct_text(n: u64) -> Words {
    Words(copy_text(
        "acct.val.perPct",
        &[("n", &copy_core::count_short(n))],
    ))
}

/// ★ 判好的显示态写成字（唯一一处）：每个语义位那一格 · 各窗口与语义位的「1% ≈ 多少」· 用量那一格。
/// `reading_reset` ＝ 回包说的几点能再用。
pub(crate) fn written(sh: &mut QuotaShow, reading_reset: Option<u64>) {
    let (state, limiting) = (sh.state, sh.limiting.clone());
    for x in &mut sh.slots {
        (x.text, x.tone) = slot_words(
            state,
            limiting.as_deref() == Some(x.slot.as_str()),
            x.pct,
            x.full,
        );
    }
    for w in &mut sh.windows {
        w.per_pct_text = w.tokens_per_pct.map(per_pct_text);
    }
    for s in &mut sh.slots {
        s.per_pct_text = sh
            .windows
            .iter()
            .find(|w| w.name == s.window)
            .and_then(|w| w.per_pct_text.clone());
    }
    sh.usage = usage_of(
        sh.kind,
        state,
        limiting.as_deref(),
        &sh.slots,
        reading_reset,
    );
}

/// 出口那一下：按钟写时刻旁边那几格。
pub(crate) trait Stamp {
    fn stamp(&mut self, clock: &TextClock);
}

impl Stamp for Usage {
    /// 写重置那一格，再连成一句。
    fn stamp(&mut self, clock: &TextClock) {
        self.reset = self
            .resets_at
            .map(|t| Words(copy_text("acct.reset.at", &[("at", &clock.text(t).0)])));
        joined(self);
    }
}

impl Stamp for SlotShow {
    /// 按钟写 `resets_at` 旁边那两格。
    fn stamp(&mut self, clock: &TextClock) {
        if let Some(t) = self.resets_at {
            let at = clock.text(t);
            self.resets_at_rel_text = clock.rel(t);
            self.reset_text = Some(reset_text(&at, self.resets_at_rel_text.is_none()));
            self.resets_at_text = Some(at);
        }
    }
}

impl Stamp for WindowShow {
    /// 按钟写 `resets_at` 旁边那两格。
    fn stamp(&mut self, clock: &TextClock) {
        if let Some(t) = self.resets_at {
            self.resets_at_text = Some(clock.text(t));
            self.resets_at_rel_text = clock.rel(t);
        }
    }
}

impl Stamp for QuotaShow {
    /// 每一格的时刻字按钟写好。
    fn stamp(&mut self, clock: &TextClock) {
        self.slots.iter_mut().for_each(|s| s.stamp(clock));
        self.windows.iter_mut().for_each(|w| w.stamp(clock));
        self.usage.stamp(clock);
    }
}

/// 重置那一格的字（唯一写法）：还没到 ⇒ `↻18:30`；已过 ⇒ `↻13:10 已过`。
pub(crate) fn reset_text(at: &Words, past: bool) -> Words {
    Words(if past {
        copy_text("acct.reset.past", &[("at", &at.0)])
    } else {
        copy_text("acct.reset.at", &[("at", &at.0)])
    })
}

/// 一个窗口那一行的名字：窗口键 `5h` · `7d` 照语义位的字；`7d:<档>` ⇒「7d <档>」；不算用量窗口的（超额）⇒「超额」。
pub(crate) fn window_label(key: Option<&str>) -> String {
    match key {
        None => copy_text("acct.row.over", &[]),
        Some(k) => match k.split_once(':') {
            Some((slot, tier)) => copy_text(
                "acct.window.tier",
                &[
                    ("slot", &slot_text(slot)),
                    ("tier", &tier.replace('_', " ")),
                ],
            ),
            None => slot_text(k),
        },
    }
}

// ── 每号几行 ───────────────────────────────────────────────────────────────────────

fn c(text: String) -> Cell {
    Cell {
        text: Words(text),
        tone: Tone::Plain,
    }
}

fn toned(text: String, tone: Tone) -> Cell {
    Cell {
        text: Words(text),
        tone,
    }
}

fn text_of(w: Option<&Words>) -> &str {
    w.map_or("", |w| w.0.as_str())
}

fn slot_label(slot: &str) -> Cell {
    c(crate::accounts::quota::name_words::slot_text(slot))
}

fn head(account: &str, kind: Kind, login: LoginState) -> Vec<Cell> {
    let mut row = vec![
        c(crate::accounts::quota::name_words::account_text(account)),
        // 类型字与账号行第二行（`name_words::kind_line`）同一处写。
        c(crate::accounts::quota::name_words::kind_text(kind)),
    ];
    match login {
        LoginState::NeedsLogin => row.push(toned(copy_text("acct.tag.login", &[]), Tone::Warn)),
        LoginState::NeedsKey => row.push(toned(copy_text("acct.tag.key", &[]), Tone::Warn)),
        LoginState::Ok => {}
    }
    row
}

/// 重置那一格与距今那一格（那一时刻旁边写好的字）；已过 ⇒ `↻.. 已过`、没有距今。
fn reset_cells(at: Option<u64>, text: Option<&Words>, rel: Option<&Words>) -> Vec<Cell> {
    if at.is_none() {
        return Vec::new();
    }
    let at = Words(text_of(text).to_string());
    let mut row = vec![c(reset_text(&at, rel.is_none()).0)];
    row.extend(rel.map(|r| c(r.0.clone())));
    row
}

/// 更细的窗口（窗口键带 `:` 的，如 `7d:opus`）各一行：名字 · 值（重置已过 ⇒ `—`，用满 ⇒ `✕`）· ↻ · 距今。
fn tier_rows(show: &QuotaShow) -> impl Iterator<Item = Vec<Cell>> + '_ {
    show.windows
        .iter()
        .filter(|w| w.key.as_deref().is_some_and(|k| k.contains(':')))
        .map(|w| {
            let value = if w.reset_since_seen {
                copy_text("acct.val.none", &[])
            } else if w.tone == Tone::Fail {
                copy_text("acct.val.full", &[])
            } else {
                w.pct.map_or_else(
                    || copy_text("acct.val.none", &[]),
                    |p| copy_text("acct.val.pct", &[("pct", &p.to_string())]),
                )
            };
            let mut row = vec![c(window_label(w.key.as_deref())), toned(value, w.tone)];
            row.extend(reset_cells(
                w.resets_at,
                w.resets_at_text.as_ref(),
                w.resets_at_rel_text.as_ref(),
            ));
            row
        })
}

fn slot_of<'a>(show: &'a QuotaShow, slot: &str) -> Option<&'a SlotShow> {
    show.slots.iter().find(|x| x.slot == slot)
}

fn slot_row(show: &QuotaShow, slot: &str) -> Vec<Cell> {
    let Some(x) = slot_of(show, slot) else {
        return vec![slot_label(slot), c(copy_text("acct.val.none", &[]))];
    };
    let mut row = vec![slot_label(slot), toned(x.text.0.clone(), x.tone)];
    row.extend(reset_cells(
        x.resets_at,
        x.resets_at_text.as_ref(),
        x.resets_at_rel_text.as_ref(),
    ));
    row
}

fn state_row(show: &QuotaShow, reading: &ResetsAt) -> Option<Vec<Cell>> {
    let value = match show.state {
        QuotaState::Refused => toned(copy_text("reason.code.refused", &[]), Tone::Fail),
        QuotaState::ResetSinceSeen => c(copy_text("acct.val.none", &[])),
        _ => return None,
    };
    let mut row = vec![c(copy_text("backend.column.status", &[])), value];
    row.extend(reset_cells(
        reading.resets_at,
        reading.resets_at_text.as_ref(),
        reading.resets_at_rel_text.as_ref(),
    ));
    Some(row)
}

fn seen_row(a: &AccountHead) -> Vec<Cell> {
    let at = &a.seen_at_text.0;
    let value = if a.show.stale {
        toned(copy_text("acct.seen.staleShort", &[("at", at)]), Tone::Warn)
    } else {
        c(at.clone())
    };
    vec![c(copy_text("acct.row.seen", &[])), value]
}

/// 出过数的号那一段。
pub(crate) fn seen_block(a: &AccountHead) -> Rows {
    let show = &a.show;
    let mut rows = vec![head(&a.account, show.kind, show.login)];
    let windowed = show
        .limiting
        .as_deref()
        .is_some_and(|l| !l.is_empty() && slot_of(show, l).is_some());
    let reading = &a.resets_at;
    if show.kind == Kind::Api {
        rows.push(state_row(show, reading).unwrap_or_else(|| {
            vec![
                c(copy_text("acct.slot.both", &[])),
                c(copy_text("acct.val.none", &[])),
                c(copy_text("acct.val.noLimit", &[])),
            ]
        }));
    } else {
        rows.push(slot_row(show, SLOT_5H));
        rows.push(slot_row(show, SLOT_7D));
        rows.extend(tier_rows(show));
        if !windowed {
            rows.extend(state_row(show, reading));
        }
        rows.push(if show.state == QuotaState::OverageInUse {
            vec![
                c(copy_text("acct.row.over", &[])),
                toned(copy_text("acct.val.overOn", &[]), Tone::Warn),
                c(copy_text("acct.val.overBilled", &[])),
            ]
        } else {
            vec![
                c(copy_text("acct.row.over", &[])),
                c(copy_text("acct.val.none", &[])),
            ]
        });
    }
    rows.push(seen_row(a));
    rows
}

/// 没出过数的号那一段：`— 无采样`（按量号 `5h/7d — 无采样`）。
pub(crate) fn unseen_block(u: &UnseenHead) -> Rows {
    let mut rows = vec![head(&u.account, u.kind, u.login)];
    if u.kind == Kind::Api {
        rows.push(vec![
            c(copy_text("acct.slot.both", &[])),
            c(copy_text("acct.val.none", &[])),
            c(copy_text("acct.seen.none", &[])),
        ]);
    } else {
        rows.push(vec![
            slot_label(SLOT_5H),
            c(copy_text("acct.val.none", &[])),
            c(copy_text("acct.seen.none", &[])),
        ]);
        rows.push(vec![
            slot_label(SLOT_7D),
            c(copy_text("acct.val.none", &[])),
        ]);
    }
    rows
}

/// `quota-read` 顶上那一句：读不出 ⇒ 读答的原因那一句（读的哪份 · 原因词；没给才说「原因不明」）；一个号都没有 ⇒ 无采样；否则没有。
pub(crate) fn head_text(
    state: crate::accounts::quota::ledger::ReadState,
    empty: bool,
    reason: Option<&str>,
) -> Option<Words> {
    if state == crate::accounts::quota::ledger::ReadState::Unreadable {
        Some(Words(reason.filter(|r| !r.trim().is_empty()).map_or_else(
            || copy_text("acct.text.readFail", &[]),
            str::to_string,
        )))
    } else if empty {
        Some(Words(copy_text("acct.seen.none", &[])))
    } else {
        None
    }
}

/// 「5h 那一格」（恢复菜单 · 新会话框每个号后面那一格）：`5h 41%` · 卡着的照语义位那一格的字。
pub(crate) fn five_hour(value: &str) -> String {
    copy_text(
        "resumeMenu.account.quota",
        &[
            (
                "slot",
                &crate::accounts::quota::name_words::slot_text(SLOT_5H),
            ),
            ("value", value),
        ],
    )
}

/// `quota-read` 顶上那一格「5h」：账读不出 ⇒ `5h 读不到`（每号那一格这时没有号可挂）；否则没有。
pub(crate) fn head_five_hour(state: crate::accounts::quota::ledger::ReadState) -> Option<Words> {
    (state == crate::accounts::quota::ledger::ReadState::Unreadable)
        .then(|| Words(five_hour(&copy_text("acct.val.unreadable", &[]))))
}

/// 开窗那一判：重置时刻之后再等这么久才看（重置按整点 / 十分对齐，留余量）。
const WARM_MARGIN_S: i64 = 60;
/// 登录拿不到 ⇒ 过这么久再看。
const WARM_LOGIN_RETRY_S: i64 = 60 * 60;
/// 额度账读不懂 ⇒ 过这么久再看。
const WARM_UNREADABLE_RETRY_S: i64 = 15 * 60;

/// 出口那一下：按钟写 `at` 旁边那两格。
pub(crate) fn stamped(mut w: Warm, clock: &crate::common::time::TextClock) -> Warm {
    if let Some(t) = w.at.and_then(|t| u64::try_from(t).ok()) {
        w.at_text = Some(clock.text(t));
    }
    w
}

/// ★ 开窗那一判写成线上那一格：判在 accounts（[`warm_verdict`] 的码），这里只写那一句与几点再看。
/// `seen` ＝ 出过数的号的显示态（没出过数 ⇒ `None`），只用来挑字（卡人的是哪一窗 · 5h 用到几 · 有没有见过 5h 窗）。
/// 说得出几点 ⇒ 那一刻再加 [`WARM_MARGIN_S`]；说不出（按量号 · 要登录 · 缺 key）⇒ 过 [`WARM_LOGIN_RETRY_S`] 再看。
pub(crate) fn warm_of(
    v: WarmVerdict,
    seen: Option<&QuotaShow>,
    clock: &crate::common::time::TextClock,
) -> Warm {
    let now = clock.now;
    let s5 = seen.and_then(|s| slot_of(s, SLOT_5H));
    let (why, until) = match v {
        WarmVerdict::Send(_) => {
            // 发的码只有「5 小时窗没在计时」一种；那一句照显示态挑：没出过数 · 见过 5h 窗（已过重置）· 没见过。
            let text = if seen.is_none() {
                copy_text("acct.warm.unseen", &[])
            } else if s5.is_some() {
                copy_text("acct.warm.expired", &[])
            } else {
                copy_text("acct.warm.noWindow", &[])
            };
            return Warm {
                act: WarmAct::Send,
                at: None,
                at_text: None,
                text: Words(text),
            };
        }
        WarmVerdict::Wait { until, why } => (why, until),
    };
    let at_text = until.map(|u| clock.text(u).0).unwrap_or_default();
    let pct = s5.and_then(|x| x.pct).unwrap_or(0).to_string();
    let text = match why {
        WarmWhy::Api => copy_text("acct.val.noLimit", &[]),
        WarmWhy::Login => copy_text("acct.tag.login", &[]),
        WarmWhy::Key => copy_text("acct.tag.key", &[]),
        WarmWhy::Overage => copy_text("acct.row.over", &[]),
        WarmWhy::Refused => {
            // 卡在哪一窗照显示态；按量号没有窗 ⇒ 只说被拒（订阅号说不出 ⇒ 5h，回包默认卡的那一窗）。
            let w = seen
                .and_then(|s| s.limiting.as_deref())
                .filter(|l| !l.is_empty())
                .or((!seen.is_some_and(|s| s.kind == Kind::Api)).then_some(SLOT_5H));
            match (w, until) {
                (Some(w), Some(_)) => copy_text("acct.warm.refused", &[("w", w), ("at", &at_text)]),
                _ => copy_text("reason.code.refused", &[]),
            }
        }
        WarmWhy::Full(1) => copy_text("acct.warm.weekFull", &[("at", &at_text)]),
        WarmWhy::Full(_) | WarmWhy::Ticking | WarmWhy::NoWindow => {
            copy_text("acct.warm.ticking", &[("pct", &pct), ("at", &at_text)])
        }
    };
    let at = until.map_or(now.saturating_add(WARM_LOGIN_RETRY_S), |u| {
        i64::try_from(u)
            .unwrap_or(i64::MAX)
            .saturating_add(WARM_MARGIN_S)
    });
    stamped(
        Warm {
            act: WarmAct::Wait,
            at: Some(at),
            at_text: None,
            text: Words(text),
        },
        clock,
    )
}

/// ★ 额度账读不懂时每个号那一判：一个都不发（账上说不出它此刻什么样，「没见过就发」那一条不成立），过一阵再看；
/// 那一句照实说哪一份读不懂、为什么（`why` ＝ 顶上那一句，[`head_text`]）。
pub(crate) fn warm_unreadable(why: &str, now: i64) -> Warm {
    Warm {
        act: WarmAct::Wait,
        at: Some(now + WARM_UNREADABLE_RETRY_S),
        at_text: None,
        text: Words(copy_text("acct.warm.unreadable", &[("why", why)])),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/quota_rows_tests.rs"]
mod tests;
