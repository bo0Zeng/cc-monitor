//! 帧命令 `relay-optin`：直接敲的那一家也走中转（可选、用户自己贴）。读这台那份用户级设置文件，出状态 ＋ 要贴的那一段；
//! 判定与成品都在 `accounts/upstream_select/endpoint.rs`，这里只是帧命令那一层（入参 · 码表 · 怎么跑）。

use copy_core::copy_text;

use crate::accounts::upstream_select::endpoint;

/// `relay-optin`：入参 `{agent}`（线上的 kind）→ 那一家的 `{state, note, missing, source, snippet, listening}`。
///
/// 该贴的那一条 ＝ 决策表里「没表态是哪个号」那一发（`endpoint::decide_launch`，全量注入按「是」—— 贴这一段就是用户自己选了全量）插上这台的钥匙；
/// 已装 ⇒ 不再带那一段（钥匙只在要贴的时候才出这台）。只读：那份文件由用户自己合并，后端一个字节不写。
pub(crate) fn answer_optin(req: &OptinReq) -> Result<endpoint::Optin, (&'static str, String)> {
    let agent = crate::agents::pick_route(Some(&req.agent))
        .map_err(|say| ("bad_args", say))?
        .id();
    let home = crate::platform::paths::home_dir()
        .ok_or(("failed", copy_text("beUpstreamEndpoint.optin.noHome", &[])))?;
    let face = crate::agents::settings_env_face(agent)
        .ok_or(("failed", copy_text("beUpstreamEndpoint.optin.noAgent", &[])))?;
    let registered =
        crate::accounts::upstream_select::Upstreams::from_env(&|k| std::env::var(k).ok())
            .is_some_and(|u| u.has(agent));
    Ok(endpoint::optin_at(
        &home,
        agent,
        &face,
        &crate::accounts::upstream_select::file_face::machine_rows(),
        registered,
        &crate::relay::our_relay_listening,
    ))
}

/// `relay-optin` 的入参：哪一家的那一份（线上的 kind；空串 ⇒ 默认那一家；注册表里没有 ⇒ `bad_args`，那句话列出认得的几家）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct OptinReq {
    pub(crate) agent: crate::agents::AgentKind,
}

crate::wire::typed::codes! {
    /// `relay-optin` 的码：家目录说不出 · 那一家没有这一形。
    pub(crate) enum OptinErr {
        Failed = "failed",
    }
}

/// `relay-optin`：直接敲的那一家也走中转（读这台那份用户级设置文件 ⇒ 阻塞档）。
pub(crate) struct RelayOptin;

impl crate::wire::typed::Command for RelayOptin {
    const NAME: &'static str = "relay-optin";
    type Req = OptinReq;
    type Reply = endpoint::Optin;
    type Err = OptinErr;
}

impl crate::wire::typed::Handle for RelayOptin {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: OptinReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<endpoint::Optin, OptinErr>> + Send + 'static {
        async move { answer_optin(&req).map_err(crate::wire::typed::of_pair(OptinErr::Failed)) }
    }
}
