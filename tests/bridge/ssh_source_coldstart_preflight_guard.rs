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
    EXPECTED_DAEMON_BUILD_ID,
};

fn prod() -> String {
    let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ssh_source.rs");
    guard_core::production_source(&std::fs::read_to_string(f).expect("读不到本文件"))
}

/// ★ 纯函数真值表：**只有逐字相等才许跳**。
#[test]
fn only_an_exact_build_match_may_skip_the_preflight() {
    let e = "abc123";
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
             那会让一个**改过的** daemon 冒充期望 build 并跳过部署"
    );
    assert!(
        !preflight_can_be_skipped(Some("abc"), e),
        "反方向的前缀也不许 —— `abc` 不是 `abc123`"
    );
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
        "抹不掉的话，一台 daemon 被删的机器会**每一轮都跳预检、每一轮都失败**"
    );
}

/// ★ **写入点恰好一处**，而且在收到 hello 之后。
///
/// 记忆的全部安全性都压在「它只记 daemon **自报**的身份」上。
/// 多一处写入 = 把自证换回了猜。
#[test]
fn the_memo_is_written_in_exactly_one_place() {
    let prod = prod();
    guard_core::find_pinned(&prod, "record_verified_build(&host_label, &build_id);")
        .unwrap_or_else(|e| {
            panic!(
                "自证记忆的写入点不是恰好一处：{e}\n\
                     ★ 这份记忆的全部安全性压在「只记 daemon **自报**的身份」上 —— \n\
                     多一处写入就把自证换回了猜（比如拿预检结论去写）。"
            )
        });
    // 反向：它得在 hello 分支里（`build_id` 这个变量只有那里有）。
    assert!(
        guard_core::contains_word(&prod, "build_id"),
        "`build_id` 不在生产段里了 —— 上面那条锚点还在，说明它锚的不是 hello 那一处"
    );
}

/// ★ 跳过预检时，`confirmed_build` **必须**给期望值。
///
/// 给 `None` 的话 caps 阶梯会掉进「③ 空集全降级」—— 省两条连接换来
/// **一轮降级 + 一轮升级重连**，比不跳还糟。
#[test]
fn skipping_the_preflight_still_feeds_the_capability_ladder() {
    let prod = prod();
    // ⚠ 用 `pin_line`（整行相等）而不是 `find_pinned`（子串 + 标识符边界）〔08-06，§5 3u〕：
    //   这个 needle 以 `)` 收尾 —— **不是标识符字符**，于是 `find_pinned` 的边界检查对它
    //   根本不起作用。实测对照（同一处撑大 `.map(|s| s)`）：`find_pinned` **通过**，
    //   `pin_line` **红**并报「没有任何一行 trim 之后等于…」。
    //   生产段那一行整行就是这个串，所以整行相等是**能用且更强**的写法。
    guard_core::pin_line(&prod, "Some(EXPECTED_DAEMON_BUILD_ID.to_string())").unwrap_or_else(
        |e| {
            panic!(
                "跳过预检那一支没有把 `confirmed_build` 置成期望值：{e}\n\
                     ★ 置 `None` 会让 caps 掉进「空集全降级」⇒ 省下两条连接、换来一轮降级\n\
                     加一轮升级重连（`should_upgrade_reconnect`）—— **比不跳还糟**。"
            )
        },
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
             ① 跳过预检后**起流失败**（daemon 被删/被换旧）；\n\
             ② 收到 hello 但 **build_id 不是期望值**（装的不是当前 build）。\n\
             少一处，那台机器就会一直跳预检、一直失败，永远等不到重新部署。"
    );
}

/// 期望 build_id 不是空串（否则真值表全塌）。
#[test]
fn the_expected_build_id_is_not_empty() {
    assert!(
        !EXPECTED_DAEMON_BUILD_ID.is_empty(),
        "`EXPECTED_DAEMON_BUILD_ID` 是空串 —— 上面那些判据会退化"
    );
}
