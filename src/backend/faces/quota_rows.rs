//! **额度每号几行**（`quota-read` 每号带的 `rows`）：一个号一段，首行抬头 `名 · 类型 · 号的状态`（号的状态只写这一处、只写一个；几点采的不成行，进这个号的悬停提示），其余每个收到的用量窗口一行、只两样：用量 % · 几点重置 ＋ 还剩多久。
//!
//! 唯一一处行模型：桌面悬停卡、设置页额度那一列、CLI `--text`（通用拼字，见 `stream/cli.rs`）、手机（计量条照 `windows[].row` 指的那一行画）、skill 都只照抄这几行，不按码取字。
//! 窗口那几格的时刻字与距今（后端私有）由出口那一下按同一把钟写（`faces/quota_read.rs`，`TextClock`）；
//! 显示态那一判在 `accounts/quota/show.rs`；判好的写成字与语气（每个语义位那一格 · 用量那一格 · 时刻旁边那几格）也在这里（[`written`] · [`Stamp`]）。
//! 重置时刻已过的那一窗不写旧数：读取中（后端到点在重读，`accounts/quota/reread.rs`）· 已重置（查不了 · 没读成）。超额只在开着或在用时一行。

use crate::accounts::quota::name_words::slot_text;
use crate::accounts::quota::rules::decide::{WarmVerdict, WarmWhy};
use crate::accounts::quota::show::{Lapsed, QuotaShow, SlotShow, WindowShow};
use crate::common::cells::{Tone, Words};
use crate::common::time::TextClock;
use cmd_wire::quota::{Kind, LoginState, QuotaState, UnseenHead, UnseenUsage, Usage, SLOT_5H};

/// 出过数的一个号写行要、却不上线的那几格（后端私有）：看到于那一格的字 · 回包说的几点能再用（照 [`reset_at_text`] 写好的）· 超额那一行。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Written {
    pub(crate) seen_at_text: Words,
    pub(crate) reading_reset_text: Option<Words>,
    /// 超额那一行（[`crate::accounts::quota::show::overage_of`] 判好的：`None` 不出 · `Some(在用)`）。
    pub(crate) overage: Option<bool>,
}
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
    if here && state == QuotaState::Unseen {
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

/// ★ 一个窗口那一格的语气（唯一一处）：照显示态判好的用满 ⇒ fail · 快满 ⇒ warn · 其余常规。
fn window_tone(w: &WindowShow) -> Tone {
    if w.full {
        Tone::Fail
    } else if w.near {
        Tone::Warn
    } else {
        Tone::Plain
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

/// 一个号用量那一格（还没写重置那一格）＋ 它几点重置（被拒才有，交给调用方按钟写）。
/// `reading_reset` ＝ 回包说的几点能再用（卡着的那一格没给时刻时用它）。回包没说卡在哪一窗 ⇒ 画 5h（这一判也只在这里）。
fn usage_of(
    kind: Kind,
    state: QuotaState,
    limiting: Option<&str>,
    slots: &[SlotShow],
    reading_reset: Option<u64>,
) -> (Usage, Option<u64>) {
    let (mut u, resets_at) = if kind == Kind::Api {
        if state == QuotaState::Refused {
            let u = Usage {
                value: Words(copy_text("reason.code.refused", &[])),
                tone: Tone::Fail,
                ..Usage::default()
            };
            (u, reading_reset)
        } else {
            let u = Usage {
                value: Words(copy_text("acct.kind.api", &[])),
                tone: Tone::Plain,
                ..Usage::default()
            };
            (u, None)
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
        let u = Usage {
            slot: Some(w.to_string()),
            window: Some(Words(slot_text(w))),
            value,
            tone,
            ..Usage::default()
        };
        (u, resets_at)
    };
    joined(&mut u);
    (u, resets_at)
}

/// 没出过数的号那一格用量（只看种类；没出过数就说不出几点重置）。
pub(crate) fn unseen_usage(kind: Kind) -> UnseenUsage {
    usage_of(kind, QuotaState::Unseen, None, &[], None).0.into()
}

/// 「1% ≈ 多少用量」那一格的字（唯一写法）。
fn per_pct_text(n: u64) -> Words {
    Words(copy_text(
        "acct.val.perPct",
        &[("n", &copy_core::count_short(n))],
    ))
}

/// ★ 判好的显示态写成字（唯一一处）：每个语义位那一格 · 各窗口与语义位的「1% ≈ 多少」· 用量那一格（重置那一格按 `clock` 当场写）。
/// `reading_reset` ＝ 回包说的几点能再用。
pub(crate) fn written(sh: &mut QuotaShow, reading_reset: Option<u64>, clock: &TextClock) {
    let (state, limiting) = (sh.state, sh.limiting.clone());
    let lapsed_here = sh.settle_lapsed();
    for x in &mut sh.slots {
        // 它取的那一窗过了重置时刻 ⇒ 同那一窗那一行的字（读取中 / 已重置），不写数；卡着的那一格照号级那一判（同一个判法）。
        let here = limiting.as_deref() == Some(x.slot.as_str());
        let lapsed = sh
            .windows
            .iter()
            .find(|w| w.name == x.window)
            .and_then(|w| w.lapsed)
            .or(lapsed_here.filter(|_| here));
        (x.text, x.tone) = match lapsed {
            Some(l) => {
                // 那一窗的数过时了：不上线（不写旧数，也不编 0）。
                (x.pct, x.full) = (None, false);
                (Words(lapsed_text(l)), Tone::Plain)
            }
            None => slot_words(state, here, x.pct, x.full),
        };
    }
    for w in &mut sh.windows {
        if w.lapsed.is_some() {
            (w.pct, w.full, w.near) = (None, false, false);
        }
        w.tone = window_tone(w);
        w.per_pct_text = w.tokens_per_pct.map(per_pct_text);
    }
    for s in &mut sh.slots {
        s.per_pct_text = sh
            .windows
            .iter()
            .find(|w| w.name == s.window)
            .and_then(|w| w.per_pct_text.clone());
    }
    let (mut u, resets_at) = usage_of(
        sh.kind,
        state,
        limiting.as_deref(),
        &sh.slots,
        reading_reset,
    );
    u.reset = resets_at.map(|t| reset_text(&Words(reset_at_text(t, clock))));
    joined(&mut u);
    sh.usage = u;
}

/// 出口那一下：按钟写时刻旁边那几格。
pub(crate) trait Stamp {
    fn stamp(&mut self, clock: &TextClock);
}

/// ★ 紧凑那一格（账号行 · 起会话菜单）的重置那一格（唯一写法，同行里那一格）：`↻18:30` · `↻明天 04:00`；过了 ⇒ 不写那一刻（手上那份读数过时了，不说「已过」）。
pub(crate) fn slot_reset(resets_at: Option<u64>, clock: &TextClock) -> Option<Words> {
    resets_at
        .filter(|t| clock.rel(*t).is_some())
        .map(|t| reset_text(&Words(reset_at_text(t, clock))))
}

impl Stamp for WindowShow {
    /// 按钟写 `resets_at` 旁边那两格（时刻字照 [`reset_at_text`]）。
    fn stamp(&mut self, clock: &TextClock) {
        if let Some(t) = self.resets_at {
            self.resets_at_text = Some(Words(reset_at_text(t, clock)));
            self.resets_at_rel_text = clock.rel(t);
        }
    }
}

impl Stamp for QuotaShow {
    /// 每一格的时刻字按钟写好。
    fn stamp(&mut self, clock: &TextClock) {
        self.windows.iter_mut().for_each(|w| w.stamp(clock));
    }
}

/// 重置那一格的字（唯一写法）：`↻18:30`（过了的那一刻不写，调用方不交）。
pub(crate) fn reset_text(at: &Words) -> Words {
    Words(copy_text("acct.reset.at", &[("at", &at.0)]))
}

/// ★ 几点重置那一刻的字（唯一写法：行 · 按钮那一格 · 起会话菜单那一格 · 开窗那句都照它）：当天 ⇒ `HH:MM`；
/// 明天（看的那一台的本地日期比今天晚一天，[`TextClock::days_from_today`]）⇒「明天 HH:MM」；别天 ⇒ 带日期（[`TextClock::text`]）。
pub(crate) fn reset_at_text(t: u64, clock: &TextClock) -> String {
    if clock.days_from_today(t) == 1 {
        copy_text("acct.reset.tomorrow", &[("at", &clock.face(t).0)])
    } else {
        clock.text(t).0
    }
}

/// 一个用量窗口那一行的名字（唯一写法，照窗口键）：`5h` · `7d` 照语义位的字；`7d:<档>` ⇒「7d <档>」（档名照字段名，`_` 换空格）。
pub(crate) fn window_label(key: &str) -> String {
    match key.split_once(':') {
        Some((slot, tier)) => copy_text(
            "acct.window.tier",
            &[
                ("slot", &slot_text(slot)),
                ("tier", &tier.replace('_', " ")),
            ],
        ),
        None => span_label(key).unwrap_or_else(|| slot_text(key)),
    }
}

/// 照时长写的窗口键（[`crate::common::time::span_key`]：`6h` · `30d` · `90m`）给人看的名字，经文案表；认得的语义位（`5h` · `7d`）照语义位的字。不是这一形 ⇒ 缺。
fn span_label(key: &str) -> Option<String> {
    if key == SLOT_5H || key == cmd_wire::quota::SLOT_7D {
        return None;
    }
    let unit = key.chars().last()?;
    let n: u64 = key[..key.len() - unit.len_utf8()].parse().ok()?;
    let n = n.to_string();
    match unit {
        'd' => Some(copy_text("durationFormat.short.day", &[("n", &n)])),
        'h' => Some(copy_text("durationFormat.short.hour", &[("n", &n)])),
        'm' => Some(copy_text("durationFormat.short.min", &[("n", &n)])),
        _ => None,
    }
}

// ── 每号几行 ───────────────────────────────────────────────────────────────────────

fn c(text: String) -> Cell {
    toned(text, Tone::Plain)
}

fn toned(text: String, tone: Tone) -> Cell {
    Cell {
        text: Words(text),
        tone,
    }
}

/// ★ 抬头那一行：名 · 类型 · 号的状态（只写一个，唯一判法）。号的状态只写在这一行，先后：登录态（需登录 · 需 key · 不由 cc-monitor 续）>
/// `rest`（调用方交的那一格：被拒至几点 · 无采样）；登录态不对就只写登录态（需登录自然无采样，两样都写是重复）。
fn head(
    agent: &str,
    account: &str,
    kind: Kind,
    login: LoginState,
    rest: Option<Cell>,
) -> Vec<Cell> {
    let mut row = vec![
        c(crate::accounts::quota::name_words::account_text_of(
            agent, account,
        )),
        // 类型字与账号行第二行（`name_words::kind_line`）同一处写。
        c(crate::accounts::quota::name_words::kind_text(kind)),
    ];
    match login {
        LoginState::NeedsLogin => row.push(toned(copy_text("acct.tag.login", &[]), Tone::Warn)),
        LoginState::NeedsKey => row.push(toned(copy_text("acct.tag.key", &[]), Tone::Warn)),
        LoginState::BaseUntouched => {
            row.push(toned(copy_text("acct.tag.baseUntouched", &[]), Tone::Warn));
        }
        LoginState::Ok => row.extend(rest),
    }
    row
}

/// ★ 账号行（设置页）那几行（唯一一处）：这个号那一段（[`seen_block`] · [`unseen_block`]）去掉抬头里账号行自己有的那几格
/// （[`head`] 写的名 · 类型 · 登录态），剩下的状态（被拒到几点 · 无采样）留成一行（没有就不成行），其余各行照抄。
/// `stale` ＝ 那一号的数旧（[`QuotaShow::stale`]）⇒ 名字之外常规的那几格画警示（[`stale_tone`]，同紧凑那一格）。
pub(crate) fn account_line_rows(rows: &Rows, login: LoginState, stale: bool) -> Rows {
    let Some((top, rest)) = rows.split_first() else {
        return Vec::new();
    };
    // 抬头只写一个状态（[`head`]）：登录态不对 ⇒ 那一格就是登录态，账号行自己有；否则名 · 类型之后那一格（有就）留成一行。
    let state: Vec<Cell> = if login == LoginState::Ok {
        top.iter().skip(2).cloned().collect()
    } else {
        Vec::new()
    };
    (!state.is_empty())
        .then_some(state)
        .into_iter()
        .chain(rest.iter().cloned())
        .map(|mut r| {
            r.iter_mut()
                .skip(1)
                .for_each(|c| c.tone = stale_tone(c.tone, stale));
            r
        })
        .collect()
}

/// ★ 数旧的语气（唯一判法：账号行那几行 · 紧凑那一格）：数旧 ⇒ 常规画警示；别的语气照旧。
pub(crate) fn stale_tone(tone: Tone, stale: bool) -> Tone {
    if stale && tone == Tone::Plain {
        Tone::Warn
    } else {
        tone
    }
}

/// 一个窗口那一行（只两样）：名字 · 用量 % · 几点重置 · 还剩多久。重置已过 ⇒ 名字 ·「读取中」/「已重置」，不写旧数与那一刻；
/// 没有数的那一窗不出行。
fn window_row(w: &WindowShow) -> Option<Vec<Cell>> {
    // 没有窗口键的（超额那一档）不算用量窗口：超额另有一行（[`overage_row`]）。
    let name = c(window_label(w.key.as_deref()?));
    if let Some(l) = w.lapsed {
        return Some(vec![name, c(lapsed_text(l))]);
    }
    let pct = w.pct?;
    let mut row = vec![
        name,
        toned(
            copy_text("acct.val.pct", &[("pct", &pct.to_string())]),
            w.tone,
        ),
    ];
    if let Some(at) = &w.resets_at_text {
        row.push(c(reset_text(at).0));
        row.extend(w.resets_at_rel_text.iter().map(|r| c(r.0.clone())));
    }
    Some(row)
}

/// ★ 过了重置时刻那一窗的字（唯一写法）：读取中 · 已重置。
fn lapsed_text(l: Lapsed) -> String {
    match l {
        Lapsed::Reading => copy_text("kit.state.reading", &[]),
        Lapsed::Reset => copy_text("acct.window.reset", &[]),
    }
}

/// 「超额」那一行：只在开着或在用时出（`over` ＝ [`crate::accounts::quota::show::overage_of`]）；在用 ⇒ 警示。
fn overage_row(over: Option<bool>) -> Option<Vec<Cell>> {
    over.map(|in_use| {
        let value = if in_use {
            toned(copy_text("acct.val.overOn", &[]), Tone::Warn)
        } else {
            c(copy_text("acct.val.overReady", &[]))
        };
        vec![c(copy_text("acct.row.over", &[])), value]
    })
}

/// 出过数的号那一段（`agent` ＝ 线上的 kind 名，`show` ＝ 后端的显示态行，造线上那一格之前）：
/// 抬头（被拒 ⇒ 带被拒到几点）· 收到的每个用量窗口一行 · 超额开着才一行。几点采的不成行（[`sampled_text`]）。
/// 成了行的那一窗记下它是第几行（`row`，手机计量条照那一行画）。
pub(crate) fn seen_block(agent: &str, account: &str, show: &mut QuotaShow, w: &Written) -> Rows {
    // 被拒到几点照回包说的那一刻（「被拒」那一判 `facts::refused_at` 也照它）。
    let refused = (show.state == QuotaState::Refused).then(|| {
        toned(
            w.reading_reset_text.as_ref().map_or_else(
                || copy_text("reason.code.refused", &[]),
                |at| copy_text("acct.head.refused", &[("at", &at.0)]),
            ),
            Tone::Fail,
        )
    });
    let top = head(agent, account, show.kind, show.login, refused);
    let mut rows = vec![top];
    for win in &mut show.windows {
        win.row = window_row(win).map(|r| {
            rows.push(r);
            u32::try_from(rows.len() - 1).unwrap_or(u32::MAX)
        });
    }
    rows.extend(overage_row(w.overage));
    rows
}

/// ★ 几点采的那一句（唯一写法）：「采样于 12:03」；不成行，出口放进这个号的悬停提示（`QuotaAccount.sampled`）。
pub(crate) fn sampled_text(w: &Written) -> Words {
    Words(copy_text(
        "beLastSeen.said.sampled",
        &[("time", &w.seen_at_text.0)],
    ))
}

/// 没出过数的号那一段：只有抬头，状态那一格写「无采样」（登录态不对 ⇒ 照 [`head`] 只写登录态；没收到的窗口不出行）。
pub(crate) fn unseen_block(u: &UnseenHead) -> Rows {
    vec![head(
        u.agent.as_str(),
        &u.account,
        u.kind,
        u.login,
        Some(c(copy_text("acct.seen.none", &[]))),
    )]
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

fn slot_of<'a>(show: &'a QuotaShow, slot: &str) -> Option<&'a SlotShow> {
    show.slots.iter().find(|x| x.slot == slot)
}

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
/// 说得出几点 ⇒ 那一刻再加 [`crate::accounts::quota::RESET_MARGIN_S`]（同到点重读那一处的余量）；说不出（按量号 · 要登录 · 缺 key）⇒ 过 [`WARM_LOGIN_RETRY_S`] 再看。
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
            } else if let Some(s) = s5 {
                // 见过 5h 窗、它没在计时 ⇒ 照 5h 那一格写好的字（同那一行：`5h 读取中` · `5h 已重置`）。
                copy_text(
                    "acct.usage.wv",
                    &[("w", &slot_text(SLOT_5H)), ("v", &s.text.0)],
                )
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
    // 几点重置同行的写法（明天写「明天 HH:MM」）。
    let at_text = until.map(|u| reset_at_text(u, clock)).unwrap_or_default();
    let pct = s5.and_then(|x| x.pct).unwrap_or(0).to_string();
    let text = match why {
        WarmWhy::Api => copy_text("acct.val.noLimit", &[]),
        WarmWhy::Login => copy_text("acct.tag.login", &[]),
        WarmWhy::Key => copy_text("acct.tag.key", &[]),
        WarmWhy::BaseUntouched => copy_text("acct.tag.baseUntouched", &[]),
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
            .saturating_add_unsigned(crate::accounts::quota::RESET_MARGIN_S)
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
