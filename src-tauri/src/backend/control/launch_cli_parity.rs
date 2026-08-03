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
const EXPECT_CASES: usize = 16;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    #[serde(rename = "_")]
    _comment: String,
    /// **刻意声明却不读**：`deny_unknown_fields` 要求把 TS 侧每个顶层字段都列出来，
    /// 而启动器现在由 `req.default_launcher` 自带 ⇒ 这里只为「让夹具能被解析」。
    #[allow(dead_code)]
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
mod tests {
    use super::*;

    fn fixture() -> Fixture {
        serde_json::from_str(FIXTURE).expect("夹具不是合法 JSON —— 重跑 npm run gen:payload-golden")
    }

    /// ★ 计数自检：先证明「有东西可比」，再比。**ok 与 refusal 各自也要有下限** ——
    /// 只剩 ok 那半的话，「该降级却渲染出来了」就没人管了。
    #[test]
    fn the_fixture_covers_both_ok_and_refusal() {
        let f = fixture();
        assert_eq!(
            f.cases.len(),
            EXPECT_CASES,
            "夹具用例数变了（加/删用例请一起改 EXPECT_CASES）"
        );
        let ok = f.cases.iter().filter(|c| c.ok).count();
        let refused = f.cases.len() - ok;
        // 6 → 9 / 5 → 7（实测 9 ok + 7 refusal）。复盘 P3：**这是唯一一侧下限**了
        // （TS 那半的重复副本已删），所以它得说真数 —— 6/5 意味着能静默丢掉三条 ok
        // 和两条 refusal 而不红。
        assert!(ok >= 9, "ok 类只有 {ok} 条（实测应为 9）");
        assert!(
            refused >= 7,
            "refusal 类只有 {refused} 条 —— §33 要防的正是「该降级却渲染出来了」"
        );
    }

    /// ★ 正题：同一组输入，两种语言的产出（命令串**或**降级理由）逐字节相同。
    #[test]
    fn rust_cli_rendering_matches_the_typescript_golden_byte_for_byte() {
        let f = fixture();
        let mut bad = Vec::new();
        for c in f.cases {
            // ★ 跑的是**生产命令本体**（`render_ccm_launch`），不是自己重搭一遍 spec。
            let res = crate::backend::control::launch_wire::render_ccm_launch(c.req);
            let (got_ok, got) = match (res.ok, res.cmd, res.reason) {
                (true, Some(cmd), _) => (true, cmd),
                (false, _, Some(r)) => (false, r),
                other => (false, format!("<命令返回了不合法的组合：{other:?}>")),
            };
            if got_ok != c.ok || got != c.out {
                bad.push(format!(
                    "  用例「{}」\n    TS  : ok={} {:?}\n    Rust: ok={} {:?}",
                    c.name, c.ok, c.out, got_ok, got
                ));
            }
        }
        assert!(
            bad.is_empty(),
            "{} 条 CLI 渲染两侧不一致：\n{}",
            bad.len(),
            bad.join("\n")
        );
    }
}
