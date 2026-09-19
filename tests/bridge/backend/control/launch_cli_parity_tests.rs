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
/// `tests/e2e/ccm-contract-parity.sh:291` 逐字按**文件路径 ＋ 单行数组字面量**从
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
             ⚠ 顺序也算：`tests/e2e/ccm-contract-parity.sh` 按这一行的**字面量**抽它去比真 ccm 的 \
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
