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

/// ★ `ci.yml` 的**读取与切块只有一个家**〔audit-0805 08-07，定框 E3〕。
///
/// 抽出来的原因是实测撞见的：`lockfile_conflict_guard` 的前提判据要问的是
/// 「跨 target check **和** `working-directory: remote-daemon-proto` 在不在同一个 job」，
/// 而它当时只能在整份文件里各找一次字符串 ⇒ 两件事各自成立、关系没人钉。
/// 要钉那个关系就得会切 job 块，而切块的实现当时住在本文件的 `mod tests` 里、别人够不着 ——
/// **判据之间借不到量具，就会各写一份近似的**，那正是 E3 要防的。
#[cfg(test)]
pub(crate) mod ci_yaml {
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级 = 仓根")
            .to_path_buf()
    }

    pub(crate) fn yml() -> String {
        std::fs::read_to_string(repo_root().join(".github/workflows/ci.yml"))
            .expect("ci.yml 读不到")
    }

    /// `ci.yml` 的**有效行**（整行注释剔掉）。
    ///
    /// ⚠ 住在这里而不是各判据自己写一遍：`ci.yml` 怎么读、怎么剔注释是**一个事实**（E3）。
    /// 08-08 `e2e_gate_registry` 要用同一件事时，`structural_scan` 的「剥注释转换器必须登记」
    /// 当场逮住了那份新拷贝 —— 于是把原来住在 `mod tests` 里的 `ci_live_lines` 搬到这里，
    /// **是收口不是新增**（共享原语 `guard_core::strip_comment_lines` 接不住这一半：
    /// 它认的是 `//` / `/*` 那套 Rust/TS 形态，而 YAML 的注释是 `#`）。
    pub(crate) fn live_lines() -> String {
        guard_core::strip_hash_comment_lines(&yml())
    }

    /// 切出某个顶层 job 的行范围（剔注释）。
    ///
    /// 顶层 job 键的形状是**两个空格 + 名字 + 冒号**（`  daemon:`），下一个同缩进的键即块尾。
    /// ⚠ 用它而不是整份 `contains` 的理由见 `ci_actually_runs_the_daemon_four_steps`：
    /// 有的步骤命令是别的 job 里某条命令的**子串**，整份查会被盖住。
    pub(crate) fn job_block(name: &str) -> String {
        let head = format!("  {name}:");
        let yml = yml();
        let mut out = Vec::new();
        let mut inside = false;
        for line in yml.lines() {
            if line == head {
                inside = true;
                continue;
            }
            if inside {
                // 同缩进的下一个键 = 块尾（两空格开头、非空白第三字符、以冒号结尾）。
                let is_next_key = line.len() > 2
                    && line.starts_with("  ")
                    && !line.as_bytes()[2].is_ascii_whitespace()
                    && line.trim_end().ends_with(':')
                    && !line.trim_start().starts_with('#');
                if is_next_key {
                    break;
                }
                if !line.trim_start().starts_with('#') {
                    out.push(line);
                }
            }
        }
        out.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::ci_yaml;
    use super::ci_yaml::{job_block as ci_job_block, yml as ci_yml};
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

    /// `ci.yml` 里**真的会跑**的那些行 —— 注释行剔掉。
    ///
    /// ⚠ 实测（2026-08-03 复盘 P3）：本模块此前直接对整份 `ci.yml` 做 `contains`，
    /// 把 `cargo test -p shell-quote-core` **注释掉**之后守卫**照旧全绿**（3 passed）。
    /// 而「注释掉一步」正是本模块要防的那个病的最省事形态 —— 它连 diff 都很小。
    /// 顺带：文件头那段散文注释里也写着这套纪律的命令形态，散文不该当证据。
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
    /// ⚠ 用 [`ci_yaml::live_lines`]（剔注释）—— 复盘 P3 实测过：直接对整份 `ci.yml` 做
    /// `contains`，把某一步**注释掉**之后守卫照旧全绿，而「注释掉一步」正是最省事的错法。
    #[test]
    fn ci_actually_runs_the_three_converged_commands() {
        let ci = ci_yaml::live_lines();
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

    /// ★★ **daemon job 那四步也必须真的在跑**〔audit-0805 F01〕。
    ///
    /// # 为什么单开一条（而不是往上面那条的 needle 表里加）
    ///
    /// 上面那条钉的四条 needle **全部命中 monitor job**，与 daemon job 的四步**一条都不重叠**。
    /// 实测（`audit-0805` 的只读核实）：当时全仓读 `.github/workflows` 的**只有两处**
    /// （本文件 + `backend/control/local_backend.rs:719` 读 `release.yml`；
    /// ⚠ **08-08 起是三处** —— `sftp.rs` 新增了「发版流水线要为每个 arch 备料」那条，
    /// 这句话记的是**建本条当天**的度量面，别当成今天的事实），
    /// `actionlint` / `yamllint` **全仓零命中** ⇒ **把 `cargo check --target …` 那一步注释掉，
    /// 今天全仓门禁一条都不会红。** 而 `ci.yml` 在那一步上方逐字写着
    /// 「**§1.1 第一条解耦线（平台线）的唯一真判据**」——
    /// 一条自称唯一真判据的步骤，自己没有任何东西守着。
    ///
    /// # 为什么按 job 块切 + 只看 `run:` 行 + **精确相等**
    ///
    /// 这条判据自己被变异抓过**两次**，每次都是同一族的「匹配到了别的地方」：
    ///
    /// 1. **整份 `contains` 会被别的 job 的子串盖住**：daemon 那步是**裸** `cargo test`，
    ///    而它是 monitor 那条 `cargo test --workspace --exclude code-picture-core` 的子串
    ///    ⇒ 注释掉 daemon 那步，整份 `contains("cargo test")` 照样命中（实测全文件 10 处命中）。
    ///    ⇒ 先切 job 块。
    /// 2. ★ **切了块还不够：needle 会匹配到步骤名那一行。** 第一版切了块、但用
    ///    `block.contains("cargo test")`，而块里有 `- name: cargo test` ⇒
    ///    **把 `run: cargo test` 注释掉，判据照样绿**（变异实测落地了、判据没红）。
    ///    ⇒ 只收集 `run:` 行，且**精确相等**而不是 `contains`。
    ///
    /// 一般化：**判据要钉的是那条真的会被执行的行，不是恰好长得像它的那些字**。
    #[test]
    fn ci_actually_runs_the_daemon_four_steps() {
        let block = ci_job_block("daemon");
        // 抽取器自检 ①：切不出块 / 切错块时，下面四条会零命中地绿。
        assert!(
            block.lines().count() >= 10,
            "从 ci.yml 切 `daemon:` job 只得到 {} 行 —— job 名或缩进变了，本条会零命中地绿",
            block.lines().count()
        );
        assert!(
            block.contains("working-directory: remote-daemon-proto"),
            "切出来的块里没有 `working-directory: remote-daemon-proto` —— 切错 job 了，本条会零命中地绿"
        );
        // 只取真会被执行的那些行（`run: <单行命令>`），并剥成纯命令。
        let runs: Vec<&str> = block
            .lines()
            .map(str::trim)
            .filter_map(|l| l.strip_prefix("run: "))
            .map(str::trim)
            .collect();
        // 抽取器自检 ②：**只挡「一条都没抽到」**。
        // ⚠ Phase D 审计抓到的：这里原本写 `runs.len() >= 4`，于是「删掉一步」这个
        //    最省事的错法**先撞上自检**、拿到的是「抽取器坏了」这条**误导性诊断**，
        //    而下面那段精心写的「删掉等于把 Windows 编译信号交出去」在删除场景下**不可达**。
        //    计划里四条变异全部落进了这个坑 —— 它们确实红了，但**红的理由是错的**。
        //    ⇒ 自检只管「零命中」，「少了一条」交给下面的正文断言去说。
        assert!(
            !runs.is_empty(),
            "从 `daemon:` job 一条 `run:` 行都没抽到 —— 抽取器坏了，本条会零命中地绿"
        );
        for want in [
            "cargo fmt --check",
            // ★ 平台线的唯一真判据。`--all-targets` 是其中一半：不加只编生产段，
            //   而 U4a 实测 12 个错里有 1 个在测试段（`libc::getuid`）。
            "cargo check --all-targets --target x86_64-pc-windows-msvc",
            "cargo clippy --all-targets",
            "cargo test",
        ] {
            assert!(
                runs.contains(&want),
                "`ci.yml` 的 `daemon:` job 里没有（未注释的）`run: {want}`。\n\
                 今天抽到的 run 行：{runs:?}\n\
                 两种可能，看上面那行就能分辨：\n\
                 ① 这一步真的被删/被注释/被改了；\n\
                 ② 它改成了本抽取器不认的写法 —— **只认单行 `run: <命令>`**，\n\
                    多行 `run: |` 与裸 `- run: <命令>` 都抽不到（方向 fail-safe：假红不假绿）。\n\
                 ⚠ 若涉及那条跨 target check：它是 `ci.yml` 自己写的\n\
                 「§1.1 第一条解耦线（平台线）的唯一真判据」，删掉等于把 Windows 编译信号交出去。"
            );
        }
    }

    /// ★★★ **Windows 信号的锚是 `runs-on:` 那一行，它自己也得有人守**〔audit-0805 F01 的 Phase D〕。
    ///
    /// # 这条是审计打脸打出来的
    ///
    /// 上面那条刚给 daemon 四步补完守卫，Phase D 立刻指出：**更中心的那一行仍然零守卫** ——
    /// 把 `rust` job 的 `runs-on` 从 `windows-latest` 改成 `ubuntu-latest`，
    /// **全仓一条判据都不红**（审计变异实测：改完 `cargo test --lib` 仍 888 passed / 0 failed）。
    /// 而那个 job 是**生产平台唯一的编译与测试信号** —— 换掉 runner 等于把它整个交出去，
    /// 且交出去之后所有门禁**依旧全绿**，比删掉一步隐蔽得多。
    ///
    /// 全仓读 `.github/workflows` 的只有两处（本文件 + `local_backend.rs:719` 读 `release.yml`），（08-08 起三处，见上一条的订正）
    /// **两处都不看 `runs-on`**；`windows-latest` 这个字面量在仓里其余命中全是散文注释。
    ///
    /// ⚠ 本条**不管** daemon job 在哪跑（它在 ubuntu 上跨 target check，那是刻意的、
    /// `ci.yml:159-161` 有论证）—— 只钉「那个真跑 Windows 的 job 还在 Windows 上跑」。
    #[test]
    fn the_only_windows_signal_still_runs_on_a_windows_runner() {
        let block = ci_job_block("rust");
        // 抽取器自检：切不出块就零命中地绿。
        assert!(
            block.lines().count() >= 10,
            "从 ci.yml 切 `rust:` job 只得到 {} 行 —— job 名或缩进变了，本条会零命中地绿",
            block.lines().count()
        );
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
             真要换平台：先在 `audit-0805/ROADMAP §5` 写清「此后没有 Windows 证据」再改这里。",
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

    /// 〔audit-0805 08-06〕**`ci.yml` 的每一个 `run:` 步骤都要有归属**：
    /// 要么本地必跑，要么写清「结构上为什么跑不了」。
    ///
    /// **为什么建它**（实测，不是设想）：Windows 信号定格后 72 个提交没有任何一次 CI 执行，
    /// 而本地那一路的门禁命令**只有 `cargo test`**。08-06 第一次把 `cargo fmt --all --check`
    /// 补进去，**两侧当场都红**（13 个文件，`blame` 落在十来个不同提交上）——
    /// 也就是说 CI 的**第一个 Rust 步骤**已经红了很久，而每一轮的结论都写着「全绿」。
    ///
    /// 病根不是「漏跑一条命令」，是**本地门禁的度量面比 CI 小，而小了多少没人数过**。
    /// 上面那几条 `ci_actually_runs_*` 守的是反方向（CI 里别把步骤悄悄删了）；
    /// 本条守的是这一侧：**CI 里有而本地没数过的步骤，一步都不许有。**
    ///
    /// ⇒ 新增 / 改名一个 CI 步骤就会红，直到有人回答「本地跑不跑它」。
    ///
    /// ⚠⚠ **08-07：本条自己也犯了它要防的那个病。** 人群原本是「带 `- name:` 的 `run:` 步骤」——
    /// 按**怎么写的**取，而 GitHub Actions 的步骤不要求有名字。51 条步骤里它只看得见 47 条。
    /// 隐形的四条中三条是 `- run: npm ci`（环境步骤，无害），**第四条是 `- run: npx tsc --noEmit`**
    /// —— 一条**真门禁命令**，一直在 CI 里跑、本地也一直在跑，但**从来没被本条数过**。
    /// 也就是说：本条自陈守的是「本地度量面比 CI 小了多少」，而它自己的度量面就小了一块。
    /// ⇒ 人群改按「它是不是 `steps:` 里的一条 `run:`」取，无名步骤用命令首行当标识。
    #[test]
    fn every_ci_run_step_is_classified_as_local_or_unrunnable() {
        /// 整个 job 结构上跑不了 —— 理由**逐 job 一条**，且下面有前提触发器盯着它别过期。
        ///
        /// ⚠ **08-06 订正：这条豁免曾经过宽。** 它当初写成「整个 job 跑不了」，
        /// 而实测（用一个只会报错的 `tmux` 桩遮住 PATH，谁碰谁当场失败）发现
        /// 这两个 job 里有**四套根本不碰 tmux**、在本机跑得通且全过：
        /// `ccm-cli`(53) · `ccm-contract-parity`(45) · `ccm-print-parity`(12) · `daemon-fork`(10)
        /// —— 合计 **120 条断言**，此前被我按 job 一刀切成「结构上跑不了」。
        /// 那四套的名字登记在 [`LOCALLY_RUNNABLE`]，本地门禁要跑它们。
        /// ⇒ 教训：**豁免的粒度要贴着「为什么跑不了」的粒度**。job 级理由（装 tmux）
        /// 不等于步骤级理由（这一步用不用 tmux）。
        const BLANKET: &[(&str, &str)] = &[
            (
                "e2e-tmux",
                "本区红线〔用〕：绝不用真 tmux server。该 job 逐字 `apt-get install -y tmux` 并起真 server",
            ),
            (
                "e2e-tmux-rust",
                "同上（红线：真 tmux server）——它还额外装整套 Tauri Linux 依赖",
            ),
        ];
        /// **住在 blanket job 里、但本机跑得通**的那几套（08-06 用 tmux 桩实测）。
        /// 它们不需要 tmux，红线挡不住它们 —— 登记在这里是为了**别让 job 级豁免把它们盖住**。
        const LOCALLY_RUNNABLE: &[(&str, &str)] = &[
            ("ccm CLI 契约（F02）", "`npm run test:ccm-cli` —— 全走 `--print` 断言命令串，实测 PASS=53"),
            (
                "ccm 契约差分对拍（U9a · S10 保住清单）",
                "`npm run test:ccm-contract-parity` —— 实测 PASS=45",
            ),
            (
                "ccm --print 平价预言机（F03）",
                "`npm run test:ccm-print-parity` —— 实测 PASS=12",
            ),
            (
                "daemon 分叉（G2 `--fork-session`）",
                "`npm run test:daemon-fork` —— 脚本头注逐字「不需要 tmux、不需要 ssh」，实测 PASS=10",
            ),
            (
                "本机后端监护真进程验收（F05a）",
                "`npm run test:local-backend` —— 实测 PASS=7，且它**执行了 `local_backend.rs` 的三条                  `#[ignore]`**（真起了一个 daemon 进程）。                 ⚠ **只许带 tmux 桩跑**：脚本会 `export TMUX_TMPDIR`（本区红线括号里点名的动作），                 带桩时没有任何 tmux 进程能起来，裸跑则不然",
            ),
        ];
        /// 其余每一步逐条登记：`(步骤名, 本地跑不跑, 说法)`。
        /// 「跑不了」那几条要写**结构性**理由，不许写「太慢」这种可以克服的话。
        const STEPS: &[(&str, bool, &str)] = &[
            // ── job rust
            ("cargo fmt --check（整个 workspace）", true, "`cd src-tauri && cargo fmt --all --check`"),
            ("cargo clippy（整个 workspace，vendor 除外）", true, "同名命令；无 `-D warnings` ⇒ 只有真错才红"),
            ("cargo test（整个 workspace，vendor 除外）", true, "本区门禁主命令"),
            ("生成物必须最新（C05；改了 Rust 就得重新生成并提交）", true, "`git diff --exit-code -- ../src/generated/`"),
            ("cargo test (vendor code-picture-core)", true, "只**读地跑**；实测跑完 `git status` 对 vendor 零改动 ⇒ 不违反红线"),
            // ── job frontend
            ("npm audit (production deps, high)", true, "同名命令"),
            ("eslint (advisory, baseline)", true, "同名命令。⚠ 它带 `|| true` ⇒ **结构上不会红**；登记它是为了别把「不会红」误当成「跑过了」"),
            ("stylelint (advisory, baseline)", true, "同上，也带 `|| true`"),
            ("unit tests (node pure-fn + vitest DOM)", true, "`npm test`"),
            ("coverage floor (vitest jsdom)", true, "`npm run coverage`"),
            ("coverage per-file floors + zero-coverage ratchet", true, "`node scripts/assert-coverage-floors.mjs`"),
            ("vite build (dist/)", true, "`npm run build`"),
            // ── job daemon
            ("cargo fmt --check", true, "`cd remote-daemon-proto && cargo fmt --check`"),
            ("cargo check（跨 target：Windows 编得过 —— 平台线的真判据）", true, "E7 的真判据；本机装了 `x86_64-pc-windows-msvc` target，实测跑得通"),
            ("cargo clippy", true, "同名命令"),
            ("cargo test", true, "同名命令"),
            // ── job linux-app-build
            ("Install Linux build deps", false, "apt 装系统依赖：本机已装，且要 sudo ⇒ 属于**环境准备**不是判据"),
            ("npm ci", false, "按 lockfile **重装** node_modules：本地等价物是既有依赖树，重跑改变的是环境不是结论"),
            ("npm run build (tsc + vite)", true, "与 frontend job 同一条命令"),
            ("cargo build (full app binary, not --lib)", true, "`cd src-tauri && cargo build` —— 它编的是 bin，`--lib` 那条盖不住"),
            // ── job e2e-smoke
            ("shellcheck (errors only)", true, "本机装了 shellcheck；步骤体从 `ci.yml` 原样抽出来跑"),
            ("vendored cc-acct-iso self-tests (sandboxed, 294 assertions)", true, "沙箱内自测；vendor 是 `cc-acct-iso` 不是红线点名的 `code-picture-core`"),
            ("python syntax compile", true, "`python3 -m py_compile e2e/*.py`"),
            ("G-A/G-C 覆盖面地板（22 套真机套件都必须带断言数地板）", true, "纯 `grep` 数 `ci.yml` 自己，不需要 tmux"),
            ("exec-bit guard (shared/** shebang files must be 100755 in git)", true, "`bash e2e/exec-bit-guard.sh`"),
            // ── 无名步骤（`- run: <命令>`，08-07 人群扩到它们之后才第一次可见）。
            // 标识是命令本身，多个 job 里同一条命令共用这一行登记。
            ("run: npm ci", false, "按 lockfile **重装** node_modules（三个 job 各一条无名步骤）：本地等价物是既有依赖树，重跑改变的是环境不是结论 —— 与上面那条有名字的 `npm ci` 同一个理由"),
            // ★ 这一条是人群扩面**当场**逮出来的，而且不是无害的环境步骤：它是一条**真门禁命令**。
            // 它一直在 CI 里跑、本地也一直在跑（本区每轮门禁都有它），但**从来没被这条判据数过** ——
            // 「没人守着」与「碰巧没坏」是两回事，本仓第二次在同一句话上撞到实例。
            ("run: npx tsc --noEmit", true, "`npx tsc --noEmit`（仓根）—— 本区门禁固定项之一"),
        ];

        // ── 解析：(job, 步骤标识)
        //
        // ⚠⚠ **08-07 订正：人群原本是「带 `- name:` 的 `run:` 步骤」** ——
        // 那是按**怎么写的**取人群，而 GitHub Actions 的步骤**根本不要求有名字**。
        // 实测：`ci.yml` 的 51 条步骤里，判据看得见 47 条（`defaults:` 底下那几条 `run:` 不是步骤，不计），
        // **四条无名步骤整个在人群之外**（三条 `- run: npm ci` + 一条 `- run: npx tsc --noEmit`）——
        // 也就是说往 CI 里加一步 `- run: cargo something`（不写 name）
        // 本条**一个字都不会说**，而它存在的全部理由正是「CI 里有而本地没数过的步骤，一步都不许有」。
        // ⇒ 人群改按**「它是不是一个步骤」**取：`steps:` 之内的每一条 `run:` 都算，
        //   有名字用名字当标识，没名字用 `run: <命令首行>`。
        //
        // ⚠ `defaults:` 底下那三条 `run:`（`working-directory` / `shell` 的容器）**不是步骤** ——
        //   靠 `steps:` 之内这个条件排除，不靠「它没名字」。
        let yml = ci_yml();
        let src: Vec<&str> = yml.lines().collect();
        let mut found: Vec<(String, String)> = Vec::new();
        let mut job = String::new();
        let mut name: Option<String> = None;
        let mut in_steps = false;
        let mut unnamed_seen = 0usize;
        for (i, line) in src.iter().enumerate() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            let t = line.trim_end();
            if t.len() > 2
                && t.starts_with("  ")
                && !t.as_bytes()[2].is_ascii_whitespace()
                && t.ends_with(':')
            {
                job = t.trim().trim_end_matches(':').to_string();
                name = None;
                in_steps = false;
                continue;
            }
            if t.trim() == "steps:" {
                in_steps = true;
                continue;
            }
            if let Some(rest) = line.trim_start().strip_prefix("- name: ") {
                name = Some(rest.trim().to_string());
                continue;
            }
            if !in_steps {
                continue;
            }
            let trimmed = line.trim_start();
            // 两种步骤写法：`- name:` 之后的 `run:`，与直接内联的 `- run:`。
            let inline = trimmed.strip_prefix("- run:");
            if trimmed.starts_with("run:") || inline.is_some() {
                match name.take() {
                    Some(n) => found.push((job.clone(), n)),
                    None => {
                        // 无名步骤：拿命令首行当标识。`|` / `>` 块标量则往下看一行。
                        let raw = inline.unwrap_or_else(|| {
                            trimmed.strip_prefix("run:").expect("上面已判过前缀")
                        });
                        let head = match raw.trim() {
                            "" | "|" | ">" | "|-" | ">-" => src
                                .get(i + 1)
                                .map(|l| l.trim())
                                .unwrap_or("(空)")
                                .to_string(),
                            other => other.to_string(),
                        };
                        unnamed_seen += 1;
                        found.push((job.clone(), format!("run: {head}")));
                    }
                }
            }
        }
        // ★ 抽取器自检：数量掉下来就说明 YAML 形态变了、下面整条会零命中地绿。
        // 地板只管「抽取器抓没抓到」，**留足余量**：某一步真被删掉那件事由下面的
        // `stale`（登记表里有、ci.yml 里没有）逐条点名 —— 上一轮刚学到的教训是
        // 「地板与回归锚点抢同一个变异时，先响的那个会讲错成因」。
        assert!(
            found.len() >= 45,
            "只从 `ci.yml` 解析到 {} 个 `run:` 步骤（其中有名字 {}、无名 {}）—— 解析坏了。\n\
             08-07 改成按步骤取人群后实测 51 = 47 有名 + 4 无名。",
            found.len(),
            found.len() - unnamed_seen,
            unnamed_seen
        );
        // ★ **常驻自检：无名那一支必须真的被行使过**。
        // 它是本次订正新长出来的分支，而「新分支平时没人走」在本仓已连着栽过五次 ——
        // 哪天三条 `- run: npm ci` 都补上名字，这里会红，提醒人确认那一支还认得出无名步骤
        // （处置：造一条无名步骤当样本，或确认这条自检已无意义再撤）。
        assert!(
            unnamed_seen >= 1,
            "解析结果里一条**无名步骤**都没有 —— 要么 ci.yml 里真的不剩无名步骤了，\n\
             要么无名那一支又不认识它们了（那正是 08-07 之前的状态：四条无名步骤隐形，含 `npx tsc --noEmit`）。"
        );

        // ★ 前提触发器：blanket 豁免的理由是「这个 job 要真 tmux」——理由没了就得重判。
        for (j, why) in BLANKET {
            let block = ci_job_block(j);
            assert!(
                !block.trim().is_empty(),
                "`ci.yml` 里已经没有 job `{j}` 了 —— 删掉这条整 job 豁免（它当初的理由：{why}）"
            );
            assert!(
                block.contains("install -y tmux"),
                "job `{j}` **不再装 tmux 了** —— 那么「红线挡住、结构上跑不了」这个豁免理由就没了，\n\
                 请重新逐步登记它（当初的理由：{why}）"
            );
        }

        // ★ 登记表保鲜：`LOCALLY_RUNNABLE` 里的步骤必须仍住在 blanket job 里。
        // 哪天它被挪出去（或改名），本地门禁那份清单就该跟着改 —— 不许它悄悄失联。
        for (step, how) in LOCALLY_RUNNABLE {
            let at = found.iter().find(|(_, n)| n == step).unwrap_or_else(|| {
                panic!(
                    "`{step}` 在 `ci.yml` 里找不到了 —— 本地门禁那份清单要跟着改。（跑法：{how}）"
                )
            });
            assert!(
                BLANKET.iter().any(|(b, _)| *b == at.0),
                "`{step}` 已经不在 blanket job 里了（现在在 `{}`）—— \n\
                 那它就该按普通步骤逐条登记，而不是靠 `LOCALLY_RUNNABLE` 兜着。（跑法：{how}）",
                at.0
            );
        }

        let unregistered: Vec<String> = found
            .iter()
            .filter(|(j, _)| !BLANKET.iter().any(|(b, _)| b == j))
            .filter(|(_, n)| !STEPS.iter().any(|(s, _, _)| s == n))
            .map(|(j, n)| format!("  [{j}] {n}"))
            .collect();
        assert!(
            unregistered.is_empty(),
            "`ci.yml` 里这些步骤**没人回答「本地跑不跑」**：\n{}\n\n\
             ⚠ 这正是 fmt 那条溜掉的方式：CI 里加了一步、本地门禁不知道，\n\
             于是「本地全绿」与「CI 全绿」之间的差距**一直在长而没人数**。\n\
             登记进 `STEPS`：能跑就写下本地怎么跑，跑不了就写**结构性**理由（「慢」不算）。",
            unregistered.join("\n")
        );

        // ★ 登记表保鲜：登记了一条 `ci.yml` 里已经没有的步骤 ⇒ 它在替真判据挡枪。
        let stale: Vec<&str> = STEPS
            .iter()
            .map(|(s, _, _)| *s)
            .filter(|s| !found.iter().any(|(_, n)| n == s))
            .collect();
        assert!(
            stale.is_empty(),
            "登记表里这些步骤 `ci.yml` 里已经找不到了（改名或删了）：{stale:?}\n\
             改名也要红 —— 名字变了就该有人重新回答一次「本地跑不跑它」。"
        );
    }

    /// 〔audit-0805 08-06〕**`package.json` 里每个 `test*` 脚本，要么 CI 会跑它，
    /// 要么在这里登记成「手测」并写清原因。**
    ///
    /// 与上一条是同一族的**另一个方向**：那条问「CI 有的步骤本地数过没有」，
    /// 本条问「**仓里写好的套件，有没有谁会去跑**」。
    ///
    /// **为什么建它**（实测撞见的，不是设想）：`e2e/graylight-suite.sh` 是一整套
    /// 跨进程整链 e2e（130 行，驱 gray-light 生命周期、断言 `[e2e] tab-state` 序列），
    /// 而 **CI 一次都不跑它** —— CI 跑的是名字很像的另一个 `graylight-daemon-frames.sh`。
    /// 它的前置逐字写着「Xvfb 上跑着 `npx tauri dev`」⇒ 结构上确实进不了 CI，这没问题；
    /// **问题是 `doc/RELEASING.md` 里零处提到它**：`test:f40` 好歹进了发版手测清单，它没有。
    /// ⇒ 于是这套件的唯一触发条件是「有人想起来」。
    ///
    /// ⚠ 顺带澄清一处容易误读的历史：最后改它的提交叫「G-C：三族 e2e 进 CI」，
    /// 查过那次 diff —— 进 CI 的是 `graylight-frames` 等五条，**不含本套件**，提交没说假话。
    ///
    /// 与 `src/node-suite-registry-guard.vitest.ts` 不冲突（E3）：那条钉的是
    /// 「16 个 tsx 套件各自有断言地板」，本条钉的是「套件有没有人调」——两个事实。
    #[test]
    fn every_test_script_is_either_run_by_ci_or_registered_as_manual() {
        /// 手测套件：**CI 结构上跑不了**的，逐条写清为什么、以及谁会去跑它。
        const MANUAL: &[(&str, &str)] = &[
            (
                "test:f40",
                "需 Xvfb 上跑着 `npx tauri dev`（真 WebView）⇒ 结构上进不了 CI；\
                 `doc/RELEASING.md § 1` 已把它列进发版手测清单。\
                 ★ **08-06 实测补一条更硬的理由**：它 `PROJ_DIR=\"$HOME/.claude/projects/-tmp-e2e-fork\"`、\
                 `PIDFILE=\"$HOME/.claude/sessions/…\"` —— **固有地往 `~/.claude/` 写**，\
                 而本区红线是「`~/.claude/` 只读」⇒ **本机绝不能跑它，带不带 tmux 桩都不行**。\
                 （实测那次它建了 fixture 目录、随后被自己的 trap 清掉，无残留；但那一瞬是真写。）",
            ),
            (
                "test:graylight",
                "同 f40 契约（脚本头注逐字「前置同 e2e/f40-suite.sh」）⇒ 同样进不了 CI。\
                 ⚠ 但**发版清单里此前没有它** —— 这条例外就是那笔欠账的落点：\
                 谁要删这条例外，得先说清楚它改由谁来跑。\
                 ★ 08-06 实测：带 tmux 桩跑，**第一次碰 tmux 就被拒（exit 99）** ⇒ 它要真 tmux；\
                 但它的 claude_dir 在 `/tmp` 下，**不碰 `~/.claude/`**（与 f40 的红线情形不同）",
            ),
        ];

        let pkg = std::fs::read_to_string(root().parent().unwrap().join("package.json"))
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
        // ★ 抽取器自检：脚本条数掉下来 ⇒ 剥法坏了，下面会零命中地绿。
        assert!(
            scripts.len() >= 30,
            "从 `package.json` 只剥到 {} 个 `test*` 脚本 —— 剥法坏了（建判据当天实测 40 个）",
            scripts.len()
        );

        let ci = ci_yaml::live_lines();
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
            // ① `assert-pass-floor.sh <后缀>`；② CI 直接 `bash e2e/xxx.sh`（`exec-bits` 就是这样）。
            if let Some(suffix) = name.strip_prefix("test:") {
                if ci.contains(&format!("assert-pass-floor.sh {suffix} ")) {
                    return true;
                }
            }
            !cmd.is_empty() && ci.contains(cmd)
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

        // ★ 前提触发器〔08-06〕：f40 那条例外的**硬理由**是「它往 `~/.claude/` 写」。
        // 哪天它改用临时目录，这条理由就消失、它可能变成本机跑得动的 —— 必须回来重判。
        {
            let f40 =
                std::fs::read_to_string(root().parent().expect("仓根").join("e2e/f40-suite.sh"))
                    .expect("读不到 e2e/f40-suite.sh");
            assert!(
                f40.lines()
                    .any(|l| !l.trim_start().starts_with('#') && l.contains("$HOME/.claude/")),
                "`e2e/f40-suite.sh` 不再往 `$HOME/.claude/` 写了 —— \n\
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

    /// 〔audit-0805 08-06〕**每条 `#[ignore]` 测试都要真有人来触发它。**
    ///
    /// 本族第三条（前两条：CI 步骤本地数过没有 · 套件有没有人调）。这条问最里面那层：
    /// **被 `#[ignore]` 挡在常规门禁之外的测试，说好的那个「触发者」还在吗。**
    ///
    /// **为什么建它**：这七条的头注都写着「由 `e2e/xxx.sh` 驱动」，而那是一句**散文**。
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
    /// 并且**真的执行了 `local_backend.rs` 的那三条**（`3 passed`，还起了一个真 daemon 进程）。
    /// ⇒ 今天的准确说法：**七条里三条在本机跑得动（带桩）、四条仍要真 tmux**；
    /// 而后四条确实自 `1eeb4bf` 起零执行（`ci.yml` 只在 push/PR 触发，停推后没跑过）。
    /// 本条守的是「链还连着」，**不是**「它们跑过了」——两件事别混。
    #[test]
    /// ⚠⚠ **`ROADMAP §5 3x`「七条 `#[ignore]` 执行次数为零」这条账，08-13 已不再成立**：
    /// 本轮把触发脚本一条条真跑了，逐条读数（每次都对照用户真实 server，**9 个会话逐字未变**）：
    ///
    /// | 触发脚本 | 读数 | 带动的 `#[ignore]` |
    /// |---|---|---|
    /// | `local-backend-supervise.sh` | **7 过 / 0 败** | **4 条** |
    /// | `tmux-guarded-acceptance.sh` | **14 过 / 0 败** | 1 条（`emit_guarded_commands_for_e2e`） |
    /// | `usage-probe-acceptance.sh` | **11 过 / 0 败** | 2 条（F08 段 + 命令串产出） |
    ///
    /// ⇒ **7 条里 7 条都跑过了**（`local_backend` 那 4 条此前一次都没跑过 ——
    /// 其中 `the_local_tmux_frames_really_land_in_the_ledger` **首跑就是红的**，
    /// 病根是「测试与 daemon 不在同一台 tmux server」，已修，见 `P3 §0h-2`）。
    ///
    /// ★ 「触发者登记在册」与「真的有人跑」是两件事 —— 本条只守前者，
    /// 后者靠人真跑。**别把这段读成「以后会自动跑」**：它们仍不在 CI 里（要真 tmux/真进程）。
    fn every_ignored_test_still_has_someone_who_triggers_it() {
        /// 不由 e2e 驱动、**刻意手动**的，逐条写清谁在什么时候跑它。
        const MANUAL: &[(&str, &str)] = &[(
            "f63_real_data_ledger",
            "不是 e2e：它要本机真实历史数据（头注记着 771 会话 / 643MB 的基线），\
             跑法写在自己的头注里，属于「改 F63 解析时人工重算的台账」",
        )];

        let repo = root().parent().expect("仓根").to_path_buf();
        // ── 收 `#[ignore]` 测试：(文件名 stem, fn 名)
        let mut ignored: Vec<(String, String)> = Vec::new();
        for (path, src) in guard_core::scan_tree!(&repo.join("src-tauri/src"), &["rs"]) {
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
        // ★ 自检 1：一条都收不到 ⇒ 剥法坏了（下面会零命中地绿）。
        assert!(
            ignored.len() >= 5,
            "全仓只收到 {} 条 `#[ignore]` 测试 —— 剥法坏了（建判据当天实测 7 条）",
            ignored.len()
        );

        // ── 收 e2e 脚本里的触发过滤串
        let mut filters: Vec<(String, String)> = Vec::new();
        for e in std::fs::read_dir(repo.join("e2e"))
            .expect("读不到 e2e/")
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
            filters.len() >= 3,
            "从 `e2e/*.sh` 只收到 {} 个 `--ignored` 触发过滤串 —— 抽取器坏了（建判据当天实测 4 个）：{filters:?}",
            filters.len()
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

        let orphan: Vec<String> = ignored
            .iter()
            .filter(|(_, n)| !MANUAL.iter().any(|(m, _)| m == n))
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

    /// 〔audit-0805 08-06〕**跨 target（Windows）编译信号今天只覆盖 daemon，不覆盖 monitor
    /// —— 把这个不对称本身钉住，让它不能悄悄变。**
    ///
    /// 定框 **E7** 逐字写着「C10 的**真判据是跨 target 编得过**
    /// （`cargo check --all-targets --target x86_64-pc-windows-msvc`），**不是 cfg 位置扫描**」。
    /// 所以本条**刻意不去数 `#[cfg(windows)]` 的位置**——那会与 E7 相悖。它只钉「真判据在不在」。
    ///
    /// **实测到的实况**（08-06）：
    /// - daemon 有那一步，本机跑得通（exit 0）；
    /// - **monitor 没有**。它的 Windows 面只由跑在 `windows-latest` 的 `rust` job 编译，
    ///   而〔用 08-05〕停推后 `ci.yml` 至今 72 个提交一次没跑 ⇒ **那 31 处 `cfg(windows)`
    ///   已经很久没有被任何编译器看过**。
    /// - 本机补不上：`cargo check --target x86_64-pc-windows-msvc -p monitor` 挂在
    ///   `tree-sitter-*` 的 C build script 上（`cc-rs: failed to find tool "lib.exe"`，12 个 error
    ///   全是它，**我们自己的代码零 error**），而那些 crate 来自 vendor `code-picture-core`——
    ///   它是 monitor 的**无条件 path 依赖**，且 vendor 是本区红线，不许动。
    ///
    /// ⚠ 顺带一条方法论（本轮变异抽样撞出来的）：**`#[cfg(windows)]` 里的变异在 Linux 上
    /// 连编译错误都不报**（实证：往里写一个不存在的标识符，`cargo build` **零 error**）。
    /// ⇒ 对那片代码做变异抽样得到的「SURVIVED」**没有信息量**，那是方法的盲区不是门禁的漏洞。
    ///
    /// 本条是那条诚实边界（`ROADMAP §5`）的**前提触发器**：前提一旦消失就红，逼人回来重判。
    #[test]
    fn the_windows_cross_target_signal_covers_only_the_daemon() {
        const NEEDLE: &str = "--target x86_64-pc-windows-msvc";

        // ① daemon 那一步还在吗（E7 点名的真判据，本仓唯一一处）。
        let daemon_block = ci_job_block("daemon");
        assert!(
            daemon_block.contains(NEEDLE),
            "`daemon` job 里的跨 target check 不见了 —— 那是 E7 逐字点名的**真判据**，\n\
             删它等于把平台线上唯一还活着的编译信号也关掉。"
        );

        // ② monitor 那侧仍然没有 —— 有了就说明边界过期了，本条要红。
        let rust_block = ci_job_block("rust");
        assert!(
            !rust_block.contains(NEEDLE),
            "`rust` job（monitor）现在**有跨 target check 了** —— 好事，但请顺手：\n\
             ① 删掉 `ROADMAP §5` 里「monitor 的 Windows 面没有编译信号」那条诚实边界；\n\
             ② 删掉本条判据（它的全部意义就是钉住这个不对称）。"
        );

        // ③ 那条「本机补不上」的理由还成立吗：vendor 仍是**无条件** path 依赖。
        //    哪天它变成 optional / 被摘掉，本机就可能 check 得动 ⇒ 理由消失，必须重判。
        let cargo = std::fs::read_to_string(root().join("Cargo.toml")).expect("读不到 Cargo.toml");
        let dep_line = cargo
            .lines()
            .find(|l| {
                l.trim_start().starts_with("code-picture-core")
                    && guard_core::contains_word(l, "path")
            })
            .unwrap_or_else(|| {
                panic!(
                    "`src-tauri/Cargo.toml` 里找不到 `code-picture-core` 的 path 依赖行 ——\n\
                     「本机跨 target check 被 vendor 的 C build script 挡住」这个理由可能已经不成立，\n\
                     请重新试一次 `cargo check --target x86_64-pc-windows-msvc -p monitor`。"
                )
            });
        assert!(
            !guard_core::contains_word(dep_line, "optional"),
            "`code-picture-core` 变成 optional 依赖了 —— 那么 `--no-default-features` 之类\n\
             也许就能在本机做 monitor 的跨 target check。**理由变了，结论要重量。**\n\
             （当初的实测：12 个 error 全是 `tree-sitter-*` 的 `cc-rs: lib.exe`，我们自己的代码零 error。）"
        );
    }

    /// 〔audit-0805 08-06〕**三条诚实边界压在同一个前提上：「CI 今天不会跑」——把这个前提钉住。**
    ///
    /// `ROADMAP §5` 的 3w（release.yml 的版本 guard 不触发）· 3x（七条 `#[ignore]` 执行次数为零）·
    /// 3y（monitor 的 Windows 面没有编译信号），**三条的成立都只因为一件事**：
    /// 两个 workflow 都只在 `push` / `pull_request` 上触发，而〔用 08-05〕裁定不再 push。
    ///
    /// 这个前提**没人盯**。谁加一个 `workflow_dispatch`（手点就能跑）或 `schedule`（定时跑），
    /// 三条边界当天就该重判 —— 而在本条之前，它们会**继续以「已登记的诚实边界」的样子留在表里**，
    /// 那正是本会话反复量到的**停滞式腐坏**：世界变了、文本一个字没动。
    ///
    /// ⚠ 本条**不断言 CI 应该怎么触发**（那是〔用〕的裁决）。它只断言
    /// 「触发方式没变过」——变了就红，逼人回来把那三条边界重新过一遍。
    #[test]
    fn the_premise_behind_three_honesty_boundaries_still_holds() {
        /// 会让 workflow **在没有 push 的情况下也能跑起来**的触发器。
        const SELF_STARTING: &[&str] = &["workflow_dispatch", "schedule", "repository_dispatch"];

        let root = root().parent().expect("仓根").to_path_buf();
        for wf in ["ci.yml", "release.yml"] {
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
                assert!(
                    !guard_core::contains_word(&seg, trig),
                    "{wf} 新增了 `{trig}` 触发器 —— **CI 从此可以在没有 push 的情况下跑起来**。\n\
                     ⇒ `ROADMAP §5` 的 3w / 3x / 3y 三条诚实边界的**前提当场消失**，必须重判：\n\
                     · 3w：release.yml 的版本一致性 guard 又会跑了；\n\
                     · 3x：七条 `#[ignore]` 的 e2e 触发路重新接通；\n\
                     · 3y：monitor 的 Windows 面重新有编译信号。\n\
                     本条不反对加触发器 —— 它只是不许**加了而没人回来改那三条**。"
                );
            }
        }
    }

    /// 〔audit-0805 08-06〕**跑不了的那批 e2e，静态断言条数只许涨不许掉。**
    ///
    /// # 为什么需要它
    ///
    /// 那些套件的运行期地板（`assert-pass-floor.sh <套件> <N>`）住在 `ci.yml` 里，
    /// 而 `ci.yml` 只在 `push`/`pull_request` 触发、〔用 08-05〕停推后至今没跑过
    /// ⇒ **地板今天是惰的**。同时这几套本机也跑不了（真 tmux / 写 `~/.claude/`）。
    /// 于是「有人删掉几条断言」在本地和 CI **都不会红** —— 它们是仓里最没人看着的一批断言。
    ///
    /// 本条是它们唯一活着的保护：**把源码里的断言调用数钉成递增棘轮**。
    ///
    /// # 为什么只钉一部分（先量人群再决定，别一刀切）
    ///
    /// 实测 14 个跑不了的套件，静态计数与运行期地板的关系分成两族：
    /// - **`ck` 族**：静态数与地板几乎逐个相等（19/19 · 13/13 · 14/14 · 26/26 · 9→11）
    ///   ⇒ 静态计数是有意义的代理，钉它。
    /// - **`ok` 族**：静态 0–12 而地板 5–36（断言写在循环与 helper 里）
    ///   ⇒ 静态计数**不是**那个量的代理，钉 0 是个空转的地板。如实登记为「静态无信号」，
    ///   并配前提触发器：哪天它们的静态数追上地板，说明改成了内联写法、该挪进棘轮。
    #[test]
    fn dormant_e2e_suites_keep_their_assertions() {
        /// `(套件, 脚本名, 断言助手, 当日静态条数)` —— **只许涨**。
        const RATCHET: &[(&str, &str, &str, usize)] = &[
            ("ccm-acceptance", "ccm-acceptance.sh", "ck", 19),
            ("ccm-pretrust", "ccm-pretrust-acceptance.sh", "ck", 15),
            ("tmux-guarded", "tmux-guarded-acceptance.sh", "ck", 14),
            ("tmux-target", "tmux-target-acceptance.sh", "ck", 26),
            ("usage-probe", "usage-probe-acceptance.sh", "ck", 9),
        ];
        /// 静态计数不是那个量的代理的套件 —— `(脚本名, 助手, 当日静态数, CI 地板)`。
        const NO_STATIC_SIGNAL: &[(&str, &str, usize, usize)] = &[
            ("cc-spawn-uplift.sh", "-", 0, 51),
            ("ccm-rbind-title.sh", "ok", 0, 8),
            ("daemon-gate2-acceptance.sh", "ok", 3, 36),
            ("graylight-daemon-frames.sh", "ok", 9, 12),
            ("inbound-daemon-frames.sh", "ok", 12, 32),
            ("restart-suite.sh", "ok", 0, 24),
            ("restart-daemon-frames.sh", "ok", 0, 5),
            ("resume-suite.sh", "ok", 2, 17),
            ("resume-daemon-frames.sh", "ok", 1, 7),
        ];

        let e2e = root().parent().expect("仓根").join("e2e");
        let count = |script: &str, helper: &str| -> (usize, String) {
            let txt = std::fs::read_to_string(e2e.join(script))
                .unwrap_or_else(|e| panic!("读不到 e2e/{script}: {e}"));
            let n = txt
                .lines()
                .filter(|l| {
                    let s = l.trim_start();
                    s.starts_with(&format!("{helper} ")) || s.starts_with(&format!("{helper}\t"))
                })
                .count();
            (n, txt)
        };

        for (suite, script, helper, base) in RATCHET {
            let (n, txt) = count(script, helper);
            // ★ 自检：助手还在定义。改了名字会让计数掉成 0，那时该看到的是这句而不是「掉了」。
            assert!(
                txt.contains(&format!("{helper}()")),
                "`e2e/{script}` 里找不到断言助手 `{helper}()` 的定义 —— 它被改名了，\n\
                 本条的计数会跟着失真。先把登记里的助手名改对，再谈条数。"
            );
            assert!(
                n >= *base,
                "`{suite}`（e2e/{script}）的断言从 {base} 条掉到 {n} 条。\n\
                 ⚠ 这一批是**仓里最没人看着的断言**：它们的运行期地板住在 `ci.yml`，\n\
                 而停推后 CI 一次没跑；本机也跑不了（真 tmux / 写 `~/.claude/`）。\n\
                 ⇒ 删掉它们在本地与 CI **都不会红**，只有本条会。\n\
                 真要减，先说清楚那条性质改由谁接。"
            );
        }

        // ★ 前提触发器：「静态无信号」的理由是静态数远低于地板。追上了就该重判。
        for (script, helper, base, floor) in NO_STATIC_SIGNAL {
            if *helper == "-" {
                continue;
            }
            let (n, _) = count(script, helper);
            assert!(
                n < *floor,
                "`e2e/{script}` 的静态断言数已达 {n}（当初 {base}，CI 地板 {floor}）——\n\
                 「静态计数不是那个量的代理」这个理由不成立了：它多半改成了内联写法。\n\
                 ⇒ 把它从 `NO_STATIC_SIGNAL` 挪进 `RATCHET`，让它也受棘轮保护。"
            );
        }
    }

    /// 〔audit-0805 08-06〕**唯一量「提交状态」的那道门，本身没人守着。**
    ///
    /// `scripts/verify-committed-state.sh` 的头注逐字写着它为什么必须存在：
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
    /// 本条钉住那三项还在：`monitor-lib` · `daemon` · `daemon-win`。
    ///
    /// ⚠ **如实记一处局限**（免得把它的绿读大）：monitor 那项是 `cargo check --lib`，
    /// **不编测试段**。所以「提交状态编得过」不等于「提交状态的测试编得过」。
    /// 真要覆盖那一半得改成 `--all-targets`，代价是本机每次多编一大块 —— 不在本轮做，
    /// 写在这里让下一个人看得见。
    #[test]
    fn the_only_gate_that_measures_committed_state_still_does_all_three_checks() {
        let path = root()
            .parent()
            .expect("仓根")
            .join("scripts/verify-committed-state.sh");
        let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {path:?}: {e}"));
        // ★ 抽取器自检：文件被掏空/改名时，下面三条会零命中地绿。
        assert!(
            src.lines().count() >= 40,
            "`verify-committed-state.sh` 只剩 {} 行 —— 读法坏了或被掏空",
            src.lines().count()
        );

        // `(检查名, 那一行还必须含什么)` —— 钉性质不钉整行，留出改写空间。
        const CHECKS: &[(&str, &str)] = &[
            ("run monitor-lib", "cargo check"),
            ("run daemon ", "cargo check --all-targets"),
            ("run daemon-win", "x86_64-pc-windows-msvc"),
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

    /// 〔audit-0805 08-08〕**三项还在 ≠ 三项都跑了**。
    ///
    /// 上一条钉的是那三项检查**存在**。而 `daemon-win` 那项包在
    /// `if rustup target list --installed | grep -q x86_64-pc-windows-msvc` 里 ——
    /// ★ 08-08 **真路实测**（临时放一个假 `rustup` 到 `PATH` 前面、让它报「什么都没装」）：
    ///
    /// ```text
    ///    ok   monitor-lib
    ///    ok   daemon
    ///    skip daemon-win（没装 x86_64-pc-windows-msvc target）
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
    /// 放进 `cargo test` 等于每轮门禁多编一遍全仓。⇒ 登记进 `ROADMAP §5`。
    /// 本条能挡的是「有人把降级那一支改回成和成功一样的结论」；
    /// 挡不住的是「`run` 函数本身坏掉但文本还在」。
    #[test]
    fn a_skipped_windows_check_cannot_look_like_a_full_pass() {
        let path = root()
            .parent()
            .expect("仓根")
            .join("scripts/verify-committed-state.sh");
        let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {path:?}: {e}"));
        // ⚠ **只看非注释行**：本脚本的头注里逐字引用着那句成功结论（讲的就是这次事故），
        // 连注释一起数，下面「恰好一处」当场变成两处 —— 08-08 写这条时就差点踩上。
        let code: Vec<&str> = src
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#'))
            .collect();
        assert!(
            code.len() >= 25,
            "剥注释后只剩 {} 行 —— 读法坏了，下面几条会零命中地绿",
            code.len()
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
                .any(|l| l.starts_with("skipped=") && l.contains("daemon-win")),
            "跳过 `daemon-win` 那一支没有把这件事**记进变量**（找不到 `skipped=…daemon-win…`）。\n\
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
}
