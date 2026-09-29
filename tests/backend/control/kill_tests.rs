use super::*;
use serde_json::json;

#[test]
fn shape_validation_rejects_what_would_break_the_tmux_target() {
    for (bad, why) in [
        (json!({}), "缺 name"),
        (json!({ "name": "" }), "空"),
        (json!({ "name": "a:b" }), "含 `:`（tmux 目标语法）"),
        (json!({ "name": "a\nb" }), "含控制字符"),
        (
            json!({ "name": "a\u{202e}b" }),
            "含视觉欺骗字符（`gate_rules` 那一格）",
        ),
    ] {
        assert!(parse_name(&bad).is_err(), "{why} 应当被拒：{bad:?}");
    }
    // 〔TAIL · DUP3 §5 ③〕`=` 放行：`=a=b:` 精确命中名叫 `a=b` 的会话（attach 那一条同样放行）。
    for good in ["proj-cc", "a=b", "=a"] {
        assert_eq!(parse_name(&json!({ "name": good })).unwrap(), good);
    }
}

/// ★ **生产接线（顺序钉）**：门必须在 `kill-session` **之前**。
///
/// 反过来（先杀再判）＝ 门形同虚设，而「函数被调用了」这种判据照样绿。
/// 同 `launch_tests.rs::the_send_into_arm_admits_before_it_types` 一族 ——
/// **破坏性动作的顺序错法后果最重**，所以单独钉。
#[test]
fn the_kill_path_admits_before_it_kills() {
    let src =
        crate::guard_support::production_code(include_str!("../../../src/backend/control/kill.rs"));
    let admit = src
        .find("gate::admit_destructive")
        .expect("生产段里没有 `gate::admit_destructive` —— 这条 kill 没过门");
    // ⚠ **锚在调用形态上，不是那个词**〔08-08〕：生产段里 `kill-session` 有**两处**
    //（真调用 + 一句错误消息「kill-session 失败…」），`find` 取首处 —— 今天命中对的
    // 那一处**是排序运气**。换成 `.args([` 那个形状（唯一），并当场核一次唯一性。
    let verb = format!(".args([\"kill-{}\"", "session");
    let n = src.matches(verb.as_str()).count();
    assert_eq!(
        n, 1,
        "生产段里 `{verb}` 出现 {n} 次（应恰好 1 次）—— \
             0 次 = 调用写法变了（本条会零命中地绿）；≥2 次 = 有第二条 kill 路，\
             那就得逐条问「它过门了吗」，而不是只比第一处的位置"
    );
    let act = src.find(verb.as_str()).expect("上面已断言恰好一处");
    assert!(
        admit < act,
        "`kill-session` 排在过门之前 —— 先杀再判，门就没意义了"
    );
    // 杀的必须是 `admit_destructive` 回的**句柄**，不是名字。
    assert!(
        src.contains("\"-t\", &handle"),
        "kill 的目标不是过门时拿到的句柄 —— 对名字下手就把 TOCTOU 窗口放回来了，\
             而这是**破坏性**动作"
    );
}

/// 〔SH1 · D-g〕顺序钉：读 pane pid 在**过门之后、杀之前**（杀了就读不到了）；顺手注销在**杀成之后**（没杀成不注销）。
#[test]
fn the_kill_path_reads_panes_before_it_kills_and_unregisters_only_after() {
    let src =
        crate::guard_support::production_code(include_str!("../../../src/backend/control/kill.rs"));
    let at = |needle: &str| {
        assert_eq!(
            src.matches(needle).count(),
            1,
            "`{needle}` 在生产段里不是恰好一处"
        );
        src.find(needle).unwrap()
    };
    let admit = at("gate::admit_destructive");
    let panes = at("let panes = pane_pids(&handle);");
    let kill = at(".args([\"kill-session\"");
    let ok = at("if out.status.success() {");
    let unregister = at("let bus = super::cc_bus::unregister_panes(name, &panes);");
    assert!(
        admit < panes && panes < kill,
        "pane pid 要在过门之后、kill-session 之前读"
    );
    assert!(ok < unregister, "顺手注销只能在杀成那一支里");
    let after_ok = &src[ok..];
    assert!(
        after_ok.find("unregister_panes").unwrap() < after_ok.find("return Ok(bus)").unwrap(),
        "顺手注销要在杀成那一支返回之前"
    );
}

/// ★ Gate 3 只给破坏性动作：本模块用 `admit_destructive`，**不是** `admit`。
#[test]
fn kill_uses_the_destructive_gate_not_the_plain_one() {
    let src =
        crate::guard_support::production_code(include_str!("../../../src/backend/control/kill.rs"));
    assert!(
        src.contains("admit_destructive"),
        "kill 必须走带 Gate 3 的那个门"
    );
    // 运行时拼，免得命中上一行自己。
    let plain = format!("gate::admit({}", "");
    assert!(
        !src.contains(plain.as_str()),
        "kill 走了非破坏性的 `admit`（没有 Gate 3）—— 多窗口会话会被误杀"
    );
}

/// ★★〔C4e · 第四波 4C〕**跨语言金样**：界面直接收的 `kill` 成品，两侧读同一份 `tests/__fixtures__/tmux-control.golden.json`。
///
/// 守的要求：`设计/05 §14.3` 逐字「**成品的两侧对拍**：界面按形状严格收……线上形状由一份跨语言金样钉住
/// （后端测试产出 == 金样 · TS 解码器读同一份）」。杀会话从这一拍起由界面经通道直接说（`src/frontend/ui/tmux-control.ts::killSession`），
/// monitor 那一跳只搬字节 —— 成品的键、拒绝码的集合从此只有后端这一侧与金样说了算。
///
/// 三格各自异源：请求样例过**生产**解析器 [`parse_name`] · 成品 == 生产构造器 [`reply`] ·
/// 码集合 == 后端登记表 `inbound::REGISTRY` 那一块（手写在 `inbound.rs`，不从本文件派生）。
#[test]
fn the_kill_product_matches_the_cross_language_golden() {
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/tmux-control.golden.json"))
            .expect("金样读不出来");
    let k = &g["kill"];
    let name = parse_name(&k["request"]).expect("金样的请求样例过不了生产解析器");
    assert_eq!(
        reply(&name, &crate::control::cc_bus::BusCleanup::default()),
        k["reply"],
        "后端出的 kill 成品与金样不相等 —— 改了键名或多 / 少一格，界面那一侧就会读成「不知道结束了没有」"
    );
    let spec = crate::stream::inbound::REGISTRY
        .iter()
        .find(|s| s.name == "kill")
        .expect("后端登记表里没有 `kill`");
    let mut want: Vec<&str> = spec.codes.to_vec();
    want.sort_unstable();
    let mut got: Vec<&str> = k["codes"]
        .as_array()
        .expect("金样缺 `codes`")
        .iter()
        .map(|v| v.as_str().expect("码不是字符串"))
        .collect();
    got.sort_unstable();
    assert_eq!(
        got, want,
        "金样里的拒绝码与后端登记的不相等 —— 界面那张「码 → 一句话」的表就会漏一档或多一档"
    );
}

/// 设计/95 §6「要说得给 `kill` 的成品加一格」：注销结局三样原样进 `bus` 那一格（期望值手写）。
#[test]
fn the_kill_reply_carries_what_the_bus_cleanup_did() {
    let c = crate::control::cc_bus::BusCleanup {
        removed: vec!["p_cc".into()],
        failed: vec![("r_cc".into(), "它不在".into())],
        unread: None,
    };
    assert_eq!(
        reply("demo-cc", &c),
        serde_json::json!({"session": "demo-cc", "killed": true,
            "bus": {"removed": ["p_cc"], "failed": [{"id": "r_cc", "why": "它不在"}], "unread": null}})
    );
}
