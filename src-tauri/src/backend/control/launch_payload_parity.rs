//! U8c-1：`launch-core` 的载荷渲染 **↔** TS `launch-render-fallback.ts` 的**逐字节对拍**。
//!
//! # 为什么这条判据活在 `src-tauri` 而不在 crate 里
//!
//! `launch-core` 是**零外部依赖**的共享 crate（同 branch-core / usage-core / acct-core /
//! guard-core 的约束：daemon 是 Linux-only 静态 musl 二进制，一旦引入平台相关的东西共享就破了）。
//! 读夹具要 `serde_json`，而 `src-tauri` 本来就有。⇒ 内核保持纯，判据放在有依赖的这一侧。
//!
//! # 为什么不是自洽夹具（U7-4 的病根）
//!
//! 夹具**入库**，两侧各自与它比：
//! - TS 侧 `src/launch-payload-golden.vitest.ts` 断言「入库的 == 现场渲染的」⇒ 改 TS 不重生成 ⇒ 红；
//! - 本模块断言「Rust 渲染的 == 入库的」⇒ 改 Rust ⇒ 红。
//!
//! 没有任何一侧在运行时去调另一侧 —— 挡住的是 U7-4 那种**自洽夹具**（夹具由被测代码
//! 现场产出、永远自己对自己）。
//!
//! ⚠ **订正（2026-08-03 复盘）**：这里原本写的是「**所以**不存在『两边同时错、对拍照样绿』
//! 那种自洽」—— **那个因果不成立**。Rust 那侧是照着 TS 逐行写的；两边**一致地**错时，
//! 入库夹具照样全绿（复盘审计用变异实测过）。夹具挡的是**单侧静默漂移**，不是两侧同错。
//!
//! 真正挡「两侧同错」的是**各侧自己的语义判据**：TS 的 `launch-render-*.test.ts` +
//! Rust 的 `launch-core` 自测（`cli.rs` 那半到 UB-复盘2 才有，此前是 0 条）。
//! ⚠ 而 TS 那半**排期在 U8c-3 被删** —— 那天一到，本对拍只剩「Rust 跟上次一样」的快照意义。

use serde::Deserialize;

/// 夹具**编译期**嵌进来 —— 文件被删/改名 ⇒ **编译失败**，不是运行时跳过。
const FIXTURE: &str = include_str!("fixtures/payload-golden.json");

/// ★ 对拍的 **TS 那一半**也要被钉住（审计 S3）。
///
/// 夹具本身有 `include_str!` 保护（删了就编译失败），但阻止「夹具变陈旧」的唯一机制是那个
/// vitest 文件 —— 把它改名成 `.spec.ts` 就同时从 vitest 的 glob 和 `npm test` 里消失，
/// 之后两种语言可以**永远静默分家**。改名/删除 ⇒ 这里编译失败。
const TS_HALF: &str = include_str!("../../../../src/launch-payload-golden.vitest.ts");

/// 用例数。夹具被清空/截断时，逐条循环会「零命中零失败」地绿 —— 这条挡的正是那个。
///
/// **是 `assert_eq!` 不是地板**（审计建议）：地板只维持到加第 11 条用例为止 ——
/// 加到 11 之后再删掉一条，`>= 10` 照样绿。写成相等就把「地板」变成**强制触碰**：
/// 加/删用例都必须回来改这个数，改的时候人会看见它。
/// 2026-08-02 U8c-1 交付时 10 条。
const EXPECT_CASES: usize = 10;

/// `deny_unknown_fields`（审计 S2）：未知**变体**本来就会响，但未知**字段**三个层级
/// 全都静默吞掉 —— 顶层多一个像 `nestedEnvKeys` 那样的键表就会静默漂。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    /// 夹具头上那句「勿手改」。**必须在这里声明** —— 否则 `deny_unknown_fields` 会把它当
    /// 未知字段拒掉（加 `deny_unknown_fields` 的第一次运行就是这么红的，正好证明它是活的）。
    #[serde(rename = "_")]
    _comment: String,
    #[serde(rename = "nestedEnvKeys")]
    nested_env_keys: Vec<String>,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    /// ★ **生产 wire 类型**（由 TS 的 `buildPayloadRenderRequest` 构造）。
    /// 同 `launch_cli_parity`：跑生产命令而不是自己重搭 spec —— 复盘实测，
    /// 此前 `render_launch_payload` 本体零调用零判据，「清空 `nested_env`」那个变异全绿。
    req: crate::backend::control::launch_wire::PayloadRenderRequest,
    /// 下面这几个是**夹具的可读性字段**（人看 diff 用），Rust 侧不消费；
    /// `deny_unknown_fields` 要求声明，故留。
    #[allow(dead_code)]
    env: Vec<FixtureEnvOp>,
    #[allow(dead_code)]
    cwd: Option<String>,
    #[allow(dead_code)]
    launcher: String,
    #[allow(dead_code)]
    args: Vec<String>,
    #[allow(dead_code)]
    wrap: Vec<FixtureWrap>,
    payload: String,
}

/// 与 TS `launch-plan.ts::EnvOp` 的 wire 形状一一对应（`kind` 判别式 + 可选 `value`）。
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum FixtureEnvOp {
    // 这几个载荷只为「让夹具能被解析」而存在（真值走 `req`）—— 精确开口，不给整个类型加 allow。
    ExportConfigDir {
        #[allow(dead_code)]
        value: String,
    },
    ExportModel {
        #[allow(dead_code)]
        value: String,
    },
    UnsetConfigDir,
    UnsetNestedEnv,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureWrap {
    #[allow(dead_code)]
    order: i64,
    #[allow(dead_code)]
    prelude: String,
}

#[cfg(test)]
mod tests {
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
}
