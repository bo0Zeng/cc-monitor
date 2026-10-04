//! **`resync`（手动对齐）的帧面宿主** —— 薄壳，本体在 `observe/watcher.rs::resync`。
//!
//! 住顶层的理由同 `read_face`：`stream/inbound/` 不许出现 `observe::`，`control/` 不许引用 `observe/`（`layering_guard`）。
//! 用户按钮触发、不是节拍：零定时器铁律不变。

use serde_json::{json, Value};

/// `args.sid` 可选：给了 ⇒ 只对那一个会话（关卡 2「对齐后重试」）；不给 ⇒ 整机。
pub(crate) fn answer(args: &Value) -> Result<Value, (&'static str, String)> {
    let only = match args.get("sid") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if !s.is_empty() => Some(s.as_str()),
        Some(_) => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("`sid` must be a non-empty string when given"),
            ))
        }
    };
    let (d, watchers) = crate::observe::watcher::resync(only);
    // 顺带交回这台**当下**的能力事实（与 hello 那两格同一个函数、同形）：
    // 握手之后才装上 tmux 的机器，靠「重新对齐」让客户端认出来（monitor 换进 `chan::wire::Offer`）。
    Ok(json!({
        "added": d.added,
        "removed": d.removed,
        "retagged": d.retagged,
        "caught_up": d.caught_up, // 在跟的会话从游标补读出的行数合计
        "watchers": watchers,
        "unavailable": crate::unavailable_here(),
        "uncancellable": crate::stream::inbound::uncancellable(),
    }))
}
