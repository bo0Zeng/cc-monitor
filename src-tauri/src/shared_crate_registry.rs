//! U8c-1：把「新增共享 crate 时 CI 三样都要补」从**散文**变成机检。
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
        // 地板从 4 棘到 5（实测 5：branch-core / usage-core / acct-core / guard-core /
        // shell-quote-core）。差一个的地板意味着「少抽到一个 crate」不会红 —— 而少抽到的那个
        // 恰好就是没人管 CI 的那个。删共享 crate 时来改这个数，是刻意的摩擦。
        assert!(
            n >= 5,
            "只从 crates/*/Cargo.toml 抽到 {n} 个包名（实测应为 5）—— 抽取器坏了，\
             下面那条「三样都在 CI 里」会零命中零失败地绿"
        );
    }

    /// ★ 每个共享 crate 都必须在 `ci.yml` 里出现在 test / fmt / clippy **三处**。
    #[test]
    fn every_shared_crate_gets_all_three_ci_steps() {
        let ci = ci_live_lines();
        let mut missing = Vec::new();
        for name in shared_crate_names() {
            for (what, needle) in [
                ("cargo test", format!("cargo test -p {name}")),
                (
                    "cargo fmt",
                    format!("cargo fmt --check --manifest-path crates/{name}/Cargo.toml"),
                ),
                (
                    "cargo clippy",
                    format!("cargo clippy --manifest-path crates/{name}/Cargo.toml"),
                ),
            ] {
                if !ci.contains(&needle) {
                    missing.push(format!("  {name}: 缺 {what}（找不到 `{needle}`）"));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "共享 crate 的 CI 三样没配齐 —— 违反它不会红，只会静默少跑：\n{}",
            missing.join("\n")
        );
    }

    /// 反过来也要成立：`ci.yml` 里提到的 crate 必须真的存在。
    /// 挡的是「crate 改名/删除后 CI 步骤留成僵尸」。
    ///
    /// ⚠ 复盘 P3 订正：这条的说明写着「如果有人把 test 那步顺手删掉而不删对应的
    /// fmt/clippy，覆盖面就悄悄缩水了」，**而它此前只扫 `cargo test -p` 那一种形态** ——
    /// 也就是它声称担心的 fmt/clippy 僵尸行，它自己看不见。现在**三种形态都扫**。
    #[test]
    fn ci_does_not_reference_crates_that_no_longer_exist() {
        let names = shared_crate_names();
        let mut ghosts = Vec::new();
        let mut scanned = 0usize;
        for line in ci_live_lines().lines() {
            // 两种 YAML 写法都要认：`run: |` 块里独立成行的命令，与内联的
            // `run: cargo test -p branch-core`。⚠ 初版只认前者 ⇒ 自检报「只扫到 14 处」，
            // 而**分家的是我这个抽取器、不是 ci.yml**（`branch-core` 那三步是内联写法）。
            // 那条自检第一次跑就抓到了它自己上游的这个错，值。
            let t = line
                .trim()
                .trim_start_matches("- ")
                .trim_start_matches("run: ")
                .trim();
            // 三种步骤形态各自的取名方式。
            let referenced = if let Some(rest) = t.strip_prefix("cargo test -p ") {
                rest.split_whitespace().next().unwrap_or("")
            } else if let Some(at) = t.find("--manifest-path crates/") {
                let rest = &t[at + "--manifest-path crates/".len()..];
                rest.split('/').next().unwrap_or("")
            } else {
                continue;
            };
            // vendored 的 code-picture-core 不在 crates/ 下，走自己的一步。
            if referenced == "code-picture-core" || referenced.is_empty() {
                continue;
            }
            scanned += 1;
            if !names.iter().any(|n| n == referenced) {
                ghosts.push(referenced.to_string());
            }
        }
        // 抽取器自检：三种形态 × 5 个 crate = 15 行。扫不到就是取名方式跟 ci.yml 分家了。
        assert!(
            scanned >= 15,
            "只扫到 {scanned} 处 crate 引用（三种形态 × 5 个 crate 应有 15 处）—— \
             要么有一步被注释/删掉了，要么本抽取器的取名方式与 ci.yml 分家了。\
             两种都得看一眼：后者会让下面那条零命中变绿"
        );
        assert!(
            ghosts.is_empty(),
            "ci.yml 引用了 crates/ 下不存在的包：{ghosts:?}"
        );
    }
}
