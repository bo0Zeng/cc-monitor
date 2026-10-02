//! **起一个会话的那一行** —— 这台后端出成品，界面只把那一行交给 monitor 开终端跑。
//!
//! 起会话只有 `ccm` 一处：每一条起会话 / 接回 / 换号重启 / 分叉交给终端的都只是一行 `ccm …`
//! （[`ccm_invocation`] 渲），环境、中转地址、身份标记由那台机器上的 `ccm` 自己做。
//! [`wire`]：`launch-render-cli` 的线上形状（远端，以及本机就地 resume）。[`local`]：本机起会话（`launch-local`）。
//! 本机远端同一条 `chan.call(origin, …)`：渲的就是那台机器要跑的那一行，能力问它自己。

pub mod ccm_invocation;
pub(crate) mod local;
pub mod wire;

#[cfg(test)]
mod cli_parity;

use serde_json::Value;

type Answer = Result<Value, (&'static str, String)>;

/// 入参按线上形状严格收（`deny_unknown_fields`）；收不下是契约错。
fn decode<T: serde::de::DeserializeOwned>(args: &Value) -> Result<T, (&'static str, String)> {
    serde_json::from_value(args.clone()).map_err(|e| {
        (
            "bad_args",
            crate::common::contract::malformed(&e.to_string()),
        )
    })
}

/// `launch-render-cli`：那台机器要跑的那一行 `ccm …`。成品 `{cmd}`；渲不出来 ⇒ `refused` ＋ 理由。
pub(crate) fn answer_cli(args: &Value) -> Answer {
    let req: wire::CliRenderRequest = decode(args)?;
    let cmd = wire::render_ccm_launch(&req).map_err(|said| ("refused", said))?;
    Ok(serde_json::json!({ "cmd": cmd }))
}

/// `launch-local`：本机起会话那一行。成品 `{cmd, launchId}`；坏输入 / 目录不在 ⇒ `refused`。
pub(crate) fn answer_local(args: &Value) -> Answer {
    let req: local::LocalLaunchRequest = decode(args)?;
    let out = local::plan(&req, &local::Facts::PRODUCTION).map_err(|said| ("refused", said))?;
    Ok(serde_json::json!({ "cmd": out.cmd, "launchId": out.launch_id }))
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/answers_tests.rs"]
mod tests;
