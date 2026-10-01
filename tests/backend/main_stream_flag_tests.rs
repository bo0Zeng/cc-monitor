use super::split_stream_flags;

fn v(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// F66（#58③）★ §26 死循环护栏的**代码强制**：`CAPABILITIES` 里每个能力 token 的
/// CLI flag 都必须被 `split_stream_flags` 剥离——否则声明它 = 埋 monitor 侧死循环
/// （monitor 发该 flag → 本后端不剥 → 当一次性查询退出 → 无 hello → 重连死循环）。
/// 加新能力 token 时，若忘了在此登记它的 flag、或忘了给 `split_stream_flags` 加剥离
/// 分支，本测试红。把审计指出的「约定强制」拉回「代码强制」。
#[test]
fn every_capability_token_is_strippable() {
    // token → 它对应的 CLI flag（加新能力时同步扩这张表）
    fn flag_of(token: &str) -> &'static str {
        match token {
            "bg" => "--with-bg",
            "tail-only" => "--tail-only",
            // 启动期令牌那一条。
            // ⚠ 它的 flag **不是为了喂饱本条判据编出来的** ——
            // 默认关、由客户端显式索要，因为令牌是敏感数据（`§8.6 ③`）；
            // 整段论证住 `lib.rs::CAPABILITIES` 的头注。
            "rbind-token" => "--with-rbind-token",
            other => panic!(
                "CAPABILITIES 声明了 token `{other}` 但此处无 flag 映射——加新能力必须在此登记它的 flag 并确认 split_stream_flags 剥离它（否则埋 §26 死循环）"
            ),
        }
    }
    for &token in super::CAPABILITIES {
        let flag = flag_of(token);
        let (rest, _, _, _) = split_stream_flags(v(&[flag]));
        assert!(
            rest.is_empty(),
            "能力 token `{token}` 的 flag `{flag}` 未被 split_stream_flags 剥离 → §26 死循环"
        );
    }
}

/// F25 DoD ③：流模式 flag 剥离后不残留（不会误入查询模式判定）。
#[test]
fn flags_are_stripped_and_detected() {
    let (rest, bg, tail, rbind) = split_stream_flags(v(&["--with-bg", "--tail-only"]));
    assert!(
        rest.is_empty(),
        "剥净 → 流模式（!args.is_empty() 为 false）"
    );
    assert!(bg);
    assert!(tail);
    assert!(
        !rbind,
        "没发 `--with-rbind-token` 就不该置位 —— 令牌默认不上 wire"
    );
    let (rest, bg, tail, rbind) = split_stream_flags(v(&["--tail-only"]));
    assert!(rest.is_empty());
    assert!(!bg);
    assert!(tail);
    assert!(!rbind);
    let (rest, bg, tail, rbind) = split_stream_flags(v(&[]));
    assert!(rest.is_empty());
    assert!(!bg);
    assert!(!tail);
    assert!(!rbind);
    // ★ 第三条 flag 自己那一格：**剥得干净 ＋ 只置自己那一位**。
    // 两半都要断：只断“置位了”会漏掉 §26 那条（不剥 ⇒ 当查询退出 ⇒ 无 hello），
    // 只断“剥干净了”会漏掉“剥掉了但忘了抬位”（那会让客户端永远收不到令牌、而没任何信号）。
    let (rest, bg, tail, rbind) = split_stream_flags(v(&["--with-rbind-token"]));
    assert!(
        rest.is_empty(),
        "`--with-rbind-token` 没被剥干净 → §26 死循环"
    );
    assert!(
        rbind,
        "剥掉了却没置位 ⇒ 客户端永远收不到 `rbind_token`，而且没有任何信号"
    );
    assert!(!bg, "三条 flag 互不干扰");
    assert!(!tail, "三条 flag 互不干扰");
}

/// 查询参数与流 flag 互不干扰：查询参数原样保留（顺带守住"flag 混进查询
/// 命令行也不会破坏查询"的边角）。
#[test]
fn query_args_pass_through() {
    let (rest, bg, tail, rbind) =
        split_stream_flags(v(&["--read-session", "/p/s.jsonl", "--with-bg"]));
    assert_eq!(rest, v(&["--read-session", "/p/s.jsonl"]));
    assert!(bg);
    assert!(!tail);
    assert!(!rbind);
}

/// ★★ `KR86D1` 的**接线那一半**：`--capture-pane` 真的**够得到**那条原语。
///
/// # 失效方向（件文件逐字点名的那个）：**别判「源码里有 `capture-pane` 字面量」**
///
/// `control/ccm/plan.rs` 今天就有那个字面量（信任框轮询那条串），
/// 按字面量判会**恒绿**。本条断的是两处**承重点**，两处都不是「某个字面量出现过」：
///
/// 1. **闸门**：`is_query_mode` 认它。不认 ⇒ 被当未知 flag ⇒ 打一行 warn 之后
///    **照常进流模式**，CLI 面看上去「存在」却永远调不到（`p2b` 08-13 实测过这个形状）。
/// 2. **分派臂**：生产段里那一行**整行**就是「把它交给原语本体」。
///    整行相等（`pin_line`）比 `contains` 强一格：撑大成别的表达式时那一行就不见了。
///
/// ⚠ 「拿回来的真是屏幕内容」不在本条射程内 —— 那一格由
/// `control::capture_pane::tests::capturing_a_real_pane_brings_the_screen_back`
/// 在**真 tmux**（隔离 socket）上断。两条合起来才是 `KR86D1`。
#[test]
fn the_capture_pane_subcommand_is_actually_reachable() {
    let flag = "--capture-pane".to_string();
    assert!(
        super::is_query_mode(std::slice::from_ref(&flag)),
        "`--capture-pane` 没进 `is_query_mode` 的闸门 —— 它会静默变成「起了个流」"
    );
    let prod = crate::guard_support::production_code(include_str!("../../src/backend/main.rs"));
    guard_core::pin_line(
        &prod,
        "Some(\"--capture-pane\") => control::capture_pane::run(&args),",
    )
    .unwrap_or_else(|e| {
        panic!(
            "一次性查询的分派里没有那条把 `--capture-pane` 交给原语本体的臂：{e}\n\
                 ⇒ 闸门放它进查询模式，而下面没人接 ⇒ 它落进 `_` 臂走历史查询、\n\
                 报 `unknown argument` + exit 2（v3.4.0 `--account-trust-zero` 那次事故的形状）。"
        )
    });
}
