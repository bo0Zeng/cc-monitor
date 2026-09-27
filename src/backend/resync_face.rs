//! 〔RESYNC · V149 · `设计/15 §4.1b`〕**`resync`（手动对齐）的帧面宿主** —— 薄壳，本体在 `observe/watcher.rs::resync`。
//!
//! 住顶层的理由同 `read_face`：`inbound.rs` 不许出现 `observe::`，`control/` 不许引用 `observe/`（`layering_guard`）。
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
    Ok(json!({
        "added": d.added,
        "removed": d.removed,
        "retagged": d.retagged,
        "watchers": watchers,
    }))
}
