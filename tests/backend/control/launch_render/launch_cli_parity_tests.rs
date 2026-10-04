use super::*;

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("夹具不是合法 JSON —— 重跑 npm run gen:cli-golden")
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

// 这里原来有三条：两条拿后端现场产出去逐字节钉 TS 渲染器源码里还留着的
// 能力清单与两句「维度 …」降级理由（TS 渲染器已删，被钉的那一侧没了 ⇒ 整条退役；
// 清单今天只有 `ccm_invocation.rs` 一份，`tests/e2e/ccm-contract-parity.sh` 改抽它），
// 一条钉「`accounts.ts` 用计算键读 wire 键名」—— 那张键名表随界面那份挑号一起删了（线上那一格今天是生成的类型）。

/// 号照请求原样当已判好（这一族比的是渲染，判号由 `launch_account_tests.rs` 钉）。
fn as_asked(r: &super::super::wire::CliRenderRequest) -> crate::control::launch_account::Settled {
    crate::control::launch_account::settled_as_asked(&r.account, &r.models)
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
    // 改成**相等**（原是 ≥9 / ≥7 的地板）：两类各自的条数是用例表写死的，
    // 地板只挡「少」、挡不住「某条 refusal 悄悄变成 ok」—— 而那一条恰恰让 ok 数变多。
    // 实数：23 ok（9 ＋ 新两形 2 ＋ Codex resume 1 ＋ path 7 ＋ print-parity 4）＋ 6 refusal（「启动期令牌」ok 与「坏令牌」拒随令牌删了；
    // 加「Codex 会话选了具名账号」一条拒）。
    assert_eq!(ok, 23, "ok 类条数变了（实数 23）");
    assert_eq!(
        refused, 6,
        "refusal 类条数变了（实数 6）—— 要防的正是「该拒却渲染出来了」"
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
        let caps: std::collections::BTreeSet<String> = c.caps.iter().cloned().collect();
        let (got_ok, got) =
            match super::super::wire::render_ccm_launch_with(&c.req, &as_asked(&c.req), &caps) {
                Ok(cmd) => (true, cmd),
                Err(r) => (false, r),
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
        "{} 条 CLI 渲染与用例表的期望不一致（改了渲染器就回 `src/frontend/ui/launch-cli-golden.ts` 改期望）：\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// 生产那一格的能力问的是这台后端自己（`ccm_launcher_with(TMUX_PLATFORM)`，与 `--ccm-probe` 同一份），
/// 不是一份写死的表：能力齐全的那条用例在生产命令上照样渲得出、与夹具期望逐字节同。
#[test]
fn the_production_cli_render_asks_this_backend_for_its_own_capabilities() {
    let f = fixture();
    let c = f
        .cases
        .into_iter()
        .find(|c| c.name == "new + base")
        .expect("夹具里没有「new + base」");
    assert_eq!(
        super::super::wire::render_ccm_launch(&c.req, &as_asked(&c.req)).as_deref(),
        Ok(c.out.as_str()),
        "生产那一格没按这台后端自己的能力渲"
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

/// （「`ccm` 就是后端本身」）：渲染器搬进后端之后，与 `ccm` 那套 argv 的**唯一解析口**同在一个 crate ——
/// 渲出来的每一行 `ccm …`（夹具里全部 ok 用例）都得被 `control::ccm::argv::parse` 收下（异源：渲染一侧 vs 解析一侧）。
/// 它是 `protocol_doc_guard::TERMINAL_SURFACE_FILES` 里 `ccm_invocation.rs` 那一格的接盘判据（那份文件的 `--旗标` 是终端面，不是 wire 面）。
#[test]
fn every_rendered_ccm_line_is_accepted_by_the_ccm_argv() {
    let f = fixture();
    let mut checked = 0;
    for c in f
        .cases
        .iter()
        .filter(|c| c.ok && !c.out.starts_with("tmux send-keys"))
    {
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
        checked, 21,
        "ok 用例条数不对（23 条 ok 去掉外层包了 tmux 的那两条）—— 上面那条在少数几行上成立不算数"
    );
    assert_eq!(
        shell_words("ccm -- new --cwd '/home/用户/带 空格'"),
        ["ccm", "--", "new", "--cwd", "/home/用户/带 空格"],
        "切词器自己坏了"
    );
}

/// ★★ 起会话只有 ccm 一处：monitor 每一条远端起会话路径真发出去的那一形（夹具里的 `path:` 那几条，意图由生产 `plan*` 现造）
/// 过生产命令，交出去的都只是一行 `ccm …`；就地 resume 回落那一形外层只包一层 tmux，包的那一行同样以 `ccm ` 开头。
/// 路径名单手写、与夹具两向相等（少一条路径 / 多一条没登记的都红）。本机那几条在 `local_tests.rs`。
#[test]
fn every_monitor_launch_path_hands_over_one_ccm_line() {
    const PATHS: &[&str] = &[
        "path:远端直连 resume",
        "path:远端 tmux 建会话 resume（换号重启 · 分叉）",
        "path:分叉继承源会话的目录（说不出名字）",
        "path:就地 resume 键进 pane 的那一行",
        "path:就地 resume 回落那一整串（外层只包那一行）",
        "path:远端开新会话",
        "path:远端接回",
    ];
    let f = fixture();
    let got: Vec<&str> = f
        .cases
        .iter()
        .filter(|c| c.name.starts_with("path:"))
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(got, PATHS, "夹具里的起会话路径与这张名单对不上");
    for c in f.cases.iter().filter(|c| c.name.starts_with("path:")) {
        let caps: std::collections::BTreeSet<String> = c.caps.iter().cloned().collect();
        let line = super::super::wire::render_ccm_launch_with(&c.req, &as_asked(&c.req), &caps)
            .unwrap_or_else(|e| panic!("「{}」渲不出来：{e}", c.name));
        let words = shell_words(&line);
        let ok = match words.first().map(String::as_str) {
            Some("ccm") => true,
            // 外层只包一层：`tmux send-keys -t '=名:' '<那一行>' Enter; tmux attach -t '=名:'`。
            Some("tmux") => {
                words.get(1).map(String::as_str) == Some("send-keys")
                    && words.get(4).is_some_and(|inner| inner.starts_with("ccm "))
            }
            _ => false,
        };
        assert!(ok, "「{}」交出去的不是一行 ccm：{line}", c.name);
    }
}

/// **e2e 取「app 真正会跑的那一行」的 Rust 那一跳**（`#[ignore]` 数据出口，`tests/e2e/launch-render-emit.sh` 跑它）。
///
/// 请求由 e2e 那一侧用生产 TS 的 `plan*` ＋ `buildCliRenderRequest` 产（`tests/e2e/launch-render-driver.ts`），
/// 经环境变量 `CCM_E2E_RENDER_REQ` 递进来；这里拿生产 wire 类型反序列化、跑生产命令本体（能力问这台后端自己），原样吐出去。
/// 渲得出 ⇒ `LAUNCH_RENDER<<<命令>>>`；拒 ⇒ `LAUNCH_RENDER_ERR<<<理由>>>`（拒绝本身是读数）；请求缺失 / 解析不了 ⇒ panic。
#[test]
#[ignore]
fn emit_launch_render_for_e2e() {
    let raw = std::env::var("CCM_E2E_RENDER_REQ")
        .expect("缺 CCM_E2E_RENDER_REQ —— 本出口只给 tests/e2e/launch-render-driver.ts 用");
    let req: super::super::wire::CliRenderRequest =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("请求解析不了（{e}）：{raw}"));
    match super::super::wire::render_ccm_launch(&req, &as_asked(&req)) {
        Ok(cmd) => {
            assert!(
                !cmd.contains('\n'),
                "渲出来的命令带换行，标记行会被截断：{cmd:?}"
            );
            println!("LAUNCH_RENDER<<<{cmd}>>>");
        }
        Err(e) => println!("LAUNCH_RENDER_ERR<<<{}>>>", e.replace('\n', " ")),
    }
}
