use super::*;

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("夹具不是合法 JSON —— 重跑 npm run gen:payload-golden")
}

/// ★ 对拍的 TS 那一半必须还在**且还在做那件事** —— 只 `include_str!` 不读它，
/// 编译器会说 `never used`；而只查「文件在」也拦不住有人把断言掏空。
#[test]
fn the_typescript_half_still_asserts_the_fixture_is_current() {
    assert!(
        TS_HALF.contains("renderGoldenFixture()"),
        "TS 那一半不再调用现场渲染 ⇒ 「夹具陈旧」这件事没人管了"
    );
    assert!(
        TS_HALF.contains("payload-golden.json"),
        "TS 那一半不再读入库夹具"
    );
}

/// ★ 计数自检：先证明「有东西可比」，再比。
#[test]
fn the_fixture_actually_has_cases() {
    let f = fixture();
    assert_eq!(
        f.cases.len(),
        EXPECT_CASES,
        "夹具用例数变了。夹具被清空/截断时，下面那条逐条对拍会零命中零失败地绿；\
             正常加用例请把 EXPECT_CASES 一起改（那正是它写成相等的理由）"
    );
    assert!(
        !f.nested_env_keys.is_empty(),
        "nestedEnvKeys 空了 ⇒ `unset-nested-env` 那几条会渲染成 `unset ; `，\
             而 TS 侧同样为空时也一样 —— 对拍看不出来，只有这条能"
    );
}

/// ★ 本模块的正题：同一组输入，Rust 渲染出来的必须与 TS 入库的**逐字节**相同。
#[test]
fn rust_payload_rendering_matches_the_typescript_golden_byte_for_byte() {
    let f = fixture();
    let mut mismatches = Vec::new();
    for c in f.cases {
        let name = c.name.clone();
        let want = c.payload.clone();
        // ★ 跑**生产命令本体**（`render_launch_payload`），不是自己重搭 `PayloadSpec`。
        let got = match crate::backend::control::launch_wire::render_launch_payload(c.req) {
            Ok(p) => p,
            Err(e) => format!("<Err: {e}>"),
        };
        if got != want {
            mismatches.push(format!(
                "  用例「{name}」\n    TS  : {want:?}\n    Rust: {got:?}"
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} 条载荷两侧不一致：\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

/// ★ 夹具的 `nestedEnvKeys` 必须与 **monitor 自己那份 Rust 常量**集合相等。
///
/// ⚠ **我第一版这里写的是「monitor 的 Rust 侧今天没有自己的 nestedEnvVars 常量」——
/// 那是事实错误，代码审计当场证伪**：`adapter/claude_code.rs::CLAUDE_NESTED_ENV` 就是它，
/// `lib.rs` 的 `scrub_env_vars` 在用。那句错话恰好挡住了本来可以补上的这条钉子。
///
/// # 为什么这条不是 `agent-profile-parity.vitest.ts` 的重复
///
/// 那条 L2 守卫钉的是 **TS 源码 ↔ Rust 源码**的集合相等（且刻意不钉顺序）。
/// 本条钉的是**夹具里那份**（= 载荷里 `unset` 的实际顺序来源）↔ Rust 常量。
/// 这是本对拍里**唯一一处 Rust 不是独立实现、而是照抄 TS 输入**的地方 ——
/// 没有它，「改 TS 顺序 + 重生成夹具」会让 Rust 对拍绿、L2 守卫绿（它按集合比）、
/// 而载荷字节已经变了。
///
/// ⚠ **两侧顺序今天确实不同**（TS: CLAUDECODE/ENTRYPOINT/SESSION_ID/CHILD_SESSION；
/// Rust: CLAUDECODE/CHILD_SESSION/SESSION_ID/ENTRYPOINT）。`unset` 的顺序不影响语义，
/// 所以这里也**按集合**比。但 U8c-2 让 Rust 当**生产者**之后，它会去用
/// `nested_env_to_scrub()`，那一刻产出的字节就与今天的夹具不同 ——
/// **那不是 bug，是必须在 U8c-2 一并重生成夹具的信号。**
#[test]
fn fixture_nested_env_keys_match_the_rust_constant_as_a_set() {
    let mut from_fixture = fixture().nested_env_keys;
    from_fixture.sort();
    let mut from_rust: Vec<String> = crate::adapter::active()
        .nested_env_to_scrub()
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    from_rust.sort();
    assert_eq!(
        from_fixture, from_rust,
        "夹具里的嵌套 env 清单与 monitor 的 Rust 常量不是同一个集合"
    );
}

#[test]
fn nested_env_keys_look_like_env_var_names() {
    for k in fixture().nested_env_keys {
        assert!(
            !k.is_empty()
                && k.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                && !k.starts_with(|c: char| c.is_ascii_digit()),
            "不像环境变量名：{k:?}（它会被原样拼进 `unset …`）"
        );
    }
}
