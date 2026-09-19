//! U8c-1：`backend::control::payload` 的载荷渲染 **↔** TS `launch-render-fallback.ts` 的**逐字节对拍**。
//!
//! # 为什么这条判据活在 `src/bridge` 而不在 crate 里
//!
//! 渲染器**曾经**在零外部依赖的共享 crate 里（同 branch-core / usage-core / acct-core /
//! guard-core 的约束：daemon 是 Linux-only 静态 musl 二进制，一旦引入平台相关的东西共享就破了）。
//! 读夹具要 `serde_json`，而 `src/bridge` 本来就有。⇒ 内核保持纯，判据放在有依赖的这一侧。
//!
//! # 为什么不是自洽夹具（U7-4 的病根）
//!
//! 夹具**入库**，两侧各自与它比：
//! - TS 侧 `tests/launch-payload-golden.vitest.ts` 断言「入库的 == 现场渲染的」⇒ 改 TS 不重生成 ⇒ 红；
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
//! Rust 侧自己的单测（`ccm_invocation.rs` 那半到 UB-复盘2 才有，此前是 0 条）。
//! ⚠ 而 TS 那半**排期在 U8c-3 被删** —— 那天一到，本对拍只剩「Rust 跟上次一样」的快照意义。

use serde::Deserialize;

/// 夹具**编译期**嵌进来 —— 文件被删/改名 ⇒ **编译失败**，不是运行时跳过。
const FIXTURE: &str = include_str!("fixtures/payload-golden.json");

/// ★ 对拍的 **TS 那一半**也要被钉住（审计 S3）。
///
/// 夹具本身有 `include_str!` 保护（删了就编译失败），但阻止「夹具变陈旧」的唯一机制是那个
/// vitest 文件 —— 把它改名成 `.spec.ts` 就同时从 vitest 的 glob 和 `npm test` 里消失，
/// 之后两种语言可以**永远静默分家**。改名/删除 ⇒ 这里编译失败。
const TS_HALF: &str = include_str!("../../../../../tests/launch-payload-golden.vitest.ts");

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
#[path = "../../../../../tests/bridge/backend/control/launch_payload_parity_tests.rs"]
mod tests;
