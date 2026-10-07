//! 生产命令 `wire::render_ccm_launch`（那一行 `ccm …`）**↔** 入库夹具 `fixtures/cli-golden.json` 的**逐字节对拍**。
//!
//! `out` 由 `tests/test-support/launch-cli-golden.ts` 用例表里**手写**，`req` 由生产请求构造（`buildCliRenderRequest`）现产
//! ⇒ 钉的是「生产 TS 请求构造 → 线 → 生产 wire 类型 → 生产命令」这一整条。改渲染器的产出 ⇒ 本条红 ⇒ 回去改期望。
//! ok 与 refusal 两类都比：只比 ok 的话，「该拒却渲染出来了」抓不到。

use serde::Deserialize;

const FIXTURE: &str = include_str!("fixtures/cli-golden.json");

/// 写成相等而不是地板：加 / 删用例被迫回来改这个数。
const EXPECT_CASES: usize = 23;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    #[serde(rename = "_")]
    _comment: String,
    /// 「`--launcher` 吐不吐」那条分支的唯一输入；由 `the_fixture_default_launcher_is_the_one_the_backend_says` 接到后端那一份上。
    #[serde(rename = "defaultLauncher")]
    default_launcher: String,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    /// 那台 `ccm` 的能力：不上线（生产上问那台后端自己），对拍这一侧拿它喂 `render_ccm_launch_with`。
    caps: Vec<String>,
    /// 生产 wire 类型：由 TS 的 `buildCliRenderRequest` 构造、落盘（不另镜像一份）。
    req: super::wire::CliRenderRequest,
    ok: bool,
    out: String,
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/launch_cli_parity_tests.rs"]
mod tests;
