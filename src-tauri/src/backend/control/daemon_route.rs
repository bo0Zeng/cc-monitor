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
//! 调用方只负责给「拒绝该怎么对用户说」。由 `every_daemon_sender_is_registered_and_uses_the_one_router`
//! 钉住 —— ⚠ **它的发现机制是遍历目录，不是手写清单**（F12 的 `/full-audit` 逮到手写那版
//! 漏掉了第三个发送端，见那条判据的头注）。
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

    /// `backend/control/` 里每个**走 daemon 的发送端**的判定。
    ///
    /// # ⚠ 为什么这里是登记表而不是一句「都必须用分流器」
    ///
    /// 第一版是**手写的两条清单**（`daemon_kill.rs` / `daemon_send_keys.rs`）——
    /// Phase G 的 `/full-audit` 当场指出：同一个目录里**第三个**走 daemon 的发送端
    /// `daemon_launch.rs::daemon_send_into`（U8a-2c-1，早于本工作区）**不在清单里**，
    /// 于是它自己 match 一整套 `CallError`、把**每一档**都折成「诚实降级」，
    /// 而调用方拿到降级就**回落到 TS 渲染的整串**（那条串没有 §34 的 Gate 2）。
    ///
    /// ⇒ **一次 `wrong_owner`（门说「这不是本工具的会话」）会被回落成一条无门的 shell 路。**
    /// 那正是本模块开头声称要挡的「把被门拒绝洗成另一条路的成功」，
    /// **而它就发生在守卫脚下** —— 因为守卫的发现机制是手写清单。
    ///
    /// ★ 「硬编码清单 vs 递归遍历」这一族在本仓这是**第三个模块**
    /// （`readonly_guard::spawn_registry` · `tmux_daemon_gate_guard` 的两文件表 · 本条），
    /// 而这一次是**我自己本轮亲手挖的**。⇒ 改成**遍历 + 登记表**：
    /// 目录里每个 `.call(` 的文件都必须在下表里，要么用分流器、要么是**带理由的刻意例外**。
    #[cfg(test)]
    const SENDERS: &[(&str, Verdict)] = &[
        ("daemon_kill.rs", Verdict::UsesRouter),
        ("daemon_send_keys.rs", Verdict::UsesRouter),
        // ✅ **F14 已收进来**：`SendIntoResponse` 加了第三个字段 `may_fall_back`
        // （两态表达不出「不许回落」），分流本体改调 `route_call_error`。
        // ⚠ 上一版把它记成 `ExemptPendingF14`，而那条判据断言例外那格**不**用分流器
        // ⇒ 改好的当天它**如设计般红了一次**，逼人回来把登记改对。**那是它的岗位。**
        ("daemon_launch.rs", Verdict::UsesRouter),
    ];

    #[cfg(test)]
    #[derive(PartialEq, Eq, Debug)]
    enum Verdict {
        UsesRouter,
        /// ⚠ **今天零个成员**（F14 之后）。**刻意保留这一档**：
        /// 它是「带理由的刻意例外」这个形态本身，下一个发送端要走例外时有地方落。
        /// 删掉它 = 逼下一个人要么硬改要么偷偷绕过登记表（铁律 13：别因「暂时没人用」删判据形态）。
        #[allow(dead_code)]
        ExemptPendingF14,
    }

    /// ★★ **零命中守卫：分流规则不许有第二份实现 —— 而发现机制是遍历，不是手写清单。**
    #[test]
    fn every_daemon_sender_is_registered_and_uses_the_one_router() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend/control");
        // 目录里所有「走 daemon」的文件：生产段出现 `.call(` 的。
        let verb = format!(".call({}", "");
        let mut senders: Vec<String> = Vec::new();
        for e in std::fs::read_dir(&dir)
            .expect("读不到 backend/control/")
            .flatten()
        {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("rs") {
                continue;
            }
            let prod =
                guard_core::production_code(&std::fs::read_to_string(&p).unwrap_or_default());
            if prod.contains(verb.as_str()) {
                senders.push(p.file_name().unwrap().to_string_lossy().to_string());
            }
        }
        senders.sort();
        // ★ 抽取器自检：遍历坏掉时下面几条会零命中地绿。
        assert!(
            senders.len() >= 3,
            "只扫到 {} 个走 daemon 的发送端（{senders:?}）—— 遍历坏了。\n\
             F12 实测是 3 个：daemon_kill.rs · daemon_launch.rs · daemon_send_keys.rs",
            senders.len()
        );
        let mut registered: Vec<String> = SENDERS.iter().map(|(f, _)| f.to_string()).collect();
        registered.sort();
        assert_eq!(
            senders, registered,
            "`backend/control/` 里走 daemon 的发送端与登记表对不上。\n\
             ⚠ **新增一个发送端就必须在这里表态**：要么用共用分流器 `route_call_error`，\n\
             要么写成带理由的刻意例外。**手写清单看不见新文件** —— 那正是 F12 的\n\
             `/full-audit` 在本守卫身上逮到的东西（第三个发送端整个逃出了扫描面）。"
        );
        for (name, verdict) in SENDERS {
            let prod = guard_core::production_code(
                &std::fs::read_to_string(dir.join(name)).unwrap_or_default(),
            );
            let uses = prod.contains("route_call_error");
            // 运行时拼，免得命中本行自己。
            let needle = format!("CallError::{}", "");
            let own = prod.contains(needle.as_str());
            match verdict {
                Verdict::UsesRouter => {
                    assert!(
                        uses,
                        "`{name}` 登记为用分流器，生产段却没有 `route_call_error`"
                    );
                    assert!(
                        !own,
                        "`{name}` 的生产段自己在 match `CallError` —— 那是分流规则的第二份实现。\n\
                         它一旦与本模块漂开，一次 `wrong_owner` 就可能被另一条路重做一遍。"
                    );
                }
                Verdict::ExemptPendingF14 => {
                    // ★ **例外也要钉**：它今天确实还没用分流器（那是 F14 的活）；
                    // 一旦它改好了，本条会红 —— 逼人把登记改成 `UsesRouter`。
                    assert!(
                        !uses,
                        "`{name}` 已经在用 `route_call_error` 了 —— **这多半是好事**（F14 做完了）：\n\
                         把它的登记从 `ExemptPendingF14` 改成 `UsesRouter`，并关掉 ROADMAP 的 F14。"
                    );
                }
            }
        }
    }
}
