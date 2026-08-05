//! U8c-1：把「新增共享 crate 时别漏跑」从**散文**变成机检。
//!
//! ⚠⚠ **G2（2026-08-04）换了机制，本模块整体改写** —— 原来的形态是「每个 crate 必须在
//! `ci.yml` 里出现在 test / fmt / clippy **三处**」。那条纪律存在的唯一原因是
//! **`src-tauri/Cargo.toml` 当时没有 `[workspace]` 表**：六个 crate 只是 path 依赖，
//! `--all` 覆不到，只能一个个手工列。
//! **现在它们是真 workspace member**，`cargo fmt --all` / `--workspace` 自动覆盖 ⇒
//! 「补三处」这条纪律**消失了**，取而代之的是「**必须在 members 里**」。
//! ⇒ 判据跟着换靶：不再数 CI 步骤，改钉 `[workspace] members`。
//! ★ 这两条**不是被删的**（铁律 13：删判据前先证明它恒绿）——它们是**被改写成继任者**的：
//! 同一个失效模式（「静默少跑」），换了一个载体。
//!
//! # 为什么这条值得一个判据
//!
//! `ci.yml` 里那句纪律已经被违反过**两次**，而且都是事后补账发现的：
//! `branch-core` 当初漏了 fmt/clippy；`usage-core`/`acct-core` 三样**全漏**、漏了两轮
//! ——那段补账注释自己写着「它们的测试在 CI 里等于不存在」。
//!
//! 违反它不会红，只会**静默少跑**。这正是本工作区在治的那个病的形状。
//!
//! # 判据形态
//!
//! 遍历 `crates/*/Cargo.toml` 拿包名（**不是手写清单** —— 手写清单本身就是下一个漂移源），
//! 然后要求每个包名在 `ci.yml` 里同时出现在 `cargo test -p <名>`、
//! `cargo fmt --check --manifest-path crates/<名>/Cargo.toml`、
//! `cargo clippy --manifest-path crates/<名>/Cargo.toml` 三处。
//!
//! ⚠ **`vendor/` 下的不算** —— 那是 vendored 第三方（`code-picture-core`），
//! 有自己的一套（`ci.yml` 单独一步），不受本约定管。

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    /// 本文件所在 crate 的根（`src-tauri/`）。
    fn root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
    }

    /// `crates/*/Cargo.toml` 里的包名。
    fn shared_crate_names() -> Vec<String> {
        let dir = root().join("crates");
        let mut names: Vec<String> = fs::read_dir(&dir)
            .expect("crates/ 读不到")
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

    fn ci_yml() -> String {
        fs::read_to_string(root().parent().unwrap().join(".github/workflows/ci.yml"))
            .expect("ci.yml 读不到")
    }

    /// `ci.yml` 里**真的会跑**的那些行 —— 注释行剔掉。
    ///
    /// ⚠ 实测（2026-08-03 复盘 P3）：本模块此前直接对整份 `ci.yml` 做 `contains`，
    /// 把 `cargo test -p shell-quote-core` **注释掉**之后守卫**照旧全绿**（3 passed）。
    /// 而「注释掉一步」正是本模块要防的那个病的最省事形态 —— 它连 diff 都很小。
    /// 顺带：文件头那段散文注释里也写着这套纪律的命令形态，散文不该当证据。
    fn ci_live_lines() -> String {
        ci_yml()
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// ★ 抽取器自检：`crates/` 下一个都没抽到时，下面那条会零命中零失败地绿。
    #[test]
    fn the_crate_scan_actually_finds_crates() {
        let n = shared_crate_names().len();
        // 地板 4 → 5 → **6**（F12 棘：实测 6 —— acct-core / branch-core / **gate-core** /
        // guard-core / shell-quote-core / usage-core）。差一个的地板意味着「少抽到一个 crate」
        // 不会红 —— 而少抽到的那个恰好就是没人管 CI 的那个。删共享 crate 时来改这个数，是刻意的摩擦。
        //
        // ⚠ **它落后过一次，而且正好落后一个**：F03 新增 `gate-core` 时补了 CI 三样、
        // 却没回来棘这个数 ⇒ 从 F03 到 F12 之间，「抽取器少认一个包」这件事**不会红**，
        // 而那正是上面这段注释逐字警告的场景。是 Phase G 的 `/full-audit` 把它逮出来的。
        // ⇒ 一般化：**「新增一个 X 要补 N 处」的清单里，必须包含「回来棘那条自检的地板」。**
        assert!(
            n >= 6,
            "只从 crates/*/Cargo.toml 抽到 {n} 个包名（F12 实测应为 6）—— 抽取器坏了，\
             下面那条「三样都在 CI 里」会零命中零失败地绿"
        );
    }

    /// ★ 每个共享 crate 都必须在 `src-tauri/Cargo.toml` 的 `[workspace] members` 里。
    ///
    /// 违反它**不会红，只会静默少跑** —— `cargo test --workspace` 覆不到非成员，
    /// 而那个 crate 的测试就此从门禁里**消失**（不是失败，是不存在）。
    /// 这正是 G2 之前 `branch-core`（漏 fmt/clippy）与 `usage-core`/`acct-core`
    /// （三样全漏、漏了两轮）那两次事故的形状，只是载体从「CI 步骤」变成了「members 列表」。
    ///
    /// ⚠ **必须先切出 `[workspace]` 段再找** —— 直接全文 `contains("crates/gate-core")`
    /// 会匹配到**依赖声明行**（`gate-core = { path = "crates/gate-core" }`），
    /// 于是「从 members 里删掉一个」这种变异**照样绿**（G2 实测，变异 V1 第一版就这么活的）。
    /// ★ 判据覆盖面**第④格·性质面**：它比的必须是它声称的那个性质。
    #[test]
    fn every_shared_crate_is_a_workspace_member() {
        let toml = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml 读不到");
        assert!(
            toml.contains("[workspace]"),
            "`src-tauri/Cargo.toml` 的 `[workspace]` 没了 —— 六个共享 crate 会退回\n\
             「path 依赖但非成员」，`--workspace` 从此静默少测它们"
        );
        let beg = toml.find("[workspace]").expect("上面刚断言过");
        let end = toml[beg + 1..]
            .find("\n[")
            .map(|k| beg + 1 + k)
            .unwrap_or(toml.len());
        let ws = &toml[beg..end];
        // 段界自检：切出来的必须真是那一段（含 members、不含 dependencies、不过长）。
        assert!(
            ws.contains("members") && !ws.contains("[dependencies]") && ws.len() < 2_000,
            "`[workspace]` 段界切错了（{} 字节）—— 本条会在全文里瞎找",
            ws.len()
        );
        let missing: Vec<String> = shared_crate_names()
            .into_iter()
            .filter(|n| !ws.contains(&format!("\"crates/{n}\"")))
            .collect();
        assert!(
            missing.is_empty(),
            "这些共享 crate 不在 `[workspace] members` 里：{missing:?}\n\
             ⇒ `cargo test --workspace` 覆不到它们，测试会**静默地**从门禁里消失。"
        );
        // vendor 那条 exclude 不许没掉：没了它，vendor 的 25 条会掺进 `--workspace` 的读数。
        // ⚠ 它**挡不住** vendor 成为成员（`exclude` 对成员的 path 依赖不生效，G2 实测）——
        //    真正把它挡在外面的是 CI 命令行上的 `--exclude`，本条钉的是「这个意图还在」。
        assert!(
            ws.contains("exclude = [\"vendor/code-picture-core\"]"),
            "`[workspace] exclude` 里的 vendor 那条没了"
        );
    }

    /// ★★ **每个 path 依赖的 `Cargo.toml` 都必须已被 git 跟踪**〔G2-3〕。
    ///
    /// # 它是那个真事故的**结构性**修法
    ///
    /// 事故原文（`scripts/verify-committed-state.sh` 头注）：`gate-core` 这条依赖
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
        // 抽 `path = "…"` 的值。
        let mut paths: Vec<String> = Vec::new();
        for (i, _) in toml.match_indices("path = \"") {
            let rest = &toml[i + "path = \"".len()..];
            if let Some(end) = rest.find('"') {
                paths.push(rest[..end].to_string());
            }
        }
        // 抽取器自检：至少要抽到那 6 个共享 crate + vendor = 7 条（按实测）。
        assert!(
            paths.len() >= 7,
            "只从 Cargo.toml 抽到 {} 条 path 依赖（应 ≥7）—— 抽取坏了，本条会零命中地绿：{paths:?}",
            paths.len()
        );
        let mut untracked = Vec::new();
        for rel in &paths {
            let manifest = format!("src-tauri/{rel}/Cargo.toml");
            let out = std::process::Command::new("git")
                .args(["ls-files", "--error-unmatch", "--", &manifest])
                .current_dir(root().parent().expect("仓根"))
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
             这正是 `scripts/verify-committed-state.sh` 头注记的那个真事故\n\
             （`gate-core` 从没被提交，committed main 连续约 20 轮编不过）。"
        );
    }

    /// ★ CI 必须真的在用那三条收敛后的命令（不是把它们注释掉了）。
    ///
    /// ⚠ 用 [`ci_live_lines`]（剔注释）—— 复盘 P3 实测过：直接对整份 `ci.yml` 做
    /// `contains`，把某一步**注释掉**之后守卫照旧全绿，而「注释掉一步」正是最省事的错法。
    #[test]
    fn ci_actually_runs_the_three_converged_commands() {
        let ci = ci_live_lines();
        for needle in [
            "cargo fmt --all --check",
            "cargo clippy --workspace --exclude code-picture-core --all-targets",
            "cargo test --workspace --exclude code-picture-core",
            // vendor 仍单独一步（红线：别误伤 vendor，它不进 `--workspace`）。
            "cargo test -p code-picture-core",
        ] {
            assert!(
                ci.contains(needle),
                "`ci.yml` 里找不到（未注释的）`{needle}` —— 收敛后的门禁少了一条"
            );
        }
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
        for line in ws.lines() {
            let t = line.trim().trim_matches(',').trim_matches('"');
            let Some(name) = t.strip_prefix("crates/") else {
                continue;
            };
            let name = name.trim_matches('"');
            scanned += 1;
            if !root()
                .join("crates")
                .join(name)
                .join("Cargo.toml")
                .is_file()
            {
                ghosts.push(name.to_string());
            }
        }
        // 抽取器自检：members 里应有 **6** 条 `crates/…`（与 `the_crate_scan_actually_finds_crates`
        // 的地板同源）。扫不到就是取名方式与 Cargo.toml 的写法分家了。
        assert!(
            scanned >= 6,
            "只从 `[workspace] members` 扫到 {scanned} 条 `crates/…`（应 ≥6）—— \
             要么真少了，要么本抽取器与 Cargo.toml 的写法分家了。后者会让下面那条零命中变绿"
        );
        assert!(
            ghosts.is_empty(),
            "`[workspace] members` 里列了不存在的 crate：{ghosts:?}"
        );
    }
}
