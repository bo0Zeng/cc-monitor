//! 生产命令 `wire::render_ccm_launch`（那一行 `ccm …`）**↔** 入库夹具 `fixtures/cli-golden.json` 的**逐字节对拍**。
//!
//! `out` 由 `tests/test-support/launch-cli-golden.ts` 用例表里**手写**，`req` 由生产请求构造（`buildCliRenderRequest`）现产
//! ⇒ 钉的是「生产 TS 请求构造 → 线 → 生产 wire 类型 → 生产命令」这一整条。改渲染器的产出 ⇒ 本条红 ⇒ 回去改期望。
//! ok 与 refusal 两类都比：只比 ok 的话，「该拒却渲染出来了」抓不到。

use serde::Deserialize;

const FIXTURE: &str = include_str!("fixtures/cli-golden.json");

/// 写成相等而不是地板，加/删用例被迫回来改这个数。
/// 20 → 29：删「未装」「本地 transport」「send-into 无 CLI 形」三条拒（那三形今天都渲得出 / 产不出来了），
/// 加「只有目录」「启动期令牌」「就地 resume」三条 ok 与「缺 cwd 能力」「坏 sid」两条拒（tmux 那条拒换成「容器要 tmux」），
/// 再加 `path:` 七条（monitor 每条远端起会话路径真发出去的那一形）。
/// 29 → 27：「启动期令牌」那条 ok 与「坏令牌」那条拒随起会话时注的令牌删了。
const EXPECT_CASES: usize = 27;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    #[serde(rename = "_")]
    _comment: String,
    /// 🔴 `K-R95`：**此前是「刻意声明却不读」**（原注逐字：「只为『让夹具能被解析』」）。
    /// 它是「`--launcher` 吐不吐」那条分支的唯一输入，却没有任何东西钉着它 ——
    /// 现在由 `the_fixture_default_launcher_is_the_one_the_backend_says` 接到后端那一份上。
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
    /// ★ **生产 wire 类型** —— 由 TS 的 `buildCliRenderRequest` 构造、落盘（不另镜像一份）。
    req: super::wire::CliRenderRequest,
    ok: bool,
    out: String,
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/launch_cli_parity_tests.rs"]
mod tests;
