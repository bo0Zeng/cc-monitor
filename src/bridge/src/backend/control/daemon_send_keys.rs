//! F04c：**daemon `send-keys` 的 monitor 侧发送端** —— C6 那条顺序的收尾。
//!
//! # 它在定框里的位置
//!
//! C5：任何**改状态**的 tmux 命令一律归 `control/`。`send-keys` 是「往别人的会话里打字」——
//! 改的是那个会话的状态，所以它归这边。C6 的顺序（先搬门、再切路由）到 F04b 只走完了 kill 那半，
//! **本模块是另一半**。
//!
//! # ★★ 两个 mode，不是一个带开关的 mode
//!
//! monitor 侧 `tmux_send_keys(…, enter)` 有两种语义，daemon 侧对应**两个 mode 名**：
//!
//! | `enter` | daemon mode | 生产上是谁 |
//! |---|---|---|
//! | `true` | `send-into`（**既有**，F03 起就带 Gate 2） | `/compact` · `/exit` |
//! | `false` | `send-keys-raw`（**F04c 新增**） | 优雅退出的 `Escape`（打断当前回合） |
//!
//! **为什么不是给 `launch` 加一个 `enter` 字段**：daemon 的 `parse_request` 是手工从 `Map`
//! 取键的、**不 deny unknown fields** ⇒ 旧版本 daemon 会**静默忽略**那个字段、照样附 `Enter`
//! ⇒ 把「打断当前回合」变成「**提交用户输入框里排队的文本**」。
//! 换成新 mode 名则天然 fail-closed：旧 daemon 回 `invalid_args`，monitor 拿到明确错误。
//!
//! ⚠ **顺带的好处**：`enter=true` 那两支用的是**既有** mode ⇒ 旧 daemon 也接得住，
//! 只有 `Escape` 那一支需要新版本。**兼容面不是全有全无。**
//!
//! # 回落与「不许回落」
//!
//! 分流不在这里写 —— 它住 [`super::daemon_route`]，两个命令共用一份
//! （两份会漂，而漂开的后果是把一次门拒绝洗成另一条路的成功）。

use std::time::Duration;

/// 一次 `send-keys` 的往返上限。同 `daemon_kill` / `daemon_launch` 的理由。
const CALL_TIMEOUT_SECS: u64 = 10;

/// `enter` → daemon 的 mode 名。**这是本模块唯一的「决策」**，所以抠成纯函数。
///
/// ⚠ 别把它「简化」成一个 `enter` 字段传下去 —— 见模块头注，那是静默做错。
pub(crate) fn mode_for(enter: bool) -> &'static str {
    if enter {
        // 既有 mode：键入载荷 + 尾 `Enter`。逐字就是 `enter=true` 的语义。
        "send-into"
    } else {
        // F04c 新增：发裸键、不附 `Enter`。
        "send-keys-raw"
    }
}

/// daemon 的错误码 → 用户看的话。**与 SSH 那条路的文案逐条对齐**，
/// 否则同一个拒绝在两条路上说两种话。
fn refusal_text(code: &str, message: &str) -> String {
    match code {
        "no_tmux" => "远端未安装 tmux".to_string(),
        "no_such_session" => "远端会话已不存在（可能已被终止）".to_string(),
        "wrong_owner" => {
            format!("拒绝 send-keys：目标未通过身份守卫（{message}）——可能不是本工具管理的会话")
        }
        // daemon 侧的 `typed_unconfirmed`：会话在，但键未必落。**不许当成成功**。
        "typed_unconfirmed" => format!("按键未必送达（{message}）—— 会话在，但 send-keys 失败"),
        _ => format!("远端 send-keys 失败（{code}）：{message}"),
    }
}

/// **F04c：往一个已存在的远端 tmux 会话发按键（走 daemon `control/launch.rs`）。**
///
/// 不是 `#[tauri::command]` —— 前端**够不着才对**（C9：frontend 只剩开窗）。
/// 唯一调用方是 `tmux.rs::tmux_send_keys`，它按三态分流。
pub(crate) async fn daemon_send_keys(
    origin: &str,
    name: &str,
    keys: &str,
    enter: bool,
) -> super::daemon_route::Routed {
    let Some(client) = crate::inbound_client::client_for(origin) else {
        return super::daemon_route::no_channel(origin);
    };
    // ⚠ `LaunchExtras::default()`：这条路发的是 `send-into` / `send-keys-raw`，
    //   而 `agent` / `width` / `height` 只对**新建会话**有意义（`K-P2` `D3`）。
    let args = crate::inbound_client::launch_args(
        mode_for(enter),
        name,
        keys,
        None,
        None,
        Default::default(),
    );
    match client
        .call("launch", args, Duration::from_secs(CALL_TIMEOUT_SECS))
        .await
    {
        Ok(reply) => match super::daemon_launch::typed_from_reply(reply.as_ref()) {
            Ok(true) => super::daemon_route::Routed::Done,
            Ok(false) => super::daemon_route::Routed::Refused(
                "daemon 回报未键入，但也没给错误码 —— 协议漂移，不再用另一条路重试".into(),
            ),
            Err(e) => super::daemon_route::Routed::Refused(format!(
                "{e} —— ⚠ 应答形状不认识时无法判断键有没有落进去，因此不再用另一条路重发"
            )),
        },
        Err(e) => super::daemon_route::route_call_error(&e, refusal_text),
    }
}

#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/daemon_send_keys_tests.rs"]
mod tests;
