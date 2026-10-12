//! ★ 轮换第五版后端的积木（`轮换-v5-算法.md` · `轮换-v5-分层.md`）：事实 · 动作 · 叫醒 · 推演 · 兜底。换号的想法全在策略里。
//!
//! - [`standing`]：事实上发不发得出去；
//! - [`decide`]：一判 —— 叫策略、照它交的先后做动作（纯函数：不读钟、不写盘）；
//! - [`simulate`]：离散事件推演，节点 ＝ 判的那一串，用的是同一个 [`decide::turn`]；
//! - [`script`]：策略文件（JSON 头 ＋ Rhai 代码）与沙箱里的 `decide(ctx)`（挂着积木库 `lib`）。

pub(crate) mod decide;
pub(crate) mod script;
pub(crate) mod simulate;
pub(crate) mod standing;

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
    /// 用速（%/分，只在在用的号上有；没根据 ⇒ 空）。
    pub(crate) rate: [Option<f64>; 2],
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
            api: f.kind == super::decide::Kind::Api,
            used: [f.w5h.used, f.w7d.used],
            resets: [f.w5h.resets, f.w7d.resets],
            refused_until,
            barred,
            rate: [f.w5h.rate, f.w7d.rate],
        }
    }
}

/// 默认用速（`rate` 空时推演用的，%/分）：5 小时窗按 5 小时用满 · 7 天窗按 7 天用满。
pub(crate) const RATE_DEFAULT: [f64; 2] = [100.0 / 300.0, 100.0 / 10_080.0];
