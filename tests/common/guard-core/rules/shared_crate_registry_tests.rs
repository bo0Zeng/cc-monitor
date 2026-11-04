use super::ci_yaml;
use super::ci_yaml::job_block as ci_job_block;
use std::fs;
use std::path::PathBuf;

/// 壳那棵 workspace 的根（`src/frontend/shell/`）。
fn root() -> PathBuf {
    guard_core::addr::shell_root()
}

/// 共享 crate 的家 —— `<repo>/src/common/`。
fn common_dir() -> std::path::PathBuf {
    guard_core::addr::repo_src_root().join("common")
}

/// 从 workspace 根（`members` 数组的基准）到 [`common_dir`] 的相对前缀（带尾 `/`）。
/// 现算，不写死 —— 包根搬一级，这个前缀就跟着变。
fn member_prefix() -> String {
    let to: Vec<_> = common_dir()
        .components()
        .map(|c| c.as_os_str().to_owned())
        .collect();
    let from: Vec<_> = root()
        .components()
        .map(|c| c.as_os_str().to_owned())
        .collect();
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
    // 只认 `src/common/` 下一层的包：更深处的 `Cargo.toml` 不是共享 crate。
    let mut names: Vec<String> = guard_core::scan_tree!(&dir, &["toml"])
        .into_iter()
        .filter_map(|(toml, text)| {
            if toml.file_name()? != "Cargo.toml" || toml.parent()?.parent()? != dir.as_path() {
                return None;
            }
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
    //   （「语料变量上的裸匹配……needle 被撑大时照样绿」），第一版写成
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
}

/// ★★ **每个 path 依赖的 `Cargo.toml` 都必须已被 git 跟踪**。
///
/// 2026-08-04 真事故：`gate-core` 这条依赖**从没被提交过** ⇒ 提交状态的 `main` 连续约 20 轮编不过，
/// 而每一次「全绿」都来自工作树。本条跑在 `cargo test` 里、几十毫秒，在本机就把这一类钉住，不必等 CI 检出那一趟。
#[test]
fn every_path_dependency_is_actually_committed() {
    let toml = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml 读不到");
    // 只认内联表里的 `path = "…"`（`x = { path = "…" }`）：`[[bin]]` 那种目标声明里的 `path` 指入口文件，不是一棵 crate 树。
    // 解析住 `guard_core::inline_table_paths`。
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
            .current_dir(guard_core::addr::repo_root())
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
             ⇒ 别人（和 CI）检出这个提交会直接编不过，而你的工作树一切正常。"
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

/// ★★★ **Windows 信号的锚是 `runs-on:` 那一行，它自己也得有人守**。
///
/// # 为什么要单独守这一行
///
/// 上面那条守的是后端那几步；**更中心的是这一行** ——
/// 把 `rust` job 的 `runs-on` 从 `windows-latest` 改成 `ubuntu-latest`，
/// 没有本条时**全仓一条判据都不红**（变异实测：改完 `cargo test --lib` 照样全过）。
/// 而那个 job 是**生产平台唯一的编译与测试信号** —— 换掉 runner 等于把它整个交出去，
/// 且交出去之后所有门禁**依旧全绿**，比删掉一步隐蔽得多。
///
/// 别的读 `.github/workflows` 的判据（如
/// `local_backend_tests.rs::no_bundle_job_puts_a_second_local_backend_beside_the_exe` 读 `release.yml`）
/// **都不看 `runs-on`**；`windows-latest` 这个字面量在仓里其余命中全是散文注释。
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

    let pkg = std::fs::read_to_string(guard_core::addr::repo_root().join("package.json"))
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
    // ★ 抽取器自检：剥法坏了 ⇒ 下面会零命中地绿（正控：`test` 与 `test:f40` 两条必在）。
    assert!(
        ["test", "test:f40"]
            .iter()
            .all(|want| scripts.iter().any(|(n, _)| n == want)),
        "从 `package.json` 剥到的 {} 个 `test*` 脚本里缺 `test` 或 `test:f40` —— 剥法坏了",
        scripts.len()
    );

    let ci = ci_yaml::live_lines();
    let local_gate =
        std::fs::read_to_string(guard_core::addr::repo_root().join("tests/scripts/gate.sh"))
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
        let f40 =
            std::fs::read_to_string(guard_core::addr::repo_root().join("tests/e2e/f40-suite.sh"))
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
/// ⚠ 顺带一条方法论（变异抽样撞出来的）：**`#[cfg(windows)]` 里的变异在 Linux 上
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

/// **几条诚实边界压在同一个前提上：「CI 不会自己跑」——把这个前提钉住。**
///
/// 3w（release.yml 的版本 guard 不触发）· 3x（七条 `#[ignore]` 执行次数为零）·
/// 3y（monitor 的 Windows 面没有编译信号），三条的成立**都只因为一件事**：
/// workflow 只在 `push` / `pull_request` 上触发，而用户定了不再 push。
///
/// 谁加一个 `workflow_dispatch`（手点就能跑）或 `schedule`（定时跑），
/// 压在那个文件上的边界当天就该重判 —— 没有本条，它们会**继续以「已登记的诚实边界」的样子留着**：
/// 世界变了、文本一个字没动。
///
/// # 禁 ＋ 登记
///
/// `release.yml` 有 `workflow_dispatch`（验一次发版流水线不必真推 `v*` tag），裁过：
/// 依据是真跑的那一趟 CI（run `34861383050`），不是读配置推的 ——
/// **`3w` · `3y` 翻正**（那趟真跑了版本 guard、真编了 Windows 面）；
/// **`3x` 不翻正**（它压在 **`ci.yml`** 上，而 `ci.yml` 没有自启触发器）。
///
/// ⇒ 本条**不许整条删掉**：删了会把 `3x` 那一半的守卫一起砍掉。
/// 形状是：**`ci.yml` 一个自启触发器都不许有**；`release.yml` 上那一个是**裁过的**，
/// 进 `ADJUDICATED` 登记并带住址回指裁决记在哪。**谁再往 `ci.yml` 加，当场红。**
///
/// ⚠ 本条**不断言 CI 应该怎么触发**（那是用户的裁决）。它断言的是
/// **「自启触发器要么没有，要么有人裁过并留了住址」** —— 两者都不成立就红，逼人回来重判。
///
/// ⚠ **诚实边界，三条，别读宽**：
/// ① 登记表里那个住址本条**只验形状**（非空 ＋ 形如 `<文件>#<锚点>`），
///    **不去核它真的指得到**；
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
    const ADJUDICATED: &[(&str, &str, &str)] = &[(
        "release.yml",
        "workflow_dispatch",
        "tests/common/guard-core/rules/shared_crate_registry_tests.rs#STILL_RESTING_ON",
    )];
    /// 扫描面 ＋ **每个文件今天还压着哪几条边界**（红了要人去重判的就是这些）。
    /// 🔴 这一栏不是装饰：诊断里要说得出「你这一下动的是**谁**的前提」。
    const STILL_RESTING_ON: &[(&str, &str)] = &[
        (
            "ci.yml",
            "3x —— 七条 `#[ignore]` 的 e2e 触发路**只有 ci.yml 这一条**，\
                 `release.yml` 的自启触发器**不翻它**",
        ),
        (
            "release.yml",
            "3w / 3y —— 依据 run 34861383050 翻正；\
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
                 ⚠ 本条只验形状，不去核它真指得到。"
        );
    }

    let root = guard_core::addr::repo_root().to_path_buf();
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
        // ⚠ **两种 YAML 写法都要认**：**块写法**（行首是 `push:` / `workflow_dispatch:`）
        // 与**序列写法** `on: [push, pull_request, workflow_dispatch]`。只认块写法的话，
        // 序列写法会红在抽取器自检上、诊断说「段界切错了」—— 照它去查的人会去修切块逻辑，
        // 而真实事件是 **CI 从此可以手点运行**、诚实边界的前提当场消失。
        // 「讲错成因的红灯比不红更坏」⇒ 按**词**匹配。
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

/// workflow 里的一个顶层 job：键名、`steps:` 之前那几行（`timeout-minutes` · `env` 住那里）、切好的步骤。
struct WorkflowJob {
    id: String,
    head: String,
    steps: Vec<String>,
}

/// 一份 workflow（剔掉整行注释）按顶层 job 切开，每个 job 再切成步骤：
/// job 键是 `jobs:` 底下「两个空格 + 名字 + 冒号」；每一步从 `      - ` 起，到下一步或缩进退回步骤那一层之外为止。
fn workflow_jobs(rel: &str) -> (String, Vec<WorkflowJob>) {
    let raw = fs::read_to_string(guard_core::addr::repo_root().join(rel))
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
    let script = fs::read_to_string(guard_core::addr::repo_root().join(APT_HELPER))
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

/// **候选分支 `next`：推 main 之前 CI 先在它上面跑同一套，绿了 main 才快进过去**。
///
/// 推送脚本（仓外）把合并好的分支强推到 `origin/next`、等这一趟 CI 全绿才 `merge --ff-only` 并推 main；
/// main 于是只收 CI 绿过的提交。这条钉住那条路在仓里的一半：
/// ① `ci.yml` 的 push 触发同时认 `main` 与 `next`（少了 `next` ⇒ 推候选分支什么都不跑，脚本等到超时）；
/// ② `ci.yml` 里没有一条按分支 / 事件分叉的条件（`if:` · `github.ref` · `github.event_name` …）——
///    有了它，`next` 上跑的就不再是 main 上那一套，「候选绿 ⇒ main 绿」不成立；
/// ③ 候选分支上不打标签、不发版：`ci.yml` 不写仓（没有 `contents: write` · `git tag` · `git push` · `gh release`），
///    `release.yml` 的 push 触发只认标签、不认任何分支。
#[test]
fn ci_runs_the_same_jobs_on_the_candidate_branch_and_never_releases_from_it() {
    let ci = ci_yaml::live_lines();
    let trigger: Vec<&str> = ci.lines().take_while(|l| l.trim_end() != "jobs:").collect();
    let push_branches: Vec<String> = trigger
        .iter()
        .skip_while(|l| l.trim_end() != "  push:")
        .skip(1)
        .take_while(|l| l.len() - l.trim_start().len() >= 4)
        .filter_map(|l| l.trim().strip_prefix("branches:"))
        .flat_map(|v| {
            v.trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(|s| s.trim().trim_matches(|c| c == '\'' || c == '"').to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    for want in ["main", "next"] {
        assert!(
            push_branches.iter().any(|b| b == want),
            "`ci.yml` 的 `on.push.branches` 里没有 `{want}`（抽到 {push_branches:?}）—— \
             候选分支 `next` 是推 main 之前让 CI 先跑的那一趟；少了它，推送脚本等不到任何 run"
        );
    }
    for needle in [
        "if:",
        "github.ref",
        "github.event_name",
        "github.head_ref",
        "github.base_ref",
    ] {
        assert!(
            !guard_core::contains_word(&ci, needle),
            "`ci.yml` 里出现了（未注释的）`{needle}` —— 按分支 / 事件分叉之后，`next` 上跑的就不是 main 上那一套，\
             「候选分支绿了才快进 main」那条路失去意义"
        );
    }
    for needle in [
        "contents: write",
        "git tag",
        "git push",
        "gh release",
        "action-gh-release",
    ] {
        assert!(
            !guard_core::contains_word(&ci, needle),
            "`ci.yml` 里出现了（未注释的）`{needle}` —— CI 在 main 与候选分支 `next` 上跑同一套，它不许写仓、打标签、发版（发版只走 release.yml 的标签触发）"
        );
    }
    let rel = guard_core::strip_hash_comment_lines(
        &fs::read_to_string(guard_core::addr::repo_root().join(".github/workflows/release.yml"))
            .expect("release.yml 读不到"),
    );
    let rel_push: Vec<&str> = rel
        .lines()
        .skip_while(|l| l.trim_end() != "  push:")
        .skip(1)
        .take_while(|l| l.len() - l.trim_start().len() >= 4)
        .collect();
    assert!(
        rel_push.iter().any(|l| l.trim().starts_with("tags:")),
        "`release.yml` 的 push 触发里抽不到 `tags:`（抽到 {rel_push:?}）—— 抽取器坏了或触发器形状变了"
    );
    assert!(
        !rel_push.iter().any(|l| l.trim().starts_with("branches")),
        "`release.yml` 的 push 触发认了分支（{rel_push:?}）—— 推候选分支 / main 就会起发版；发版只许由标签触发"
    );
}

/// ★ `lockfile_conflict_guard` 那套论证的**前提**：跨 target check 确实跑在 `src/backend` 下。
///
/// 这个 `working-directory` 一改，「两份 lock 编的不是同一棵树」这句话就不成立了 ——
/// 那时该回来重判整条，而不是留着一条论证已经落空的判据。
///
/// ⚠⚠ **08-07 订正：本条原本钉的是两个「字符串各自存在」，不是它们的关系。**
/// 变异实测两刀：① 把跨 target check 整步搬进 `rust` job（它改走 monitor 的 lock）；
/// ② 把 backend job 的 `working-directory` 改成 `src/frontend/shell`、同时把那一行原样挪到别的 job。
/// 两刀都让那套论证**反过来**，而本条**一声不吭**（三条全绿）。
///
/// ★ 两刀确实各有别的判据红了 —— 但读它们的诊断：说的是
/// 「切出来的块里没有 `working-directory: src/backend` —— **切错 job 了，本条会零命中地绿**」，
/// 那是当时另一条判据（钉 backend job 四步的那条，后来随四步改调门禁删了）的**抽取器自检**在说话。
/// 照它去修，人会去查切块逻辑，而真实事件是 **backend job 换了工作目录**。
/// ⇒ **「有别的判据接住」不等于「有人把这件事讲对了」** —— 本条才是该讲这句话的那条。
///
/// 改法：钉**关系** —— 两件事必须落在**同一个 job 块**里。切块用
/// `ci_yaml`（E3：`ci.yml` 的读取与切块只有一个家）。
#[test]
fn the_cross_target_check_still_runs_under_the_backend_lock() {
    const CHECK: &str = "cargo check --workspace --all-targets --target x86_64-pc-windows-msvc";
    // ⚠ **行锚定**，不能用 `contains` 裸匹配：`src/backend` 是
    // `src/backend-X` 的**前缀** —— 变异实测过，裸 `contains` 照样绿。
    const WD: &str = "working-directory: src/backend";

    let block = ci_yaml::job_block("backend");
    // 抽取器自检：切不出块时下面两条会零命中地绿。
    assert!(
        block.lines().count() >= 10,
        "从 `ci.yml` 切 `backend:` job 只得到 {} 行 —— job 名或缩进变了，本条会零命中地绿",
        block.lines().count()
    );
    assert!(
        block.lines().any(|l| l.trim() == WD),
        "`backend:` job 里没有 `{WD}` 了 —— 它可能被改了值、也可能被挪到了别的 job。\n\
             ⚠ **别只看「这行字在不在 ci.yml 里」** —— 它在别处照样在，而那套论证要的是\n\
             「**跨 target check 所在的那个 job** 跑在后端的 lock 下」。\n\
             这个前提一没，`lockfile_conflict_guard_tests.rs::the_two_lockfiles_have_no_real_version_conflict` 整套论证就落空，\n\
             该回来重判整条，而不是留着一条论证已经落空的判据。"
    );
    assert!(
        block.contains(CHECK),
        "跨 target Windows check 不在 `backend:` job 里了（它是 `ci.yml` 自称的\n\
             「平台线唯一真判据」）。要么它被删了（那是个更大的问题），\n\
             要么它被搬进了别的 job —— 而别的 job 的 `working-directory` 不是\n\
             `src/backend` ⇒ 它改走 **monitor 的 lock**，那套论证反过来。\n\
             ⚠ 变异实测过：把它搬出 `backend:` job，本条红。"
    );
}
