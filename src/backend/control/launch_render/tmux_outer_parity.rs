//! `设计/90 §4 E`：**外层 tmux 命令那三格**的逐字节金标准。
//!
//! Rust 那半住 [`super::payload::render_tmux_outer`]（并进「Rust 载荷」那一份，不新开模块 ——
//! `设计/00 §2.5 ④` 要的是**消灭副本**）。〔LR2〕夹具左边原来是 TS 兜底渲染器 ＋ 座（`renderFallback`
//! → `SESSION_BACKEND`）现场渲的串；那一族零生产调用、按 `00 §2.5 ④` 删了，左边换成
//! `tests/test-support/launch-tmux-outer-golden.ts` 用例表里的**手写期望**（值就是 TS 那份最后一次渲出、与 Rust 对过的原样）。
//! `req` 仍由生产的 `buildTmuxOuterRenderRequest` 现产 ⇒ 本对拍钉的是「生产请求 → 线 → 生产命令」这一整条。
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
//! # 诚实边界
//!
//! - 〔LR2〕**「两种语言渲出同一串」那一维没了**（TS 那份删了）：左边是手写期望，
//!   「期望本身写错」只有人读 diff 才看得见 —— 与 `cli-golden.json` 在 LR1 之后同一性质。
//! - **非法输入那一维进不了夹具**：Rust 侧对空会话名 / 越界 `@ccm_sid` / 空 cwd 一律 `Err`，
//!   那一维由本模块自己的语义判据兜
//!   （`the_rust_side_refuses_what_the_typescript_seat_would_have_concatenated`）。
//! - 生产早已切过来（步 22b·B：`renderLaunchCommand` 那三格走 `render_launch_payload` 带 `outer`）。

use serde::Deserialize;

/// 夹具**编译期**嵌进来 —— 文件被删/改名 ⇒ **编译失败**，不是运行时跳过。
const FIXTURE: &str = include_str!("fixtures/tmux-outer-golden.json");

/// ★ 对拍的 **TS 那一半**也要被钉住（同 `launch_payload_parity` 的 `TS_HALF`）。
///
/// 夹具本身有 `include_str!` 保护（删了就编译失败），但阻止「夹具变陈旧」的唯一机制是
/// 那个 vitest 文件 —— 把它改名成 `.spec.ts` 就同时从 vitest 的 glob 和门禁里消失，
/// 之后两种语言可以**永远静默分家**。改名/删除 ⇒ 这里编译失败。
const TS_HALF: &str = include_str!("../../../../tests/launch-tmux-outer-golden.vitest.ts");

/// 用例数。夹具被清空/截断时，逐条循环会「零命中零失败」地绿 —— 这条挡的正是那个。
///
/// **是 `assert_eq!` 不是地板**：地板在「变少」方向是瞎的，而这三格的覆盖面
/// （create 的 cwd × ccm_sid × quoting 组合 · send-into · attach）恰恰是靠条数撑起来的。
/// 写成相等 ⇒ 加/删用例都必须回来改这个数，改的时候人会看见它。
/// 🔴 `设计/80 §8` 步 1（2026-09-23）：13 → **14**，多的那条是「tmux 那一格也带启动期令牌」
/// —— `§8.4` 说的「`EnvOp` 容器无关」在这三格上的逐字节读数。
const EXPECT_CASES: usize = 14;

/// 三格**每一格都得有人**。只数总条数挡不住「把 14 条全写成 create」。
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
    req: crate::control::launch_render::wire::PayloadRenderRequest,
    /// 〔LR2〕用例表里**手写**的期望串（原来是 TS 兜底渲染器现场渲的；那份删了）。
    cmd: String,
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/launch_tmux_outer_parity_tests.rs"]
mod tests;
