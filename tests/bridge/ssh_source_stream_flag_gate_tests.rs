use super::decide_stream_flags;

fn caps(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// F66 DoD：能力门控矩阵——空集（旧 daemon/未确认）恒 (false,false)；
/// 声明 bg+tail-only 后 tail_only 恒开、with_bg 随 showBgSessions；
/// 部分声明只开对应位；未知 token 忽略。
/// ★ U-CC1：`KNOWN_CAPABILITY_TOKENS` 与 `decide_stream_flags` 必须是同一份事实。
///
/// 漂开的后果是**漂移记账说谎**：daemon 声明了一个我们其实认识的 token，诊断面却把它
/// 报成「不认识」；或者反过来，真的新 token 被当成已知、一声不吭。
#[test]
fn known_capability_tokens_match_decide_stream_flags() {
    for t in super::KNOWN_CAPABILITY_TOKENS {
        let one = vec![t.to_string()];
        assert_ne!(
            super::decide_stream_flags(&one, true),
            (false, false),
            "`{t}` 在名单里，但 decide_stream_flags 根本不看它 —— 两份事实漂了"
        );
    }
    for junk in ["nope", "future-thing", ""] {
        assert_eq!(
            super::decide_stream_flags(&[junk.to_string()], true),
            (false, false),
            "`{junk}` 不在名单里却影响了 flag —— 名单漏登记了一个真能力"
        );
    }
    assert!(
        super::KNOWN_CAPABILITY_TOKENS.len() >= 2,
        "名单只剩 {} 个 —— 维护坏了，本断言在空转",
        super::KNOWN_CAPABILITY_TOKENS.len()
    );
}

#[test]
fn capability_gate_matrix() {
    // 空集 = 旧 daemon / 尚未收到 hello → 全降级
    assert_eq!(decide_stream_flags(&caps(&[]), true), (false, false));
    assert_eq!(decide_stream_flags(&caps(&[]), false), (false, false));
    // 全能力声明
    assert_eq!(
        decide_stream_flags(&caps(&["bg", "tail-only"]), true),
        (true, true)
    );
    assert_eq!(
        decide_stream_flags(&caps(&["bg", "tail-only"]), false),
        (false, true),
        "关 showBgSessions 只关 with_bg，tail-only 照开"
    );
    // 部分声明：只有 tail-only → with_bg 恒 false（即便 show_bg）
    assert_eq!(
        decide_stream_flags(&caps(&["tail-only"]), true),
        (false, true),
        "daemon 没声明 bg → 即便用户想看也不发 --with-bg"
    );
    // 部分声明：只有 bg
    assert_eq!(
        decide_stream_flags(&caps(&["bg"]), true),
        (true, false),
        "daemon 没声明 tail-only → 不发 --tail-only（历史走全量推流）"
    );
    // 未知 token 忽略（加法式向前兼容：未来 daemon 声明我们还不认识的能力）
    assert_eq!(
        decide_stream_flags(&caps(&["bg", "tail-only", "future-x"]), true),
        (true, true),
        "未知能力 token 不影响已知门控"
    );
}

use super::should_upgrade_reconnect as up;

/// F66 ★ 防无限重连：`should_upgrade_reconnect` 只在「下一轮严格增开一个本轮关着的
/// flag」时才 true——保证收敛。这条测试是收 hello 自愈升级那段的回归护栏
/// （审计阻塞：那段防死循环逻辑此前零测试；抽成纯函数后在此穷举）。
#[test]
fn upgrade_reconnect_converges() {
    // 升级值得：本轮全关，下一轮能开
    assert!(up((false, false), (true, true)), "全关→全开 该升级");
    assert!(up((false, false), (false, true)), "开 tail 该升级");
    assert!(up((false, false), (true, false)), "开 bg 该升级");
    // ★ 关键收敛点：记账后本轮 caps=声明集 → next==cur → 恒不再重连
    assert!(!up((true, true), (true, true)), "记账后 next==cur 不再重连");
    assert!(
        !up((true, false), (true, false)),
        "只 bg：记账后不抖（!cur_bg 挡住）"
    );
    assert!(!up((false, true), (false, true)), "只 tail：记账后不抖");
    // 本轮已开 bg、下一轮又加 tail → tail 项触发升级（严格增开）
    assert!(
        up((true, false), (true, true)),
        "本轮 bg、下一轮加 tail → 升级"
    );
    // 下一轮反而关了某 flag（不该发生，但函数须安全）→ 不重连
    assert!(!up((true, true), (false, false)), "下一轮更弱 → 不重连");
    // swap 边角（本轮 bg、下一轮只 tail）：tail 项触发（靠 tail latch 后续收敛）
    assert!(up((true, false), (false, true)), "swap：新开 tail 该升级");
}

/// F66：确认 `build.rs::emit_daemon_capabilities` 那条单源管道真的通（非空、含当前
/// token）——否则乐观路径静默退化成「第一轮降级 + hello 自愈」（仍正确，只慢一轮）。
/// 用 `contains` 而非精确相等：daemon 将来加 token 时本测试仍过，不误红。
///
/// 〔`K-R19` 订正 09-03〕这一句原先点的是 `EMBEDDED_DAEMON_CAPABILITIES`，**全仓零定义**
/// ——管道上三个真名依次是：`build.rs::emit_daemon_capabilities` → 编译期 env
/// `DAEMON_CAPABILITIES` → `ssh_source.rs::embedded_daemon_capabilities`。
#[test]
fn embedded_capabilities_single_source_wired() {
    let caps = super::embedded_daemon_capabilities();
    assert!(caps.contains(&"bg".to_string()), "单源应含 bg：{caps:?}");
    assert!(
        caps.contains(&"tail-only".to_string()),
        "单源应含 tail-only：{caps:?}"
    );
}

/// U-1（2026-08-01）：**`build_id` 那半单源管道一直没有等价断言。**
///
/// `build.rs::emit_daemon_build_id` 抠不到就 `unwrap_or_else(|| "unknown")` —— **静默退化**。
/// 一旦 daemon crate 改名 / `BUILD_ID` 挪出 `main.rs` / `const` 写法换行，
/// `EXPECTED_DAEMON_BUILD_ID` 会变成 `"unknown"`，而**编译通过、测试全绿**，
/// 运行期把每台远端 daemon 都判成 `StaleBuild` → 无限重装。
///
/// capabilities 那半有 `embedded_capabilities_single_source_wired` 兜着，这半没有。
/// U13 的仓库级重命名**必须**先有这条，否则那次重命名是静默失败。
///
/// 判据刻意宽松（不写死具体 id）：只要不是兜底值、且长得像一个 build id 就行 ——
/// 写死 id 会让每次正常 bump 都误红，那种守卫最后会被人删掉。
#[test]
fn embedded_build_id_single_source_wired() {
    let id = super::EXPECTED_DAEMON_BUILD_ID;
    assert_ne!(
        id, "unknown",
        "`build.rs::emit_daemon_build_id` 没抠到 daemon 的 `const BUILD_ID` —— \
             多半是路径失效（crate 改名 / 文件搬家）或 `const` 写法变了。\
             它是**静默退化**：不修的话每台远端都会被判 StaleBuild 并无限重装。"
    );
    assert!(
        !id.is_empty() && id.len() <= 64,
        "build_id 形状可疑：{id:?}"
    );
    assert!(
        id.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'),
        "build_id 含意外字符（多半是抠错了行）：{id:?}"
    );
}
