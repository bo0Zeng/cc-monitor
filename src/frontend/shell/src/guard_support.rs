//! 测试用的**住址唯一源** —— 「仓根在哪 / 本 crate 的源码在哪 / 测试树在哪」只答一次。
//!
//! 🔴 **它为什么存在**：2026-09-18 装齐 Tauri 栈、本 crate 在本机第一次编译并跑测试，
//! 一次就红了 **160 条**。根因只有一个：仓库重组把本 crate 从 `<repo>/src-tauri` 搬到
//! `<repo>/src/bridge`，于是 `CARGO_MANIFEST_DIR` 到仓根**从一级变成两级**（收尾重排再下沉一级到
//! `<repo>/src/frontend/shell`，三级；这一回只改了 [`repo_root`] 一处），
//! 而「仓根在哪」这件事在本 crate 里有 **24 份各自独立的答案**，一份都没跟着改。
//!
//! 其中 `shared_crate_registry` 那份的 `.expect()` 消息已经被改成
//! 「`src/bridge` 的上级 = 仓根」——**注释改对了，代码没改**。这是最能说明问题的一处：
//! 散落的副本会各自漂，而没有任何判据数着它们。
//!
//! ⇒ 纪律（一条形状出现 N 次，就抽一个住址）
//! ＋ `§5.2`（扫描型测试拿不到人群会扫空集 ⇒ 恒绿 ⇒ 必须配反空真自检）。
//! 后端那一半早已这么做（`src/backend/guard_support.rs`），本文件是它的对侧。

#![cfg(test)]

use std::path::{Path, PathBuf};

/// 本 crate 的包根 —— `<repo>/src/frontend/shell`。
pub(crate) fn crate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 本 crate 自己的源码树 —— `<repo>/src/frontend/shell/src`。
/// ⚠ 与 `repo_src_root()` 不是一件事，别混。
/// 拿它当根交给 `guard_core` 扫的时候，人群还含本包 manifest 明写的兄弟源码树（`[package.metadata.guard] population`：
/// 通道 · 前端宿主原语 · 开窗契约 · 文件窗口，`guard_core::population_trees`）—— monitor 的代码搬进去了，人群不变；住址 `<包名>/…`。
pub(crate) fn crate_src_root() -> PathBuf {
    crate_root().join("src")
}

/// 本 crate 源码人群的全部根：自己的 `src/` ＋ manifest 明写的兄弟包的 `src/`（`guard_core::population_trees`）。
/// 自己走目录（不经 `guard_core` 遍历器）的那几条判据按它逐棵走 —— 人群与经遍历器的那几条同一群，声明只有 manifest 那一处。
pub(crate) fn crate_population_roots() -> Vec<PathBuf> {
    std::iter::once(crate_src_root())
        .chain(
            guard_core::population_trees(&crate_src_root())
                .into_iter()
                .map(|(_, src)| src),
        )
        .collect()
}

/// 仓根 —— `<repo>`。**`src/frontend/shell` 的上三级**（包根从 `src/bridge` 下沉一级）。
pub(crate) fn repo_root() -> PathBuf {
    crate_root()
        .ancestors()
        .nth(3)
        .expect("src/frontend/shell 的上三级 = 仓根")
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

/// 后端命令表各族（`src/backend/stream/inbound/registry/*.rs`）：`(仓内相对路径, 生产段)`，按路径排，一族一份。
///
/// 按 `CommandSpec {` 切块的判据要**逐份切**，别把几份拼起来再切：拼起来的话，
/// 一份的文件头（`use` · 常量）会粘到上一份的最后一块上，被算成那条命令的一部分。
pub(crate) fn backend_registry_sources() -> Vec<(String, String)> {
    let root = repo_root();
    let mut out: Vec<(String, String)> = guard_core::scan_tree_excluding(
        &backend_src_root().join("stream/inbound/registry"),
        &["rs"],
        &[],
    )
    .into_iter()
    .map(|(path, src)| {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        (rel, guard_core::production_code(&src))
    })
    .collect();
    out.sort();
    assert!(
        out.len() >= 2,
        "只扫到 {} 份后端命令表族文件 —— 住址坏了，用它的判据会在空人群上恒绿",
        out.len()
    );
    out
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/guard_support_tests.rs"]
mod tests;
