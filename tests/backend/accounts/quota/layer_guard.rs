//! 轮换后端两块的方向：配置编辑层（`accounts/quota/setup/` ＋ 配置那几份面）可以问基础功能层，基础功能层一处都不许引配置编辑层。
//! 基础层 ＝ `accounts/quota/` 里除 `setup/` 外的全部 · `accounts/upstream_select/` 全部 · 基础层那几份面。
//! 两块的接线只在 `accounts/quota/mod.rs`（挂模块）与 `faces/rotation_face.rs`（`Ctx` · 交给中转的那一份），不算任何一层。
//! 只看生产段（测试跨层造夹具是正常的）；把符号先 `pub use` 到第三处再引那种间接扫不到。

use guard_core::production_code;
use std::path::{Path, PathBuf};

/// 配置编辑层的名字：模块 `setup` 与配置那几份面。基础层生产段里这几个词一个都不许出现（不管怎么拼进来）。
const CONFIG_NAMES: &[&str] = &[
    "setup",
    "rotation_view_face",
    "rotation_edit_face",
    "rotation_try_face",
];

/// 基础层那几份面（`src/backend/faces/` 下）。
const BASE_FACES: &[&str] = &["rotation_facts_face.rs", "rotation_act_face.rs"];

/// 两块的接线（`src/backend/` 下）。
const WIRING: &[&str] = &["accounts/quota/mod.rs", "faces/rotation_face.rs"];

/// 树上 `dir` 下全部 `.rs`（绝对路径）。
fn walk(dir: &Path) -> Vec<PathBuf> {
    guard_core::scan_tree!(dir, &["rs"])
        .into_iter()
        .map(|(p, _)| p)
        .collect()
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// `code` 里以整词出现的配置层名字（`setup_dir` · `rotation_view_face_tests` 不算）。
fn config_hits(code: &str) -> Vec<&'static str> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    CONFIG_NAMES
        .iter()
        .copied()
        .filter(|n| {
            code.match_indices(n).any(|(i, _)| {
                let before = code[..i].chars().next_back();
                let after = code[i + n.len()..].chars().next();
                !before.is_some_and(word) && !after.is_some_and(word)
            })
        })
        .collect()
}

/// 基础层的文件（相对 `src/backend/`）· `accounts/quota/` 下按层分的两份 ＋ 接线，与树上数出来的比。
fn base_files(root: &Path) -> Vec<String> {
    let quota = root.join("accounts/quota");
    let all = walk(&quota);
    let setup = quota.join("setup");
    let mut base: Vec<String> = all
        .iter()
        .filter(|p| !p.starts_with(&setup))
        .map(|p| rel(root, p))
        .filter(|r| !WIRING.contains(&r.as_str()))
        .collect();
    let config = walk(&setup);
    let wired = all
        .iter()
        .filter(|p| WIRING.contains(&rel(root, p).as_str()))
        .count();
    assert_eq!(
        base.len() + config.len() + wired,
        all.len(),
        "accounts/quota/ 下的文件没分干净：基础层 {} ＋ 配置层 {} ＋ 接线 {wired} ≠ 树上 {}",
        base.len(),
        config.len(),
        all.len()
    );
    assert!(
        !config.is_empty(),
        "accounts/quota/setup/ 里一个 .rs 都没有 —— 配置编辑层是不是挪走了？"
    );
    let up = walk(&root.join("accounts/upstream_select"));
    base.extend(up.iter().map(|p| rel(root, p)));
    for f in BASE_FACES {
        let p = root.join("faces").join(f);
        assert!(p.is_file(), "基础层的面 {f} 不在了：表跟着改");
        base.push(rel(root, &p));
    }
    base.sort();
    base
}

#[test]
fn the_needle_sees_every_way_in() {
    for code in [
        "use crate::accounts::quota::setup::shelf;",
        "use super::super::setup;",
        "use crate::accounts::quota::{rules, setup};",
        "let x = super::setup::plan_at(None, \"\", None);",
        "crate::faces::rotation_view_face::view_with(&c)",
    ] {
        assert!(!config_hits(code).is_empty(), "该抓的没抓到：{code}");
    }
    for code in [
        "let setup_dir = 1;",
        "mod rotation_view_face_tests;",
        "fn resetup() {}",
    ] {
        assert!(config_hits(code).is_empty(), "不该抓的抓了：{code}");
    }
}

#[test]
fn the_base_layer_never_names_the_config_layer() {
    let root = guard_core::addr::backend_src_root();
    let mut bad = Vec::new();
    for f in base_files(&root) {
        let src = std::fs::read_to_string(root.join(&f)).expect("read rs");
        let prod = production_code(&src);
        guard_core::assert_no_test_code(&f, &prod);
        for n in config_hits(&prod) {
            bad.push(format!("{f} → {n}"));
        }
    }
    assert!(
        bad.is_empty(),
        "基础功能层引了配置编辑层（只许配置层问基础层）：\n{}",
        bad.join("\n")
    );
}
