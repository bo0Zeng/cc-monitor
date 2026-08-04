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
    let args = crate::inbound_client::launch_args(mode_for(enter), name, keys, None, None);
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
mod tests {
    use super::*;

    /// ★★ **本件的核心决策**：`enter=false` 必须走**新** mode 名。
    ///
    /// 反过来（两支都发 `send-into`）就是 F04c 存在的理由本身被抹掉：
    /// `Escape` 会被 daemon 补上 `Enter` ⇒ 提交用户排队的文本。
    #[test]
    fn the_no_enter_case_uses_the_new_mode_name_and_the_other_reuses_the_old_one() {
        assert_eq!(mode_for(true), "send-into", "带回车那支应当复用既有 mode");
        assert_eq!(mode_for(false), "send-keys-raw", "裸键那支必须是新 mode 名");
        assert_ne!(
            mode_for(true),
            mode_for(false),
            "两支用了同一个 mode —— 那 `enter` 就没被表达出去，\
             `Escape` 会被 daemon 补上 `Enter`（= 提交用户输入框里排队的文本）"
        );
    }

    /// ★ 跨轨钉：这两个 mode 名 daemon 侧**真的认**。
    ///
    /// 「monitor 发一个 daemon 不认的 mode」的后果是每次都 `invalid_args` 然后回落 SSH ——
    /// **功能看着正常**（回落能用），而 daemon 那条路事实上从没被走过。
    /// 那正是这一整族「切了路由但其实没切」最难发现的形状。
    #[test]
    fn both_mode_names_are_ones_the_daemon_actually_parses() {
        let daemon = guard_core::production_code(include_str!(
            "../../../../remote-daemon-proto/src/control/launch.rs"
        ));
        for m in [mode_for(true), mode_for(false)] {
            let needle = format!("\"{m}\" => Some(Mode::");
            assert!(
                daemon.contains(needle.as_str()),
                "daemon 的 `Mode::parse` 里没有 `{m}` —— monitor 会一直拿 `invalid_args` \n\
                 然后回落 SSH：**功能正常、而 daemon 那条路从没走过**"
            );
        }
    }

    /// 与 SSH 那条路的拒绝文案对齐（含反向锚点：SSH 侧那些串还在）。
    #[test]
    fn the_refusal_wording_matches_the_ssh_path() {
        let ssh = include_str!("../../tmux.rs");
        for (code, needle) in [
            ("no_tmux", "远端未安装 tmux"),
            ("no_such_session", "远端会话已不存在（可能已被终止）"),
            ("wrong_owner", "可能不是本工具管理的会话"),
        ] {
            assert!(
                refusal_text(code, "m").contains(needle),
                "`{code}` 的文案里没有 {needle:?}"
            );
            assert!(
                ssh.contains(needle),
                "SSH 那条路里已经没有 {needle:?} 了 —— 两条路的文案漂了"
            );
        }
        // `typed_unconfirmed` 是 daemon 独有的一档（SSH 那条路分不出来）⇒ 只要求它不被吞掉。
        assert!(
            refusal_text("typed_unconfirmed", "x").contains("未必"),
            "`typed_unconfirmed` 被说成了确定的成功或确定的失败 —— 它是「不确定」那一档"
        );
    }
}
