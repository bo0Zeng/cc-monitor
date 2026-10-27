//! **起会话时接管 agent 原本的出网设置**：没有 cc-monitor 时 agent 怎么出网，经 `ccm` 起之后照样怎么出网，代理在最外层
//! （agent → 中转 → 后端的出网口 → 代理）。
//!
//! - 代理变量本身照「其余环境变量」继承（直路照调用方 · tmux 路照那台 tmux 的环境），一个都不剥：agent 别的出网（取网页 · MCP）照旧走代理。
//! - 这一发注入中转、而且设了代理 ⇒ 这一家认的免代理变量（每一形）设成「原来那一份 ＋ 中转口的主机」：模型请求直达中转口，不绕代理。
//!   随中转地址一起设（[`super::plan::Direct::relay_env`]），打印那一行与真跑同一份。
//!
//! 认哪几个变量由适配层申报（[`crate::agents::ProxyFace`]）；代理 URL · 免代理表怎么读只在出网口一处（`egress`），这里只认名字、拼表。

use super::plan::Plan;
use crate::agents::ProxyFace;

/// 计划面入口那一下（`plan_of`）：直路注入了中转、这一家认的代理变量设了 ⇒ 补上免代理那几个变量。
/// `face` ＝ 这一家申报的变量（没登记 ⇒ `None`）；`get` ＝ 继承来的环境（不继承：别名预览 ⇒ 答什么都没有）。
pub(crate) fn onto(plan: &mut Plan, face: Option<ProxyFace>, get: &dyn Fn(&str) -> Option<String>) {
    let Plan::Direct(d) = plan else {
        return;
    };
    let (Some(face), Some(relay)) = (face, d.relay.as_deref()) else {
        return;
    };
    let set = |n: &&str| {
        get(n)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    if !face.via.iter().any(|n| set(n).is_some()) {
        return;
    }
    let Ok(relay) = upstream_url_core::parse(relay) else {
        return;
    };
    let had = face.skip.iter().find_map(set).unwrap_or_default();
    let merged = if had
        .split([',', ' '])
        .any(|h| h.trim().eq_ignore_ascii_case(&relay.host))
    {
        had
    } else if had.is_empty() {
        relay.host
    } else {
        format!("{had},{}", relay.host)
    };
    for n in face.skip {
        d.relay_env.retain(|(k, _)| k != n);
        d.relay_env.push((n.to_string(), merged.clone()));
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/ccm/egress_tests.rs"]
mod tests;
