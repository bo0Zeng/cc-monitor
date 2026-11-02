//! 监听口拒绝理由那一条：喂给 `refusal_line` 的实参只许来自闭集 —— 两处调用点里小中继那一处住控制面（`control/resident.rs`），
//! 本条要问它的 `relay_refusal` ⇒ 挂在 `control` 底下（帧面零件是独立的包、够不到上层）。其余监听判定的判据住 `listen_tests.rs`。

use crate::wire::listen::{refusal_line, REFUSE_ABSENT, REFUSE_MALFORMED, REFUSE_UNREACHABLE};

/// 拒绝理由是闭集，且拼出来的每一行都是**合法 JSON**。
///
/// ⚠ 这条防的是「顺手把一段外来字节拼进那行」：只要 `reason` 还是这三个常量，
/// 那行就不可能被撕开；哪天有人改成 `refusal_line(&err.to_string())`，本条当场红。
#[test]
fn refusal_reasons_are_a_closed_set() {
    for r in [REFUSE_MALFORMED, REFUSE_ABSENT, REFUSE_UNREACHABLE] {
        let line = refusal_line(r);
        assert!(line.ends_with('\n'), "NDJSON 每行必须以换行收尾：{line:?}");
        let v: serde_json::Value =
            serde_json::from_str(line.trim()).expect("拒绝行必须是合法 JSON");
        assert_eq!(v["attach"], "refused");
        assert_eq!(v["reason"], r);
    }
    // ★★ 反向锚点：**喂给 `refusal_line` 的实参只许来自那个闭集。**
    //
    // ⚠⚠ 〔08-26 收工前自查逮到的〕本条第一版扫的是 **`listen.rs` 自己**，
    //    而 `refusal_line` 的**唯一调用点在进程入口 `stream/entry.rs`** —— 本模块生产段里
    //    带 `refusal_line(` 的行只有那条 `pub fn` 声明（而它被 filter 掉了）
    //    ⇒ 那个循环**跑零圈**，是个**空转**的判据。
    //    它读起来完全正常，而它一个字节都没在守。⇒ 人群搬到真正的调用点。
    //    ★ 这是「守卫范围 ≠ 性质范围」那一族的第四形：**人群画在了错的文件上**。
    // 小中继（`control/resident.rs`）那一处由下面另一段钉。
    let caller = guard_core::production_code(include_str!("../../../src/backend/stream/entry.rs"));
    let calls: Vec<&str> = caller
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("refusal_line("))
        .collect();
    // 反空真：一个调用点都扫不到 ⇒ 下面整段空转（第一版就是死在这一格）。
    assert_eq!(
        calls.len(),
        1,
        "`stream/entry.rs` 生产段里 `refusal_line(` 有 {} 处（应恰好 1 处）：{calls:?}\n\
             0 处 = 抽取坏了或调用点搬家了，本条在空转；≥2 处 = 拒绝理由有第二个产出点。",
        calls.len()
    );
    assert!(
        calls[0].contains("refusal_line(reason)"),
        "`refusal_line` 的实参不是那个从 `Admit::Refuse` 里解出来的 `reason`：{}\n\
             ⇒ 有人往那行 JSON 里拼了一段**外来字节**（比如 `&e.to_string()`）—— 那会撕行。",
        calls[0]
    );
    assert!(
        caller.contains("listen::Admit::Refuse(reason)"),
        "`stream/entry.rs` 里那个 `reason` 不再是从 `Admit::Refuse` 解出来的 —— \
             那它是从哪来的？闭集这件事就断在这里。"
    );
    // 小中继那一处：实参是 `relay_refusal` 的答（只回两个常量）。
    let relay =
        guard_core::production_code(include_str!("../../../src/backend/control/resident.rs"));
    let relay_calls: Vec<&str> = relay
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("refusal_line("))
        .collect();
    assert_eq!(relay_calls.len(), 1, "{relay_calls:?}");
    assert!(relay_calls[0].contains("refusal_line(reason)"));
    assert!(relay.contains("let reason = relay_refusal(e.kind());"));
    assert_eq!(
        crate::control::resident::relay_refusal(std::io::ErrorKind::NotFound),
        REFUSE_ABSENT
    );
    assert_eq!(
        crate::control::resident::relay_refusal(std::io::ErrorKind::ConnectionRefused),
        REFUSE_ABSENT
    );
    assert_eq!(
        crate::control::resident::relay_refusal(std::io::ErrorKind::PermissionDenied),
        REFUSE_UNREACHABLE
    );
}
