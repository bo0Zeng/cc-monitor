//! U8c-2c-1：`backend::control::ccm_invocation::render_ccm_invocation` **↔** TS `tryRenderCli`
//! 的**逐字节对拍**。
//!
//! 机制与 `launch_payload_parity.rs` 完全相同（入库夹具，两侧各自与它比，
//! 绝不让 Rust 去调 TS 现场生成 —— 那是 U7-4 的自洽夹具病根）。
//!
//! ⚠ **ok 与 refusal 两类都比**：只比 ok 的话，「该降级却渲染出来了」抓不到，
//! 而那正是 §33 铁律要防的形态。

use serde::Deserialize;

const FIXTURE: &str = include_str!("fixtures/cli-golden.json");

/// 与 `launch_payload_parity` 同理：写成相等而不是地板，加/删用例被迫回来改这个数。
///
/// 🔴 `K-R95` `KR95D2`（纪律 ⑱）：本件把前端那几格「自己写死的说法」删掉、改从后端取，
/// **这 16 例一例没少**，仍旧逐字节钉着那一份产出。夹具数变少 ⇒ 这一行当场红。
const EXPECT_CASES: usize = 16;

/// 🔴 `K-R95`：前端**还留着**的那两句降级理由，逐字节钉在后端那份上。
///
/// # 为什么还留着（这是登记在案的边界，不是漏了）
///
/// 本件把八句降级理由里的六句搬进生成物 `src/generated/launch-render-facts.ts`。
/// 剩下两句（两条「维度 …」闸门）搬不动 —— `tests/launch-render-cli.vitest.ts` 那两条判据
/// 是**按措辞 grep 源码**钉的（「各恰好一处」＋ 必须是 `` return { ok: false, reason: `…` } ``
/// 这个模板形），而那个文件不在本件写区。⇒ 搬走它们会让那两条判据当场假红。
///
/// ⚠ 那正好是 `KR95D1` 点名的失效方向（「判前端源码里还有没有那几个字符串 = 判写法」），
/// 本条**不重蹈**：它不是查「那几个字在不在」，而是拿**后端现场产出的措辞**去比
/// —— 改 Rust 的措辞而不改 TS ⇒ 红；两边一起改 ⇒ 绿。
const TS_CLI_RENDERER: &str = include_str!("../../../../launch-render-cli.ts");

/// 🔴 `K-R95`：本机拉起载荷里「哪个号」那一格的**取值口**。
const TS_ACCOUNTS: &str = include_str!("../../../../accounts.ts");

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
    /// ★ **生产 wire 类型** —— 由 TS 的 `buildCliRenderRequest`（`renderCliViaBackend` 用的
    /// 同一个）构造、落盘。用它而不是自己再镜像一份，是本轮复盘的核心修复：
    /// 判据体系审计实测，此前 `render_ccm_launch` 这个命令**本体零调用零判据**，
    /// 5 个 wire 映射变异（`send_into` 恒 false / 具名账号降成 base / 丢 cwd / 丢 model /
    /// 清空 nested_env）**全部存活**；wire 字段改名（`send_into`→`sendInto`）也全绿 ——
    /// 而那在生产里表现为**每次 tmux 拉起都静默回退 TS 兜底**。
    req: crate::backend::control::launch_wire::CliRenderRequest,
    ok: bool,
    out: String,
}

// U8a-2c-pre 复盘：这里原本有 `Ctx` / `FxAction` / `FxContainer` / `FxAccount` 四个
// **手写镜像**（微架构审计点名：`FxAction` 与 `launch_wire::WireAction` 逐字相同，
// 连映射 match 都是复制的）。改成直接反序列化**生产 wire 类型**之后它们全成了死代码 ⇒ 删。
// 净效果：少四个类型、少一份 match，而且对拍从「我重搭一个 spec」升级成「跑生产命令」。

#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/launch_cli_parity_tests.rs"]
mod tests;
