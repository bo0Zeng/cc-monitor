//! 测试用的**住址唯一源** —— 「仓根在哪 / 本 crate 的源码在哪 / 测试树在哪」只答一次。
//!
//! 🔴 **它为什么存在**：2026-09-18 装齐 Tauri 栈、本 crate 在本机第一次编译并跑测试，
//! 一次就红了 **160 条**。根因只有一个：仓库重组把本 crate 从 `<repo>/src-tauri` 搬到
//! `<repo>/src/bridge`，于是 `CARGO_MANIFEST_DIR` 到仓根**从一级变成两级**，
//! 而「仓根在哪」这件事在本 crate 里有 **24 份各自独立的答案**，一份都没跟着改。
//!
//! 其中 `shared_crate_registry` 那份的 `.expect()` 消息已经被改成
//! 「`src/bridge` 的上级 = 仓根」——**注释改对了，代码没改**。这是最能说明问题的一处：
//! 散落的副本会各自漂，而没有任何判据数着它们。
//!
//! ⇒ 纪律出处：`调研/设计/16 §5.1`（一条形状出现 N 次，就抽一个住址）
//! ＋ `§5.2`（扫描型测试拿不到人群会扫空集 ⇒ 恒绿 ⇒ 必须配反空真自检）。
//! 后端那一半早已这么做（`src/backend/guard_support.rs`），本文件是它的对侧。

#![cfg(test)]

use std::path::{Path, PathBuf};

/// 本 crate 的包根 —— `<repo>/src/bridge`。
pub(crate) fn crate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 本 crate 自己的源码树 —— `<repo>/src/bridge/src`。
/// ⚠ 与 `repo_src_root()` 不是一件事，别混。
pub(crate) fn crate_src_root() -> PathBuf {
    crate_root().join("src")
}

/// 仓根 —— `<repo>`。**`src/bridge` 的上两级**（不是上一级）。
pub(crate) fn repo_root() -> PathBuf {
    crate_root()
        .parent()
        .and_then(|p| p.parent())
        .expect("src/bridge 的上两级 = 仓根")
        .to_path_buf()
}

/// 仓里的 `src/`（前端 TS ＋ 两个 Rust 半都住这儿）—— `<repo>/src`。
pub(crate) fn repo_src_root() -> PathBuf {
    repo_root().join("src")
}

/// 仓里的测试树 —— `<repo>/tests`。
pub(crate) fn tests_root() -> PathBuf {
    repo_root().join("tests")
}

/// 后端那一半的源码树 —— `<repo>/src/backend`。
pub(crate) fn backend_src_root() -> PathBuf {
    repo_src_root().join("backend")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 🔴 反空真自检（`设计/16 §5.2`）—— 没有这条，上面每个住址都可能安静地指向
    /// 一个不存在的目录，而所有靠它取人群的测试会**扫空集然后通过**。
    ///
    /// ⚠ 判据**点名具体文件**，不数条目数。两个理由：
    /// ① 数条目数是弱判据（一个装了别的东西的目录也能过）；
    /// ② `scanning_guard_registry` 那条元判据禁止测试段里**裸遍历目录**
    ///    （理由：判据在自己那份语料里找到自己 ⇒ 恒绿），而那张存量清单**只许变短**
    ///    ⇒ 新增的判据不该去要豁免，应当换成不遍历的写法。
    #[test]
    fn every_address_points_at_something_we_can_name() {
        for (name, dir, probes) in [
            ("crate_root", crate_root(), &["Cargo.toml", "build.rs"][..]),
            (
                "crate_src_root",
                crate_src_root(),
                &["lib.rs", "main.rs"][..],
            ),
            (
                "repo_root",
                repo_root(),
                &["package.json", "tsconfig.json", "vite.config.ts"][..],
            ),
            (
                "repo_src_root",
                repo_src_root(),
                &["main.ts", "backend", "bridge"][..],
            ),
            ("tests_root", tests_root(), &["backend", "e2e"][..]),
            (
                "backend_src_root",
                backend_src_root(),
                &["main.rs", "Cargo.toml"][..],
            ),
        ] {
            assert!(dir.is_dir(), "{name}() = {dir:?}，不是目录");
            for probe in probes {
                assert!(
                    dir.join(probe).exists(),
                    "{name}() = {dir:?} 下没有 {probe} —— 这个住址指错了地方"
                );
            }
        }
        // 两个 `src` 必须是不同的地方 —— 这是本文件最容易被误用的一格。
        assert_ne!(
            crate_src_root(),
            repo_src_root(),
            "crate_src_root() 与 repo_src_root() 撞了 —— 说明某一级爬错"
        );
        assert!(
            crate_root().starts_with(repo_src_root()),
            "本 crate 应当住在 <repo>/src/ 下面"
        );
    }
}
