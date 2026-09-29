//! `backend::control::launch_wire::render_ccm_launch`（生产命令）**↔** 入库夹具
//! `fixtures/cli-golden.json` 的**逐字节对拍**。
//!
//! 机制与 `launch_payload_parity.rs` 相同（入库夹具，两侧各自与它比，
//! 绝不让 Rust 去调 TS 现场生成 —— 那是 U7-4 的自洽夹具病根）。
//!
//! # 〔LR1 · U8c-3〕左边换了：从「TS 渲染器的产出」换成「手写期望」
//!
//! TS 那份 `ccm …` 渲染器（`tryRenderCli`）已删 —— 生产从 U8c-2c-2 起只走 Rust。
//! 这份夹具**没跟着删**，因为它一直在钉两件事，只有第一件随渲染器走：
//! 1. 两种语言渲出同一行 —— 另一种语言没了，这一件没了（已知代价）。
//! 2. **生产的 TS 请求构造（`buildCliRenderRequest`）→ 线 → 生产 wire 类型 → 生产命令**
//!    这一整条。它与 TS 渲染器无关：没有它，下面 `Case::req` 注释里那几个 wire 映射变异
//!    与请求构造上的变异全都静默（样子是「降级到载荷那条，门禁全绿」）。
//!
//! ⇒ `out` 由 `src/frontend/ui/launch-cli-golden.ts` 用例表里**手写**（前 16 条是 TS 渲染器最后一次
//! 跑出、与本条逐字节对过的原样），`req` 仍由生产请求构造现产。
//! 改 Rust 渲染器的产出 ⇒ 本条红 ⇒ 回去改用例表里的期望 —— 那一步是人做的，这是它的全部意义。
//!
//! ⚠ **ok 与 refusal 两类都比**：只比 ok 的话，「该降级却渲染出来了」抓不到，
//! 而那正是 §33 铁律要防的形态。

use serde::Deserialize;

const FIXTURE: &str = include_str!("fixtures/cli-golden.json");

/// 与 `launch_payload_parity` 同理：写成相等而不是地板，加/删用例被迫回来改这个数。
///
/// 🔴 〔LR1〕16 → 20：`ccm-print-parity` 那四个场景搬进来当用例（那套 e2e 从本夹具按名取行，
/// 取代它原先现场跑的 TS 渲染器）。原 16 例一例没少。
const EXPECT_CASES: usize = 20; // 〔LR2 · R95b〕20 → 21：+「没探出来」；〔MIG-2〕21 → 20：那一态产不出来了（能力问那台后端自己）

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
    /// 〔MIG-2〕那台 `ccm` 的能力（`null` = 没装）：不上线（生产上问那台后端自己），对拍这一侧拿它喂 `render_ccm_launch_with`。
    caps: Option<Vec<String>>,
    /// ★ **生产 wire 类型** —— 由 TS 的 `buildCliRenderRequest`（`renderCliViaBackend` 用的
    /// 同一个）构造、落盘。用它而不是自己再镜像一份，是本轮复盘的核心修复：
    /// 判据体系审计实测，此前 `render_ccm_launch` 这个命令**本体零调用零判据**，
    /// 5 个 wire 映射变异（`send_into` 恒 false / 具名账号降成 base / 丢 cwd / 丢 model /
    /// 清空 nested_env）**全部存活**；wire 字段改名（`send_into`→`sendInto`）也全绿 ——
    /// 而那在生产里表现为**每次 tmux 拉起都静默回退 TS 兜底**。
    req: super::wire::CliRenderRequest,
    ok: bool,
    out: String,
}

// U8a-2c-pre 复盘：这里原本有 `Ctx` / `FxAction` / `FxContainer` / `FxAccount` 四个
// **手写镜像**（微架构审计点名：`FxAction` 与 `launch_wire::WireAction` 逐字相同，
// 连映射 match 都是复制的）。改成直接反序列化**生产 wire 类型**之后它们全成了死代码 ⇒ 删。
// 净效果：少四个类型、少一份 match，而且对拍从「我重搭一个 spec」升级成「跑生产命令」。

#[cfg(test)]
#[path = "../../../../tests/backend/control/launch_render/launch_cli_parity_tests.rs"]
mod tests;
