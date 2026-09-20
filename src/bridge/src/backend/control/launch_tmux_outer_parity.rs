//! `设计/90 §4 E`：**外层 tmux 命令那三格**的跨语言逐字节对拍。
//!
//! Rust 那半住 [`super::payload::render_tmux_outer`]（并进「Rust 载荷」那一份，不新开模块 ——
//! `设计/00 §2.5 ④` 要的是**消灭副本**）；TS 那半是今天线上真在跑的
//! `launch-render-fallback.ts::renderFallback` → `session-backend.ts` 那条。
//!
//! # 它与 [`super::launch_payload_parity`] 的分工
//!
//! | | 覆盖哪一层 | 夹具 |
//! |---|---|---|
//! | `launch_payload_parity` | **内层**：`env → cd → argv → wrap`（`container:"none"`） | `payload-golden.json` |
//! | **本模块** | **外层**：`new-session` / `send-keys` / `attach` 那三格 | `tmux-outer-golden.json` |
//!
//! 两份都跑**生产命令本体** `render_launch_payload`，不是各自重搭一个 spec ——
//! 复盘实测过一次反例：那条命令本体当时零调用零判据，「清空 `nested_env`」那个变异全绿。
//!
//! # 诚实边界（别读成「合完了」）
//!
//! - **它挡的是单侧静默漂移，不是两侧同错**（同 `launch_payload_parity` 头注那条订正）。
//!   Rust 那半是照着 TS 逐行写的；两边一致地错时入库夹具照样全绿。
//! - **非法输入那一维结构上进不了夹具**：Rust 侧对空会话名 / 越界 `@ccm_sid` /
//!   空 cwd 一律 `Err`，而 TS 座逐字「不做校验/转义」照拼 —— 左边产得出、右边拒的样本
//!   根本写不进「两侧同一串」的夹具。那一维由本模块自己的语义判据兜
//!   （`the_rust_side_refuses_what_the_typescript_seat_would_have_concatenated`）。
//! - **生产今天还没切过来**：这一拍只做到「后端产得出，且与线上字节逐字节相同」。
//!   `remote-launch-run.ts::renderLaunchCommand` 最后那一行仍是 `renderFallback(plan)` ——
//!   两把尺子的读数（`launch_wire.rs` 的 `TS_FALLBACK_KEEPERS` / `TS_FALLBACK_REACH`）
//!   因此**一个字节都没动**。⚠ 别把「产得出」读成「在用」，那正是 `K-R105` 治的那个病。

use serde::Deserialize;

/// 夹具**编译期**嵌进来 —— 文件被删/改名 ⇒ **编译失败**，不是运行时跳过。
const FIXTURE: &str = include_str!("fixtures/tmux-outer-golden.json");

/// ★ 对拍的 **TS 那一半**也要被钉住（同 `launch_payload_parity` 的 `TS_HALF`）。
///
/// 夹具本身有 `include_str!` 保护（删了就编译失败），但阻止「夹具变陈旧」的唯一机制是
/// 那个 vitest 文件 —— 把它改名成 `.spec.ts` 就同时从 vitest 的 glob 和门禁里消失，
/// 之后两种语言可以**永远静默分家**。改名/删除 ⇒ 这里编译失败。
const TS_HALF: &str = include_str!("../../../../../tests/launch-tmux-outer-golden.vitest.ts");

/// 用例数。夹具被清空/截断时，逐条循环会「零命中零失败」地绿 —— 这条挡的正是那个。
///
/// **是 `assert_eq!` 不是地板**：地板在「变少」方向是瞎的，而这三格的覆盖面
/// （create 的 cwd × ccm_sid × quoting 组合 · send-into · attach）恰恰是靠条数撑起来的。
/// 写成相等 ⇒ 加/删用例都必须回来改这个数，改的时候人会看见它。
const EXPECT_CASES: usize = 13;

/// 三格**每一格都得有人**。只数总条数挡不住「把 13 条全写成 create」。
const EXPECT_MODES: &[&str] = &["create", "send-into", "attach"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    /// 夹具头上那句「勿手改」。**必须在这里声明** —— 否则 `deny_unknown_fields` 会把它
    /// 当未知字段拒掉。
    #[serde(rename = "_")]
    _comment: String,
    #[serde(rename = "nestedEnvKeys")]
    #[allow(dead_code)]
    nested_env_keys: Vec<String>,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    /// 这条用例落在三格的哪一格（`create` / `send-into` / `attach`）。
    /// 人看 diff 用，同时给 [`EXPECT_MODES`] 那条覆盖面判据当分组键。
    mode: String,
    /// ★ **生产 wire 类型**（由 TS 的 `buildTmuxOuterRenderRequest` 构造）。
    req: crate::backend::control::launch_wire::PayloadRenderRequest,
    /// TS 侧现场渲染出来的那一串（`renderFallback` → `SESSION_BACKEND`）。
    cmd: String,
}

#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/launch_tmux_outer_parity_tests.rs"]
mod tests;
