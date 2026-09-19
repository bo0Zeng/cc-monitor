use super::*;

/// ★ **`mode` 恒 `send-into`**，而且 `create-or-attach` 绝不许从这条路溜出去。
///
/// 变异「把 mode 改成 create-or-attach」= 让这条路开始**新建会话** ——
/// 那正是 issue #76 的失管会话形态（本条与 CLI 渲染器的 #76 防线是同一条纪律的两侧）。
#[test]
fn the_only_mode_this_channel_can_speak_is_send_into() {
    let args = crate::inbound_client::launch_args(
        "send-into",
        "cc-x",
        "true",
        None,
        None,
        Default::default(),
    );
    assert_eq!(args["mode"], "send-into");
    // 本模块的生产段里不许出现另一个 mode 字面量。
    let prod = guard_core::production_code(include_str!(
        "../../../../src/bridge/src/backend/control/daemon_launch.rs"
    ));
    assert!(
        !prod.contains(&format!("\"create{}attach\"", "-or-")),
        "生产段出现了 create-or-attach —— 这条路一旦能新建会话，就是 #76 的失管会话形态；\
             那一格的幂等语义与 daemon 的 created/typed 不逐字等价，要切得先对拍（见模块头注）"
    );
    assert_eq!(
        prod.matches("\"send-into\"").count(),
        1,
        "`send-into` 这个字面量在生产段应当只出现一次（就是发出去那处）"
    );
}

/// ★ 没有控制通道 ⇒ **诚实降级**，而不是 panic、也不是假装键入了。
#[test]
fn no_control_channel_degrades_honestly_with_a_reason() {
    let r = tokio::runtime::Runtime::new().unwrap().block_on(async {
        daemon_send_into(SendIntoRequest {
            // 不可能被注册的 origin（注册表是进程内的）。
            origin: "u8a-2c-1-没有这个远端".into(),
            name: "cc-x".into(),
            payload: "true".into(),
        })
        .await
    });
    assert!(!r.typed, "没有通道却报 typed=true —— 调用方会以为已经键入");
    let reason = r.reason.expect("降级必须带理由（前端唯一的回落线索）");
    assert!(
        reason.contains("控制通道"),
        "理由没说清是通道不在：{reason}"
    );
}

/// 坏数据不是缺省：空会话名 / 空载荷 ⇒ 一个字节都不发。
#[test]
fn empty_name_or_payload_is_refused_before_any_io() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    for (name, payload) in [("", "true"), ("  ", "true"), ("cc-x", "")] {
        let r = rt.block_on(daemon_send_into(SendIntoRequest {
            origin: "任意".into(),
            name: name.into(),
            payload: payload.into(),
        }));
        assert!(!r.typed);
        assert!(
            r.reason.as_deref().is_some_and(|s| s.contains("为空")),
            "({name:?},{payload:?}) 的理由不对：{:?}",
            r.reason
        );
    }
}

/// ★ `typed` 三态：true / false / **形状不认识**。
///
/// 第三态是要紧的那个：字段缺了**不能当 false** —— 那会把「协议漂移」报成
/// 「会话不存在」，而这两件事的处置完全不同（一个要改代码，一个让用户重试）。
#[test]
fn typed_is_read_out_of_three_states_not_two() {
    use serde_json::json;
    assert_eq!(typed_from_reply(Some(&json!({"typed": true}))), Ok(true));
    assert_eq!(
        typed_from_reply(Some(&json!({"typed": false, "created": false}))),
        Ok(false)
    );
    for bad in [json!({}), json!({"typed": "yes"}), json!({"typed": 1})] {
        assert!(
            typed_from_reply(Some(&bad)).is_err(),
            "{bad} 应当报「形状不认识」而不是当 false"
        );
    }
    assert!(typed_from_reply(None).is_err());
}

/// ★ 跨轨：本模块发的字段名必须是 **daemon 那条命令声明过**的。
///
/// `inbound_client::launch_args_field_names_match_the_daemon_parser` 钉的是编码器 ↔ 解析器；
/// 这条钉的是**本模块用到的那三个**在 daemon 的 `REGISTRY` 里真有登记 ——
/// daemon 改字段名或把 launch 摘掉，这条红。
#[test]
fn the_fields_this_channel_sends_are_declared_by_the_daemon_registry() {
    const INBOUND: &str = include_str!("../../../../src/backend/inbound.rs");
    let at = INBOUND
        .find("name: \"launch\",")
        .expect("daemon 的 REGISTRY 里找不到 launch —— 抽取坏了，本断言在空转");
    let rest = &INBOUND[at..];
    let fields_at = rest.find("fields: &[").expect("launch 那条没有 fields");
    let body = &rest[fields_at..];
    let body = &body[..body.find(']').expect("fields 没收尾")];
    assert!(body.len() > 30, "抽到的 fields 太短（{body:?}）—— 抽取坏了");
    for f in ["mode", "name", "payload"] {
        assert!(
            body.contains(&format!("\"{f}\"")),
            "daemon 的 launch 没有声明字段 `{f}`，而本模块在发它：{body}"
        );
    }
}

/// ★★ **`may_fall_back: true` 只许出现在一处**〔audit-0805 08-08，Phase G 第 77 件〕。
///
/// 上面那条（`only_the_provably_unsent_cases_may_fall_back`）钉的是**语义**：
/// 每一档翻译得对不对。旁边那条钉的是**接线**：TS 侧真的按三态分流。
/// ★ 两条都不管**第三件事**：**谁能构造出一个可回落的结局**。
///
/// 08-08 实测：把「协议漂移」那档从 `SendIntoResponse::refused(…)` 改成直接写字面量
/// `SendIntoResponse { typed: false, reason: …, may_fall_back: true }`，
/// **monitor 1003 + vitest 1294 一条都不红**。而那正是模块头注逐字警告的那件事：
/// 漂移时我们**不知道它键没键入**，回落等于用一条**没有 §34 门**的整串重做一遍，
/// 后果不可撤销（载荷第二次键进正在跑 claude 的 pane、被当成 prompt 提交、写进历史）。
///
/// # 修法两层，失效模式不同
///
/// 1. **字段私有**（本轮一并做）—— 别的模块由**编译器**挡住；
/// 2. **本条** —— 模块自己这一半编译器管不着（同文件的分支与 `mod tests` 都够得着），
///    于是钉：生产段里 `may_fall_back: true` **恰好一处**，且那一处在 `fn unsent` 体内。
///
/// ⚠ 为什么不钉「所有构造都必须走两个构造器」：`Routed::Done` 那档本来就是直接写字面量
/// （`typed: true`，它既不是 unsent 也不是 refused），硬要求走构造器会逼人再造一个
/// `fn done()` —— 那是**为判据改代码形状**，而真正要守的性质只有一句：
/// **可回落只能从「能证明没发出去」那一档产出**。
#[test]
fn may_fall_back_true_lives_in_exactly_one_place() {
    let src = std::fs::read_to_string(file!())
        .or_else(|_| {
            std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("src/backend/control/daemon_launch.rs"),
            )
        })
        .expect("读不到本模块源码");
    let prod = guard_core::production_code(&src);
    // 运行时拼，免得命中本条自己的说明文字（F58 在这上面栽过两次）。
    let needle = format!("may_fall_back: {}", "true");
    let hits: Vec<&str> = prod
        .lines()
        .map(str::trim)
        .filter(|l| l.contains(&needle))
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "生产段里 `{needle}` 出现 {} 处（应恰好 1 处，在 `fn unsent` 里）：\n{}\n\n\
             ★ 多出来的那处意味着**有一档不经「能证明没发出去」这个判断就允许了回落**。\n\
             那条回落走的整串没有 §34 的门 ⇒ 一次门拒绝、或一次「daemon 可能已经键入过」\n\
             会被原样重做一遍；后者**不可撤销**（载荷第二次进正在跑 claude 的 pane，\n\
             被当成 prompt 提交并写进对话历史）。\n\
             ⇒ 新的一档要允许回落，就让它走 `SendIntoResponse::unsent(...)`，\n\
             并先回答一句「凭什么能证明这条命令根本没发出去」。\n\
             ⚠ 0 处 = `unsent` 那一处被改写了 ⇒ 没有 daemon 的远端全都用不了（C7 过渡期），\n\
             那是另一个方向的坏，同样要红。",
        hits.len(),
        hits.join("\n")
    );

    // 那一处必须**在 `fn unsent` 体内** —— 否则「只有一处」可以被搬到任何地方去满足。
    let at = prod
        .find("fn unsent(")
        .expect("`fn unsent` 不见了 —— 本条的锚点没了");
    let end = prod[at..]
        .find("\n    }")
        .map(|k| at + k)
        .unwrap_or(prod.len());
    assert!(
        prod[at..end].contains(&needle),
        "唯一那处 `{needle}` 不在 `fn unsent` 体内了。\n\
             「恰好一处」这件事本身不够 —— 它可以被搬进任何一档去满足计数，\n\
             而语义要求它只能从「能证明没发出去」那一档产出。"
    );
}

/// ★★ **F14 的核心性质**：只有「能证明这条命令根本没发出去」的档才许回落。
///
/// 反过来错法（把 `Refused` 也映射成 `mayFallBack:true`）会让调用方用那条**无门**的整串
/// 把一次门拒绝、或一次「daemon 可能已经键入过」重做一遍 —— 后者的后果是
/// **载荷第二次被键入进一个正在跑 claude 的 pane**、被当成 prompt 提交、不可撤销。
#[test]
fn only_the_provably_unsent_cases_may_fall_back() {
    use super::super::daemon_route::Routed;
    let done = SendIntoResponse::from_routed(Routed::Done);
    assert!(done.typed && !done.may_fall_back && done.reason.is_none());

    let unsent = SendIntoResponse::from_routed(Routed::NoChannel("没通道".into()));
    assert!(
        !unsent.typed && unsent.may_fall_back,
        "「证明没发出去」那档必须允许回落 —— 否则没有 daemon 的远端全都用不了（C7 过渡期）"
    );
    assert!(unsent.reason.is_some(), "回落时 reason 是调用方唯一的线索");

    for why in ["拒绝：wrong_owner", "等应答超时", "协议漂移"] {
        let r = SendIntoResponse::from_routed(Routed::Refused(why.into()));
        assert!(
            !r.typed && !r.may_fall_back,
            "`Refused({why})` 被判成了可回落 —— 那条整串**没有 §34 的门**，\n\
                 回落等于用一条无门的路把「被门拒绝」或「可能已经键入过」重做一遍。"
        );
    }
    // 坏数据那一档也不许回落：拿空载荷去渲染整串只会产出无意义的命令。
    let bad = SendIntoResponse::refused("会话名或载荷为空");
    assert!(!bad.may_fall_back);
}

/// ★★ **生产接线（跨语言）**：TS 侧真的按三态分流，`refused` 那支**不许**落到整串。
///
/// # 为什么这条判据长在 Rust 侧
///
/// `SendIntoResponse` 的 TS 类型是**手写**的（`src/launch-cli-wire.ts`，不是 ts-rs 生成）
/// ⇒ 字段名两侧靠人同步。而「回落契约」这件事**只有两侧一起看才成立**：
/// Rust 说了 `may_fall_back:false`，TS 不读它就等于没说。
#[test]
fn refused_never_falls_back_to_the_whole_string() {
    let root = crate::guard_support::repo_root();
    let read = |rel: &str| {
        let p = root.join(rel);
        assert!(p.is_file(), "读不到 {rel} —— 读不到的文件只会静默返回空串");
        std::fs::read_to_string(p).unwrap_or_default()
    };

    // ① 跨语言字段名：手写类型里必须有 serde camelCase 后的那个名字。
    let wire = read("src/launch-cli-wire.ts");
    assert!(
        wire.contains("mayFallBack"),
        "`src/launch-cli-wire.ts` 的手写 `SendIntoResponse` 里没有 `mayFallBack` ——\n\
             Rust 侧发了这个字段而 TS 侧读不到 ⇒ 回落契约单方面失效（`undefined` 是 falsy，\n\
             恰好会被读成「不许回落」，所以它是 fail-closed 的 —— 但那是巧合，不是设计）。"
    );

    // ② 三态分流真的在生产路径上。
    let run = read("src/remote-launch-run.ts");
    for needle in ["\"refused\"", "\"typed\"", "mayFallBack"] {
        assert!(
            run.contains(needle),
            "`remote-launch-run.ts` 里找不到 {needle} —— 三态分流没接上"
        );
    }
    // ③ ★ `refused` 那支必须**在**回落之前就地返回。
    let at = run
        .find("sent.verdict === \"refused\"")
        .expect("上面已断言过存在");
    let arm = &run[at..(at + 700).min(run.len())];
    assert!(
        arm.contains("return false"),
        "`refused` 那支没有就地 `return` —— 它会穿到下面的整串回落。\n\
             那条整串（`session-backend.ts` 的 `send-keys …; attach …`）**没有 §34 的门**。\n\
             实得这一段：{arm:?}"
    );
    // ④ 而且必须让用户看见（回落可以静默，**拒绝不行** —— 否则他以为成功了）。
    assert!(
        arm.contains("showActionFailureToast"),
        "`refused` 那支没有 toast —— 用户会以为就地 resume 成功了。\n\
             ⚠ 这与「回落绝不 toast」那条纪律**不矛盾**：回落用户看不出区别，拒绝是真没做成。"
    );
}
