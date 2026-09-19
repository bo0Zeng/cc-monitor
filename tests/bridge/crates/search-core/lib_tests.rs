use super::*;

#[test]
fn find_ci_ascii_and_cjk() {
    assert_eq!(find_ci("Hello World", "world"), Some((6, 11)));
    assert_eq!(find_ci("HELLO", "hell"), Some((0, 4)));
    assert_eq!(find_ci("abc", "xyz"), None);
    let s = "你好世界 docker 部署";
    let (start, end) = find_ci(s, "docker").unwrap();
    assert_eq!(&s[start..end], "docker");
}

#[test]
fn make_snippet_three_parts() {
    let (before, matched, after) =
        make_snippet("请问怎么用 Docker 部署这个服务到生产环境", "docker");
    assert_eq!(matched, "Docker");
    assert!(before.contains("怎么用"));
    assert!(after.contains("部署"));
}

/// snippet 上下文窗口**就是** `SNIPPET_CTX` —— 期望值从常量取，不写字面量。
/// 改 `SNIPPET_CTX` 一处，这条自动跟上；哪一侧改成自己的数，那一侧就红。
#[test]
fn snippet_context_window_is_exactly_snippet_ctx() {
    let filler = "x".repeat(SNIPPET_CTX * 4);
    let text = format!("{filler}docker{filler}");
    let (before, _m, after) = make_snippet(&text, "docker");
    // 截断了 ⇒ before 前缀一个 `…`、after 后缀一个 `…`，各自内容恰好 SNIPPET_CTX 字符。
    assert_eq!(
        before.chars().count(),
        SNIPPET_CTX + 1,
        "before = … + CTX 字符"
    );
    assert_eq!(
        after.chars().count(),
        SNIPPET_CTX + 1,
        "after = CTX 字符 + …"
    );
}

#[test]
fn extract_text_blocks_string_and_array() {
    assert_eq!(extract_text_blocks(&Value::String("hi".into())), "hi");
    let arr = serde_json::json!([
        {"type":"text","text":"line1"},
        {"type":"tool_use","name":"Bash","input":{}},
        {"type":"text","text":"line2"}
    ]);
    assert_eq!(extract_text_blocks(&arr), "line1\nline2");
}

#[test]
fn extract_tool_text_assistant_and_user() {
    let asst =
        serde_json::json!([{"type":"tool_use","name":"Bash","input":{"command":"ls -la"}}]);
    let t = extract_tool_text(&asst, true);
    assert!(t.contains("Bash") && t.contains("ls -la"));
    let user = serde_json::json!([{"type":"tool_result","content":[{"type":"text","text":"file out"}]}]);
    assert!(extract_tool_text(&user, false).contains("file out"));
}

#[test]
fn clean_user_text_strips_wrappers_and_interrupt() {
    assert_eq!(
        clean_user_text("<system-reminder>noise</system-reminder>真内容"),
        "真内容"
    );
    assert_eq!(clean_user_text("[Request interrupted by user]"), "");
}

#[test]
fn truncate_helpers() {
    assert_eq!(truncate_plain("hello", 3), "hel");
    assert_eq!(truncate_plain("你好世界", 2), "你好");
    assert_eq!(truncate_plain("hi", 10), "hi");
    assert_eq!(truncate_excerpt("a\nb", 10), "a b");
    assert_eq!(truncate_excerpt("abcd", 3), "abc…");
}

#[test]
fn clamp_limit_bounds() {
    assert_eq!(clamp_limit(0), LIMIT_MIN);
    assert_eq!(clamp_limit(99_999), LIMIT_MAX);
    assert_eq!(clamp_limit(50), 50);
}

/// 🔴 预算的两个「不给」必须分得开。
#[test]
fn budget_tells_exhausted_apart_from_session_cap() {
    let mut b = SnippetBudget::new(2);
    assert_eq!(b.take(0), SnippetVerdict::Give);
    assert_eq!(b.take(1), SnippetVerdict::Give);
    assert_eq!(b.spent(), 2);
    assert_eq!(b.take(2), SnippetVerdict::BudgetExhausted);
    assert!(b.starved(), "全局预算用完过 ⇒ 整份结果被砍，要告诉用户");

    // 预算充足、但单会话满了 ⇒ **不是**「结果被砍」。
    let mut c = SnippetBudget::new(1_000);
    assert_eq!(c.take(PER_SESSION_CAP), SnippetVerdict::SessionCapped);
    assert!(
        !c.starved(),
        "单会话超 {PER_SESSION_CAP} 条只是「这个会话话多」，整份结果没被砍 —— \
             这两件事合成一个 bool 正是本 crate 要拆开的那个病"
    );
}

/// 最近优先：原地降序，稳定到「同 key 保持原相对序」。
#[test]
fn sort_by_recency_is_newest_first() {
    let mut v = vec![("a", 10i64), ("b", 300), ("c", 200)];
    sort_by_recency(&mut v, |x| x.1);
    assert_eq!(v.iter().map(|x| x.0).collect::<Vec<_>>(), ["b", "c", "a"]);
}

#[test]
fn session_title_picks_in_order() {
    assert_eq!(session_title(Some("AI"), "首句", "abcdefghij"), "AI");
    assert_eq!(session_title(Some("  "), "首句", "abcdefghij"), "首句");
    assert_eq!(session_title(None, "", "abcdefghij"), "abcdefgh");
}
