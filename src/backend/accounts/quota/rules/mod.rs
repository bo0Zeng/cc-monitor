//! ★ 轮换第五版后端的积木：事实 · 动作 · 叫醒 · 推演 · 兜底。换号的想法全在策略里。
//!
//! - [`standing`]：事实上发不发得出去；
//! - [`decide`]：一判 —— 叫策略、照它交的先后做动作（纯函数：不读钟、不写盘）；
//! - [`past`]：已经发生的那一截（照换号记录与此刻的读数，说不出的空着）；
//! - [`simulate`]：离散事件推演，节点 ＝ 判的那一串，用的是同一个 [`decide::turn`]；
//! - [`script`]：策略文件（JSON 头 ＋ Rhai 代码）与沙箱里的 `decide(ctx)`（挂着积木库 `lib`）。

pub(crate) mod book;
pub(crate) mod decide;
pub(crate) mod form;
pub(crate) mod past;
pub(crate) mod script;
pub(crate) mod shelf;
pub(crate) mod simulate;
pub(crate) mod standing;
pub(crate) mod upgrade;

/// 窗口长度（秒）：下标 0 ＝ 5 小时 · 1 ＝ 7 天。
pub(crate) const WINDOW_SECS: [u64; 2] = [5 * 3600, 7 * 86_400];

/// 判与推演看的那一份事实（一个号）：只有数，没有字。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Acct {
    pub(crate) id: String,
    pub(crate) api: bool,
    /// 两窗用了多少（0–100）。
    pub(crate) used: [f64; 2],
    /// 两窗几点重置（没开 ⇒ 空）。
    pub(crate) resets: [Option<u64>; 2],
    /// 被拒到几点（空 ＝ 没被拒）。
    pub(crate) refused_until: Option<u64>,
    /// 跟窗口无关的「用不了」：要重新登录 · 缺 key · 在超额上兜着（到 `until` 为止，说不出 ⇒ 空 ＝ 一直）。
    pub(crate) barred: Option<Barred>,
    /// 用速（%/分）：这个号自己在额度账里量出来的（最近 30 分钟读数在涨；所有会话的用量都在里面）；量不到 ⇒ 空。
    pub(crate) rate: [Option<f64>; 2],
    /// 两窗「1% ≈ 多少用量」（折合输入 token）：这个号自己在额度账里量出来的；量不到 ⇒ 空。只交给策略，判与推演不看它。
    pub(crate) per_pct: [Option<u64>; 2],
}

/// 跟窗口无关的「用不了」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Barred {
    pub(crate) why: super::facts::FactWhy,
    pub(crate) until: Option<u64>,
}

impl Acct {
    /// 由一条事实拼（[`super::facts::Fact`]）。
    pub(crate) fn of_fact(f: &super::facts::Fact) -> Self {
        use super::facts::FactWhy;
        let barred = f.blocked.as_ref().and_then(|b| match b.why {
            FactWhy::Login | FactWhy::Key | FactWhy::Overage => Some(Barred {
                why: b.why,
                until: b.until,
            }),
            FactWhy::Refused | FactWhy::Used => None,
        });
        let refused_until = f
            .blocked
            .as_ref()
            .filter(|b| b.why == FactWhy::Refused)
            .map(|b| b.until.unwrap_or(u64::MAX));
        Self {
            id: f.account.clone(),
            api: f.kind == super::facts::Kind::Api,
            used: [f.w5h.used, f.w7d.used],
            resets: [f.w5h.resets, f.w7d.resets],
            refused_until,
            barred,
            rate: [f.w5h.rate, f.w7d.rate],
            per_pct: [f.w5h.tokens_per_pct, f.w7d.tokens_per_pct],
        }
    }
}

/// 策略所在时区里 `t` 是一天的第几分钟（判与推演交给策略的 `clock`）。
pub(crate) fn clock_at(tz: &crate::Tz, t: u64) -> u16 {
    let t = i64::try_from(t).unwrap_or(i64::MAX);
    let local = i128::from(t) + i128::from(tz.offset_secs(t));
    u16::try_from(local.rem_euclid(86_400) / 60).unwrap_or(0)
}

/// 策略头里的时区（缺 · 认不出 ⇒ UTC，界面另提示「时区不明」）。
pub(crate) fn tz_of(head: Option<&script::Head>) -> crate::Tz {
    head.and_then(|h| h.tz.as_deref())
        .and_then(crate::Tz::iana)
        .unwrap_or_default()
}
