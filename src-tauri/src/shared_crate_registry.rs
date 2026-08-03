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

    /// ★ 抽取器自检：`crates/` 下一个都没抽到时，下面那条会零命中零失败地绿。
    #[test]
    fn the_crate_scan_actually_finds_crates() {
        let n = shared_crate_names().len();
        assert!(
            n >= 4,
            "只从 crates/*/Cargo.toml 抽到 {n} 个包名 —— 抽取器坏了，\
             下面那条「三样都在 CI 里」会零命中零失败地绿"
        );
    }

    /// ★ 每个共享 crate 都必须在 `ci.yml` 里出现在 test / fmt / clippy **三处**。
    #[test]
    fn every_shared_crate_gets_all_three_ci_steps() {
        let ci = ci_yml();
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

    /// 反过来也要成立：`ci.yml` 里 `cargo test -p X` 提到的 crate 必须真的存在。
    /// 挡的是「crate 改名/删除后 CI 步骤留成僵尸」——那一步会一直失败，但如果有人把它
    /// 顺手删掉而不删对应的 fmt/clippy，覆盖面就悄悄缩水了。
    #[test]
    fn ci_does_not_reference_crates_that_no_longer_exist() {
        let ci = ci_yml();
        let names = shared_crate_names();
        let mut ghosts = Vec::new();
        for line in ci.lines() {
            let Some(rest) = line.trim().strip_prefix("cargo test -p ") else {
                continue;
            };
            let referenced = rest.split_whitespace().next().unwrap_or("");
            // vendored 的 code-picture-core 不在 crates/ 下，走自己的一步。
            if referenced == "code-picture-core" || referenced.is_empty() {
                continue;
            }
            if !names.iter().any(|n| n == referenced) {
                ghosts.push(referenced.to_string());
            }
        }
        assert!(
            ghosts.is_empty(),
            "ci.yml 引用了 crates/ 下不存在的包：{ghosts:?}"
        );
    }
}
