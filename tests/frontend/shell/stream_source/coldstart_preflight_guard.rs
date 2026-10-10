//! ★〔audit-0805 F05 下半，报告「可选 12」〕**冷启动的三条 SSH 连接合并**。
//!
//! 正题见 [`super::VERIFIED_BUILD`] 的头注。本模块钉三件事：
//! 判定本身（纯函数真值表）· 记忆**只在 hello 那一处写**· 失败路径**真的抹掉**。
//!
//! ⚠ **钉不了「真的少连了两次」** —— 那要真 SSH（红线禁）。
//! 本模块钉的是**结构**：那两条连接被一个门控包着、门控的输入只来自 hello 自证。
//! **别把它读成性能判据**（同 `coldstart_perf_guard` 的边界）。

use super::{
    forget_verified_build, preflight_can_be_skipped, record_verified_build, verified_build_of,
};

fn prod() -> String {
    crate::guard_support::stream_source_files()
        .iter()
        .map(|(_, src)| guard_core::production_source(src))
        .collect::<Vec<_>>()
        .join("\n")
}

/// ★ 纯函数真值表：**只有逐字相等才许跳**；手上没带后端字节（「我这一版」是 `None`）⇒ 恒不跳。
#[test]
fn only_an_exact_build_match_may_skip_the_preflight() {
    let e = Some("abc123");
    assert!(
        preflight_can_be_skipped(Some("abc123"), e),
        "自证过就是期望 build ⇒ 该跳"
    );
    assert!(
        !preflight_can_be_skipped(None, e),
        "没记过 ⇒ 必须跑（可能要部署）"
    );
    assert!(
        !preflight_can_be_skipped(Some("def456"), e),
        "记的是**别的** build ⇒ 必须跑 —— 那台机器上装的不是当前版本，正是要部署的情形"
    );
    // ★ F24 那一族：前缀相等不算相等。
    assert!(
        !preflight_can_be_skipped(Some("abc123-dirty"), e),
        "`abc123-dirty` 被当成了 `abc123` —— 判据写成了前缀/包含匹配。\
             那会让一个**改过的** backend 冒充期望 build 并跳过部署"
    );
    assert!(
        !preflight_can_be_skipped(Some("abc"), e),
        "反方向的前缀也不许 —— `abc` 不是 `abc123`"
    );
    for verified in [None, Some("abc123"), Some("")] {
        assert!(
            !preflight_can_be_skipped(verified, None),
            "手上没带后端字节却跳了预检（记的是 {verified:?}）—— 没有对照物，「那台就是这一版」无从说起"
        );
    }
}

/// 记忆的存 / 取 / 抹。
#[test]
fn the_memo_round_trips_and_can_be_forgotten() {
    let host = "f05-guard-host";
    forget_verified_build(host);
    assert_eq!(verified_build_of(host), None, "起点该是空的");
    record_verified_build(host, "build-X");
    assert_eq!(verified_build_of(host).as_deref(), Some("build-X"));
    forget_verified_build(host);
    assert_eq!(
        verified_build_of(host),
        None,
        "抹不掉的话，一台后端被删的机器会**每一轮都跳预检、每一轮都失败**"
    );
}

/// ★ **写入点恰好一处**，而且在收到 hello 之后。
///
/// 记忆的全部安全性都压在「它只记 backend **自报**的身份」上。
/// 多一处写入 = 把自证换回了猜。
#[test]
fn the_memo_is_written_in_exactly_one_place() {
    let prod = prod();
    guard_core::find_pinned(&prod, "record_verified_build(&host_label, &build_id);")
        .unwrap_or_else(|e| {
            panic!(
                "自证记忆的写入点不是恰好一处：{e}\n\
                     ★ 这份记忆的全部安全性压在「只记 backend **自报**的身份」上 —— \n\
                     多一处写入就把自证换回了猜（比如拿预检结论去写）。"
            )
        });
    // 反向：它得在 hello 分支里（`build_id` 这个变量只有那里有）。
    assert!(
        guard_core::contains_word(&prod, "build_id"),
        "`build_id` 不在生产段里了 —— 上面那条锚点还在，说明它锚的不是 hello 那一处"
    );
}

/// ★ 失败路径必须抹记忆，而且**不止一处**（起流失败 + hello 身份不符）。
#[test]
fn every_failure_path_forgets_the_memo() {
    let prod = prod();
    let n = prod.matches("forget_verified_build(&host_label)").count();
    assert!(
        n >= 2,
        "生产段只有 {n} 处 `forget_verified_build` —— 该有两处：\n\
             ① 跳过预检后**起流失败**（backend 被删/被换旧）；\n\
             ② 收到 hello 但 **build_id 不是期望值**（装的不是当前 build）。\n\
             少一处，那台机器就会一直跳预检、一直失败，永远等不到重新部署。"
    );
}
