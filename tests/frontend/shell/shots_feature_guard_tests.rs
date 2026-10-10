//! **截图台架那个无头壳只在台架编译时有** 的判据。
//!
//! 截图台架（`tests/shots/`）要让会话流走壳的真代码（`stream_source` → `session_book` → `event_replay` → 通道交格），
//! 于是本包有一个只在特性 `shots` 下编的模块 `shots_shell` 和一个例子程序 `ccm-shots-shell`（由 `tests/shots/real/pool.mjs` 编、起）。
//! 发版构建里不许带上它：它从标准输入收命令、按命令起进程，装进用户手里的二进制就是一个后门。
//!
//! | # | 判什么 |
//! |---|---|
//! | S1 | `Cargo.toml`：有特性 `shots`、它不开任何依赖的特性；`default` 里没有它；例子 `ccm-shots-shell` 带 `required-features = ["shots"]`；没有任何 `[[bin]]` 指向那个模块 |
//! | S2 | `lib.rs`：`shots_shell` 那一行模块声明紧跟在 `#[cfg(feature = "shots")]` 下面；生产源码里别处不提这个模块 |
//! | S3 | 全仓里点名特性 `shots` 去编的地方只有台架（`tests/shots/`）；发版 / CI 工作流与门禁脚本里没有 `--features …shots…`，也没有 `--all-features` |
//! | S4 | 默认构建（本测试自己这一档）里特性没开 |

use crate::guard_support::{crate_root, crate_src_root, repo_root};
use std::path::Path;

const FEATURE: &str = "shots";
const MODULE: &str = "shots_shell";
const EXAMPLE: &str = "ccm-shots-shell";

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("读不到 {}：{e}", p.display()))
}

/// `Cargo.toml` 拆成一段一段：（段头, 这一段里 `键 = 值` 的那几行）。只认本包这份手写清单的形状（键值一行一条）。
fn manifest_sections() -> Vec<(String, Vec<(String, String)>)> {
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for l in read(&crate_root().join("Cargo.toml")).lines() {
        let t = l.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if t.starts_with('[') {
            out.push((t.to_string(), Vec::new()));
        } else if let (Some((k, v)), Some(last)) = (t.split_once('='), out.last_mut()) {
            last.1.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    out
}

fn get<'a>(kv: &'a [(String, String)], key: &str) -> Option<&'a str> {
    kv.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

#[test]
fn s1_manifest_keeps_the_feature_off_by_default_and_only_an_example_needs_it() {
    let m = manifest_sections();
    let features = m
        .iter()
        .find(|(h, _)| h == "[features]")
        .map(|(_, kv)| kv)
        .expect("Cargo.toml 有 [features]");
    assert_eq!(
        get(features, FEATURE),
        Some("[]"),
        "[features] 里有 `{FEATURE} = []`（它不开任何依赖的特性，只管编不编那个模块）"
    );
    if let Some(def) = get(features, "default") {
        assert!(
            !def.contains(&format!("\"{FEATURE}\"")),
            "`default` 里不许有 `{FEATURE}`：{def}"
        );
    }
    let examples: Vec<&Vec<(String, String)>> = m
        .iter()
        .filter(|(h, _)| h == "[[example]]")
        .map(|(_, kv)| kv)
        .collect();
    let ex = examples
        .iter()
        .find(|kv| get(kv, "name") == Some(&format!("\"{EXAMPLE}\"")))
        .unwrap_or_else(|| panic!("[[example]] 里有 `{EXAMPLE}`"));
    assert_eq!(
        get(ex, "required-features"),
        Some(format!("[\"{FEATURE}\"]").as_str()),
        "例子 `{EXAMPLE}` 只在特性 `{FEATURE}` 下编"
    );
    let path = get(ex, "path")
        .expect("例子写明 path")
        .trim_matches('"')
        .to_string();
    assert!(crate_root().join(&path).is_file(), "例子那份源码在：{path}");
    for (_, b) in m.iter().filter(|(h, _)| h == "[[bin]]") {
        let p = get(b, "path").unwrap_or_default().trim_matches('"');
        assert!(
            p != path && !p.contains(MODULE),
            "发版的二进制（[[bin]]）不许指向台架那个小程序：{p}"
        );
    }
    // 依赖里没有谁替它开（`monitor/shots` 这一形）。
    for (h, kv) in &m {
        for (k, v) in kv {
            assert!(
                !v.contains(&format!("/{FEATURE}\"")),
                "{h} 里 `{k}` 替别人开了台架特性：{v}"
            );
        }
    }
}

#[test]
fn s2_the_module_is_declared_only_under_the_feature() {
    let lib = read(&crate_src_root().join("lib.rs"));
    let lines: Vec<&str> = lib.lines().collect();
    let decl: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            let t = l.trim_start();
            !t.starts_with("//") && t.contains(&format!("mod {MODULE}"))
        })
        .map(|(i, _)| i)
        .collect();
    assert_eq!(decl.len(), 1, "lib.rs 里 `mod {MODULE}` 恰好声明一次");
    let above = lines[decl[0] - 1].trim();
    assert_eq!(
        above,
        format!("#[cfg(feature = \"{FEATURE}\")]"),
        "`mod {MODULE}` 紧跟在特性闸下面"
    );
    // 生产源码里别处不提它（提了就是默认构建也要它）。
    let mut hits = Vec::new();
    for f in walk(&crate_src_root()) {
        let rel = f
            .strip_prefix(crate_src_root())
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "lib.rs" || rel == format!("{MODULE}.rs") {
            continue;
        }
        let src = read(&f);
        if src
            .lines()
            .any(|l| !l.trim_start().starts_with("//") && l.contains(&format!("{MODULE}::")))
        {
            hits.push(rel);
        }
    }
    assert!(hits.is_empty(), "生产源码里别处引了 `{MODULE}`：{hits:?}");
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
    out
}

#[test]
fn s3_only_the_screenshot_rig_builds_with_the_feature() {
    let root = repo_root();
    // 会去编发版 / 跑 CI / 跑门禁的那几处：一处都不许点名这个特性，也不许 `--all-features`。
    let mut scanned = 0;
    let mut bad = Vec::new();
    for dir in [".github/workflows", "tests/scripts"] {
        for e in std::fs::read_dir(root.join(dir)).unwrap().flatten() {
            let p = e.path();
            if !p.is_file() {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&p) else {
                continue;
            };
            scanned += 1;
            for (i, l) in src.lines().enumerate() {
                let feat = l.contains("--features") && l.contains(FEATURE);
                if feat || l.contains("--all-features") {
                    bad.push(format!("{}:{}: {}", p.display(), i + 1, l.trim()));
                }
            }
        }
    }
    assert!(
        scanned > 5,
        "人群不空（工作流与门禁脚本读到了 {scanned} 份）"
    );
    assert!(bad.is_empty(), "发版 / CI / 门禁里点名了台架特性：{bad:#?}");
    // 台架自己那一处是真的在编它（不然上面那条是在守一个不存在的东西）。
    let pool = read(&root.join("tests/shots/real/pool.mjs"));
    assert!(
        pool.contains(&format!("\"--features\", \"{FEATURE}\"")) && pool.contains(EXAMPLE),
        "台架（tests/shots/real/pool.mjs）按特性 `{FEATURE}` 编 `{EXAMPLE}`"
    );
}

#[test]
fn s4_default_build_has_the_feature_off() {
    assert!(
        !cfg!(feature = "shots"),
        "默认构建（cargo test 这一档）里特性 `shots` 没开"
    );
}
