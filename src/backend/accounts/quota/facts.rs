//! ★ 轮换的**事实 F**（`轮换-v5-算法.md` §1 · `轮换-v5-分层.md` §1.1）：每个号的用量 · 重置 · 用速 · 被拒 · 超额 · 登录。
//! 只读额度账 ＋ 账号库 ＋ 登录，换一份策略不变；上限、余量、「能不能用」里跟设置有关的那一半不在这里（在判的那一层）。
//!
//! 拼装只有一处（[`crate::accounts::upstream_select::rotate::Hop::facts`]，它手上有额度账与账号库）；这里是形状与纯函数。

use super::decide::{self, Kind};
use super::show::LoginState;
use crate::agents::QuotaReading;
use serde::Serialize;

/// 一个窗口（`5h` / `7d`）的事实：在计时吗 · 用了多少（0–100）· 几点重置 · 用速（%/分，没根据 ⇒ 缺）。
/// 一个语义位有几个窗口（按模型分的 7 天额度）⇒ 取用得最多的那个。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct WinFact {
    pub open: bool,
    pub used: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub resets: Option<u64>,
    /// `resets` 写给人看的样子（按看的那一台的时区）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub resets_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub rate: Option<f64>,
}

/// 事实上用不了的原因（跟设置无关的那几种）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum FactWhy {
    /// 被拒（429 等），到 `until` 恢复。
    Refused,
    /// 订阅号满了、在付费超额上兜着。
    Overage,
    /// 某个窗口用到 100%。
    Used,
    /// 订阅号要重新登录。
    Login,
    /// 按量号这台没有 key。
    Key,
}

/// 用不了：为什么 · 几点回来（说不出 ⇒ 缺）· 写好的那一句。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct FactBlocked {
    pub why: FactWhy,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub until: Option<u64>,
    pub text: String,
}

/// 一个号此刻的事实。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct Fact {
    /// 路由第 2 段。
    pub account: String,
    pub kind: Kind,
    pub w5h: WinFact,
    pub w7d: WinFact,
    pub login: LoginState,
    /// 额度账上最后一次看到它（没出过数 ⇒ 缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub seen_at: Option<u64>,
    /// 事实上用不了 ⇒ 有；能用 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub blocked: Option<FactBlocked>,
}

/// 一个语义位此刻的窗口事实（`rate` 由调用方按窗口原名问额度账）。
fn win_of(
    r: Option<&QuotaReading>,
    slot: &str,
    now: u64,
    slot_of: &dyn Fn(&str) -> Option<&'static str>,
    rate: &dyn Fn(&str) -> Option<f64>,
    when: &dyn Fn(u64) -> String,
) -> WinFact {
    let w = decide::window_fact(r, slot, now, slot_of);
    // 用速取那一位上用得最多的那个窗口（同 window_fact 的取法）。
    let name = r.and_then(|r| {
        r.windows
            .iter()
            .filter(|x| slot_of(&x.name) == Some(slot))
            .filter(|x| x.used.is_some() && x.resets_at.is_some_and(|t| t > now))
            .max_by(|a, b| decide::used_now(a, now).total_cmp(&decide::used_now(b, now)))
            .map(|x| x.name.clone())
    });
    WinFact {
        open: w.open,
        used: w.used,
        resets: w.resets,
        resets_text: w.resets.map(when),
        rate: name.as_deref().and_then(rate),
    }
}

/// ★ 拼一个号的事实（唯一一处的纯函数部分）：额度账那一条 ＋ 种类 ＋ 登录。`when` 把时刻写成给人看的样子。
#[allow(clippy::too_many_arguments)]
pub(crate) fn fact_of(
    account: &str,
    r: Option<&QuotaReading>,
    seen_at: Option<u64>,
    kind: Kind,
    login: LoginState,
    now: u64,
    slot_of: &dyn Fn(&str) -> Option<&'static str>,
    rate: &dyn Fn(&str) -> Option<f64>,
    when: &dyn Fn(u64) -> String,
) -> Fact {
    let st = decide::standing(r, false, kind, login == LoginState::Ok, now);
    let blocked = st.map(|(w, until)| {
        let why = match w {
            super::strategy::ctx::WhyNot::Refused => FactWhy::Refused,
            super::strategy::ctx::WhyNot::Overage => FactWhy::Overage,
            super::strategy::ctx::WhyNot::Full => FactWhy::Used,
            super::strategy::ctx::WhyNot::Login if login == LoginState::NeedsKey => FactWhy::Key,
            super::strategy::ctx::WhyNot::Login => FactWhy::Login,
        };
        FactBlocked {
            why,
            until,
            text: blocked_text(why, until.map(when).as_deref()),
        }
    });
    Fact {
        account: account.to_string(),
        kind,
        w5h: win_of(r, "5h", now, slot_of, rate, when),
        w7d: win_of(r, "7d", now, slot_of, rate, when),
        login,
        seen_at,
        blocked,
    }
}

/// 用不了的那一句。
fn blocked_text(why: FactWhy, at: Option<&str>) -> String {
    use copy_core::copy_text;
    match (why, at) {
        (FactWhy::Refused, Some(at)) => copy_text("beRotation.fact.refused", &[("at", at)]),
        (FactWhy::Refused, None) => copy_text("beRotation.fact.refusedOpen", &[]),
        (FactWhy::Overage, _) => copy_text("beRotation.fact.overage", &[]),
        (FactWhy::Used, Some(at)) => copy_text("beRotation.fact.used", &[("at", at)]),
        (FactWhy::Used, None) => copy_text("beRotation.fact.usedOpen", &[]),
        (FactWhy::Login, _) => copy_text("beRotation.fact.login", &[]),
        (FactWhy::Key, _) => copy_text("beRotation.fact.key", &[]),
    }
}
