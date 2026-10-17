//! ★ 轮换的基础功能层：只给信息、只执行动作 —— 事实 · 动作 · 叫醒 · 推演 · 兜底。换号的想法全在策略里；
//! 策略文件与界面那一侧归配置编辑层（`quota/setup/`），本层不认它，只认它交来的 [`Adopted`]。
//!
//! - [`standing`]：事实上发不发得出去；
//! - [`decide`]：一判 —— 叫策略、照它交的先后做动作（纯函数：不读钟、不写盘）；
//! - [`past`]：已经发生的那一截（照换号记录与此刻的读数，说不出的空着）；
//! - [`simulate`]：离散事件推演，节点 ＝ 判的那一串，用的是同一个 [`decide::turn`]；
//! - [`code`]：策略的代码在沙箱里的 `decide(ctx)`（挂着积木库 `lib`）；
//! - [`book`]：`rotation.json`（拖出来的条目 · memo · 叫醒 · 换号记录）。

pub(crate) mod book;
pub(crate) mod code;
pub(crate) mod decide;
pub(crate) mod past;
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

/// 设置里号列表那一格（先后就是号牌顺序；`{start: true}` ＝ 开会话用的号）。
pub(crate) const ACCOUNTS: &str = "accounts";

/// 设置里号列表照先后列出的号（`{start: true}` 换成 `start`）。
pub(crate) fn listed_ids<'a>(settings: &'a serde_json::Value, start: &'a str) -> Vec<&'a str> {
    settings
        .get(ACCOUNTS)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| {
            if r.get("start") == Some(&serde_json::Value::Bool(true)) {
                Some(start)
            } else {
                r.get("id").and_then(serde_json::Value::as_str)
            }
        })
        .collect()
}

/// 会话用的那份策略此刻（配置编辑层拼好交来）：`id` 空 ⇒ 不轮换；代码读不出 ⇒ 判的时候走兜底（带原因）。
#[derive(Clone)]
pub(crate) struct Adopted {
    pub(crate) id: String,
    /// 那份文件的内容版本（没了 ⇒ 空）。
    pub(crate) rev: String,
    pub(crate) code: Result<std::sync::Arc<code::Code>, super::strategy::Fail>,
    pub(crate) time_varying: bool,
    /// 生效的设置：会话改过 ⇒ 改过的；否则文件里的。
    pub(crate) settings: serde_json::Value,
    /// 分时段照它算。
    pub(crate) tz: crate::Tz,
}

impl Adopted {
    /// 不轮换的那一份。
    pub(crate) fn none() -> Self {
        Self {
            id: String::new(),
            rev: String::new(),
            code: Err(super::strategy::Fail::shape("no strategy")),
            time_varying: false,
            settings: serde_json::Value::Object(Default::default()),
            tz: crate::Tz::default(),
        }
    }
}

impl decide::Strategy for Adopted {
    fn decide(&self, ctx: &decide::Ctx<'_>) -> Result<decide::Verdict, super::strategy::Fail> {
        match &self.code {
            Ok(c) => c.decide(ctx),
            Err(f) => Err(f.clone()),
        }
    }

    fn time_varying(&self) -> bool {
        self.code.is_ok() && self.time_varying
    }
}

/// 会话用哪份策略问谁（配置编辑层实现；宿主交进来，本层不认它）。
pub(crate) trait Adoptions: Send + Sync {
    /// 这个会话此刻用的那一份；第一次问到的会话由那一侧照「新会话默认」记下（写经 `own`）。
    fn adopted(&self, own: &crate::platform::fs::Own, sid: &str, now: u64) -> Adopted;
}

/// 照这台那份 `rotation.json` 给出 [`Adoptions`] 的那一位（宿主交给上游选择：上游选择不认识策略文件）。
pub(crate) type AdoptionsAt = std::sync::Arc<
    dyn Fn(&std::sync::Arc<super::rotation::RotationStore>) -> std::sync::Arc<dyn Adoptions>
        + Send
        + Sync,
>;
