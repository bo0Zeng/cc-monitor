//! F04c：**「这条命令能不能回落」的唯一判定**（monitor 侧走 daemon 的所有控制命令共用）。
//!
//! # 为什么它必须只有一份
//!
//! F04b 给 `kill` 定下了三态分流，F04c 给 `send-keys` 也要同一套。**这条规则一旦有两份实现，
//! 它们就会漂**，而漂开的后果不是「行为不一致」这么轻 —— 它是**静默的权限旁路**：
//! 把 daemon 的一次 `wrong_owner` 当成「daemon 不可用」而回落到 SSH 路再做一次，
//! 等于把**一次被门拒绝洗成另一条路的成功**。
//! 今天两条路的门恰好等价（都是 §34 三道门）所以功能上看不出差别 —— **那正是它危险的地方**。
//!
//! ⇒ 同 `gate-core` 的手法（定框 C1「一份代码、两种承载」的同一条纪律）：判定收成一份，
//! 调用方只负责给「拒绝该怎么对用户说」。由 `both_daemon_commands_use_this_one_router` 钉住。
//!
//! # 分界线不是「成功/失败」，是「**能不能证明这条命令根本没发出去**」
//!
//! 逐档读 `inbound_client::call` 的源码定的（不是猜）：
//!
//! | 档 | 在 `call` 里的位置 | 判定 |
//! |---|---|---|
//! | `client_for(origin) == None` | 连 client 都没有，一个字节没发 | **证明没发出去** ⇒ 可回落 |
//! | `Unsupported` | `call` 第一行 `if !self.accepts(cmd)`，早于 `next_id`/`register`/`send` | **证明没发出去** ⇒ 可回落（旧 daemon） |
//! | `TooManyPending` | `register(&id)` 失败，仍早于 `writes.send` | **证明没发出去** ⇒ 可回落 |
//! | `Disconnected` | **两个产地**：写队列 send 失败（没入队）**或** 等应答时 `rx` 掉了（已发出） | 分不开 ⇒ 按最坏算 ⇒ **不回落** |
//! | `Timeout` | **两个产地**：写入段超时（源码逐字写着「daemon 没见过这条命令」）**或** 等应答段超时（可能已执行） | 分不开 ⇒ 按最坏算 ⇒ **不回落** |
//! | `Cancelled` / `Remote{..}` | daemon 说过话了 | **不回落** |
//!
//! ⚠ **诚实边界**（F04b 记的，仍未变）：`Timeout` 与 `Disconnected` 各有两个产地，
//! 一个能证明没发出去、一个不能，而**类型上分不开**。这里只能按最坏的那个处理 ⇒
//! 一次写入段超时会让用户拿到错误而不是回落。要修得在 `inbound_client` 那边把两个产地
//! 分成两个变体 —— **那是它自己的活**。记在这里，别让下一个人以为是漏了。

use crate::inbound_client::CallError;

/// 一条走 daemon 的控制命令的结局。**三态**，分界线见模块头注。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Routed {
    /// daemon 确认做完了。
    Done,
    /// **证明**这条命令没发出去 ⇒ 调用方可以回落到过渡期的 SSH 路径（C7）。
    /// 带上原因只为诊断，**不参与分流判断**。
    NoChannel(String),
    /// daemon 说了话（拒绝 / 失败），**或者**我们无法证明它没执行 ⇒
    /// **不许回落**，把这句话原样交给用户。
    Refused(String),
}

/// `CallError` → 三态。`refusal` 只负责把 `(code, message)` 翻成用户看的话 ——
/// **分流本身不许由调用方决定**，那就是本模块存在的全部意义。
pub(crate) fn route_call_error(e: &CallError, refusal: impl Fn(&str, &str) -> String) -> Routed {
    match e {
        // 以下三档都在 `call()` 真正写出去**之前**返回 —— 见模块头注那张表。
        CallError::Unsupported { cmd, offered } => Routed::NoChannel(format!(
            "远端 daemon 没声明 `{cmd}` 能力（它声明的是 {offered:?}）—— 多半是旧版本"
        )),
        CallError::TooManyPending => {
            Routed::NoChannel("入方向同时在等的命令已达上限，这条没入队".into())
        }
        // 以下都不能证明「没发出去」⇒ 按最坏算，不回落。
        CallError::Disconnected | CallError::Timeout { .. } | CallError::Cancelled => {
            Routed::Refused(format!(
                "{e} —— ⚠ 无法确认远端是否已经执行过这条命令，因此**不**再用另一条路重做一次；\
                 请刷新会话列表后再决定"
            ))
        }
        CallError::Remote { code, message } => Routed::Refused(refusal(code, message)),
    }
}

/// 没有控制通道（`client_for` 回 `None`）—— **一个字节都没发出去**，可回落。
pub(crate) fn no_channel(origin: &str) -> Routed {
    Routed::NoChannel(format!(
        "[{origin}] 没有可用的控制通道（daemon 未在场或长连接未握手）"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn plain(code: &str, message: &str) -> String {
        format!("{code}/{message}")
    }

    /// ★★ **本模块的核心性质**：只有「能证明没发出去」的三档才允许回落。
    ///
    /// 反过来错法（把 `Remote{wrong_owner}` 也当成 `NoChannel`）会让一次**被门拒绝**
    /// 转头走 SSH 再做一次 —— 那是把门拒绝洗成另一条路的成功。
    #[test]
    fn only_the_errors_that_prove_nothing_was_sent_allow_a_fallback() {
        let fallback_ok = [
            CallError::Unsupported {
                cmd: "kill".into(),
                offered: vec!["ping".into()],
            },
            CallError::TooManyPending,
        ];
        for e in &fallback_ok {
            assert!(
                matches!(route_call_error(e, plain), Routed::NoChannel(_)),
                "{e:?} 是在写出去之前返回的，应当允许回落"
            );
        }
        let no_fallback = [
            CallError::Disconnected,
            CallError::Timeout {
                after: Duration::from_secs(1),
            },
            CallError::Cancelled,
            CallError::Remote {
                code: "wrong_owner".into(),
                message: "sid=".into(),
            },
            CallError::Remote {
                code: "too_many_windows".into(),
                message: "windows=3".into(),
            },
            CallError::Remote {
                code: "kill_failed".into(),
                message: "boom".into(),
            },
            CallError::Remote {
                code: "no_such_session".into(),
                message: "".into(),
            },
            CallError::Remote {
                code: "invalid_args".into(),
                message: "未知 mode `send-keys-raw`".into(),
            },
        ];
        for e in &no_fallback {
            assert!(
                matches!(route_call_error(e, plain), Routed::Refused(_)),
                "{e:?} **不能证明**这条命令没发出去（或 daemon 已经说了话）——\n\
                 允许回落就等于在未知状态上再做一次动作，\
                 而 `wrong_owner`/`too_many_windows` 更是把门拒绝洗成另一条路的成功"
            );
        }
        // 「没有通道」那一档也必须是可回落的。
        assert!(matches!(no_channel("h1"), Routed::NoChannel(_)));
    }

    /// ⚠ **老 daemon 不认新 mode 时回的是 `invalid_args`，那**不是**「可回落」。**
    ///
    /// 这条单独写出来是因为它反直觉：F04c 选「新 mode 名」的理由正是
    /// 「老 daemon 会明确报错而不是静默做错」，很容易顺手把它归成 `NoChannel` 去回落 SSH。
    /// 但 `invalid_args` 是 **daemon 说过话了** —— 它可能是「mode 不认」，
    /// 也可能是「名字含 `:`」这种真该拒的形状问题，**在这一层分不开**。
    /// ⇒ 一律不回落；要给「老 daemon」开回落，得靠 `accepts()`/`Unsupported` 那条**命令级**
    /// 能力协商，而不是猜错误码。
    #[test]
    fn an_old_daemon_rejecting_the_new_mode_is_not_a_reason_to_fall_back() {
        let e = CallError::Remote {
            code: "invalid_args".into(),
            message: "未知 mode `send-keys-raw` —— 只有 create-or-attach / send-into".into(),
        };
        match route_call_error(&e, plain) {
            Routed::Refused(msg) => assert!(msg.contains("invalid_args")),
            other => panic!("`invalid_args` 被判成了 {other:?} —— 它是 daemon 说的话，不许回落"),
        }
    }

    /// ★ **零命中守卫：分流规则不许有第二份实现。**
    ///
    /// 两个消费方（`daemon_kill` / `daemon_send_keys`）都必须调本模块，
    /// 且**不许自己 match `CallError`** —— 那就是漂的起点。
    #[test]
    fn both_daemon_commands_use_this_one_router() {
        for (name, src) in [
            ("daemon_kill.rs", include_str!("daemon_kill.rs")),
            ("daemon_send_keys.rs", include_str!("daemon_send_keys.rs")),
        ] {
            let prod = guard_core::production_code(src);
            assert!(
                prod.contains("route_call_error"),
                "`{name}` 的生产段没有调 `route_call_error` —— 分流规则被就地重写了一份"
            );
            // 运行时拼，免得命中本行自己。
            let needle = format!("CallError::{}", "");
            assert!(
                !prod.contains(needle.as_str()),
                "`{name}` 的生产段自己在 match `CallError` —— 那是分流规则的第二份实现。\n\
                 它一旦与本模块漂开，一次 `wrong_owner` 就可能被另一条路重做一遍。"
            );
        }
    }
}
