//! 〔MIG-2 · `99 §2.1 ⑬` · `01 §1.1`〕**起一个会话的计划与渲染** —— 这台后端出成品，界面只把那串交给 monitor 开终端跑。
//!
//! 从 monitor `src/frontend/shell/src/backend/control/` 搬来（原在 monitor 进程里渲、界面 Tauri 问）：
//! [`payload`]（载荷内核 ＋ 外层 tmux 三格）· [`ccm_invocation`]（`ccm …` 调用行）· [`wire`]（线上形状 → 渲染器）。
//! [`local`]：本机起会话（新起 / resume / 接回）那一整条计划（原 `history.rs`）。
//! 本机远端同一条 `chan.call(origin, …)`：渲的就是那台机器要跑的那一串，能力 / 中转这些事实问它自己。
//!
//! 三条帧命令（`inbound::REGISTRY`）：`launch-render-cli` · `launch-render-payload` · `launch-local`。
//! 渲不出来：`ccm …` 调用行那一条是**诚实降级**（`ok:false` ＋ 理由，调用方换载荷那条）；
//! 载荷那条与本机那条是**拒**（码 `refused`，理由原样；调用方不许换条路糊过去）。

pub mod ccm_invocation;
pub(crate) mod local;
pub mod payload;
pub mod wire;

#[cfg(test)]
mod cli_parity;
#[cfg(test)]
mod payload_parity;
#[cfg(test)]
mod tmux_outer_parity;

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

/// 渲染路上的 `Err` 全带 [`payload::REFUSE_TAG`]（`every_business_rejection_is_tagged` 钉着）⇒ 码 `refused`，
/// 标本身摘掉（线上用码分，不再靠串约定）。
fn refused(msg: String) -> (&'static str, String) {
    let said = msg
        .strip_prefix(payload::REFUSE_TAG)
        .map_or(msg.as_str(), str::trim_start)
        .to_string();
    ("refused", said)
}

/// `launch-render-cli`：`ccm …` 调用行。成品 `{ok, cmd, reason}`（`ok:false` 是降级，不是错）。
pub(crate) fn answer_cli(args: &Value) -> Answer {
    let req: wire::CliRenderRequest = decode(args)?;
    Ok(serde_json::to_value(wire::render_ccm_launch(req)).expect("CliRenderResponse 可序列化"))
}

/// `launch-render-payload`：裸载荷 / 外层 tmux 三格。成品 `{cmd}`；坏输入 ⇒ `refused`。
pub(crate) fn answer_payload(args: &Value) -> Answer {
    let req: wire::PayloadRenderRequest = decode(args)?;
    let cmd = wire::render_launch_payload(req).map_err(refused)?;
    Ok(serde_json::json!({ "cmd": cmd }))
}

/// `launch-local`：本机起会话的整条计划。成品 `{cmd, launchId}`；坏输入 / 中转必需却不在 ⇒ `refused`。
pub(crate) fn answer_local(args: &Value) -> Answer {
    let req: local::LocalLaunchRequest = decode(args)?;
    let out = local::plan(&req, &local::Facts::PRODUCTION).map_err(refused)?;
    Ok(serde_json::json!({ "cmd": out.cmd, "launchId": out.launch_id }))
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/answers_tests.rs"]
mod tests;
