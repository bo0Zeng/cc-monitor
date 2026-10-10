use super::ci_yaml;
use super::ci_yaml::job_block as ci_job_block;
use std::fs;
use std::path::Path;

/// 本文件所在 crate 的根（`src/frontend/shell/`）。
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// 共享 crate 的家 —— `<repo>/src/common/`。
fn common_dir() -> std::path::PathBuf {
    crate::guard_support::repo_src_root().join("common")
}

/// 从本 crate 的包根（`members` 数组的基准）到 [`common_dir`] 的相对前缀（带尾 `/`）。
/// 现算，不写死 —— 包根搬一级，这个前缀就跟着变。
fn member_prefix() -> String {
    let from: Vec<_> = root().components().collect();
    let to: Vec<_> = common_dir()
        .components()
        .map(|c| c.as_os_str().to_owned())
        .collect();
    let from: Vec<_> = from.iter().map(|c| c.as_os_str().to_owned()).collect();
    let same = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut out = "../".repeat(from.len() - same);
    for c in &to[same..] {
        out.push_str(&c.to_string_lossy());
        out.push('/');
    }
    out
}

/// `src/common/*/Cargo.toml` 里的包名。
fn shared_crate_names() -> Vec<String> {
    let dir = common_dir();
    let mut names: Vec<String> = fs::read_dir(&dir)
        .expect("src/common/ 读不到")
        .filter_map(|e| {
            let p = e.ok()?.path();
            let toml = p.join("Cargo.toml");
            if !toml.is_file() {
                return None;
            }
            let text = fs::read_to_string(&toml).ok()?;
            text.lines()
                .find_map(|l| l.strip_prefix("name = \""))
                .map(|v| v.trim_end_matches('"').to_string())
        })
        .collect();
    names.sort();
    names
}

/// `[workspace] members` 里那几条指进 `src/common/` 的目录名。
///
/// ★ 它被抽出来是为了给两条自检当**第二个独立来源**〔`E` 阻-1 回修，08-27〕：
/// `shared_crate_names()` 读的是**文件系统**（`src/common/*/Cargo.toml` 的 `name =`），
/// 本函数读的是 **`src/frontend/shell/Cargo.toml` 的 `members` 数组**。
/// 两者**同源于盘、彼此独立** ⇒ 拿它们对拍，就不必再写一个「今天是几」的数。
fn members_crate_dirs() -> Vec<String> {
    let toml = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml 读不到");
    // ⚠ 逐行走一个**小状态机**，刻意**不用** `toml.find("…")` 去切段。
    //   两个理由：① 本仓 `needle_anchor_registry` 立着一条递减棘轮
    //   （「语料变量上的裸匹配……needle 被撑大时照样绿」），本轮第一版写成
    //   `.find("\n[")` 当场撞红（实测「`.find("…")` 9 处 > 上限 8」）；
    //   ② 状态机本身更准：它按「下一个 `[节]` 开始」收尾，不依赖某个字面量恰好出现在哪。
    let mut out: Vec<String> = Vec::new();
    let mut in_ws = false;
    let prefix = member_prefix();
    for line in toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_ws = t == "[workspace]";
            continue;
        }
        if !in_ws {
            continue;
        }
        let t = t.trim_matches(',').trim_matches('"');
        if let Some(name) = t.strip_prefix(prefix.as_str()) {
            out.push(name.trim_matches('"').to_string());
        }
    }
    out.sort();
    out
}

/// `ci.yml` 里**真的会跑**的那些行 —— 注释行剔掉。
///
/// ⚠ 实测（2026-08-03 复盘 P3）：本模块此前直接对整份 `ci.yml` 做 `contains`，
/// 把 `cargo test -p shell-quote-core` **注释掉**之后守卫**照旧全绿**（3 passed）。
/// 而「注释掉一步」正是本模块要防的那个病的最省事形态 —— 它连 diff 都很小。
/// 顺带：文件头那段散文注释里也写着这套纪律的命令形态，散文不该当证据。
/// ★ 抽取器自检：`src/common/` 下一个都没抽到时，下面那条会零命中零失败地绿。
#[test]
fn the_crate_scan_actually_finds_crates() {
    let n = shared_crate_names().len();
    // 两个独立来源对拍：`shared_crate_names()` 读文件系统，`members_crate_dirs()` 读 `members` 数组；
    // 抽取器少认一个 ⇒ 两边不等 ⇒ 当场红，不需要谁记得回来改一个数。
    let members = members_crate_dirs();
    assert_eq!(
        n,
        members.len(),
        "从 `src/common/*/Cargo.toml` 抽到 {n} 个包名，而 `[workspace] members` 里有 {} 条指进 `src/common/` 的。\n\
             两个独立来源对不上 ⇒ 要么真少了一个 crate，要么抽取器瞎了一个。\n\
             文件系统那边：{:?}\n`members` 那边：{members:?}",
        members.len(),
        shared_crate_names()
    );
    // 反空真：两边同时归零时对拍会「相等」—— 正控按名字点：守卫原语那个包必在。
    assert!(
        shared_crate_names().iter().any(|c| c == "guard-core"),
        "从 src/common/*/Cargo.toml 抽到的 {n} 个包名里没有 `guard-core` —— 抽取器坏了"
    );
}

/// ★ 每个共享 crate 都必须在 `src/frontend/shell/Cargo.toml` 的 `[workspace] members` 里。
///
/// 违反它**不会红，只会静默少跑** —— `cargo test --workspace` 覆不到非成员，
/// 而那个 crate 的测试就此从门禁里**消失**（不是失败，是不存在）。
/// 这正是 G2 之前 `branch-core`（漏 fmt/clippy）与 `usage-core`/`acct-core`
/// （三样全漏、漏了两轮）那两次事故的形状，只是载体从「CI 步骤」变成了「members 列表」。
///
/// ⚠ **必须先切出 `[workspace]` 段再找** —— 直接全文 `contains("crates/gate-core")`
/// 会匹配到**依赖声明行**（`gate-core = { path = "crates/gate-core" }`），
/// 于是「从 members 里删掉一个」这种变异**照样绿**（变异实测：第一版就这么活的）。
/// ★ 判据覆盖面**第④格·性质面**：它比的必须是它声称的那个性质。
#[test]
fn every_shared_crate_is_a_workspace_member() {
    let toml = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml 读不到");
    assert!(
        toml.contains("[workspace]"),
        "`src/frontend/shell/Cargo.toml` 的 `[workspace]` 没了 —— 六个共享 crate 会退回\n\
             「path 依赖但非成员」，`--workspace` 从此静默少测它们"
    );
    let beg = toml.find("[workspace]").expect("上面刚断言过");
    let end = toml[beg + 1..]
        .find("\n[")
        .map(|k| beg + 1 + k)
        .unwrap_or(toml.len());
    let ws = &toml[beg..end];
    // 段界自检：切出来的必须真是那一段（含 members、不含 dependencies）。
    assert!(
        ws.contains("members") && !ws.contains("[dependencies]"),
        "`[workspace]` 段界切错了（{} 字节）—— 本条会在全文里瞎找",
        ws.len()
    );
    let prefix = member_prefix();
    let missing: Vec<String> = shared_crate_names()
        .into_iter()
        .filter(|n| !ws.contains(&format!("\"{prefix}{n}\"")))
        .collect();
    assert!(
        missing.is_empty(),
        "这些共享 crate 不在 `[workspace] members` 里：{missing:?}\n\
             ⇒ `cargo test --workspace` 覆不到它们，测试会**静默地**从门禁里消失。"
    );
    // vendor 随唯一消费者搬出本包（`src/vendor/`），
    //   那条 `exclude` 随之删了：住在 workspace 根外面的 path 依赖按构造成不了成员。
    //   同一个意图（vendor 别掺进 `--workspace` 的读数）改钉位置那一半：本包根下没有 vendor 目录、也不再需要 exclude。
    assert!(
        !ws.lines().any(|l| l.trim_start().starts_with("exclude"))
            && !root().join("vendor").exists(),
        "本包根下又出现了 vendor 目录 / `[workspace] exclude` —— vendor 该跟它的唯一消费者住"
    );
}

/// ★★ **每个 path 依赖的 `Cargo.toml` 都必须已被 git 跟踪**。
///
/// # 它是那个真事故的**结构性**修法
///
/// 事故原文（`tests/scripts/verify-committed-state.sh` 头注）：`gate-core` 这条依赖
/// **从没被提交过** ⇒ **committed `main` 连续约 20 轮编不过**，而每一次「全绿」
/// 都来自我的工作树。根因是「排除用户改动」那半做了、「blob-replay 我方那几行」那半没做。
///
/// # 为什么不接 pre-push 钩子（本轮裁的，理由进 `DECISIONS §2`）
///
/// **本工作流的红线是「从不 push」** —— 那正是 `verify-committed-state.sh` 存在的理由：
/// 它是 CI 的本地替身。⇒ **pre-push 钩子永远不会触发**，接了等于没接。
/// 而 pre-commit 挂那个脚本要 80–150 秒，每次提交都付这个代价不现实。
/// ⇒ 真正的修法不是找个钩子挂它，是**把检查从「记得跑那个慢脚本」搬进「总会跑的快套件」**。
/// 本条就是那一步：它跑在 `cargo test` 里，**每轮门禁必过**，耗时几十毫秒。
///
/// ⚠ 它**不取代**那个脚本：脚本做的是「在干净检出上真跑 `cargo check`」，
/// 覆盖面更宽（任何编译错误）。本条只钉**那一类**最阴的错
/// —— 依赖声明在、而被依赖的东西根本没进版本库。**两者并存，不是二选一。**
#[test]
fn every_path_dependency_is_actually_committed() {
    let toml = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml 读不到");
    // 抽 `path = "…"` 的值 —— **只认内联表里那一个**（`x = { path = "…" }`）。
    //
    // 🔴**这一刀是现打逼出来的，不是洁癖。**
    // 上一版抽的是文件里**每一处** `path = "`，而 `[[bin]]` / `[[example]]` /
    // `[[test]]` 这种**目标**声明里那个顶格的 `path = "src/…"` 指的是**一个入口文件**，
    // 不是一棵 crate 树 ⇒ 它会被拼成 `src/frontend/shell/src/main.rs/Cargo.toml` 去问 git，
    // 而那个东西按构造永远不存在 ⇒ **本条当场假红**。
    // 现打：本包加第二个 `[[bin]]` 那一刻，这一条报的是「`src/main.rs/Cargo.toml`
    // 没被 git 跟踪」—— 一条**讲错了成因**的红灯（本仓记过：讲错成因的红比不红更坏）。
    //
    // ⚠ 判准是**形状**不是白名单：依赖声明恒是内联表（`{ path = … }`），
    // 目标声明恒是表里的一个顶格键（只认依赖段：`[[bin]]` 那条 `path` 指的是入口文件，不是一棵树）。
    // 🔴 解析那一段搬进了 `guard_core::inline_table_paths`（同拍，2026-09-23）——
    //    理由与「它买不到什么」住那个原语的头注，不在这里抄第二份。
    let paths: Vec<String> = guard_core::inline_table_paths(&toml);
    // 抽取器自检（正控）：守卫原语那个共享 crate 的 path 依赖必被抽到。
    assert!(
        paths.iter().any(|p| p.trim_end_matches('/').ends_with("common/guard-core")),
        "从 Cargo.toml 抽到的 path 依赖里没有 `common/guard-core` —— 抽取坏了，本条会零命中地绿：{paths:?}"
    );
    let mut untracked = Vec::new();
    for rel in &paths {
        let manifest = format!("src/frontend/shell/{rel}/Cargo.toml");
        let out = std::process::Command::new("git")
            .args(["ls-files", "--error-unmatch", "--", &manifest])
            .current_dir(crate::guard_support::repo_root())
            .output();
        match out {
            Ok(o) if o.status.success() => {}
            Ok(_) => untracked.push(manifest),
            // git 不在 / 不是仓 ⇒ 本条无从判断。**不许静默绿**，直接说出来。
            Err(e) => panic!("跑不了 git（{e}）—— 本条无从判断，别把它读成绿"),
        }
    }
    assert!(
        untracked.is_empty(),
        "这些 path 依赖的 `Cargo.toml` **没有被 git 跟踪**：{untracked:?}\n\
             ⇒ 别人（和 CI）检出这个提交会直接编不过，而你的工作树一切正常。\n\
             这正是 `tests/scripts/verify-committed-state.sh` 头注记的那个真事故\n\
             （`gate-core` 从没被提交，committed main 连续约 20 轮编不过）。"
    );
}

/// ★ CI 必须真的在用那三条收敛后的命令（不是把它们注释掉了）。
///
/// ⚠ 用 [`ci_yaml::live_lines`]（剔注释）—— 复盘 P3 实测过：直接对整份 `ci.yml` 做
/// `contains`，把某一步**注释掉**之后守卫照旧全绿，而「注释掉一步」正是最省事的错法。
#[test]
fn ci_actually_runs_the_three_converged_commands() {
    let ci = ci_yaml::live_lines();
    for needle in [
        "cargo fmt --all --check",
        "cargo clippy --workspace --all-targets",
        "cargo test --workspace",
    ] {
        assert!(
            ci.contains(needle),
            "`ci.yml` 里找不到（未注释的）`{needle}` —— 收敛后的门禁少了一条"
        );
    }
}

/// ★★★ **Windows 信号的锚是 `runs-on:` 那一行，它自己也得有人守**〔audit-0805 F01 的 Phase D〕。
///
/// # 这条是审计打脸打出来的
///
/// 上面那条刚给后端四步补完守卫，Phase D 立刻指出：**更中心的那一行仍然零守卫** ——
/// 把 `rust` job 的 `runs-on` 从 `windows-latest` 改成 `ubuntu-latest`，
/// **全仓一条判据都不红**（审计变异实测：改完 `cargo test --lib` 仍 888 passed / 0 failed）。
/// 而那个 job 是**生产平台唯一的编译与测试信号** —— 换掉 runner 等于把它整个交出去，
/// 且交出去之后所有门禁**依旧全绿**，比删掉一步隐蔽得多。
///
/// 全仓读 `.github/workflows` 的只有两处（本文件 +
/// `local_backend_tests.rs::every_bundle_job_stages_the_local_backend_before_building` 读 `release.yml`），（08-08 起三处，见上一条的订正）
/// **两处都不看 `runs-on`**；`windows-latest` 这个字面量在仓里其余命中全是散文注释。
///
/// ⚠ 本条**不管** backend job 在哪跑（它在 ubuntu 上跨 target check，那是刻意的、
/// `ci.yml:159-161` 有论证）—— 只钉「那个真跑 Windows 的 job 还在 Windows 上跑」。
#[test]
fn the_only_windows_signal_still_runs_on_a_windows_runner() {
    let block = ci_job_block("rust");
    // 抽取器自检：切出来的块里恰好一条 `runs-on:`（下面那条相等）—— 切不出块就红。
    let runs_on: Vec<&str> = block
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("runs-on: "))
        .map(str::trim)
        .collect();
    assert_eq!(
        runs_on.len(),
        1,
        "`rust:` job 里抽到 {} 条 `runs-on:`（应恰好 1）—— 抽取器坏了或 job 形状变了：{runs_on:?}",
        runs_on.len()
    );
    assert_eq!(
        runs_on[0], "windows-latest",
        "`rust:` job 的 runner 变成了 `{}` —— 它是**生产平台唯一的编译与测试信号**。\n\
             换掉之后所有门禁依旧全绿（审计实测：改成 ubuntu 后 `cargo test --lib` 888 passed），\n\
             ⇒ 这条判据存在的全部理由就是让这个改动红一次。\n\
             真要换平台：先在写清「此后没有 Windows 证据」再改这里。",
        runs_on[0]
    );
}

/// 反过来也要成立：`[workspace] members` 里列的目录必须真的存在。
/// 挡的是「crate 改名/删除后 members 留成僵尸」——那会让 `cargo` 直接报错，
/// 但**在本地没人跑 workspace 命令时**可以潜伏很久。
///
/// ⚠ G2 换靶说明：这条原先扫的是 `ci.yml` 里的 `cargo test -p <名>` /
/// `--manifest-path crates/<名>/` 三种形态（那时僵尸长在 CI 步骤里）。
/// 收敛后 CI 不再逐个点名 ⇒ 僵尸只能长在 `members` 里，靶子跟着搬。
#[test]
fn workspace_members_do_not_reference_crates_that_no_longer_exist() {
    let toml = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml 读不到");
    let beg = toml.find("[workspace]").expect("`[workspace]` 不见了");
    let end = toml[beg + 1..]
        .find("\n[")
        .map(|k| beg + 1 + k)
        .unwrap_or(toml.len());
    let ws = &toml[beg..end];
    let mut scanned = 0usize;
    let mut ghosts = Vec::new();
    let prefix = member_prefix();
    for line in ws.lines() {
        let t = line.trim().trim_matches(',').trim_matches('"');
        let Some(name) = t.strip_prefix(prefix.as_str()) else {
            continue;
        };
        let name = name.trim_matches('"');
        scanned += 1;
        if !common_dir().join(name).join("Cargo.toml").is_file() {
            ghosts.push(name.to_string());
        }
    }
    // 抽取器自检：members 里应有 **6** 条 `crates/…`（与 `the_crate_scan_actually_finds_crates`
    // 的地板同源）。扫不到就是取名方式与 Cargo.toml 的写法分家了。
    // 〔`E` 阻-1 回修，08-27〕同上：与**文件系统**那个来源对拍，不再记一个会过期的数。
    assert_eq!(
        scanned,
        shared_crate_names().len(),
        "从 `[workspace] members` 扫到 {scanned} 条指进 `src/common/` 的，而 `src/common/` 下有 {} 个包 —— \
             本抽取器与 Cargo.toml 的写法分家了，或者真少了一个",
        shared_crate_names().len()
    );
    assert!(
        scanned > 0,
        "从 `[workspace] members` 一条指进 `src/common/` 的都没扫到 —— 本抽取器与 Cargo.toml 的写法分家了"
    );
    assert!(
        ghosts.is_empty(),
        "`[workspace] members` 里列了不存在的 crate：{ghosts:?}"
    );
}

/// **`package.json` 里每个 `test*` 脚本，要么 CI 会跑它，
/// 要么在这里登记成「手测」并写清原因。**
///
/// 与上一条是同一族的**另一个方向**：那条问「CI 有的步骤本地数过没有」，
/// 本条问「**仓里写好的套件，有没有谁会去跑**」。
///
/// **为什么建它**（实测撞见的，不是设想）：`tests/e2e/graylight-suite.sh` 是一整套
/// 跨进程整链 e2e（130 行，驱 gray-light 生命周期、断言 `[e2e] tab-state` 序列），
/// 而 **CI 一次都不跑它** —— CI 跑的是名字很像的另一个 `graylight-backend-frames.sh`。
/// 它的前置逐字写着「Xvfb 上跑着 `npx tauri dev`」⇒ 结构上确实进不了 CI，这没问题；
/// **问题是 `src/doc/RELEASING.md` 里零处提到它**：`test:f40` 好歹进了发版手测清单，它没有。
/// ⇒ 于是这套件的唯一触发条件是「有人想起来」。
///
/// ⚠ 顺带澄清一处容易误读的历史：最后改它的提交叫「G-C：三族 e2e 进 CI」，
/// 查过那次 diff —— 进 CI 的是 `graylight-frames` 等五条，**不含本套件**，提交没说假话。
///
/// 与 `tests/frontend/ui/node-suite-registry-guard.vitest.ts` 不冲突（E3）：那条钉的是
/// 「16 个 tsx 套件各自有断言地板」，本条钉的是「套件有没有人调」——两个事实。
#[test]
fn every_test_script_is_either_run_by_ci_or_registered_as_manual() {
    /// 手测套件：**CI 结构上跑不了**的，逐条写清为什么、以及谁会去跑它。
    const MANUAL: &[(&str, &str)] = &[
        (
            "test:f40",
            "需 Xvfb 上跑着 `npx tauri dev`（真 WebView）⇒ 结构上进不了 CI；\
                 发版清单不列它、今天没有人跑它（理由是下面那条）。\
                 ★ **08-06 实测补一条更硬的理由**：它 `PROJ_DIR=\"$HOME/.claude/projects/-tmp-e2e-fork\"`、\
                 `PIDFILE=\"$HOME/.claude/sessions/…\"` —— **固有地往 `~/.claude/` 写**，\
                 而本区红线是「`~/.claude/` 只读」⇒ **本机绝不能跑它，带不带 tmux 桩都不行**。\
                 （实测那次它建了 fixture 目录、随后被自己的 trap 清掉，无残留；但那一瞬是真写。）",
        ),
        (
            "test:graylight",
            "同 f40 契约（脚本头注逐字「前置同 tests/e2e/f40-suite.sh」）⇒ 同样进不了 CI。\
                 ⚠ 但**发版清单里此前没有它** —— 这条例外就是那笔欠账的落点：\
                 谁要删这条例外，得先说清楚它改由谁来跑。\
                 ★ 08-06 实测：带 tmux 桩跑，**第一次碰 tmux 就被拒（exit 99）** ⇒ 它要真 tmux；\
                 但它的 claude_dir 在 `/tmp` 下，**不碰 `~/.claude/`**（与 f40 的红线情形不同）",
        ),
    ];

    let pkg = std::fs::read_to_string(crate::guard_support::repo_root().join("package.json"))
        .expect("读不到 package.json");
    // 只取顶层 "scripts" 里 `"test…": "…"` 这种行，不引 json 依赖。
    let scripts: Vec<(String, String)> = pkg
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            let rest = t.strip_prefix('"')?;
            let (name, rest) = rest.split_once("\": \"")?;
            if !name.starts_with("test") {
                return None;
            }
            let cmd = rest.trim_end_matches(',').trim_end_matches('"');
            Some((name.to_string(), cmd.to_string()))
        })
        .collect();
    // ★ 抽取器自检：剥法坏了 ⇒ 下面会零命中地绿（正控：`test:diff` 那一条必在）。
    assert!(
        scripts.iter().any(|(n, _)| n == "test:diff"),
        "从 `package.json` 剥到的 {} 个 `test*` 脚本里没有 `test:diff` —— 剥法坏了",
        scripts.len()
    );

    let ci = ci_yaml::live_lines();
    let local_gate =
        std::fs::read_to_string(crate::guard_support::repo_root().join("tests/scripts/gate.sh"))
            .expect("读不到 tests/scripts/gate.sh");
    // `npm test` 用 `&&` 串起来的那些，也算「CI 会跑」。
    let chained = scripts
        .iter()
        .find(|(n, _)| n == "test")
        .map(|(_, c)| c.clone())
        .unwrap_or_default();
    let run_by_ci = |name: &str, cmd: &str| -> bool {
        if name == "test" {
            return ci.contains("npm test");
        }
        if chained.contains(&format!("npm run {name}")) && ci.contains("npm test") {
            return true;
        }
        if ci.contains(&format!("npm run {name}")) {
            return true;
        }
        // ① 门禁 `tests/scripts/gate.sh` 里整行 `run_e2e <后缀>`（`pin_line`：恰好一行；CI 的 e2e job 调的就是它）；
        // ② 门禁或 CI 直接跑那条命令（`exec-bits` 的 `bash tests/e2e/exec-bit-guard.sh` 就是这样）。
        if let Some(suffix) = name.strip_prefix("test:") {
            if guard_core::pin_line(&local_gate, &format!("run_e2e {suffix}")).is_ok() {
                return true;
            }
        }
        !cmd.is_empty() && (ci.contains(cmd) || local_gate.contains(cmd))
    };

    // ★ 登记表保鲜（两个方向）。
    for (name, why) in MANUAL {
        let Some((_, cmd)) = scripts.iter().find(|(n, _)| n == name) else {
            panic!("登记成手测的 `{name}` 在 `package.json` 里已经没有了 —— 删掉这一行。（当初的理由：{why}）");
        };
        assert!(
            !run_by_ci(name, cmd),
            "`{name}` 现在**CI 会跑了** —— 把它从手测登记表里删掉。\n\
                 （当初的理由：{why}）"
        );
    }

    // ★ 前提触发器：f40 那条例外的**硬理由**是「它往 `~/.claude/` 写」。
    // 哪天它改用临时目录，这条理由就消失、它可能变成本机跑得动的 —— 必须回来重判。
    {
        let f40 = std::fs::read_to_string(
            crate::guard_support::repo_root().join("tests/e2e/f40-suite.sh"),
        )
        .expect("读不到 tests/e2e/f40-suite.sh");
        assert!(
            f40.lines()
                .any(|l| !l.trim_start().starts_with('#') && l.contains("$HOME/.claude/")),
            "`tests/e2e/f40-suite.sh` 不再往 `$HOME/.claude/` 写了 —— \n\
                 那么「它撞红线所以本机绝不能跑」这个理由就没了，请重新判它能不能进本地门禁\n\
                 （另一半理由「需 Xvfb + 跑着的 tauri dev」要单独核，别一起默认还成立）。"
        );
    }

    let orphan: Vec<String> = scripts
        .iter()
        .filter(|(n, c)| !run_by_ci(n, c) && !MANUAL.iter().any(|(m, _)| m == n))
        .map(|(n, c)| format!("  {n}  =  {c}"))
        .collect();
    assert!(
        orphan.is_empty(),
        "这些套件**没有任何人会去跑**（CI 不跑，也没登记成手测）：\n{}\n\n\
             ⚠ 写好一套 e2e 却没人调它，比没写更坏：它看起来像一层防护。\n\
             两条出路：① 接进 `ci.yml`；② 登记进本条的 `MANUAL` 并写清\
             「为什么 CI 跑不了」+「那谁来跑」。",
        orphan.join("\n")
    );
}

/// **每条 `#[ignore]` 测试都要真有人来触发它。**
///
/// 本族第三条（前两条：CI 步骤本地数过没有 · 套件有没有人调）。这条问最里面那层：
/// **被 `#[ignore]` 挡在常规门禁之外的测试，说好的那个「触发者」还在吗。**
///
/// **为什么建它**：这七条的头注都写着「由 `tests/e2e/xxx.sh` 驱动」，而那是一句**散文**。
/// e2e 脚本靠 `cargo test --lib -- --ignored <过滤串>` 点名它们 ——
/// **改个测试名，过滤串就一个都匹配不上，而 `cargo test` 跑零条测试是 exit 0**。
/// 于是链断了、两边都绿。本条把那句散文变成会红的东西。
///
/// ⚠ 08-06 逐条核过，七条的自称**当时全部成立**（脚本都在、过滤串都对得上）。
/// 又是「今天干净但没人守着」—— 与本会话另外三处同形。
///
/// ⚠ 实况（不是本条能修的，记在这里免得误读绿灯）—— **08-06 订正过一次**：
/// 原文写「这七条自 `1eeb4bf` 起执行次数为零」，理由是三个触发脚本都要真 tmux。
/// **那句话对其中三条是假的**：用一个只会报错的 `tmux` 桩遮住 PATH 实测，
/// `local-backend-supervise.sh` **零次碰 tmux 就跑完**（PASS=7），
/// 并且**真的执行了 `local_backend.rs` 的那三条**（`3 passed`，还起了一个真后端进程）。
/// ⇒ 今天的准确说法：**七条里三条在本机跑得动（带桩）、四条仍要真 tmux**；
/// 而后四条确实自 `1eeb4bf` 起零执行（`ci.yml` 只在 push/PR 触发，停推后没跑过）。
/// 本条守的是「链还连着」，**不是**「它们跑过了」——两件事别混。
#[test]
/// ⚠⚠ **「七条 `#[ignore]` 执行次数为零」这条账，08-13 已不再成立**：
/// 本轮把触发脚本一条条真跑了，逐条读数（每次都对照用户真实 server，**9 个会话逐字未变**）：
///
/// | 触发脚本 | 读数 | 带动的 `#[ignore]` |
/// |---|---|---|
/// | `local-backend-supervise.sh` | **7 过 / 0 败** | **4 条** |
/// | `tmux-guarded-acceptance.sh` | **14 过 / 0 败** | 1 条 —— 🔴 **`K-R72` 09-12：这一行是历史。**
///   那套脚本与它带动的那条 `#[ignore]` 一起删了（输入源是 `tmux.rs` 两条桌面侧 SSH 回落的
///   builder，回落删净 ⇒ 它取不到命令串）。这一行留着是因为**它记着那次读数**，不是现状 |
/// | `usage-probe-acceptance.sh` | **11 过 / 0 败** | 2 条（F08 段 + 命令串产出） |
///
/// ⇒ **7 条里 7 条都跑过了**（`local_backend` 那 4 条此前一次都没跑过 ——
/// 其中 `the_local_tmux_frames_really_land_in_the_ledger` **首跑就是红的**，
/// 病根是「测试与后端不在同一台 tmux server」，已修，见 `P3 §0h-2`）。
///
/// ★ 「触发者登记在册」与「真的有人跑」是两件事 —— 本条只守前者，
/// 后者靠人真跑。**别把这段读成「以后会自动跑」**：它们仍不在 CI 里（要真 tmux/真进程）。
fn every_ignored_test_still_has_someone_who_triggers_it() {
    /// 不由 e2e 驱动、**刻意手动**的，逐条写清谁在什么时候跑它。
    const MANUAL: &[(&str, &str)] = &[
        // `f63_real_data_ledger` 那一行随记录解析搬进了后端（`tests/backend/agents/claudecode/parse_tests.rs`）。
        (
            "wf1_real_powershell_add_then_remove_restores_the_user_path",
            "不是判据是**读数**：要一个 PowerShell（`CCM_PWSH`，收一个 `.ps1` 路径的程序；本机用容器里的 PowerShell 7）\
             跑生成的用户级 PATH 加 / 撤两段（注册表换替身）。谁什么时候跑：改 `profile_installer.rs` 那两段渲染的那一拍，交付前跑一趟、把结果贴进报告。",
        ),
        (
            "wf1_the_windows_probe_script_reports_card_and_where_ccm_resolves",
            "不是判据是**读数**：同上一条要一个 PowerShell（`CCM_PWSH`），跑 Windows 那一形的 ccm 探测串三种情形。\
             谁什么时候跑：改 `ccm_probe.rs` 里 `CCM_PROBE_PS` 的那一拍，交付前跑一趟、把结果贴进报告。",
        ),
        (
            "p2_the_path_probe_reads_back_a_non_ascii_home_under_an_oem_console",
            "不是判据是**读数**：同上要一个 PowerShell（`CCM_PWSH`），控制台编码设成 936 当替身跑 PATH 探针、读回汉字目录。\
             谁什么时候跑：改 `profile_installer.rs::render_user_path_probe_command` 的那一拍，交付前跑一趟、把结果贴进报告。",
        ),
        (
            "p2_powershell_under_a_936_console_writes_the_sample_bytes_to_stderr",
            "不是判据是**读数**：同上要一个 PowerShell（`CCM_PWSH`），核 `platform/console_text_tests.rs` 那段 936 替身字节就是 PowerShell 在 936 控制台下往 stderr 写的。\
             谁什么时候跑：改那段替身字节或 `platform/console_text.rs` 的那一拍，交付前跑一趟、把结果贴进报告。",
        ),
        (
            "the_readings_behind_the_two_thresholds",
            "不是判据是**读数**：大文件模式两个门槛的来源，只在 **release** 档上有意义\
             （debug 档慢一个数量级，拿它推门槛就是订正过的那个错）。\
             跑法写在它自己的头注里；它产出的两个数落在 `bigfile::LINE_READING` / `TOTAL_READING`，\
             由 `the_two_thresholds_are_what_the_readings_derive` 每趟钉住「推算式 == 常量」。",
        ),
        (
            "loopback_roundtrip_through_the_resident_backend",
            "不是 e2e：它要一台**真 sshd**（本用户身份的临时回环 sshd）＋ 一份编好的后端二进制（起成本机常驻后端，\
             stdio 载体、私有 HOME / TMUX_TMPDIR）。触发器是读数脚本 `tests/evidence/SR1a-link-loopback.py --monitor`\
             （它起 sshd、设好 `SR1A_LOOPBACK` 再按名字跑这一条，并核输出里那句 `SR1A-LOOPBACK-MONITOR ok`）。\
             门禁沙箱里起不了 sshd ⇒ 进不了门禁。谁什么时候跑：改 `dial_host` / `link_mux` / `ssh_link` / 后端 `dial/` 的那一拍，\
             交付前跑一趟、把输出贴进报告。",
        ),
        (
            "sr1b_loopback_deploy_and_transfer_through_the_resident_backend",
            "不是 e2e：同上一条，要**真 sshd**（sftp 子系统起始目录钉在临时目录）＋ 编好的后端二进制。\
             触发器是读数脚本 `tests/evidence/SR1b-sftp-loopback.py --monitor`（设好 `SR1B_LOOPBACK` 按名字跑这一条，\
             核输出里那句 `SR1B-LOOPBACK-MONITOR ok`）。谁什么时候跑：改 `sftp.rs` 部署那几问 / `sftp_pool.rs` 中继 /\
             `dial_host::RemoteFs` / 后端 `dial/sftp.rs` · `control/transfer.rs` 的那一拍，交付前跑一趟、把输出贴进报告。",
        ),
        (
            "a_real_backend_feeds_local_lines_through_the_production_read_loop",
            "不是 e2e：它要一份编好的后端二进制（stdio 载体、起参就是生产的 `LOCAL_STREAM_ARGS`，私有 HOME / \
             `CLAUDE_CONFIG_DIR` / `TMUX_TMPDIR`）＋ `/proc`（冒充会话的 `sleep` 要有启动时刻）。触发器是读数脚本 \
             `tests/evidence/CF1-local-lines.py`（设好 `CF1_BACKEND` 再按名字跑这一条，并核输出里那句 `CF1-LOCAL-LINES ok`）。\
             门禁那一格不先编后端二进制 ⇒ 进不了门禁。谁什么时候跑：改本机内容那条路（`local_lines` · 两条本机读循环 · \
             `stream_source::consume_local` / `LineIntake` · 后端 `observe/watcher.rs` 的 tail-only）的那一拍，交付前跑一趟、把输出贴进报告。",
        ),
        (
            "screenshot_for_the_shots_tool",
            "不是判据，是截图工具的一格：`npm run shots`（`tests/shots/filewin.mjs`）起私有 Xvfb、按全名逐场景拉起它截文件窗口。\
             谁什么时候跑：要文件窗口的底图、或动了文件窗口的样子要附截图时跑。",
        ),
        (
            "perf_rig_worker",
            "不是判据，是性能台架的一格：`tests/shots/perf/filewin-perf.sh <测试二进制>` 起私有 Xvfb、按全名拉起它，\
             印每一段（静着 · 悬停 · 滚轮 · 键盘 · 看一眼）的进程 CPU 与帧数。谁什么时候跑：动了文件窗口的重画 / 列表 / 预览、要附性能读数时跑。",
        ),
    ];

    // ── 🔴 第三档触发器：**由同一个 crate 里的判据 spawn 子进程去跑**〔2026-09-21 加〕
    //
    // # 为什么不能塞进 `MANUAL`
    //
    // `MANUAL` 那一档的语义是「**没有自动触发器**，靠人按清单跑」。而这几条不是 ——
    // 它们**每趟门禁都跑**，只是触发器不是 e2e 脚本，而是**同一个 crate 里的判据**：
    // 判据起一个子进程（`cargo test -- --ignored <名>`），在那个子进程里真开窗、
    // 读完再把子进程输出整段带回来（winit 全进程只许一个事件循环 ⇒ 实景只能落子进程）。
    // ⇒ 登记成「手动」是**说假话**：它会让人以为这几条平时不跑。
    //
    // # 而这一档比 `MANUAL` **严**，不是更松
    //
    // `MANUAL` 只要求写一句理由，**没有任何东西核那句理由是不是真的**。
    // 本档要求：这个名字**必须作为字面量出现在判据树里**（不算它自己的 `fn` 定义行）。
    // 🔴 钉的正是本条判据自己的报错逐字警告过的那一形：
    //    「改测试名 ⇒ 过滤串一个都匹配不上，而 `cargo test` 跑零条测试**退出码是 0**
    //      ⇒ 两边都绿，测试其实再没执行过。」
    // ⇒ 改了工作面的名字而没同拍改 spawn 处那个字面量，**当场红**。
    const SPAWNED_BY_JUDGE: &[(&str, &str)] = &[
        (
            "worker_runs_the_real_child_main",
            "真 `child_main` 那一趟（stdin 不关）：由 `proc_tests.rs` 的 `RealChild::start` spawn 子进程跑 \
             （列不出来 / 关窗之后进程在预算内退那两条）",
        ),
        (
            "xvfb_worker_opens_a_real_window",
            "实景开窗那一趟：由 `shell_tests.rs` 的 `scenario_a()` spawn 子进程跑",
        ),
        (
            "xvfb_worker_opens_with_no_x_server_at_all",
            "阴性对照（没有 X 服务器 ⇒ 必须回原因不是静默成功）：\
             由 `a_window_that_cannot_come_up_comes_back_as_a_reason_not_a_silent_ok` spawn",
        ),
        (
            "xvfb_worker_real_pointer_events_on_a_row",
            "真 X 鼠标事件那一趟：由 `rows_tests.rs` 的 `scenario_b()` spawn 子进程跑",
        ),
        (
            "xvfb_worker_real_keys_on_the_window",
            "真 X 键盘那一趟：由 `shell_keys_tests.rs` 的 \
             `a_real_x_keyboard_drives_the_list` spawn 子进程跑",
        ),
    ];

    let repo = crate::guard_support::repo_root().to_path_buf();
    // ── 收 `#[ignore]` 测试：(文件名 stem, fn 名)
    let mut ignored: Vec<(String, String)> = Vec::new();
    // 🔴 〔搬树 2026-09-18 ·  纪律 3〕**加上 `tests/frontend/shell` 这一棵。**
    //    那 7 条 `#[ignore]` 全都是测试，剖分之后一条都不在 `src/frontend/shell/src` 里了
    //    ⇒ 老语料收到 0 条，下面那条「剥法坏了」的反空真按设计响了。
    let mut ignore_corpus = guard_core::scan_tree!(&repo.join("src/frontend/shell/src"), &["rs"]);
    ignore_corpus.extend(guard_core::scan_tree!(
        &repo.join("tests/frontend/shell"),
        &["rs"]
    ));
    // 通信层成员的单测镜像（`tests/comms/inward/`）也是本 crate 的测试。
    ignore_corpus.extend(guard_core::scan_tree!(
        &repo.join("tests/comms/inward"),
        &["rs"]
    ));
    // 文件窗口独立成包（代码随上面那棵的人群声明收），它的测试住 `tests/frontend/filewin/`。
    ignore_corpus.extend(guard_core::scan_tree!(
        &repo.join("tests/frontend/filewin"),
        &["rs"]
    ));
    for (path, src) in ignore_corpus {
        let stem = path
            .file_stem()
            .expect("文件名")
            .to_string_lossy()
            .to_string();
        let lines: Vec<&str> = src.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            if !l.trim_start().starts_with("#[ignore") {
                continue;
            }
            let Some(f) = lines[i + 1..i + 5.min(lines.len() - i)]
                .iter()
                .find_map(|x| x.split_once("fn ").map(|(_, r)| r))
            else {
                continue;
            };
            let name: String = f
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                ignored.push((stem.clone(), name));
            }
        }
    }
    // ★ 自检 1（正控）：真机才跑的那条 PATH 往返判据必被收到 ⇒ 否则剥法坏了（下面会零命中地绿）。
    assert!(
        ignored
            .iter()
            .any(|(_, n)| n == "wf1_real_powershell_add_then_remove_restores_the_user_path"),
        "收到的 {} 条 `#[ignore]` 测试里没有 PATH 往返那一条 —— 剥法坏了",
        ignored.len()
    );

    // ── 收 e2e 脚本里的触发过滤串
    let mut filters: Vec<(String, String)> = Vec::new();
    for e in std::fs::read_dir(repo.join("tests").join("e2e"))
        .expect("读不到 tests/e2e/")
        .flatten()
    {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("sh") {
            continue;
        }
        let Ok(sh) = std::fs::read_to_string(&p) else {
            continue;
        };
        let who = p.file_name().expect("脚本名").to_string_lossy().to_string();
        for line in sh.lines() {
            let t = line.trim_start();
            // 注释里也写着同样的命令串（那是说明，不是触发）——**必须剔掉**，
            // 否则「把真调用删了只留注释」这种最省事的断链会被判据放过。
            if t.starts_with('#') || !t.contains("--ignored") {
                continue;
            }
            for tok in t.split_whitespace().skip_while(|w| *w != "--nocapture") {
                if tok.starts_with('-') || tok == "--nocapture" {
                    continue;
                }
                let tok: String = tok
                    .chars()
                    .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
                    .collect();
                if tok.len() >= 4 {
                    filters.push((who.clone(), tok));
                    break;
                }
            }
        }
    }
    // ★ 自检 2：过滤串收不到 ⇒ 下面每条都会被判成「没人触发」，看起来像大面积腐坏，
    //   实际是抽取器坏了。两种坏法要能分开。
    assert!(
        filters.iter().any(|(_, f)| f.contains("local_backend")),
        "从 `tests/e2e/*.sh` 收到的 `--ignored` 触发过滤串里没有本机后端那一条 —— 抽取器坏了：{filters:?}"
    );

    let covered = |stem: &str, name: &str| -> Option<String> {
        filters
            .iter()
            .find(|(_, f)| name.contains(f.as_str()) || stem.contains(f.as_str()))
            .map(|(who, f)| format!("{who}（过滤串 `{f}`）"))
    };

    // ★ 自检 3：手测登记表保鲜 —— 登记的那条若已被 e2e 接管，就该把它删掉。
    for (name, why) in MANUAL {
        assert!(
            ignored.iter().any(|(_, n)| n == name),
            "登记成手动的 `{name}` 已经不是 `#[ignore]` 测试了 —— 删掉这一行。（当初的理由：{why}）"
        );
        let (stem, _) = ignored
            .iter()
            .find(|(_, n)| n == name)
            .expect("上面已断言存在");
        assert!(
            covered(stem, name).is_none(),
            "`{name}` 现在**已有 e2e 触发它**了 —— 把它从手动登记表里删掉。（当初的理由：{why}）"
        );
    }

    // ★ 自检 4（本档专有，`MANUAL` 没有的那一格）：**名字真的被字面点名**。
    {
        // 🔴 **本文件必须从语料里摘掉，而 `scan_tree!` 的自动摘除在这里没生效。**
        //
        // 现打逼出来的：第一版直接用 `scan_tree!`（它号称「按构造摘除调用者自己那份」），
        // 而死值验把 spawn 处的字面量改坏之后**仍然绿** —— 命中的是**本文件里
        // `SPAWNED_BY_JUDGE` 那张表自己的那一行**。
        // ⇒ 典型的「**判据在自己的登记表里找到自己 ⇒ 恒绿**」
        //   （`scanning_guard_registry` 头注逐字：audit-0805 实测五次，**五次都不是被判据变红发现的**）。
        //
        // ⚠ **为什么宏的自动摘除没生效**（这一条值得单独记，别处可能同病）：
        //   本文件是 `#[path]` 挂进 `src/` 那一侧的 ⇒ `file!()` 给的是
        //   `src/../../../tests/bridge/shared_crate_registry_tests.rs`，
        //   而扫出来的是绝对路径 `/…/tests/frontend/shell/shared_crate_registry_tests.rs`
        //   —— 宏按**后缀**比对，那个 `src/../../../` 前缀让后缀永远对不上
        //   ⇒ **摘除静默空转**，而「摘了」与「没摘」在输出上一模一样。
        // ⇒ 这里**明写**排除，不依赖那个宏的自摘。
        let me = "shared_crate_registry_tests.rs";
        let mut judge_corpus: Vec<_> =
            guard_core::scan_tree!(&repo.join("tests").join("frontend/shell"), &["rs"])
                .into_iter()
                .filter(|(path, _)| !path.to_string_lossy().ends_with(me))
                .collect();
        // 本 crate 的第二棵测试树（通信层成员单测，`tests/comms/inward/`）。
        judge_corpus.extend(guard_core::scan_tree!(
            &repo.join("tests/comms/inward"),
            &["rs"]
        ));
        // 文件窗口的测试树。
        judge_corpus.extend(guard_core::scan_tree!(
            &repo.join("tests/frontend/filewin"),
            &["rs"]
        ));
        assert!(
            !judge_corpus.is_empty(),
            "判据语料排掉本文件之后成了空集 —— 树的住址错了，本条在空转"
        );
        for (face, why) in SPAWNED_BY_JUDGE {
            assert!(
                ignored.iter().any(|(_, n)| n == face),
                "登记成「判据 spawn」的 `{face}` 已经不是 `#[ignore]` 测试了 —— 删掉这一行。（理由：{why}）"
            );
            // 点名处：把它自己的 `fn <名>` 定义行排掉，剩下的命中才算「有人点它」。
            // 🔴 **按标识符边界认，不许用裸子串** —— 这一条是现打逼出来的：
            //    本条第一版写的是 `l.contains(face)`，而死值验把 spawn 处那个字面量
            //    改成 `<原名>_TYPO` 之后**没红** —— 因为改坏的那串**仍然包含原名作为子串**。
            //    ⇒ 「在名字后面接东西」这一形它整个看不见，而那正是改名最常见的走法。
            //    （同一形今天在 `comm_boundary_registry` 的 C1 上也修过一次：匹配单位太松。）
            let named_with_boundary = |l: &str| -> bool {
                let b = l.as_bytes();
                let mut from = 0usize;
                while let Some(hit) = l[from..].find(face) {
                    let i = from + hit;
                    let after = i + face.len();
                    // 后面不许紧跟标识符字符；否则那是**另一个**名字。
                    let tail_ok = b
                        .get(after)
                        .is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'_'));
                    if tail_ok {
                        return true;
                    }
                    from = after;
                }
                false
            };
            let named = judge_corpus.iter().any(|(_, src)| {
                src.lines()
                    .filter(|l| !l.trim_start().starts_with(&format!("fn {face}")))
                    .any(named_with_boundary)
            });
            assert!(
                named,
                "工作面 `{face}` 在判据树里**没有任何地方点名它** —— \n\
                 那意味着没人会 spawn 它，而 `cargo test --ignored <不存在的名>` \n\
                 **跑零条测试、退出码 0** ⇒ 两边都绿而它再没执行过。\n\
                 改名了就同拍改 spawn 处那个字面量。（登记的理由：{why}）"
            );
        }
    }

    let orphan: Vec<String> = ignored
        .iter()
        .filter(|(_, n)| !MANUAL.iter().any(|(m, _)| m == n))
        .filter(|(_, n)| !SPAWNED_BY_JUDGE.iter().any(|(m, _)| m == n))
        .filter(|(s, n)| covered(s, n).is_none())
        .map(|(s, n)| format!("  {s}.rs::{n}"))
        .collect();
    assert!(
        orphan.is_empty(),
        "这些 `#[ignore]` 测试**没有任何 e2e 脚本会点名它们**：\n{}\n\n\
             ⚠ 断链的典型走法是**改测试名**：e2e 里的过滤串一个都匹配不上，\n\
             而 `cargo test` 跑零条测试**退出码是 0** —— 两边都绿，测试其实再没执行过。\n\
             两条出路：① 把 e2e 里的过滤串改对；② 登记进本条 `MANUAL` 并写清谁在什么时候跑它。",
        orphan.join("\n")
    );
}

/// **跨 target（Windows）编译信号今天只覆盖后端，不覆盖 monitor
/// —— 把这个不对称本身钉住，让它不能悄悄变。**
///
/// 定框 **E7** 逐字写着「C10 的**真判据是跨 target 编得过**
/// （`cargo check --all-targets --target x86_64-pc-windows-msvc`），**不是 cfg 位置扫描**」。
/// 所以本条**刻意不去数 `#[cfg(windows)]` 的位置**——那会与 E7 相悖。它只钉「真判据在不在」。
///
/// **实测到的实况**（08-06）：
/// - backend 有那一步，本机跑得通（exit 0）；
/// - **monitor 没有**。它的 Windows 面只由跑在 `windows-latest` 的 `rust` job 编译，
///   而〔用 08-05〕停推后 `ci.yml` 至今 72 个提交一次没跑 ⇒ **那 31 处 `cfg(windows)`
///   已经很久没有被任何编译器看过**。
/// - 当时本机补不上（一条带 C build script 的第三方依赖挡着）；那条依赖早已不在，本机 msvc check 现打 Finished。
///
/// ⚠ 顺带一条方法论（本轮变异抽样撞出来的）：**`#[cfg(windows)]` 里的变异在 Linux 上
/// 连编译错误都不报**（实证：往里写一个不存在的标识符，`cargo build` **零 error**）。
/// ⇒ 对那片代码做变异抽样得到的「SURVIVED」**没有信息量**，那是方法的盲区不是门禁的漏洞。
///
/// 本条是那条诚实边界的**前提触发器**：前提一旦消失就红，逼人回来重判。
#[test]
fn the_windows_cross_target_signal_covers_only_the_backend() {
    const NEEDLE: &str = "--target x86_64-pc-windows-msvc";

    // ① backend 那一步还在吗（E7 点名的真判据，本仓唯一一处）。
    let backend_block = ci_job_block("backend");
    assert!(
        backend_block.contains(NEEDLE),
        "`backend` job 里的跨 target check 不见了 —— 那是 E7 逐字点名的**真判据**，\n\
             删它等于把平台线上唯一还活着的编译信号也关掉。"
    );

    // ② monitor 那侧仍然没有 —— 有了就说明边界过期了，本条要红。
    let rust_block = ci_job_block("rust");
    assert!(
        !rust_block.contains(NEEDLE),
        "`rust` job（monitor）现在**有跨 target check 了** —— 好事，但请顺手：\n\
             ① 删掉里「monitor 的 Windows 面没有编译信号」那条诚实边界；\n\
             ② 删掉本条判据（它的全部意义就是钉住这个不对称）。"
    );
}

/// 〔audit-0805 08-06 立 · `K-R114` / `R73` 09-14 重判后改措辞〕
/// **三条诚实边界压在同一个前提上：「CI 今天不会跑」——把这个前提钉住。**
///
/// （住 `audit-0805` 那个工作区）的 3w（release.yml 的版本 guard 不触发）·
/// 3x（七条 `#[ignore]` 执行次数为零）· 3y（monitor 的 Windows 面没有编译信号），
/// 三条当初的成立**都只因为一件事**：两个 workflow 都只在 `push` / `pull_request` 上触发，
/// 而〔用 08-05〕裁定不再 push。
///
/// 这个前提**没人盯**。谁加一个 `workflow_dispatch`（手点就能跑）或 `schedule`（定时跑），
/// 三条边界当天就该重判 —— 而在本条之前，它们会**继续以「已登记的诚实边界」的样子留在表里**，
/// 那正是本会话反复量到的**停滞式腐坏**：世界变了、文本一个字没动。
///
/// # 🔴 09-14：它红过一次，而那一次它红得对 —— 于是本条从「一律禁」变成「禁 ＋ 登记」
///
/// `K-R114` 给 `release.yml` 加了 `workflow_dispatch`（`KU27`：在那之前，想验一次发版流水线
/// 改得对不对，唯一的办法是真推一个 `v*` tag）⇒ **本条当场红**。
/// 那一件**判不了这条**（三条边界住**另一个工作区**，且翻正属重新裁定）⇒ 交 PM。
/// 重判落在 `DECISIONS.md#R73`，**依据是那一趟真跑的 CI（run `34861383050`），不是读配置推的**：
/// - **`3w` 翻正** —— 那趟**真跑了**版本 guard（逐字 `Version self-consistent: 3.7.0`）；
/// - **`3y` 翻正** —— 那趟**真编了** Windows 面；
/// - **`3x` 不翻正** —— 它压在 **`ci.yml`** 上，而那一件**一个字没动 `ci.yml`**。
///
/// ⇒ 本条因此**不许整条删掉**：删了会把 `3x` 那一半的守卫一起砍掉（**刀不许连量具一起砍**）。
/// 今天的形状是：**`ci.yml` 照旧一个自启触发器都不许有**；`release.yml` 上那一个是**裁过的**，
/// 进 `ADJUDICATED` 登记并带住址回指裁决口。**谁再往 `ci.yml` 加，照样当场红。**
///
/// ⚠ 本条**不断言 CI 应该怎么触发**（那是〔用〕/ PM 的裁决）。它断言的是
/// **「自启触发器要么没有，要么有人裁过并留了住址」** —— 两者都不成立就红，逼人回来重判。
///
/// ⚠ **诚实边界，三条，别读宽**：
/// ① 登记表里那个住址本条**只验形状**（非空 ＋ 形如 `<文件>#<锚点>`），
///    **不去那棵树上核它真的指得到** —— 裁决口住计划仓，本 crate 结构上够不着；
/// ② 它认的是 `on:` 段里**有没有那个词**，不解释 GitHub 的触发语义
///    （`schedule` 配一个永不命中的 cron，本条照样算它「有」）；
/// ③ 登记一行买到的是「**有人回来过**」，**不是**「那三条边界今天写得对」——
///    后者不在本条射程里。
#[test]
fn the_premise_behind_three_honesty_boundaries_still_holds() {
    /// 会让 workflow **在没有 push 的情况下也能跑起来**的触发器。
    const SELF_STARTING: &[&str] = &["workflow_dispatch", "schedule", "repository_dispatch"];
    /// **裁过的**自启触发器：(workflow 文件 · 触发器 · 裁决口住址)。
    ///
    /// 🔴 **一行 = 一次「有人回来把那几条边界重新过了一遍」。没有这一行 = 没人裁过。**
    /// 加一行之前先问：那几条边界你重判了吗？裁决口在哪？——答不出就别加行，让它红着。
    const ADJUDICATED: &[(&str, &str, &str)] =
        &[("release.yml", "workflow_dispatch", "DECISIONS.md#R73")];
    /// 扫描面 ＋ **每个文件今天还压着哪几条边界**（红了要人去重判的就是这些）。
    /// 🔴 这一栏不是装饰：诊断里要说得出「你这一下动的是**谁**的前提」。
    const STILL_RESTING_ON: &[(&str, &str)] = &[
        (
            "ci.yml",
            "3x —— 七条 `#[ignore]` 的 e2e 触发路今天**只有 ci.yml 这一条**，\
                 09-14 那次重判**刻意没翻它**（`R73` 逐字：K-R114 只动 release.yml）",
        ),
        (
            "release.yml",
            "3w / 3y —— 09-14 已由 `R73` 依据 run 34861383050 翻正；\
                 再加**新的**自启触发器，仍要重判一次再登记",
        ),
    ];

    // ★ 登记表自检：三条，防「登记了一行却永远轮不到」那一形（那种行只会替真判据挡枪）。
    let files: Vec<&str> = STILL_RESTING_ON.iter().map(|(f, _)| *f).collect();
    for (wf, trig, at) in ADJUDICATED {
        assert!(
            files.contains(wf),
            "`ADJUDICATED` 登记了 `{wf}`，而扫描面里没有它（今天扫的是 {files:?}）—— \n\
                 这一行永远轮不到，等于没登记。"
        );
        assert!(
            SELF_STARTING.contains(trig),
            "`ADJUDICATED` 登记的 `{trig}` 不在 `SELF_STARTING` 里 —— \n\
                 那它压根不会被检查，这一行是死的。"
        );
        assert!(
            at.contains('#') && !at.starts_with('#') && !at.ends_with('#'),
            "`{wf}` / `{trig}` 那一行的裁决口住址 `{at}` 不成形（要 `<文件>#<锚点>`）—— \n\
                 ⚠ 本条只验形状，不去那棵树上核它真指得到（裁决口住计划仓，本 crate 够不着）。"
        );
    }

    let root = crate::guard_support::repo_root().to_path_buf();
    let mut seen_adjudicated = 0usize;
    for (wf, resting) in STILL_RESTING_ON {
        let text = std::fs::read_to_string(root.join(".github/workflows").join(wf))
            .unwrap_or_else(|e| panic!("读不到 {wf}: {e}"));
        // 只看 `on:` 到 `jobs:` 之间那一段，且剔注释 —— 别把说明文字当触发器。
        let at = text.find("\non:").map(|i| i + 1).unwrap_or_else(|| {
            panic!("{wf} 里找不到顶层 `on:` —— 触发面的读法坏了，本条会零命中地绿")
        });
        let end = text[at..]
            .find("\njobs:")
            .map(|k| at + k)
            .unwrap_or(text.len());
        let seg: String = text[at..end]
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        // ★ 抽取器自检：这一段里至少要提到 `push`，否则说明切错了地方。
        //
        // ⚠ **两种 YAML 写法都要认**〔08-08 变异证伪〕：原来只认**块写法**
        //（行首是 `push:` / `workflow_dispatch:`）。把触发面改成**序列写法**
        // `on: [push, pull_request, workflow_dispatch]` —— 一种 GitHub 完全支持、
        // 也很可能被真人写出来的形态 —— 本条确实红了，**但红在抽取器自检上**，
        // 诊断说「段界切错了」。照它去查的人会去修切块逻辑，
        // 而真实事件是 **CI 从此可以手点运行**、三条诚实边界的前提当场消失。
        // 「讲错成因的红灯比不红更坏」，本会话第 N 次。⇒ 改按**词**匹配。
        assert!(
            guard_core::contains_word(&seg, "push"),
            "{wf} 的 `on:` 段里连 `push` 都没提 —— 段界切错了（切到 {} 字节）",
            seg.len()
        );
        for trig in SELF_STARTING {
            let present = guard_core::contains_word(&seg, trig);
            let registered = ADJUDICATED.iter().find(|(f, t, _)| f == wf && t == trig);
            match (present, registered) {
                // 有触发器、没人裁过 ⇒ 这就是本条要逮的那一形。
                (true, None) => panic!(
                    "{wf} 新增了 `{trig}` 触发器 —— **它从此可以在没有 push 的情况下跑起来**。\n\
                         ⇒ 今天压在这个文件上的诚实边界**前提当场消失，必须重判**：\n\
                         · {resting}\n\
                         重判完之后把这一行加进 `ADJUDICATED`（带裁决口住址），本条自然绿。\n\
                         🔴 **不许把本判据删掉或放宽** —— 别的文件那一半的守卫还压在它身上。\n\
                         （已经裁过的：{ADJUDICATED:?}）"
                ),
                // 登记还在、触发器没了 ⇒ 登记表在替真判据挡枪，摘掉它。
                (false, Some((_, _, at))) => panic!(
                    "`ADJUDICATED` 里登记着「{wf} 的 `{trig}` 已裁过（{at}）」，\n\
                         而 `{wf}` 的 `on:` 段里**今天没有它**。\n\
                         ⇒ 那一行在替真判据挡枪：它一留着，下次谁再加回来就**不会红**。\n\
                         处置：把那一行摘掉（那条边界随之回到「未裁」，加回来要重判）。"
                ),
                (true, Some(_)) => seen_adjudicated += 1,
                (false, None) => {}
            }
        }
    }
    // ★ 地板：「裁过的」那一支今天必须**真的被走过**，否则它是一条零命中的死支
    //   —— 而零命中的分支在本仓已经连着栽过五次（「新分支平时没人走」）。
    assert_eq!(
        seen_adjudicated,
        ADJUDICATED.len(),
        "「裁过的」那一支这一趟走了 {seen_adjudicated} 次，而登记表有 {} 行 —— \n\
             对不上说明有登记行没被行使（上面两条 panic 本该先响；都没响就是扫描面漏了文件）。",
        ADJUDICATED.len()
    );
}

/// **唯一量「提交状态」的那道门，本身没人守着。**
///
/// `tests/scripts/verify-committed-state.sh` 的头注逐字写着它为什么必须存在：
/// 2026-08-04 实测，`gate-core` 那条 path 依赖**一次都没落盘**，
/// 提交状态的 `main` 在任何平台上都编不过，**持续了约二十轮** ——
/// 而每一轮的 `cargo test` / fmt / clippy 读数**都是真的**，
/// 因为它们量的是**工作树**。「工作树绿」与「提交状态绿」是两件事。
/// 它同时写着「本仓红线是不 push ⇒ CI 从来没见过这些 commit，**这道门必须在本机跑**」。
///
/// # 本条钉什么
///
/// 08-06 查到：`ci.yml` 里**只有一句注释**提到这个脚本（不是步骤），
/// 而仓里**没有任何判据读它的内容** ⇒ 三项检查被删掉一项不会红。
/// 本条钉住那三项还在：`monitor-lib` · `backend` · `backend-win`。
///
/// ⚠ **如实记一处局限**（免得把它的绿读大）：monitor 那项是 `cargo check --lib`，
/// **不编测试段**。所以「提交状态编得过」不等于「提交状态的测试编得过」。
/// 真要覆盖那一半得改成 `--all-targets`，代价是本机每次多编一大块 —— 不在本轮做，
/// 写在这里让下一个人看得见。
#[test]
fn the_only_gate_that_measures_committed_state_still_does_all_three_checks() {
    let path = crate::guard_support::repo_root().join("tests/scripts/verify-committed-state.sh");
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {path:?}: {e}"));

    // `(检查名, 那一行还必须含什么)` —— 钉性质不钉整行，留出改写空间。
    const CHECKS: &[(&str, &str)] = &[
        ("run monitor-lib", "cargo check"),
        ("run backend ", "cargo check --all-targets"),
        ("run backend-win", "x86_64-pc-windows-msvc"),
    ];
    for (head, must) in CHECKS {
        let hit = src
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#'))
            .any(|l| l.starts_with(head) && l.contains(must));
        assert!(
            hit,
            "`verify-committed-state.sh` 里找不到 `{head}` 那一项（且含 `{must}`）。\n\
                 ⚠ 这是**唯一量「提交状态」的门**：其余门禁量的都是工作树。\n\
                 它存在的理由是一次真事故 —— 一条 path 依赖没落盘，提交状态的 `main`\n\
                 **约二十轮编不过**，而每一轮的工作树读数都是真的。\n\
                 而且本仓不 push ⇒ CI 见不到这些 commit，这道门**只能在本机跑**。\n\
                 删掉一项之前，先说清楚那半由谁接。"
        );
    }
}

/// **三项还在 ≠ 三项都跑了**。
///
/// 上一条钉的是那三项检查**存在**。而 `backend-win` 那项包在
/// `if rustup target list --installed | grep -q x86_64-pc-windows-msvc` 里 ——
/// ★ 08-08 **真路实测**（临时放一个假 `rustup` 到 `PATH` 前面、让它报「什么都没装」）：
///
/// ```text
///    ok   monitor-lib
///    ok   backend
///    skip backend-win（没装 x86_64-pc-windows-msvc target）
/// == 提交状态编得过 ==            ← 与三项全跑时**一字不差**，exit 0
/// ```
///
/// 而读门禁的人（以及 loop 里的我，习惯是 `| tail -2`）读的就是最后那一行 ⇒
/// **Windows 那半没量这件事，在结论里没有任何痕迹**，上一条判据照旧全绿。
///
/// 这正是本区反复逮到的那个形状在门禁自己身上的一例：
/// 「围栏有判据 ≠ 那条路过了围栏」——这次是「检查在 ≠ 检查跑」。
///
/// # 为什么它比一般的静默降级更要紧（E8）
///
/// 定框 E8 逐字规定：没有当次 Windows 证据时，「全绿」只许写成「Linux 上全绿」。
/// 而跨 target check 在**本机**只有这一处真跑 —— `ci.yml` 那条要 push 才动，
/// 本仓红线是**不 push**。⇒ 这一跳过，E8 所说的那个前提就整个没了。
///
/// # 本条是**源码层代理**，如实写明
///
/// 真路今天由人跑（改前/改后各一次，输出见上）。没做成自动判据的理由具体：
/// 它要在临时 worktree 里真编两遍 cargo check（约一分钟），
/// 放进 `cargo test` 等于每轮门禁多编一遍全仓。⇒ 登记进。
/// 本条能挡的是「有人把降级那一支改回成和成功一样的结论」；
/// 挡不住的是「`run` 函数本身坏掉但文本还在」。
#[test]
fn a_skipped_windows_check_cannot_look_like_a_full_pass() {
    let path = crate::guard_support::repo_root().join("tests/scripts/verify-committed-state.sh");
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {path:?}: {e}"));
    // ⚠ **只看非注释行**：本脚本的头注里逐字引用着那句成功结论（讲的就是这次事故），
    // 连注释一起数，下面「恰好一处」当场变成两处 —— 08-08 写这条时就差点踩上。
    let code: Vec<&str> = src
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .collect();
    assert!(
        !code.is_empty(),
        "剥注释后一行不剩 —— 读法坏了，下面几条会零命中地绿"
    );

    let plain = code
        .iter()
        .filter(|l| l.contains("提交状态编得过") && !l.contains("Linux"))
        .count();
    assert_eq!(
        plain, 1,
        "无保留的成功结论「提交状态编得过」在可执行部分出现 {plain} 次（应为 1）。\n\
             ⚠ 出现 0 次 = 结论措辞被换掉了，本条其余几款都在空转；\n\
             出现 2 次以上 = 很可能降级那一支也在打同一句话 —— 那就等于没降级。"
    );

    assert!(
        code.iter()
            .any(|l| l.starts_with("skipped=") && l.contains("backend-win")),
        "跳过 `backend-win` 那一支没有把这件事**记进变量**（找不到 `skipped=…backend-win…`）。\n\
             ⚠ 只 `echo` 一行「skip」是不够的：结论行不读它，读门禁的人只看最后一行。\n\
             08-08 真路实测过这个状态：skip 打了，最后一行仍是「== 提交状态编得过 ==」、exit 0。"
    );
    assert!(
        code.iter().any(|l| l.contains("-n \"$skipped\"")),
        "结论没有分支到 `$skipped` 上 —— 那个变量记了也没人读，等于没记。"
    );
    let degraded = code.iter().filter(|l| l.contains("Linux 上编得过")).count();
    assert_eq!(
        degraded, 1,
        "降级结论（含「Linux 上编得过」）出现 {degraded} 次（应为 1）。\n\
             ★ 定框 E8 逐字写着：没有当次 Windows 证据时，结论只许写成「Linux 上全绿」。\n\
             而跨 target check 在**本机**只有这一处真跑（`ci.yml` 那条要 push，本仓不 push）\n\
             ⇒ 它一跳过，E8 的前提就整个没了，结论必须自己把这件事说出来。"
    );
}

/// workflow 里的一个顶层 job：键名、`steps:` 之前那几行（`timeout-minutes` · `env` 住那里）、切好的步骤。
struct WorkflowJob {
    id: String,
    head: String,
    steps: Vec<String>,
}

/// 一份 workflow（剔掉整行注释）按顶层 job 切开，每个 job 再切成步骤：
/// job 键是 `jobs:` 底下「两个空格 + 名字 + 冒号」；每一步从 `      - ` 起，到下一步或缩进退回步骤那一层之外为止。
fn workflow_jobs(rel: &str) -> (String, Vec<WorkflowJob>) {
    let raw = fs::read_to_string(crate::guard_support::repo_root().join(rel))
        .unwrap_or_else(|e| panic!("{rel} 读不到：{e}"));
    let live = guard_core::strip_hash_comment_lines(&raw);
    let mut jobs: Vec<WorkflowJob> = Vec::new();
    let mut in_jobs = false;
    let mut in_steps = false;
    let mut cur: Option<Vec<&str>> = None;
    let flush = |cur: &mut Option<Vec<&str>>, jobs: &mut Vec<WorkflowJob>| {
        if let (Some(s), Some(j)) = (cur.take(), jobs.last_mut()) {
            j.steps.push(s.join("\n"));
        }
    };
    for line in live.lines() {
        let line = line.trim_end();
        let indent = line.len() - line.trim_start().len();
        if line.is_empty() {
            continue;
        }
        if indent == 0 {
            flush(&mut cur, &mut jobs);
            in_jobs = line == "jobs:";
            continue;
        }
        if !in_jobs {
            continue;
        }
        if indent == 2 && line.ends_with(':') {
            flush(&mut cur, &mut jobs);
            jobs.push(WorkflowJob {
                id: line.trim().trim_end_matches(':').to_string(),
                head: String::new(),
                steps: Vec::new(),
            });
            in_steps = false;
            continue;
        }
        let Some(job) = jobs.last_mut() else { continue };
        if line == "    steps:" {
            in_steps = true;
            continue;
        }
        if !in_steps {
            job.head.push_str(line);
            job.head.push('\n');
        } else if line.starts_with("      - ") {
            flush(&mut cur, &mut jobs);
            cur = Some(vec![line]);
        } else if indent < 8 {
            flush(&mut cur, &mut jobs);
        } else if let Some(s) = cur.as_mut() {
            s.push(line);
        }
    }
    flush(&mut cur, &mut jobs);
    (live, jobs)
}

/// 一段 YAML 里 `<缩进><键>: <数>` 那一行的数（行尾注释不算）。
fn yaml_minutes(block: &str, indent_key: &str) -> Option<u64> {
    block.lines().find_map(|l| {
        l.strip_prefix(indent_key)?
            .split('#')
            .next()?
            .trim()
            .parse()
            .ok()
    })
}

const APT_HELPER: &str = "tests/scripts/apt-install.sh";

/// 一步里调 apt-install 时脚本名后面那串参数（算缓存键那一步去掉 `--cache-key`）；不调它就是 `None`。
fn apt_helper_args(step: &str) -> Option<(bool, String)> {
    let at = step.find(APT_HELPER)?;
    let rest = step[at + APT_HELPER.len()..]
        .lines()
        .next()
        .unwrap_or("")
        .trim();
    let mut toks = rest.split_whitespace().peekable();
    let key_mode = toks.peek() == Some(&"--cache-key");
    if key_mode {
        toks.next();
    }
    let args: Vec<&str> = toks.take_while(|t| !t.starts_with('>')).collect();
    Some((key_mode, args.join(" ")))
}

/// apt-install 那份脚本（剔整行注释）与它的总时限（秒，`budget=` 那一行）。
fn apt_helper_code_and_budget() -> (String, u64) {
    let script = fs::read_to_string(crate::guard_support::repo_root().join(APT_HELPER))
        .unwrap_or_else(|e| panic!("{APT_HELPER} 读不到：{e}"));
    let code = guard_core::strip_hash_comment_lines(&script);
    let budget = code
        .lines()
        .find_map(|l| l.trim().strip_prefix("budget=")?.parse().ok())
        .unwrap_or_else(|| panic!("{APT_HELPER} 里找不到 `budget=<秒>`（一次调用的总时限）"));
    (code, budget)
}

/// CI 上装 apt 包：停住几十秒就换连接、很慢但在动的也给够时间装完、一次调用有总时限（10-07 两回 runner 上 apt 挂住，
/// 整个 job 等满 35 分钟被取消；10-08 镜像约 125 KB/s，25.5 MB 的包被每趟 200 秒的闸杀了三回）：
/// 两份 workflow 里不直接写 `apt-get`，装包一律经 `tests/scripts/apt-install.sh`（重试与超时的写法只这一处）；
/// 调它装包的每一步自带 `timeout-minutes`，比脚本的总时限长、比所在 job 的时限短；
/// 那份脚本里每条 `apt-get` 都套着 `timeout`、带 20 秒连接 / 读超时与 `Acquire::Retries=3`、最多 3 趟、
/// install 那一趟的闸是总时限里还剩的时间（不是固定的一小段）。
#[test]
fn every_apt_install_in_ci_sits_in_a_step_with_its_own_timeout() {
    let (code, budget) = apt_helper_code_and_budget();
    let mut total = 0;
    for rel in [".github/workflows/ci.yml", ".github/workflows/release.yml"] {
        let (live, jobs) = workflow_jobs(rel);
        let all_steps: Vec<&String> = jobs.iter().flat_map(|j| j.steps.iter()).collect();
        let raw_apt: Vec<&&String> = all_steps.iter().filter(|s| s.contains("apt-get")).collect();
        assert!(
            raw_apt.is_empty(),
            "{rel} 里有步骤直接写 apt-get（该经 {APT_HELPER}，重试与超时只写那一处）：\n{}",
            raw_apt
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n---\n")
        );
        let calls = all_steps.iter().filter(|s| s.contains(APT_HELPER)).count();
        assert!(
            calls > 0,
            "{rel} 里一步调 {APT_HELPER} 的都没切到（切步骤坏了？）"
        );
        assert_eq!(
            calls,
            live.matches(APT_HELPER).count(),
            "{rel}：调 {APT_HELPER} 的次数与切出来的步骤数对不上（一步调两次，或切步骤漏了）"
        );
        for job in &jobs {
            let job_limit = yaml_minutes(&job.head, "    timeout-minutes:");
            for s in &job.steps {
                let Some((false, _)) = apt_helper_args(s) else {
                    continue;
                };
                total += 1;
                let mins = yaml_minutes(s, "        timeout-minutes:").unwrap_or_else(|| {
                    panic!("{rel} · {} 里这一步装 apt 包却没有自己的 timeout-minutes（卡住会等满整个 job）：\n{s}", job.id)
                });
                assert!(
                    mins * 60 > budget + 30,
                    "{rel} · {}：装 apt 包那一步 timeout-minutes: {mins} 不比脚本总时限 {budget} 秒长\n\
                     （步骤的闸先落下，脚本来不及自己收尾报「几趟都没装成」）",
                    job.id
                );
                if let Some(limit) = job_limit {
                    assert!(
                        mins < limit,
                        "{rel} · {}：装 apt 包那一步 timeout-minutes: {mins} 不比 job 的 {limit} 短",
                        job.id
                    );
                }
            }
        }
    }
    assert!(
        total >= 4,
        "两份 workflow 里装 apt 包的步骤只切到 {total} 步"
    );

    let apt: Vec<&str> = code.lines().filter(|l| l.contains("apt-get")).collect();
    assert!(
        apt.len() >= 2,
        "{APT_HELPER} 里 update 与 install 至少两条 apt-get"
    );
    for l in &apt {
        let at = l.find("apt-get").expect("刚筛过");
        assert!(
            l[..at].contains("timeout "),
            "{APT_HELPER} 里这条 apt-get 没套 timeout：{l}"
        );
    }
    let install: Vec<&&str> = apt
        .iter()
        .filter(|l| guard_core::contains_word(l, "install -y"))
        .collect();
    assert_eq!(
        install.len(),
        1,
        "{APT_HELPER} 里应恰有一条 apt-get install：{install:?}"
    );
    assert!(
        install[0].contains("timeout -k 10 \"$left\""),
        "{APT_HELPER} 的 install 那一趟的闸该是总时限里还剩的秒数（`timeout -k 10 \"$left\"`），\n\
         不是固定的一小段 —— 10-08 镜像约 125 KB/s，固定 200 秒的闸把 25.5 MB 的包连杀三趟：{}",
        install[0]
    );
    assert!(
        code.contains("Acquire::Retries=3"),
        "{APT_HELPER} 没给 apt 带 Acquire::Retries=3"
    );
    assert!(
        guard_core::contains_word(&code, "Acquire::http::Timeout=20")
            && guard_core::contains_word(&code, "Acquire::https::Timeout=20"),
        "{APT_HELPER} 没给 apt 带 20 秒的连接 / 读超时（单个包停住会一直等，主线那趟 CI 一个 32 kB 的包停了 625 秒）"
    );
    assert!(code.contains("tries=3"), "{APT_HELPER} 的重试趟数不是 3");
    assert!(
        guard_core::contains_word(&code, "Dir::Cache::Archives="),
        "{APT_HELPER} 没把 .deb 下载目录改到当前用户写得动的地方（缓存接不住 /var/cache/apt/archives）"
    );
}

/// 每个调 apt-install 装包的 job，装之前都先用官方 `actions/cache` 把上回下好的 .deb 放回来（镜像慢时命中就不再下载）：
/// 前面有一步 `apt-install.sh --cache-key <同一串包>` 算键（带 `id:`），再一步 `actions/cache@v4`，
/// `path` / `key` / `restore-keys` 都取那一步的输出；算键那一步的包与装包那一步逐字一样（键跟着包清单走）。
#[test]
fn every_job_that_installs_apt_packages_restores_the_deb_cache_first() {
    let mut jobs_seen = 0;
    for rel in [".github/workflows/ci.yml", ".github/workflows/release.yml"] {
        let (_, jobs) = workflow_jobs(rel);
        for job in &jobs {
            let Some(first_install) = job
                .steps
                .iter()
                .position(|s| matches!(apt_helper_args(s), Some((false, _))))
            else {
                continue;
            };
            jobs_seen += 1;
            let ctx = format!("{rel} · {}", job.id);
            let before = &job.steps[..first_install];
            let key_at = before
                .iter()
                .position(|s| matches!(apt_helper_args(s), Some((true, _))))
                .unwrap_or_else(|| {
                    panic!("{ctx}：装 apt 包之前没有算缓存键那一步（`{APT_HELPER} --cache-key …`）")
                });
            let key_step = &before[key_at];
            let id = key_step
                .lines()
                .find_map(|l| {
                    l.trim()
                        .strip_prefix("- ")
                        .unwrap_or(l.trim())
                        .strip_prefix("id:")
                })
                .map(|v| v.trim().to_string())
                .unwrap_or_else(|| {
                    panic!("{ctx}：算缓存键那一步没有 `id:`（缓存那一步取不到它的输出）")
                });
            let cache = before[key_at + 1..]
                .iter()
                .find(|s| s.contains("uses: actions/cache@"))
                .unwrap_or_else(|| {
                    panic!("{ctx}：算完键到装包之间没有 `uses: actions/cache@…` 那一步")
                });
            assert!(
                cache.contains("uses: actions/cache@v4"),
                "{ctx}：缓存那一步该用官方 actions/cache@v4（存取一步到位、不引第三方）：\n{cache}"
            );
            for (field, out) in [("path", "dir"), ("key", "key"), ("restore-keys", "restore")] {
                let want = format!("{field}: ${{{{ steps.{id}.outputs.{out} }}}}");
                assert!(
                    cache.lines().any(|l| l.trim() == want),
                    "{ctx}：缓存那一步缺 `{want}`（目录与键只住脚本一处）：\n{cache}"
                );
            }
            let (_, key_pkgs) = apt_helper_args(key_step).expect("刚筛过");
            for s in &job.steps[first_install..] {
                if let Some((false, pkgs)) = apt_helper_args(s) {
                    assert_eq!(
                        pkgs, key_pkgs,
                        "{ctx}：装包那一步的包与算缓存键那一步的不一样（键不跟着清单走，缓存会装错样子）"
                    );
                }
            }
        }
    }
    assert!(
        jobs_seen >= 4,
        "两份 workflow 里装 apt 包的 job 只认出 {jobs_seen} 个"
    );
}
