use super::*;
use serde_json::json;

#[test]
fn shape_validation_rejects_what_would_break_the_tmux_target() {
    for (bad, why) in [
        (json!({}), "缺 name"),
        (json!({ "name": "" }), "空"),
        (json!({ "name": "  " }), "只有空白"),
        (json!({ "name": "a:b" }), "含 `:`（tmux 目标语法）"),
        (json!({ "name": "=a" }), "含 `=`（tmux 目标语法）"),
        (json!({ "name": "a\nb" }), "含控制字符"),
    ] {
        assert!(parse_name(&bad).is_err(), "{why} 应当被拒：{bad:?}");
    }
    assert_eq!(
        parse_name(&json!({ "name": "proj-cc" })).unwrap(),
        "proj-cc"
    );
}

/// ★ **生产接线（顺序钉）**：门必须在 `kill-session` **之前**。
///
/// 反过来（先杀再判）＝ 门形同虚设，而「函数被调用了」这种判据照样绿。
/// 同 `launch_tests.rs::the_send_into_arm_admits_before_it_types` 一族 ——
/// **破坏性动作的顺序错法后果最重**，所以单独钉。
#[test]
fn the_kill_path_admits_before_it_kills() {
    let src = crate::guard_support::production_code(include_str!("../../../src/backend/control/kill.rs"));
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

/// ★ Gate 3 只给破坏性动作：本模块用 `admit_destructive`，**不是** `admit`。
#[test]
fn kill_uses_the_destructive_gate_not_the_plain_one() {
    let src = crate::guard_support::production_code(include_str!("../../../src/backend/control/kill.rs"));
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
