//! 测试用的**住址唯一源** —— 「仓根在哪 / 本 crate 的源码在哪 / 测试树在哪」只答一次。
//!
//! 🔴 **它为什么存在**：2026-09-18 装齐 Tauri 栈、本 crate 在本机第一次编译并跑测试，
//! 一次就红了 **160 条**。根因只有一个：仓库重组把本 crate 从 `<repo>/src-tauri` 搬到
//! `<repo>/src/bridge`，于是 `CARGO_MANIFEST_DIR` 到仓根**从一级变成两级**（〔RE〕收尾重排再下沉一级到
//! `<repo>/src/frontend/shell`，三级；这一回只改了 [`repo_root`] 一处），
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

/// 本 crate 的包根 —— `<repo>/src/frontend/shell`。
pub(crate) fn crate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 本 crate 自己的源码树 —— `<repo>/src/frontend/shell/src`。
/// ⚠ 与 `repo_src_root()` 不是一件事，别混。
pub(crate) fn crate_src_root() -> PathBuf {
    crate_root().join("src")
}

/// 仓根 —— `<repo>`。**`src/frontend/shell` 的上三级**（〔RE〕包根从 `src/bridge` 下沉一级）。
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
/// 〔P4〕monitor 这个 crate 之外、也编进前端进程里的源码树（`src/common/host-core`：前端宿主原语 ·
/// `src/common/chan-core`：通道，它顺 `#[path]` 带进 `src/comms/inward/` 那几份成员）。
/// 按「这个前端进程里有谁在写盘 / 起目录 / 原子替换 / 走后端」数人群的判据，除本 crate 外还要扫它们。
pub(crate) fn other_frontend_src_roots() -> Vec<PathBuf> {
    vec![
        repo_src_root().join("common/host-core/src"),
        repo_src_root().join("common/chan-core/src"),
    ]
}

pub(crate) fn backend_src_root() -> PathBuf {
    repo_src_root().join("backend")
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/guard_support_tests.rs"]
mod tests;
