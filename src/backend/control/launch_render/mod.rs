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

use crate::control::launch_account::{self as la, Settled};
use serde_json::Value;

/// 失败：码 ＋ 那一句 ＋ 按码定形的 `data`（只有 `account_unavailable` 带：[`la::AccountUnavailable`]）。
pub(crate) type Failed = (&'static str, String, Option<Value>);
type Answer = Result<Value, Failed>;

/// 入参按线上形状严格收（`deny_unknown_fields`）；收不下是契约错。
fn decode<T: serde::de::DeserializeOwned>(args: &Value) -> Result<T, Failed> {
    serde_json::from_value(args.clone()).map_err(|e| {
        (
            "bad_args",
            crate::common::contract::malformed(&e.to_string()),
            None,
        )
    })
}

/// 选不了号 ⇒ 命令级码 `account_unavailable`，`data` 是那一形。
pub(crate) fn unavailable(u: la::AccountUnavailable) -> Failed {
    let said = la::unavailable_said(&u);
    ("account_unavailable", said, serde_json::to_value(u).ok())
}

/// 应答里的 `account`：实际用的号（账号库里的那一形；别的 ⇒ `null`）。
pub(crate) fn launched(account: &Settled) -> Value {
    match account {
        Settled::Account(a) => serde_json::to_value(a).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

fn refused(said: String) -> Failed {
    ("refused", said, None)
}

/// `launch-render-cli`：那台机器要跑的那一行 `ccm …`。成品 `{cmd, account}`；渲不出来 ⇒ `refused` ＋ 理由；
/// 要的号选不了 ⇒ `account_unavailable`。
pub(crate) fn answer_cli(args: &Value, facts: &la::Facts) -> Answer {
    let req: wire::CliRenderRequest = decode(args)?;
    let account = wire::settle(&req, facts).map_err(unavailable)?;
    let cmd = wire::render_ccm_launch(&req, &account).map_err(refused)?;
    Ok(serde_json::json!({ "cmd": cmd, "account": launched(&account) }))
}

/// `launch-local`：本机起会话那一行。成品 `{cmd, launchId, account}`；坏输入 / 目录不在 ⇒ `refused`；
/// 要的号选不了 ⇒ `account_unavailable`。
pub(crate) fn answer_local(args: &Value, facts: &la::Facts) -> Answer {
    let req: local::LocalLaunchRequest = decode(args)?;
    let account = local::settle(&req, facts).map_err(unavailable)?;
    let out = local::plan(&req, &account, &local::Facts::PRODUCTION).map_err(refused)?;
    Ok(
        serde_json::json!({ "cmd": out.cmd, "launchId": out.launch_id, "account": launched(&account) }),
    )
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/answers_tests.rs"]
mod tests;

// 假启动器是 POSIX shell 脚本、`ccm` 在那一形上 exec 掉子进程 ⇒ 只在 POSIX 上跑。
#[cfg(all(test, unix))]
#[path = "../../../../tests/backend/control/launch_render/history_resume_e2e_tests.rs"]
mod history_resume_e2e;
