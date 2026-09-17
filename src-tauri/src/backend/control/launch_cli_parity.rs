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
/// 剩下两句（两条「维度 …」闸门）搬不动 —— `test/launch-render-cli.vitest.ts` 那两条判据
/// 是**按措辞 grep 源码**钉的（「各恰好一处」＋ 必须是 `` return { ok: false, reason: `…` } ``
/// 这个模板形），而那个文件不在本件写区。⇒ 搬走它们会让那两条判据当场假红。
///
/// ⚠ 那正好是 `KR95D1` 点名的失效方向（「判前端源码里还有没有那几个字符串 = 判写法」），
/// 本条**不重蹈**：它不是查「那几个字在不在」，而是拿**后端现场产出的措辞**去比
/// —— 改 Rust 的措辞而不改 TS ⇒ 红；两边一起改 ⇒ 绿。
const TS_CLI_RENDERER: &str = include_str!("../../../../src/launch-render-cli.ts");

/// 🔴 `K-R95`：本机拉起载荷里「哪个号」那一格的**取值口**。
const TS_ACCOUNTS: &str = include_str!("../../../../src/accounts.ts");

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
mod tests {
    use super::*;

    fn fixture() -> Fixture {
        serde_json::from_str(FIXTURE).expect("夹具不是合法 JSON —— 重跑 npm run gen:payload-golden")
    }

    /// ★★ 🔴 `K-R95`：**夹具的 `defaultLauncher` 那一格此前刻意声明却不读**（头注逐字
    /// 「只为『让夹具能被解析』」）—— 于是它可以是任何字，而它是**渲染 `--launcher`
    /// 要不要吐**那条分支的唯一输入。本件把它接上后端那一份。
    ///
    /// 死值验：把夹具里的 `defaultLauncher` 改成 `"cc"` ⇒ 本条红（此前全绿）。
    #[test]
    fn the_fixture_default_launcher_is_the_one_the_backend_says() {
        assert_eq!(
            fixture().default_launcher,
            crate::adapter::active().default_launcher(),
            "夹具里的默认启动器与后端 `adapter::active()` 那一份不是同一个 —— \
             它决定 `--launcher` 吐不吐，错了整条命令就错了"
        );
    }

    /// ★★ 🔴 `K-R95`：前端还留着的那两句降级理由，**逐字节**等于后端产出的那两句。
    ///
    /// 死值验：改 `Refusal::DimensionNeedsCap` 的措辞（只改 Rust）⇒ 本条红。
    #[test]
    fn the_two_reasons_the_frontend_still_spells_out_are_pinned_to_the_backend() {
        use crate::backend::control::ccm_invocation::Refusal;
        // 用 TS 那侧的插值写法当占位喂进后端 ⇒ 产出的就是 TS 那一行该长的样子。
        let needs_cap = Refusal::DimensionNeedsCap {
            dim: "${dim.id}".into(),
            cap: "${cap}".into(),
        }
        .reason();
        let cannot_speak = Refusal::DimensionCannotSpeak("${dim.id}".into()).reason();
        for want in [&needs_cap, &cannot_speak] {
            let quoted = format!("`{want}`");
            assert!(
                TS_CLI_RENDERER.contains(&quoted),
                "`src/launch-render-cli.ts` 里没有这一句（后端现场产出的措辞）：\n  {quoted}\n\
                 两侧的降级理由是**用户看得见的生产文案**，改了一侧不改另一侧就是两份说法。"
            );
        }
    }

    /// ★★ 🔴 `K-R95`：前端还留着的那份**能力清单**，逐字节（含顺序）等于后端那一份。
    ///
    /// # 它为什么还留在前端（登记在案，不是漏了）
    ///
    /// `e2e/ccm-contract-parity.sh:291` 逐字按**文件路径 ＋ 单行数组字面量**从
    /// `src/launch-render-cli.ts` 抽这份清单（还配了「抽到 ≥5 项」的抽取器自检），
    /// 去比真 `ccm --ccm-probe` 吐的 `capabilities=`。搬走 ⇒ 它抽到 0 项 ⇒ 当场红。
    /// 而那个文件不在本件写区。
    ///
    /// ⇒ 处置：清单留着，但**此前它一条判据都没有**（只有两侧互指的注释）。本条把它钉上。
    ///
    /// 死值验：在 `ccm_invocation::CLI_REQUIRED_CAPS` 里加一项 / 换个顺序（只改 Rust）⇒ 本条红。
    #[test]
    fn the_capability_list_the_frontend_still_spells_out_is_pinned_to_the_backend() {
        use crate::backend::control::ccm_invocation::CLI_REQUIRED_CAPS;
        let want = format!(
            "const CLI_REQUIRED_CAPS = [{}] as const;",
            CLI_REQUIRED_CAPS
                .iter()
                .map(|c| format!("\"{c}\""))
                .collect::<Vec<_>>()
                .join(", ")
        );
        assert!(
            TS_CLI_RENDERER.contains(&want),
            "`src/launch-render-cli.ts` 里那份能力清单与后端的不是同一份。\n\
             后端说它该逐字是：\n  {want}\n\
             ⚠ 顺序也算：`e2e/ccm-contract-parity.sh` 按这一行的**字面量**抽它去比真 ccm 的 \
             `capabilities=`，两份分了家 = 一台装了 ccm 的机器静默退回兜底渲染器（丢账号保真度）。"
        );
    }

    /// ★★ 🔴 `K-R95` `KR95D1` **刀③**（「前端退回自己拼 ⇒ 必须红」）：
    /// `accounts.ts` 真的用**计算键**读生成物那张表，而不是把三个键名再写一遍。
    ///
    /// # ⚠ 本条是**判写法**，登记在案 —— 别把它当成 `KR95D1` 的主判据
    ///
    /// 主判据是**产出**那一侧：`launch_wire.rs` 的
    /// `the_facts_the_frontend_holds_are_recomputed_from_the_backend_every_time`
    /// （拿后端此刻的值现算一遍去比生成物）＋ 门禁第六格 `generated` 的
    /// `git diff --exit-code`（改后端不重生成 ⇒ 红）。
    ///
    /// 本条只补 **刀③** 那一格，而那一格在**同步纯函数**上除了源码层没有别的抓手：
    /// 前端把 `{ kind: "named", configDir, name }` 三个字面量写回去，产出与今天**逐字节相同**
    /// ⇒ 任何按产出判的东西都看不见它，直到某天 `history.rs` 改名才一起爆。
    /// ⇒ 判据体系里必须有一条**看得见「它是从哪儿拿的」**，代价是它按写法判。
    #[test]
    fn the_frontend_reads_the_account_wire_table_instead_of_writing_the_keys_out_again() {
        for needle in [
            "[LOCAL_LAUNCH_ACCOUNT_WIRE.tag]:",
            "[LOCAL_LAUNCH_ACCOUNT_WIRE.configDir]:",
            "[LOCAL_LAUNCH_ACCOUNT_WIRE.name]:",
        ] {
            assert!(
                TS_ACCOUNTS.contains(needle),
                "`src/accounts.ts` 里没有 `{needle}` —— 本机拉起载荷「哪个号」那一格\n\
                 又变回前端自己拼了（`K28`：前端不许自己发明对外行为）。\n\
                 ⚠ 产出**逐字节相同**，所以除了本条没有任何东西看得见它。"
            );
        }
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
