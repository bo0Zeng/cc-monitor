//! **起会话时接管 agent 原本的出网设置**：没有 cc-monitor 时 agent 怎么出网，经 `ccm` 起之后照样怎么出网，代理在最外层
//! （agent → 中转 → 后端的出网口 → 代理）。
//!
//! - 代理变量本身照「其余环境变量」继承（直路照调用方 · tmux 路照那台 tmux 的环境），一个都不剥：agent 别的出网（取网页 · MCP）照旧走代理。
//! - 这一发注入中转、而且设了代理 ⇒ 这一家认的免代理变量（每一形）设成「原来那一份 ＋ 中转口的主机」：模型请求直达中转口，不绕代理。
//!   随中转地址一起设（[`super::plan::Direct::relay_env`]），打印那一行与真跑同一份。
//! - 同一条件、中转地址带着来处 ⇒ 交回这条会话的那一份（[`Taken`]）：exec 那一刻落成上游描述
//!   （`accounts::upstream_select::session_upstream`），中转开上游那一口按来处照它走代理。
//!
//! 认哪几个变量由适配层申报（[`crate::agents::ProxyFace`]）；代理 URL · 免代理表怎么读只在出网口一处（`egress`），这里只认名字、拼表。

use super::plan::Plan;
use crate::agents::ProxyFace;

/// 这条会话要留的上游描述那一份：来处 ＋ 继承来的、这一家认的出网设置变量（名, 值）原样。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Taken {
    pub(crate) origin: String,
    pub(crate) egress: Vec<(String, String)>,
}

/// 计划面入口那一下（`plan_of`）：直路注入了中转、这一家认的代理变量设了 ⇒ 补上免代理那几个变量；
/// 中转地址带着来处 ⇒ 交回这条会话要留的那一份（exec 那一刻落盘）。
/// `face` ＝ 这一家申报的变量（没登记 ⇒ `None`）；`get` ＝ 继承来的环境（不继承：别名预览 ⇒ 答什么都没有）。
pub(crate) fn onto(
    plan: &mut Plan,
    face: Option<ProxyFace>,
    get: &dyn Fn(&str) -> Option<String>,
) -> Option<Taken> {
    let Plan::Direct(d) = plan else {
        return None;
    };
    let face = face?;
    let url = d.relay.as_deref()?;
    let set = |n: &&str| {
        get(n)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .map(|v| (n.to_string(), v))
    };
    let via: Vec<(String, String)> = face.via.iter().filter_map(set).collect();
    if via.is_empty() {
        return None;
    }
    let skip: Vec<(String, String)> = face.skip.iter().filter_map(set).collect();
    let host = upstream_url_core::parse(url).ok()?.host;
    let had = skip.first().map_or("", |(_, v)| v.as_str());
    let merged = if had
        .split([',', ' '])
        .any(|h| h.trim().eq_ignore_ascii_case(&host))
    {
        had.to_string()
    } else if had.is_empty() {
        host
    } else {
        format!("{had},{host}")
    };
    let origin = url
        .rsplit('/')
        .next()
        .and_then(relay_route_core::parse_origin)
        .map(|o| o.token.to_string());
    for n in face.skip {
        d.relay_env.retain(|(k, _)| k != n);
        d.relay_env.push((n.to_string(), merged.clone()));
    }
    Some(Taken {
        origin: origin?,
        egress: via.into_iter().chain(skip).collect(),
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/ccm/egress_tests.rs"]
mod tests;
