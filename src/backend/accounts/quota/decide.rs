//! ★ 换号的**唯一判定**（纯函数）：此刻这个会话的号该不该换、换到谁、为什么。只有一个谓词 ＋ 一个挑法。
//!
//! **谓词** [`standing`]：一个号此刻能不能用、为什么、几点回来 —— 唯一说这件事的地方：
//!
//! | 结论 | 判（先到先得） |
//! |---|---|
//! | 被拒 | 刚回来的回包被拒（429）；或额度账上被拒、还没到它说的回来时刻（[`refused_at`]） |
//! | 超额在兜 | 订阅号正在用付费超额、卡着的窗口还没重置 |
//! | 过上限 | 有窗口用到它的上限（线）、还没重置。线按 [`layers`] 取（唯一一处）：这号这窗口 → 这号这语义位 → 这号全部窗口 → 所有号这语义位（`cap["*"]`，即规则一级的「触发」那一行；那一格空着 ＝ 那一窗满了才换）；按时段写的取此刻（本地钟）落在的那一段，落不进 ⇒ 往下一层。此刻取到的上限是 `0` ⇒ 这个号不用：不看用量（刚重置的 0%、从没见过都算），回来的时刻 ＝ 那一段的止 |
//! | 能用 | 其余 |
//!
//! 「几点回来」＝ 卡着它的每一处（被拒 / 超额那个窗口 · 过了上限的每个窗口）都重置的那一刻；有一处说不出 ⇒ 说不出。
//! 记下的线（`n`）· 窗口（语义位 `w`）· 窗口键 · 几点回来取**同一处**：卡得最久的那一处（说不出的那一处算最久）。
//!
//! **挑法** [`decide`]：
//! 1. 此刻的号能用：此刻的号是兜底号（`fallback`）、池里有非兜底的号能用了 ⇒ 切到首个能用的那个（按池序，不管换法：
//!    兜底只临时用）；这一段用完了单段预算（`stint`，软的）⇒ 想走；或 `preempt` 开着、换进它那一刻挡在它前面的号
//!    （[`blocked_above`]，随会话记下）有一个又能用了 ⇒ 切回去（按池序取首个）；
//!    都不是 ⇒ 不换。想走而没有能接的 ⇒ 留着。刚回来的那一发照过了 ⇒ 这一问不换。
//! 2. 此刻的号不能用 ⇒ 按池序**从头**取首个能用、接得上的号（列表顺序 ＝ 偏好；超额在兜 ⇒ 只找订阅号接，没有就留在超额 `toOverage`）。
//!    **切兜底前最多等几分钟**（`wait` 分钟，`0` ＝ 不等）：取到的号是兜底号（`fallback`），而池里非兜底的号（含此刻的）有一个
//!    在 `wait` 分钟内回来 ⇒ 不切兜底：此刻的号被拒 ⇒ 这一发不发上游、回「用满」（[`Verdict::Wait`]，回来的时刻 ＝ 那个号回来的时刻）；
//!    此刻的号只是过了上限 ⇒ 硬上限照 `Wait` 办，软上限留在此刻的号上照发。往非兜底号切照旧立刻切；没标兜底 ⇒ 不等。
//!    这一家给不出「用满」回包 ⇒ 照旧切。超额在兜不等。
//! 3. 一个都没有 ⇒ 照到上限那一格：`continue` 且此刻的号被拒 ⇒ 退一步取首个没被拒、也不在超额上的（不管上限）；
//!    `stop` 且这份轮换有上限（触发那一行设了线，或给池里的号设了上限）、说得出几点有号回来 ⇒ 这一发不发上游（[`Verdict::Hold`]）；
//!    其余 ⇒ 不换（被拒就原样交回上游的拒绝，过上限就留在此刻的号上照发）。
//!
//! 今天的几种情况都是它的特例：429 ＝ 此刻的号被拒 · 到 N% ＝ 触发那一行的线 · 超额在兜 ＝ 只换到能用的订阅号 · `atLimit` 照旧。
//! 候选只从池里、且不含此刻的与这一发试过的 ⇒ 一发请求至多换「池子大小」次，不打转。
//! 不会来回抖：一个窗口里用量只增不减 ⇒「能用 → 不能用」只因用量涨、「不能用 → 能用」只因重置（或时段换了上限）；
//! `preempt` 只在挡在前面的号回来那一刻改结论（手动换过来时本就能用的号不在那份名单里 ⇒ 手动换号不会被切回）；`stint` 按换进来那一刻的基线算，每换进来一次只用一次。

use super::reset_since_seen;
use super::rotation::{
    AtLimit, Base, Baseline, CapValue, Caps, Stints, SwitchWhy, Unready, ALL_WINDOWS, LINE_SLOTS,
};
use crate::agents::{QuotaReading, QuotaWindow};

/// 号的种类（接超额只找订阅号）：线上 `sub` · `api`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "QuotaKind"
    )
)]
pub enum Kind {
    Sub,
    Api,
}

/// 判一次要的全部事实。
pub(crate) struct Facts<'a> {
    /// 这个会话的轮换池（按序）。
    pub(crate) pool: &'a [String],
    /// 每号的上限（线）；`"*"` 那一行 ＝ 所有号这语义位（规则一级的「触发」）。
    pub(crate) cap: &'a Caps,
    /// 每号的单段预算。
    pub(crate) stint: &'a Stints,
    /// 前面的号能用了就切回去。
    pub(crate) preempt: bool,
    /// 到上限没号可换时怎么办（这一家给不出「用满」回包 ⇒ 调用方交 `continue`）。
    pub(crate) at_limit: AtLimit,
    /// 兜底的号。
    pub(crate) fallback: &'a [String],
    /// 切兜底前最多等几分钟（`0` ＝ 不等）。
    pub(crate) wait: u8,
    /// 这一家给得出「用满」回包（停着等那一下要它）。
    pub(crate) can_hold: bool,
    /// 此刻走的号（刚回来的那个回包就是它答的）。
    pub(crate) current: &'a str,
    /// 此刻的号这一段的基线（换进来那一刻各窗口的用量）。
    pub(crate) base: &'a Baseline,
    /// 换进此刻的号那一刻，池里排在它前面、当时不能用的号（`preempt` 只等它们回来）。
    pub(crate) above: &'a [String],
    pub(crate) now: u64,
    /// 这台后端此刻的本地钟比 UTC 快几秒（按时段写的上限按本地钟取）。
    pub(crate) offset: i64,
    /// 刚回来的回包读成的额度快照；问「发之前」时为 `None`。
    pub(crate) heard: Option<&'a QuotaReading>,
    /// 额度账上各号最近的快照。
    pub(crate) seen: &'a dyn Fn(&str) -> Option<QuotaReading>,
    pub(crate) kind: &'a dyn Fn(&str) -> Kind,
    /// 窗口名 → 语义位（`5h` / `7d`；换号记录里说卡在哪用）。
    pub(crate) slot: &'a dyn Fn(&str) -> Option<&'static str>,
    /// 窗口名 → 窗口键（适配层给，不透明；上限 · 单段预算按它配）。
    pub(crate) key: &'a dyn Fn(&str) -> Option<String>,
    /// 这一发已经试过的号（含此刻的）。
    pub(crate) tried: &'a [String],
}

/// 判的结果。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Verdict {
    /// 不用换。
    Stay,
    /// 换到 `to`。
    Switch {
        to: String,
        why: SwitchWhy,
        from_resets_at: Option<u64>,
        skipped: Vec<(String, Unready)>,
    },
    /// 该换、没得换：照原样走（被拒 ⇒ 原样交回；超额在兜 ⇒ `why = toOverage`，留在超额）。
    Stuck {
        why: SwitchWhy,
        from_resets_at: Option<u64>,
        skipped: Vec<(String, Unready)>,
    },
    /// 硬上限：池里没有能接的号 ⇒ 这一发不发上游，回那一家的「用满」回包；`back` ＝ 池里最早回来的那一刻。
    Hold {
        n: u8,
        /// 卡着的那一窗（语义位）；说不出 ⇒ `None`。
        w: Option<String>,
        back: Back,
        skipped: Vec<(String, Unready)>,
    },
    /// 切兜底前等一等：本要切到兜底号 `instead`，非兜底的 `back.account` 很快回来 ⇒ 这一发不发上游，回「用满」（重置时刻 ＝ `back.at`）。
    Wait {
        instead: String,
        back: Back,
        skipped: Vec<(String, Unready)>,
    },
}

/// 池里最早回来的那个号：几点 · 卡着它到最后的那个窗口的窗口键（`5h` · `7d` · `7d:<模型>`；说不出 ⇒ `None`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Back {
    pub(crate) account: String,
    pub(crate) at: u64,
    pub(crate) slot: Option<String>,
}

/// ★ 一个号此刻能不能用。`until` ＝ 几点回来（说不出 ⇒ `None`）；`key` ＝ 卡着它到最后的那个窗口的窗口键。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Standing {
    Usable,
    Refused {
        until: Option<u64>,
        key: Option<String>,
    },
    Overage {
        until: Option<u64>,
        key: Option<String>,
    },
    /// 过上限：`n` · `w`（语义位）· `key` · `until` 都取卡得最久的那一处；`n == 0` ＝ 此刻那一格取到 0（时段停用）。
    OverCap {
        n: u8,
        w: Option<String>,
        until: Option<u64>,
        key: Option<String>,
    },
}

impl Standing {
    pub(crate) fn usable(&self) -> bool {
        matches!(self, Standing::Usable)
    }

    /// 被拒或超额在兜（不管上限）。
    fn shut(&self) -> bool {
        matches!(self, Standing::Refused { .. } | Standing::Overage { .. })
    }

    /// 几点回来 ＋ 卡到最后的那个窗口（能用 ⇒ `None`）。
    fn back(&self) -> Option<(Option<u64>, Option<String>)> {
        match self {
            Standing::Usable => None,
            Standing::Refused { until, key }
            | Standing::Overage { until, key }
            | Standing::OverCap { until, key, .. } => Some((*until, key.clone())),
        }
    }
}

fn live(t: Option<u64>, now: u64) -> bool {
    !reset_since_seen(t, now)
}

/// 「用到 N%」的唯一判法：这个窗口用到 `n`%、还没重置。
fn over(w: &QuotaWindow, n: u8, now: u64) -> bool {
    w.used.is_some_and(|u| u * 100.0 >= f64::from(n)) && live(w.resets_at, now)
}

/// 「快满」那一问（显示态用）：有语义位（`5h` / `7d`）的窗口里，第一个用到 `n(窗口名)`%、还没重置的。与 [`standing`] 的「过上限」同一个判法。
pub(crate) fn window_over<'r>(
    r: &'r QuotaReading,
    n: &dyn Fn(&str) -> u8,
    now: u64,
    slot: &dyn Fn(&str) -> Option<&'static str>,
) -> Option<&'r QuotaWindow> {
    r.windows
        .iter()
        .filter(|w| slot(&w.name).is_some())
        .find(|w| over(w, n(&w.name), now))
}

/// 这一窗用到它的线、还没重置（显示态「到线」那一枚签）。
pub(crate) fn at_line(w: &QuotaWindow, n: u8, now: u64) -> bool {
    n > 0 && over(w, n, now)
}

/// 「额度账上这个号此刻被拒着」的唯一判法：上次那一发被拒、它说的回来时刻还没到（适配层给被拒的快照恒带时刻；
/// 说不出时刻的旧账 ⇒ 不算）。显示态（`show`）同问这一处。
pub(crate) fn refused_at(r: &QuotaReading, now: u64) -> bool {
    r.refused && r.resets_at.is_some_and(|t| t > now)
}

/// 订阅号正在用付费超额、卡着的窗口还没重置。
pub(crate) fn overage_in_use(r: &QuotaReading, kind: Kind, now: u64) -> bool {
    kind == Kind::Sub && r.overage.as_ref().is_some_and(|o| o.in_use) && live(r.resets_at, now)
}

/// 此刻是本地钟一天里的第几分钟。
fn minute_of(f: &Facts<'_>) -> u16 {
    minute_at(f.now, f.offset)
}

fn minute_at(now: u64, offset: i64) -> u16 {
    let local = i128::from(now) + i128::from(offset);
    u16::try_from(local.rem_euclid(86_400) / 60).unwrap_or(0)
}

/// 一格此刻取到的值：一个数 ⇒ 它；按时段 ⇒ 此刻落在的那一段（落不进 ⇒ `None`，往下一层）。
fn pick(v: &CapValue, minute: u16) -> Option<u8> {
    match v {
        CapValue::N(n) => Some(*n),
        CapValue::Slots(s) => s.iter().find(|x| x.holds(minute)).map(|x| x.n),
    }
}

/// ★ 线的层次（唯一一处）：这号这窗口（`key`）→ 这号这语义位（`slot`，与 `key` 同名时不重复）→ 这号全部窗口 →
/// 所有号这语义位（`cap["*"][slot]`，规则一级的「触发」那一行）。`key == "*"`（全部窗口那一格）⇒ 只有这号全部窗口那一层。
/// 没写的层不出；每层带它是哪一层、那一格的值、触发那一层是哪一窗。
fn layers<'c>(
    cap: &'c Caps,
    account: &str,
    key: &str,
    slot: Option<&'static str>,
) -> Vec<(CapLayer, &'c CapValue, Option<&'static str>)> {
    let per = cap.get(account);
    let cell = |m: Option<&'c std::collections::BTreeMap<String, CapValue>>, k: &str| {
        m.and_then(|m| m.get(k))
    };
    let mut out = Vec::new();
    if key != ALL_WINDOWS {
        out.extend(cell(per, key).map(|v| (CapLayer::Window, v, None)));
        if let Some(s) = slot.filter(|s| *s != key) {
            out.extend(cell(per, s).map(|v| (CapLayer::Window, v, None)));
        }
    }
    out.extend(cell(per, ALL_WINDOWS).map(|v| (CapLayer::All, v, None)));
    if key != ALL_WINDOWS {
        if let Some(s) = slot {
            out.extend(cell(cap.get(ALL_WINDOWS), s).map(|v| (CapLayer::Trigger, v, Some(s))));
        }
    }
    out
}

/// 这个号这个窗口（窗口键 `key` · 语义位 `slot`）在本地钟第 `minute` 分钟取的线；哪层都没有 ⇒ `None`（那一窗满了才换）。
fn line_in(
    cap: &Caps,
    account: &str,
    key: &str,
    slot: Option<&'static str>,
    minute: u16,
) -> Option<u8> {
    layers(cap, account, key, slot)
        .into_iter()
        .find_map(|(_, v, _)| pick(v, minute))
}

/// 显示态用：这个号这个窗口此刻（`now` · 本地钟比 UTC 快 `offset` 秒）取的线。与 [`cap_of`] 同一处。
pub(crate) fn line_at(
    cap: &Caps,
    account: &str,
    key: &str,
    slot: Option<&'static str>,
    now: u64,
    offset: i64,
) -> Option<u8> {
    line_in(cap, account, key, slot, minute_at(now, offset))
}

/// 这个号这个窗口此刻的上限（没有 ⇒ `None`：那一窗满了才换）。
pub(crate) fn cap_of(
    f: &Facts<'_>,
    account: &str,
    key: &str,
    slot: Option<&'static str>,
) -> Option<u8> {
    line_in(f.cap, account, key, slot, minute_of(f))
}

/// 窗口键 → 语义位（照这个号的快照里同键的那个窗口；认不出 ⇒ `None`）。
fn slot_of_key(f: &Facts<'_>, r: Option<&QuotaReading>, key: &str) -> Option<String> {
    r?.windows
        .iter()
        .find(|w| (f.key)(&w.name).as_deref() == Some(key))
        .and_then(|w| (f.slot)(&w.name))
        .map(str::to_string)
}

/// 这个号此刻有没有哪一格上限取到 `0`（不用它）：有 ⇒ （那一格到几点不再是 `0`：按时段写的取那一段的止，写死的 `0` 说不出, 窗口键（`*` ⇒ `None`）, 语义位）。
/// 触发那一行不收 `0` ⇒ 只看这号自己那一行。
fn off_now(
    f: &Facts<'_>,
    account: &str,
    r: Option<&QuotaReading>,
) -> Option<(Option<u64>, Option<String>, Option<String>)> {
    let per = f.cap.get(account)?;
    let minute = minute_of(f);
    per.keys().find_map(|k| {
        let (_, v, _) = layers(f.cap, account, k, None)
            .into_iter()
            .find(|(_, v, _)| pick(v, minute).is_some())?;
        if pick(v, minute) != Some(0) {
            return None;
        }
        let until = match v {
            CapValue::Slots(s) => s
                .iter()
                .find(|x| x.n == 0 && x.holds(minute))
                .and_then(|x| span_end(f, &x.at)),
            CapValue::N(_) => None,
        };
        let key = (k != ALL_WINDOWS).then(|| k.clone());
        let w = key.as_deref().and_then(|k| slot_of_key(f, r, k));
        Some((until, key, w))
    })
}

/// 此刻所在的那一段 `at` 几点止（unix 秒）。
fn span_end(f: &Facts<'_>, at: &str) -> Option<u64> {
    let (_, to) = super::rotation::span_of(at)?;
    let local = i128::from(f.now) + i128::from(f.offset);
    let into_day = local.rem_euclid(86_400);
    let left = (i128::from(to) * 60 - into_day).rem_euclid(86_400);
    let left = if left == 0 { 86_400 } else { left };
    u64::try_from(i128::from(f.now) + left).ok()
}

/// 这份轮换有没有上限（触发那一行设了线，或给池里哪个号设了）：硬上限只在有上限时成立。
fn has_caps(f: &Facts<'_>) -> bool {
    f.cap.get(ALL_WINDOWS).is_some_and(|m| !m.is_empty())
        || f.pool.iter().any(|a| f.cap.contains_key(a))
}

/// 卡着一个号的一处：重置时刻 · 窗口键 · 那一处的线（被拒 / 超额那一处没有）· 语义位。
struct Part {
    t: Option<u64>,
    key: Option<String>,
    n: Option<u8>,
    w: Option<String>,
}

/// ★ 谓词：读成 `r` 的这个号此刻能不能用。`heard` ⇒ `r` 是刚回来的那个回包（被拒就是被拒，不看时刻）。
fn standing_in(f: &Facts<'_>, account: &str, r: &QuotaReading, heard: bool) -> Standing {
    let mut parts: Vec<Part> = Vec::new();
    let refused = (heard && r.refused) || refused_at(r, f.now);
    let overage = !refused && overage_in_use(r, (f.kind)(account), f.now);
    if refused || overage {
        parts.push(Part {
            t: r.resets_at,
            key: r.limiting.as_deref().and_then(|l| (f.key)(l)),
            n: None,
            w: r.limiting
                .as_deref()
                .and_then(|l| (f.slot)(l))
                .map(str::to_string),
        });
    }
    if let Some((t, key, w)) = off_now(f, account, Some(r)) {
        parts.push(Part {
            t,
            key,
            n: Some(0),
            w,
        });
    }
    for w in &r.windows {
        let Some(k) = (f.key)(&w.name) else { continue };
        let slot = (f.slot)(&w.name);
        // 取到 0 的那一格上面已经记过（不看用量）。
        let Some(n) = cap_of(f, account, &k, slot).filter(|n| *n > 0) else {
            continue;
        };
        if over(w, n, f.now) {
            parts.push(Part {
                t: w.resets_at,
                key: Some(k),
                n: Some(n),
                w: slot.map(str::to_string),
            });
        }
    }
    // 卡得最久的那一处：说不出几点的那一处（先列的）；都说得出 ⇒ 最晚那一处（一样晚取先列的）。
    let mut bind: Option<Part> = None;
    let mut unknown = false;
    for p in parts {
        let later = match (&bind, p.t) {
            (_, None) => !unknown,
            (None, Some(_)) => true,
            (Some(b), Some(t)) => !unknown && b.t.is_some_and(|bt| t > bt),
        };
        if p.t.is_none() {
            unknown = true;
        }
        if later {
            bind = Some(p);
        }
    }
    let Some(p) = bind else {
        return Standing::Usable;
    };
    let until = p.t.filter(|_| !unknown);
    if refused {
        Standing::Refused { until, key: p.key }
    } else if overage {
        Standing::Overage { until, key: p.key }
    } else {
        Standing::OverCap {
            n: p.n.unwrap_or(100),
            w: p.w,
            until,
            key: p.key,
        }
    }
}

/// ★ 谓词：这个号此刻能不能用（按额度账；没见过 ⇒ 只看上限有没有取到 `0`）。
pub(crate) fn standing(account: &str, f: &Facts<'_>) -> Standing {
    match (f.seen)(account) {
        Some(r) => standing_in(f, account, &r, false),
        None => off_now(f, account, None).map_or(Standing::Usable, |(until, key, w)| {
            Standing::OverCap {
                n: 0,
                w,
                until,
                key,
            }
        }),
    }
}

/// 此刻的号：刚回来的回包被拒 ⇒ 按它判；没被拒（那一发照过了）⇒ 能用；问「发之前」⇒ 按额度账。
fn current_standing(f: &Facts<'_>) -> Standing {
    match f.heard {
        Some(r) if r.refused => standing_in(f, f.current, r, true),
        Some(_) => Standing::Usable,
        None => standing(f.current, f),
    }
}

/// 此刻那个号的快照（刚回来的那个优先）。
fn current_reading(f: &Facts<'_>) -> Option<QuotaReading> {
    f.heard.cloned().or_else(|| (f.seen)(f.current))
}

/// 一个窗口此刻的用量（已重置、未计时 ⇒ 0）。
pub(crate) fn used_now(w: &QuotaWindow, now: u64) -> f64 {
    if reset_since_seen(w.resets_at, now) {
        0.0
    } else {
        w.used.unwrap_or(0.0)
    }
}

/// 换进来之后这个窗口又用了多少（比例）：基线那一期已过（窗口重置过）⇒ 只算重置之后的（下界）。
fn spent_since(w: &QuotaWindow, b: &Base, now: u64) -> f64 {
    let since = if reset_since_seen(b.resets_at, now) {
        0.0
    } else {
        b.used
    };
    (used_now(w, now) - since).max(0.0)
}

/// 这一段在此刻的号上各窗口：（窗口键, 基线, 又用了多少, 单段预算）；说不出基线的窗口不出。
pub(crate) fn segment(f: &Facts<'_>) -> Vec<(String, f64, f64, Option<u8>)> {
    let Some(r) = current_reading(f) else {
        return Vec::new();
    };
    // 这个号自己的那一行；没有 ⇒ `"*"` 那一行（所有号）。
    let per = f.stint.get(f.current).or_else(|| f.stint.get(ALL_WINDOWS));
    r.windows
        .iter()
        .filter_map(|w| {
            let k = (f.key)(&w.name)?;
            let b = f.base.get(&k)?;
            let budget = per.and_then(|p| p.get(&k).or_else(|| p.get(ALL_WINDOWS)).copied());
            Some((k, b.used, spent_since(w, b, f.now), budget))
        })
        .collect()
}

/// 这一段在此刻的号上用完了单段预算（有一个窗口用到就算）：（那个窗口的窗口键, 它的预算）。
fn stint_spent(f: &Facts<'_>) -> Option<(String, u8)> {
    segment(f).into_iter().find_map(|(k, _, spent, n)| {
        n.filter(|n| spent * 100.0 + 1e-9 >= f64::from(*n))
            .map(|n| (k, n))
    })
}

/// 原号被拒 / 超额时记的「为什么」：卡着的那个窗口的语义位。
fn full_why(f: &Facts<'_>) -> SwitchWhy {
    SwitchWhy::Full {
        w: current_reading(f)
            .and_then(|r| r.limiting)
            .and_then(|l| (f.slot)(&l))
            .map(str::to_string),
    }
}

/// 按序取首个 `take(号)` 的、接得上的号（不含此刻的 · 试过的 · `skipped` 里已经问过接不上的）；跳过的追加进 `skipped`。
fn pick_by(
    f: &Facts<'_>,
    take: &dyn Fn(&str) -> bool,
    ready: &mut dyn FnMut(&str) -> Result<(), Unready>,
    skipped: &mut Vec<(String, Unready)>,
) -> Option<String> {
    let asked: Vec<String> = skipped.iter().map(|(a, _)| a.clone()).collect();
    let candidates = f.pool.iter().filter(|a| {
        a.as_str() != f.current
            && !f.tried.iter().any(|t| t == *a)
            && !asked.iter().any(|x| x == *a)
            && take(a)
    });
    for a in candidates {
        match ready(a) {
            Ok(()) => return Some(a.clone()),
            Err(u) => skipped.push((a.clone(), u)),
        }
    }
    None
}

/// 能用的号（`subs_only` ⇒ 只要订阅号）。
fn usable_one<'f>(f: &'f Facts<'f>, subs_only: bool) -> impl Fn(&str) -> bool + 'f {
    move |a: &str| (!subs_only || (f.kind)(a) == Kind::Sub) && standing(a, f).usable()
}

/// 池里最早回来的那个号（都说不出 ⇒ `None`）。
fn earliest_back(f: &Facts<'_>) -> Option<Back> {
    f.pool
        .iter()
        .filter_map(|a| match standing(a, f).back() {
            Some((Some(at), slot)) => Some((at, a, slot)),
            _ => None,
        })
        .min_by_key(|(at, _, _)| *at)
        .map(|(at, a, slot)| Back {
            account: a.clone(),
            at,
            slot,
        })
}

/// 能用而想走（单段预算用完 · 前面的号回来了）：换到首个 `take` 的；没有 ⇒ 留着（有跳过的才记一句为什么没换成）。
fn leave(
    f: &Facts<'_>,
    why: SwitchWhy,
    take: &dyn Fn(&str) -> bool,
    ready: &mut dyn FnMut(&str) -> Result<(), Unready>,
) -> Verdict {
    let mut skipped = Vec::new();
    match pick_by(f, take, ready, &mut skipped) {
        Some(to) => Verdict::Switch {
            to,
            why,
            from_resets_at: None,
            skipped,
        },
        None if skipped.is_empty() => Verdict::Stay,
        None => Verdict::Stuck {
            why,
            from_resets_at: None,
            skipped,
        },
    }
}

/// ★ 挑法：判一次。`ready(号)` 问这个号此刻接不接得上（要登录 / key / 账号身份）—— 只对真轮到的号问。
pub(crate) fn decide(f: &Facts<'_>, ready: &mut dyn FnMut(&str) -> Result<(), Unready>) -> Verdict {
    let cur = current_standing(f);
    if cur.usable() {
        // 1. 能用：刚回来的那一发照过了 ⇒ 不换；单段预算用完（软的）⇒ 换到首个能用的；前面的号回来了 ⇒ 切回去。
        if f.heard.is_some() {
            return Verdict::Stay;
        }
        // 兜底只临时用：在兜底号上、有非兜底的号能用了 ⇒ 切到首个能用的那个（不管换法）。
        if f.fallback.iter().any(|x| x == f.current) {
            let usable = usable_one(f, false);
            let back = |a: &str| !f.fallback.iter().any(|x| x == a) && usable(a);
            if let v @ Verdict::Switch { .. } = leave(f, SwitchWhy::LeaveFallback, &back, ready) {
                return v;
            }
        }
        if let Some((w, n)) = stint_spent(f) {
            return leave(f, SwitchWhy::Stint { w, n }, &usable_one(f, false), ready);
        }
        if !f.preempt {
            return Verdict::Stay;
        }
        // 切回只等「换进来那一刻挡在前面的」回来（重置 · 时段换了上限都算）；换进来时本就能用的（手动换走的）不算。
        let usable = usable_one(f, false);
        let back = |a: &str| f.above.iter().any(|x| x == a) && usable(a);
        return leave(f, SwitchWhy::Preempt, &back, ready);
    }
    let (why, from_resets_at, subs_only) = match &cur {
        Standing::Usable => return Verdict::Stay,
        Standing::Refused { .. } => (
            full_why(f),
            current_reading(f).and_then(|r| r.resets_at),
            false,
        ),
        Standing::Overage { .. } => (
            full_why(f),
            current_reading(f).and_then(|r| r.resets_at),
            true,
        ),
        Standing::OverCap { n: 0, w, until, .. } => {
            (SwitchWhy::Off { w: w.clone() }, *until, false)
        }
        Standing::OverCap { n, w, until, .. } => (
            SwitchWhy::Threshold {
                n: *n,
                w: w.clone(),
            },
            *until,
            false,
        ),
    };
    // 2. 不能用 ⇒ 从头取首个能用的。
    let mut skipped = Vec::new();
    let mut to = pick_by(f, &usable_one(f, subs_only), ready, &mut skipped);
    // 3. 没有：软上限、此刻的号真被拒 ⇒ 退一步取首个没被拒的（过了上限的号仍能用）。
    if to.is_none() && f.at_limit == AtLimit::Continue && matches!(cur, Standing::Refused { .. }) {
        to = pick_by(f, &|a| !standing(a, f).shut(), ready, &mut skipped);
    }
    // 切兜底前等一等：要切到兜底号、非兜底的号很快回来 ⇒ 不切。
    if let Some(t) = &to {
        if let Some(v) = wait_for(f, &cur, t, &skipped) {
            return v;
        }
    }
    // 硬上限：有上限、说得出几点有号回来 ⇒ 不发上游（说不出照软上限办）。超额在兜不算。
    if to.is_none() && f.at_limit == AtLimit::Stop && !subs_only && has_caps(f) {
        if let Some(back) = earliest_back(f) {
            // 记下的线与窗口：此刻的号过了线 ⇒ 它卡着的那一处；否则 ⇒ 最早回来的那个号卡着的那一处。
            let (n, w) = match &cur {
                Standing::OverCap { n, w, .. } => (*n, w.clone()),
                _ => match standing(&back.account, f) {
                    Standing::OverCap { n, w, .. } => (n, w),
                    _ => (100, None),
                },
            };
            return Verdict::Hold {
                n,
                w,
                back,
                skipped,
            };
        }
    }
    match to {
        Some(to) => Verdict::Switch {
            to,
            why,
            from_resets_at,
            skipped,
        },
        None => Verdict::Stuck {
            why: if subs_only { SwitchWhy::ToOverage } else { why },
            from_resets_at,
            skipped,
        },
    }
}

/// 切兜底前等一等那一问：本要切到 `to`，它是兜底号，而池里非兜底的号（含此刻的；跳过的不算）有一个在 `wait` 分钟内回来 ⇒ 等它
/// （此刻的号被拒，或硬上限 ⇒ [`Verdict::Wait`]；软上限 ⇒ 留在此刻的号上照发）。`to` 不是兜底号 · 不等 ⇒ `None`。
fn wait_for(
    f: &Facts<'_>,
    cur: &Standing,
    to: &str,
    skipped: &[(String, Unready)],
) -> Option<Verdict> {
    let is_fallback = |a: &str| f.fallback.iter().any(|x| x == a);
    if f.wait == 0 || !is_fallback(to) || matches!(cur, Standing::Overage { .. }) {
        return None;
    }
    let limit = f.now + u64::from(f.wait) * 60;
    let back = f
        .pool
        .iter()
        .filter(|a| !is_fallback(a) && !skipped.iter().any(|(x, _)| x == *a))
        .filter_map(|a| {
            let st = if a == f.current {
                cur.clone()
            } else {
                standing(a, f)
            };
            match st.back() {
                Some((Some(at), slot)) if at > f.now && at <= limit => Some((at, a, slot)),
                _ => None,
            }
        })
        .min_by_key(|(at, _, _)| *at)
        .map(|(at, a, slot)| Back {
            account: a.clone(),
            at,
            slot,
        })?;
    let hold = matches!(cur, Standing::Refused { .. }) || f.at_limit == AtLimit::Stop;
    if !hold {
        return Some(Verdict::Stay);
    }
    f.can_hold.then(|| Verdict::Wait {
        instead: to.to_string(),
        back,
        skipped: skipped.to_vec(),
    })
}

/// 换进 `target` 那一刻要记下的：池里排在它前面（它不在池里 ⇒ 整个池）、此刻不能用的号，按池序。
pub(crate) fn blocked_above(f: &Facts<'_>, target: &str) -> Vec<String> {
    f.pool
        .iter()
        .take_while(|a| a.as_str() != target)
        .filter(|a| !standing(a, f).usable())
        .cloned()
        .collect()
}

/// 此刻的号要是不能用了会换到谁（不管此刻能不能用；同一套候选与次序）。
pub(crate) fn next_of(
    f: &Facts<'_>,
    ready: &mut dyn FnMut(&str) -> Result<(), Unready>,
) -> Option<String> {
    pick_by(f, &usable_one(f, false), ready, &mut Vec::new())
}

/// 此刻的号发不出去（被拒未到重置；超额在兜不算 —— 那一发照过）。
pub(crate) fn refused_now(f: &Facts<'_>) -> bool {
    matches!(current_standing(f), Standing::Refused { .. })
}

/// 这个号额度账上此刻不能用（被拒 · 超额在兜 · 过上限）。
pub(crate) fn spent(account: &str, f: &Facts<'_>) -> bool {
    !standing(account, f).usable()
}

/// 这个号此刻过线（不是被拒 · 超额 · 时段停用）⇒ 卡着它的那一窗（语义位）；否则 ⇒ `None`。
pub(crate) fn line_window(account: &str, f: &Facts<'_>) -> Option<String> {
    match standing(account, f) {
        Standing::OverCap { n, w, .. } if n > 0 => w,
        _ => None,
    }
}

/// 这个号不能用的话几点回来；说不出 ⇒ `None`。
pub(crate) fn back_at(account: &str, f: &Facts<'_>) -> Option<u64> {
    standing(account, f).back().and_then(|(t, _)| t)
}

// ── 预览：这份轮换接下来会怎么走（`rotation-plan`）─────────────────────────────────────────

/// 预览里的一段：`[from, to)` 用 `account`（`None` ＝ 这一段不发上游：硬上限停着 · 切兜底前等着）；`why` ＝ 这一段开头为什么换（头一段 · 没换 ⇒ `None`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlanStep {
    pub(crate) from: u64,
    pub(crate) to: u64,
    pub(crate) account: Option<String>,
    pub(crate) why: Option<SwitchWhy>,
}

/// 预览里一个号不能用的一段的样子。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum LaneState {
    /// 被拒、还没到回来的时刻。
    Refused,
    /// 过了上限（`n` ＝ 那条线 · `w` ＝ 那一窗）。
    Capped,
    /// 上限取到 `0`（时段停用）。
    Off,
    /// 订阅号在用付费超额。
    Overage,
}

/// 一个号在预览里不能用的一段 `[from, to)`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LaneSpan {
    pub(crate) from: u64,
    pub(crate) to: u64,
    pub(crate) state: LaneState,
    pub(crate) n: Option<u8>,
    /// 过线的那一窗（语义位；只 `capped` 有）。
    pub(crate) w: Option<String>,
}

/// 预览里结论会变的时刻（`(f.now, until)` 里，升序去重）：池里各号的重置时刻 · 被拒说的回来时刻 · 按时段写的上限的起止（本地钟）。
/// 用量只按此刻的算（没有根据说以后涨多快）⇒ 结论只在这些时刻变。
pub(crate) fn plan_events(f: &Facts<'_>, until: u64) -> Vec<u64> {
    let mut out: Vec<u64> = Vec::new();
    for a in f.pool {
        if let Some(r) = (f.seen)(a) {
            out.extend(r.resets_at);
            out.extend(r.windows.iter().filter_map(|w| w.resets_at));
        }
        for v in f.cap.get(a).into_iter().flat_map(|m| m.values()) {
            let CapValue::Slots(slots) = v else { continue };
            for (from, to) in slots.iter().filter_map(|x| super::rotation::span_of(&x.at)) {
                for m in [from, to] {
                    // 本地钟一天里第 m 分钟，从 `f.now` 往后每天一次。
                    let local = i128::from(f.now) + i128::from(f.offset);
                    let left = (i128::from(m) * 60 - local.rem_euclid(86_400)).rem_euclid(86_400);
                    let mut t = i128::from(f.now) + if left == 0 { 86_400 } else { left };
                    while t < i128::from(until) {
                        out.extend(u64::try_from(t).ok());
                        t += 86_400;
                    }
                }
            }
        }
    }
    out.retain(|t| *t > f.now && *t < until);
    out.sort_unstable();
    out.dedup();
    out
}

/// ★ 预览：从 `f.now` 到 `until`，这份轮换按 [`decide`] 会怎么走（每个结论会变的时刻判一次；单段预算要看以后的用量，不预测）。
/// 此刻的号 ＝ `f.current`、挡在它前面的 ＝ `f.above`；换进一个号那一刻照上游选择记下挡在它前面的号。
pub(crate) fn plan(
    f: &Facts<'_>,
    until: u64,
    ready: &mut dyn FnMut(&str) -> Result<(), Unready>,
) -> Vec<PlanStep> {
    let mut times = vec![f.now];
    times.extend(plan_events(f, until));
    let mut cur = f.current.to_string();
    let mut above: Vec<String> = f.above.to_vec();
    let mut steps: Vec<PlanStep> = Vec::new();
    for (i, &t) in times.iter().enumerate() {
        let end = times.get(i + 1).copied().unwrap_or(until);
        let tried = [cur.clone()];
        let g = Facts {
            now: t,
            current: &cur,
            above: &above,
            heard: None,
            tried: &tried,
            ..*f
        };
        let (account, why, enter) = match decide(&g, ready) {
            Verdict::Stay => (Some(cur.clone()), None, None),
            Verdict::Switch { to, why, .. } => (Some(to.clone()), Some(why), Some(to)),
            Verdict::Stuck { .. } => (Some(cur.clone()), None, None),
            Verdict::Hold { n, w, .. } => (None, Some(SwitchWhy::Held { n, w }), None),
            Verdict::Wait { instead, back, .. } => (
                None,
                Some(SwitchWhy::Wait {
                    account: back.account,
                    instead,
                }),
                None,
            ),
        };
        if let Some(to) = enter {
            above = blocked_above(&g, &to);
            cur = to;
        }
        match steps.last_mut() {
            Some(last) if last.account == account && (why.is_none() || last.why == why) => {
                last.to = end
            }
            _ => steps.push(PlanStep {
                from: t,
                to: end,
                account,
                why,
            }),
        }
    }
    steps
}

/// 预览里一个号不能用的那几段（每个结论会变的时刻判一次 [`standing`]，相邻同样的并成一段）。
pub(crate) fn lane(f: &Facts<'_>, account: &str, until: u64) -> Vec<LaneSpan> {
    let mut times = vec![f.now];
    times.extend(plan_events(f, until));
    let mut out: Vec<LaneSpan> = Vec::new();
    for (i, &t) in times.iter().enumerate() {
        let end = times.get(i + 1).copied().unwrap_or(until);
        let g = Facts { now: t, ..*f };
        let (state, n, w) = match standing(account, &g) {
            Standing::Usable => continue,
            Standing::Refused { .. } => (LaneState::Refused, None, None),
            Standing::Overage { .. } => (LaneState::Overage, None, None),
            Standing::OverCap { n: 0, .. } => (LaneState::Off, None, None),
            Standing::OverCap { n, w, .. } => (LaneState::Capped, Some(n), w),
        };
        match out.last_mut() {
            Some(last) if last.to == t && last.state == state && last.n == n && last.w == w => {
                last.to = end
            }
            _ => out.push(LaneSpan {
                from: t,
                to: end,
                state,
                n,
                w,
            }),
        }
    }
    out
}

/// 上限取自哪一层（界面悬停「此刻 ≤99 · 来自 全部窗口」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CapLayer {
    /// 这号这窗口那一格（含这号这语义位）。
    Window,
    /// 这号「全部窗口」那一格。
    All,
    /// 触发那一行（所有号这语义位，`cap["*"]`）。
    Trigger,
    /// 哪层都没有：不封顶。
    None,
}

/// 此刻取到的上限 ＋ 来自哪一层（`v = None` ＝ 不封顶）· 来自触发时是哪一窗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CapAt {
    pub(crate) v: Option<u8>,
    pub(crate) layer: CapLayer,
    pub(crate) w: Option<&'static str>,
}

const NO_CAP: CapAt = CapAt {
    v: None,
    layer: CapLayer::None,
    w: None,
};

/// 封顶表里一列的语义位：`5h` · `7d` 两列就是它们自己；全部窗口那一列 · 别的窗口键没有。
fn column_slot(key: &str) -> Option<&'static str> {
    LINE_SLOTS.iter().copied().find(|s| *s == key)
}

fn first_at(ls: &[(CapLayer, &CapValue, Option<&'static str>)], minute: u16) -> CapAt {
    ls.iter()
        .find_map(|(layer, v, w)| {
            pick(v, minute).map(|n| CapAt {
                v: Some(n),
                layer: *layer,
                w: *w,
            })
        })
        .unwrap_or(NO_CAP)
}

/// 封顶表这号这一列（`5h` · `7d` · `*` ＝ 全部窗口）此刻实际取的上限，与「这一格不算」时往下一层取到的（封顶浮层「其余时段 ＝ …」）。
/// 层次即 [`layers`]；全部窗口那一格下面是两窗各一条线、写不成一个数 ⇒ 它的下一层恒是「不封顶」，两窗各取多少另见 [`effective_lines`]。
pub(crate) fn effective_cap(f: &Facts<'_>, account: &str, key: &str) -> (CapAt, CapAt) {
    let minute = minute_of(f);
    let ls = layers(f.cap, account, key, column_slot(key));
    let at = first_at(&ls, minute);
    // 「这一格不算」：去掉这一格自己那一层（这号这窗口 · 全部窗口那一列则是这号全部窗口）。
    let own = if key == ALL_WINDOWS {
        CapLayer::All
    } else {
        CapLayer::Window
    };
    let rest: Vec<_> = ls.iter().copied().filter(|(l, _, _)| *l != own).collect();
    let below = first_at(&rest, minute);
    (at, below)
}

/// 这号两窗（`5h` · `7d`）此刻各取的线，与这号全部窗口那一格不算时各取的（全部窗口那一格的悬停与「其余时段」）。
pub(crate) fn effective_lines(f: &Facts<'_>, account: &str) -> Vec<(&'static str, CapAt, CapAt)> {
    let minute = minute_of(f);
    LINE_SLOTS
        .iter()
        .map(|s| {
            let ls = layers(f.cap, account, s, Some(s));
            let without_all: Vec<_> = ls
                .iter()
                .copied()
                .filter(|(l, _, _)| *l != CapLayer::All)
                .collect();
            (*s, first_at(&ls, minute), first_at(&without_all, minute))
        })
        .collect()
}

/// 时间轴顶行「距 {w} 线 {n} 点」：这个号各窗按此刻取的线（[`cap_of`]）还差几点，取差得最少的那一窗；都没线 ⇒ `None`。
pub(crate) fn to_line(f: &Facts<'_>, account: &str) -> Option<(String, u32)> {
    let r = (f.seen)(account)?;
    r.windows
        .iter()
        .filter_map(|w| {
            let slot = (f.slot)(&w.name)?;
            let k = (f.key)(&w.name)?;
            let n = cap_of(f, account, &k, Some(slot)).filter(|n| *n > 0)?;
            let pct = (used_now(w, f.now) * 100.0).round().max(0.0) as u32;
            Some((slot.to_string(), u32::from(n).saturating_sub(pct)))
        })
        .min_by_key(|(_, d)| *d)
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/decide_tests.rs"]
mod tests;
