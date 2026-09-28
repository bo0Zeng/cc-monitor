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
        crate::agents::claudecode::resume::DEFAULT_COMMAND,
        "夹具里的默认启动器与后端 `adapter::active()` 那一份不是同一个 —— \
             它决定 `--launcher` 吐不吐，错了整条命令就错了"
    );
}

// 〔LR1 · U8c-3〕这里原来有三条：两条拿后端现场产出去逐字节钉 TS 渲染器源码里还留着的
// 能力清单与两句「维度 …」降级理由（TS 渲染器已删，被钉的那一侧没了 ⇒ 整条退役；
// 清单今天只有 `ccm_invocation.rs` 一份，`tests/e2e/ccm-contract-parity.sh` 改抽它），
// 一条钉「`accounts.ts` 用计算键读 wire 键名」—— 那条与渲染器无关，搬去了它的生成器旁边
// （`launch_wire_k_r95_launch_render_facts.rs`）。

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
    // 〔LR1〕改成**相等**（原是 ≥9 / ≥7 的地板）：两类各自的条数是用例表写死的，
    // 地板只挡「少」、挡不住「某条 refusal 悄悄变成 ok」—— 而那一条恰恰让 ok 数变多。
    // 实数：13 ok（原 9 ＋ print-parity 4）＋ 7 refusal（〔MIG-2〕8 → 7：「没探出来」那一态产不出来了）。
    assert_eq!(ok, 13, "ok 类条数变了（实数 13）");
    assert_eq!(
        refused, 7,
        "refusal 类条数变了（实数 7）—— §33 要防的正是「该降级却渲染出来了」"
    );
}

/// ★ 正题：生产请求（TS `buildCliRenderRequest` 现产）过生产命令，产出（命令串**或**降级理由）
/// 与用例表里手写的期望逐字节相同。
#[test]
fn rust_cli_rendering_matches_the_typescript_golden_byte_for_byte() {
    let f = fixture();
    let mut bad = Vec::new();
    for c in f.cases {
        // ★ 跑的是**生产命令本体**（`render_ccm_launch` 的本体 `_with`，能力喂夹具那一份），不是自己重搭一遍 spec。
        let caps: std::collections::BTreeSet<String> = c.caps.iter().flatten().cloned().collect();
        let res = super::super::wire::render_ccm_launch_with(c.req, &caps, c.caps.is_some());
        let (got_ok, got) = match (res.ok, res.cmd, res.reason) {
            (true, Some(cmd), _) => (true, cmd),
            (false, _, Some(r)) => (false, r),
            other => (false, format!("<命令返回了不合法的组合：{other:?}>")),
        };
        if got_ok != c.ok || got != c.out {
            bad.push(format!(
                "  用例「{}」\n    期望: ok={} {:?}\n    Rust: ok={} {:?}",
                c.name, c.ok, c.out, got_ok, got
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "{} 条 CLI 渲染与用例表的期望不一致（改了渲染器就回 `src/launch-cli-golden.ts` 改期望）：\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// 〔MIG-2 · `99 §2.1 ⑬`〕生产那一格的能力问的是这台后端自己（`ccm_launcher_with(TMUX_PLATFORM)`，与 `--ccm-probe` 同一份），
/// 不是一份写死的表：能力齐全的那条用例在生产命令上照样渲得出、与夹具期望逐字节同。
#[test]
fn the_production_cli_render_asks_this_backend_for_its_own_capabilities() {
    let f = fixture();
    let c = f
        .cases
        .into_iter()
        .find(|c| c.name == "new + base")
        .expect("夹具里没有「new + base」");
    let res = super::super::wire::render_ccm_launch(c.req);
    assert_eq!(
        (res.ok, res.cmd.as_deref()),
        (true, Some(c.out.as_str())),
        "生产那一格没按这台后端自己的能力渲（理由：{:?}）",
        res.reason
    );
}

/// 按 POSIX 单引号规则切一行（渲染器只用 `posix_quote` 的单引号形，`'\''` 那一形也认）。
fn shell_words(line: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted, mut any) = (Vec::new(), String::new(), false, false);
    for c in line.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                any = true;
            }
            ' ' if !quoted => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                any = false;
            }
            '\\' if !quoted => {}
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// 设计/01 §6.7b（「`ccm` 就是后端本身」）：〔MIG-2〕渲染器搬进后端之后，与 `ccm` 那套 argv 的**唯一解析口**同在一个 crate ——
/// 渲出来的每一行 `ccm …`（夹具里全部 ok 用例）都得被 `control::ccm::argv::parse` 收下（异源：渲染一侧 vs 解析一侧）。
/// 它是 `protocol_doc_guard::TERMINAL_SURFACE_FILES` 里 `ccm_invocation.rs` 那一格的接盘判据（那份文件的 `--旗标` 是终端面，不是 wire 面）。
#[test]
fn every_rendered_ccm_line_is_accepted_by_the_ccm_argv() {
    let f = fixture();
    let mut checked = 0;
    for c in f.cases.iter().filter(|c| c.ok) {
        let words = shell_words(&c.out);
        assert_eq!(
            words.first().map(String::as_str),
            Some("ccm"),
            "「{}」不是一行 ccm：{}",
            c.name,
            c.out
        );
        let parsed = crate::control::ccm::argv::parse(&words[1..]);
        assert!(
            parsed.is_ok(),
            "渲染器吐的「{}」那一行 `ccm` 自己不认：{}（{:?}）",
            c.name,
            c.out,
            parsed.err()
        );
        checked += 1;
    }
    assert_eq!(
        checked, 13,
        "ok 用例条数不对 —— 上面那条在少数几行上成立不算数"
    );
    assert_eq!(
        shell_words("ccm -- new --cwd '/home/用户/带 空格'"),
        ["ccm", "--", "new", "--cwd", "/home/用户/带 空格"],
        "切词器自己坏了"
    );
}
