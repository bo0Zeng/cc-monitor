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
            "{e:?} **不能证明**这条命令没发出去（或后端已经说了话）——\n\
                 允许回落就等于在未知状态上再做一次动作，\
                 而 `wrong_owner`/`too_many_windows` 更是把门拒绝洗成另一条路的成功"
        );
    }
    // 「没有通道」那一档也必须是可回落的。
    assert!(matches!(no_channel("h1"), Routed::NoChannel(_)));
}

/// ⚠ **老后端不认新 mode 时回的是 `invalid_args`，那**不是**「可回落」。**
///
/// 这条单独写出来是因为它反直觉：F04c 选「新 mode 名」的理由正是
/// 「老后端会明确报错而不是静默做错」，很容易顺手把它归成 `NoChannel` 去回落 SSH。
/// 但 `invalid_args` 是 **backend 说过话了** —— 它可能是「mode 不认」，
/// 也可能是「名字含 `:`」这种真该拒的形状问题，**在这一层分不开**。
/// ⇒ 一律不回落；要给「老后端」开回落，得靠 `accepts()`/`Unsupported` 那条**命令级**
/// 能力协商，而不是猜错误码。
#[test]
fn an_old_backend_rejecting_the_new_mode_is_not_a_reason_to_fall_back() {
    let e = CallError::Remote {
        code: "invalid_args".into(),
        message: "未知 mode `send-keys-raw` —— 只有 create-or-attach / send-into".into(),
    };
    match route_call_error(&e, plain) {
        Routed::Refused(msg) => assert!(msg.contains("invalid_args")),
        other => panic!("`invalid_args` 被判成了 {other:?} —— 它是后端说的话，不许回落"),
    }
}

/// `backend/control/` 里每个**走后端的发送端**的判定。
///
/// # ⚠ 为什么这里是登记表而不是一句「都必须用分流器」
///
/// 第一版是**手写的两条清单**（`backend_kill.rs` / `backend_send_keys.rs`）——
/// Phase G 的 `/full-audit` 当场指出：同一个目录里**第三个**走后端的发送端
/// `backend_launch.rs::backend_send_into`（U8a-2c-1，早于本工作区）**不在清单里**，
/// 于是它自己 match 一整套 `CallError`、把**每一档**都折成「诚实降级」，
/// 而调用方拿到降级就**回落到 TS 渲染的整串**（那条串没有 §34 的 Gate 2）。
///
/// ⇒ **一次 `wrong_owner`（门说「这不是本工具的会话」）会被回落成一条无门的 shell 路。**
/// 那正是本模块开头声称要挡的「把被门拒绝洗成另一条路的成功」，
/// **而它就发生在守卫脚下** —— 因为守卫的发现机制是手写清单。
///
/// ★ 「硬编码清单 vs 递归遍历」这一族在本仓这是**第三个模块**
/// （`readonly_guard::spawn_registry` · `tmux_backend_gate_guard` 的两文件表 · 本条），
/// 而这一次是**我自己本轮亲手挖的**。⇒ 改成**遍历 + 登记表**：
/// 目录里每个 `.call(` 的文件都必须在下表里，要么用分流器、要么是**带理由的刻意例外**。
#[cfg(test)]
const SENDERS: &[(&str, Verdict)] = &[
    ("backend_kill.rs", Verdict::UsesRouter),
    ("backend_send_keys.rs", Verdict::UsesRouter),
    // ✅ **F14 已收进来**：`SendIntoResponse` 加了第三个字段 `may_fall_back`
    // （两态表达不出「不许回落」），分流本体改调 `route_call_error`。
    // ⚠ 上一版把它记成 `ExemptPendingF14`，而那条判据断言例外那格**不**用分流器
    // ⇒ 改好的当天它**如设计般红了一次**，逼人回来把登记改对。**那是它的岗位。**
    ("backend_launch.rs", Verdict::UsesRouter),
    // ★ 08-08 扩面当场逮出来的**第四个真实发送端**（此前整个在扫描面之外）。
    // 它发的是 `probe_backend` 里那条 `ping`：只把成败渲染成 `control=ok(..ms)` /
    // `control=failed(..)` 的诊断串，**不做任何回落决策** ⇒ 没有「该不该回落」这个问题。
    ("ssh_source.rs", Verdict::ProbeOnlyNoFallbackDecision),
    // ★ P4f 08-13：**本机 cc-bus 写面**（`cc_bus_send` 对 `<local>`）走后端的
    // `bus-send` 原语。它没有第二条路可回落（本机 shell 写面正是 `P4a` 拒掉的东西），
    // 两档结果都只渲染成给用户的一句话 —— 但**照样走分流器**：
    // 分流规则有第二份实现的那天，「被门拒绝」就会在某一份里被洗成「换条路重做」。
    ("cc_bus.rs", Verdict::UsesRouter),
    // ★ `K-R112` 09-13：**第七个发送端** —— 抓屏（`tmux.rs::capture_via_backend`）
    //   改走帧面 `capture-pane`。它**没有第二条路可回落**（那条一次性 SSH 本件删净了），
    //   但**照样走分流器**，理由与 `cc_bus.rs` 那条逐字相同：
    //   〔`设计/50`：原话还并列了 `account_usage.rs`（`K-R104` 的第六个发送端）——
    //    用量 ③ 轴整轴退役，那个发送端不存在了，发送端从七个变回六个。〕
    //   本模块的三态是从 `Routed` 搬过来的，不是它自己 match 一遍错误枚举。
    //   ⚠ 它回的是 `Result<String, Routed>` —— 「拿到了那一屏」与「三态里的另外两态」
    //   在类型上分得开，怎么对用户说由调用方 `capture_remote_pane` 决定。
    ("tmux.rs", Verdict::UsesRouter),
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
    /// **探测型发送端**：只把成败渲染成诊断文本，不决定「要不要回落到别的路」。
    ///
    /// ⚠ 与上面那一档**刻意分开**：`ExemptPendingF14` 说的是「本该走分流器、
    /// 但今天还差一步」，这一档说的是「**根本没有回落这回事**」。
    /// 合成一档会让「欠着」与「不适用」长得一样。
    ProbeOnlyNoFallbackDecision,
}

/// ★★ **零命中守卫：分流规则不许有第二份实现 —— 而发现机制是遍历，不是手写清单。**
#[test]
fn every_backend_sender_is_registered_and_uses_the_one_router() {
    // ⚠⚠ **08-08：发现机制原本只扫 `src/backend/control/` 这一个目录**，而本模块头注
    //   声称的是「monitor 侧走后端的**所有**控制命令共用」。实测：把一个 `.call(`
    //   发送端放在 `tmux.rs`（目录之外）并让它自己判回落，**全仓 989 条判据一条不红**
    //   —— **声称的范围与人群的范围不是同一个**。改扫整棵 monitor 源码树。
    //   ★ 扩面当场逮出**第四个真实发送端**（`ssh_source.rs` 的探测 ping），此前整个在扫描面之外。
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // 目录里所有「走后端」的文件：生产段出现 `.call(` 的。
    let verb = format!(".call({}", "");
    let mut senders: Vec<String> = Vec::new();
    // ⚠ 发现阶段就把生产段留下：下面按名字取时**不能再用 `dir.join(name)`**，
    //   扩面之后 `dir` 是整棵 `src/`，而发送端散在子目录里（第一版就栽在这，
    //   报「backend_kill.rs 生产段却没有 route_call_error」—— 其实是文件根本没读到）。
    let mut by_name: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (p, src) in guard_core::scan_tree!(&dir, &["rs"]) {
        let prod = guard_core::production_code(&src);
        if prod.contains(verb.as_str()) {
            senders.push(p.file_name().unwrap().to_string_lossy().to_string());
            by_name.insert(
                p.file_name().unwrap().to_string_lossy().to_string(),
                prod.clone(),
            );
        }
    }
    senders.sort();
    // ★ 抽取器自检：遍历坏掉时下面几条会零命中地绿。
    assert!(
        senders.len() >= 3,
        "只扫到 {} 个走后端的发送端（{senders:?}）—— 遍历坏了。\n\
             F12 实测是 3 个：backend_kill.rs · backend_launch.rs · backend_send_keys.rs",
        senders.len()
    );
    let mut registered: Vec<String> = SENDERS.iter().map(|(f, _)| f.to_string()).collect();
    registered.sort();
    assert_eq!(
        senders, registered,
        "**整棵 monitor 源码树**里走后端的发送端与登记表对不上。\n\
             ⚠ **新增一个发送端就必须在这里表态**：要么用共用分流器 `route_call_error`，\n\
             要么写成带理由的刻意例外。**手写清单看不见新文件** —— 那正是 F12 的\n\
             `/full-audit` 在本守卫身上逮到的东西（第三个发送端整个逃出了扫描面）。"
    );
    for (name, verdict) in SENDERS {
        let prod = by_name
            .get(*name)
            .unwrap_or_else(|| panic!("`{name}` 在登记表里但发现阶段没扫到 —— 上面那条已保证不会"));
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
            Verdict::ProbeOnlyNoFallbackDecision => {
                // ★ **「不做回落决策」这件事本身也要钉**：不许自己 match `CallError`
                //   去分流（那就是第二份分流规则），也不必调分流器。
                //   只写「它是探测」而不判任何东西 = 把登记表变成免检章。
                assert!(
                    !uses,
                    "`{name}` 登记为「探测型、不做回落决策」，却在用 `route_call_error` —— \
                         那说明它其实有回落决策，登记该改成 `UsesRouter`"
                );
                assert!(
                    !own,
                    "`{name}` 登记为「探测型、不做回落决策」，却自己在 match `CallError` —— \
                         那就是第二份分流规则。要么改用 `route_call_error`，要么说清它判的不是回落。"
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
