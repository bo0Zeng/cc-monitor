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
            other => panic!(
                "CAPABILITIES 声明了 token `{other}` 但此处无 flag 映射——加新能力必须在此登记它的 flag 并确认 split_stream_flags 剥离它（否则埋 §26 死循环）"
            ),
        }
    }
    for &token in super::CAPABILITIES {
        let flag = flag_of(token);
        let (rest, _, _) = split_stream_flags(v(&[flag]));
        assert!(
            rest.is_empty(),
            "能力 token `{token}` 的 flag `{flag}` 未被 split_stream_flags 剥离 → §26 死循环"
        );
    }
}

/// F25 DoD ③：流模式 flag 剥离后不残留（不会误入查询模式判定）。
#[test]
fn flags_are_stripped_and_detected() {
    let (rest, bg, tail) = split_stream_flags(v(&["--with-bg", "--tail-only"]));
    assert!(
        rest.is_empty(),
        "剥净 → 流模式（!args.is_empty() 为 false）"
    );
    assert!(bg);
    assert!(tail);
    let (rest, bg, tail) = split_stream_flags(v(&["--tail-only"]));
    assert!(rest.is_empty());
    assert!(!bg);
    assert!(tail);
    let (rest, bg, tail) = split_stream_flags(v(&[]));
    assert!(rest.is_empty());
    assert!(!bg);
    assert!(!tail);
}

/// 查询参数与流 flag 互不干扰：查询参数原样保留（顺带守住"flag 混进查询
/// 命令行也不会破坏查询"的边角）。
#[test]
fn query_args_pass_through() {
    let (rest, bg, tail) = split_stream_flags(v(&["--read-session", "/p/s.jsonl", "--with-bg"]));
    assert_eq!(rest, v(&["--read-session", "/p/s.jsonl"]));
    assert!(bg);
    assert!(!tail);
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
