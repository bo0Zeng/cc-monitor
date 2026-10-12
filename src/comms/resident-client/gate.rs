//! **版本门**（桌面与手机一条规矩）：那台比手上这一版**旧** ⇒ 换掉它（桌面换装一次；手机还装不了 ⇒ 照接、说「要更新」）；
//! **同或新** ⇒ 接。能不能用不看 `BUILD_ID` 等不等，逐条命令由形状指纹判（[`crate::client::InboundClient::call`] 对不上的不发，说「后端版本不同」）。
//!
//! 新旧只由后端判（帧命令 `resident-verdict`，`control/deploy_plan.rs` 那一家）：桌面问本机常驻后端，手机问接上的那一台自己
//! （它没有本机后端可问；序是 `BUILD_ID` 的序键，哪一版后端算出来都一样）。这里只有问法与答的读法。

use std::time::Duration;

/// 那条命令的名字（与后端 `stream/inbound/registry/machine.rs` 同名）。
pub const VERDICT_CMD: &str = "resident-verdict";

/// 问那一趟的上限：纯判定、不拨号，只是一条长连接上一问一答。
pub const VERDICT_BUDGET: Duration = Duration::from_secs(10);

/// 后端对 hello 那一问的答：`replace` = 换掉再接 · `older` = 那台比手上这一版旧（版本那句话按它挑）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verdict {
    pub replace: bool,
    pub older: bool,
}

/// 入参：手上这一版 · 那台 hello 报的 · 这一趟换过没有（只有事实）。
pub fn verdict_args(mine: &str, theirs: &str, replaced: bool) -> serde_json::Value {
    serde_json::json!({ "mine": mine, "theirs": theirs, "replaced": replaced })
}

/// 应答 → [`Verdict`]（**严格收**：恰 `{action, older}` 两格、`action` 只认两个词 —— 两侧漂了当场说出来）。`Err` = 哪里不对（英文诊断，进原话）。
pub fn decode_verdict(v: &serde_json::Value) -> Result<Verdict, &'static str> {
    let obj = v.as_object().ok_or("not an object")?;
    if obj.len() != 2 {
        return Err("expected exactly `action` and `older`");
    }
    let replace = match obj.get("action").and_then(serde_json::Value::as_str) {
        Some("replace") => true,
        Some("attach") => false,
        _ => return Err("`action` is neither `replace` nor `attach`"),
    };
    let older = obj
        .get("older")
        .and_then(serde_json::Value::as_bool)
        .ok_or("`older` is not a bool")?;
    Ok(Verdict { replace, older })
}

/// 换不换、旧不旧：「我这一版」（`mine`）有值才去问（`ask`）；手上没带这一版的身份（`None`）⇒
/// **不问**、直接「接、不判旧」—— 没有对照物，新旧无从比。
pub async fn verdict_for<'a, E, F, Fut>(mine: Option<&'a str>, ask: F) -> Result<Verdict, E>
where
    F: FnOnce(&'a str) -> Fut,
    Fut: std::future::Future<Output = Result<Verdict, E>>,
{
    match mine {
        Some(m) => ask(m).await,
        None => Ok(Verdict {
            replace: false,
            older: false,
        }),
    }
}

#[cfg(test)]
#[path = "../../../tests/comms/resident-client/gate_tests.rs"]
mod tests;
