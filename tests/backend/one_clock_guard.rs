//! **后端的「现在」只有一处**：各处取当前墙钟一律调 `common::time::now`（及它的几个读法 `now_secs` · `now_ms`），
//! 别处不许直接问系统钟。只有一处，截图台架才能把「现在」给死（台架起每台后端时交一个时刻，页面在同一刻拨 `Date`）。
//!
//! | # | 判什么 |
//! |---|---|
//! | C1 | 生产段里除 `common/time.rs` 外，没有 `SystemTime::now` · `Utc::now` · `Local::now` · `Timestamp::now` · `Zoned::now`，也没有拿 `Instant` 去换算墙钟（同一行里 `SystemTime` 与 `elapsed(` / `Instant` 碰头）|
//! | C2 | 拨钟那一段只在特性 `shots` 下编：`Cargo.toml` 的 `shots = []` 不在 `default` 里；`common/time.rs` 里 `mod shots_clock` 紧跟在 `#[cfg(feature = "shots")]` 下；台架那个环境变量名只在那一段里出现，生产源码别处不提 |
//! | C3 | 台架真在编它、真在交时刻：`tests/shots/real/pool.mjs` 按 `--features shots` 编后端、给每台后端设那个环境变量 |
//! | C4 | 默认构建（本测试这一档）里特性没开；`now()` 就是系统钟 |
//!
//! 买不到：读的是源码文本；把系统钟包进别的名字（`use std::time::SystemTime as S; S::now()`）认不出。
//! 发版 / CI / 门禁里不许点名特性 `shots`，由壳那一侧的 `shots_feature_guard_tests::s3` 一并钉着（它扫的是全仓的工作流与门禁脚本）。

use std::path::Path;

const FEATURE: &str = "shots";
const CLOCK_FILE: &str = "common/time.rs";
const MODULE: &str = "shots_clock";
/// 台架交时刻的那个环境变量（unix 毫秒）。
const ENV: &str = "CCM_SHOTS_NOW_MS";

const WALL_CLOCK_CALLS: &[&str] = &[
    "SystemTime::now",
    "Utc::now",
    "Local::now",
    "Timestamp::now",
    "Zoned::now",
];

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("读不到 {}：{e}", p.display()))
}

/// 一份生产段里直接问系统钟的那几行。
fn wall_clock_sites(prod: &str) -> Vec<String> {
    guard_core::strip_comment_lines(prod)
        .lines()
        .filter(|l| {
            let flat: String = l.chars().filter(|c| !c.is_whitespace()).collect();
            WALL_CLOCK_CALLS.iter().any(|c| flat.contains(c))
                || (flat.contains("SystemTime") && (flat.contains("elapsed(") || flat.contains("Instant")))
        })
        .map(|l| l.trim().to_string())
        .collect()
}

fn production_files() -> Vec<(String, String)> {
    let root = crate::guard_support::src_root();
    guard_core::scan_tree_excluding(&root, &["rs"], &[])
        .into_iter()
        .map(|(p, src)| {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, crate::guard_support::production_code(&src))
        })
        .collect()
}

#[test]
fn c1_only_common_time_reads_the_wall_clock() {
    let files = production_files();
    assert!(files.len() > 200, "人群不空（读到 {} 份 .rs）", files.len());
    assert!(
        files.iter().any(|(rel, _)| rel == CLOCK_FILE),
        "{CLOCK_FILE} 在人群里"
    );
    let mut bad = Vec::new();
    for (rel, prod) in &files {
        if rel == CLOCK_FILE {
            continue;
        }
        for l in wall_clock_sites(prod) {
            bad.push(format!("{rel}: {l}"));
        }
    }
    assert!(
        bad.is_empty(),
        "后端的「现在」只有一处：改调 `crate::common::time::{{now, now_secs, now_ms}}`\n{bad:#?}"
    );
    // 那一处自己真在问系统钟（不然上面是在守一个空名字）。
    let clock = &files.iter().find(|(rel, _)| rel == CLOCK_FILE).unwrap().1;
    assert!(
        !wall_clock_sites(clock).is_empty(),
        "{CLOCK_FILE} 里有那一处取系统钟"
    );
}

#[test]
fn c1_the_shapes_are_recognised() {
    assert_eq!(wall_clock_sites("let t = std::time::SystemTime::now();").len(), 1);
    assert_eq!(wall_clock_sites("let t = SystemTime :: now ();").len(), 1);
    assert_eq!(wall_clock_sites("let t = chrono::Utc::now();").len(), 1);
    assert_eq!(wall_clock_sites("let z = jiff::Zoned::now();").len(), 1);
    assert_eq!(wall_clock_sites("let w = boot_wall + SystemTime::UNIX_EPOCH; t0.elapsed()").len(), 1);
    assert_eq!(wall_clock_sites("let t = crate::common::time::now();").len(), 0);
    assert_eq!(wall_clock_sites("// SystemTime::now() 不许").len(), 0);
    assert_eq!(wall_clock_sites("let d = t0.elapsed();").len(), 0);
}

#[test]
fn c2_the_clock_dial_only_compiles_under_the_feature() {
    let manifest = read(&crate::guard_support::src_root().join("Cargo.toml"));
    let mut in_features = false;
    let mut feat = None;
    let mut default = None;
    for l in manifest.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            in_features = t == "[features]";
            continue;
        }
        if !in_features {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            match k.trim() {
                k if k == FEATURE => feat = Some(v.trim().to_string()),
                "default" => default = Some(v.trim().to_string()),
                _ => {}
            }
        }
    }
    assert_eq!(
        feat.as_deref(),
        Some("[]"),
        "Cargo.toml 的 [features] 里有 `{FEATURE} = []`（它不开任何依赖的特性）"
    );
    if let Some(d) = default {
        assert!(!d.contains(&format!("\"{FEATURE}\"")), "`default` 里不许有 `{FEATURE}`：{d}");
    }

    let src = read(&crate::guard_support::src_root().join(CLOCK_FILE));
    let prod = crate::guard_support::production_code(&src);
    let lines: Vec<&str> = prod.lines().collect();
    let decl: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim() == format!("mod {MODULE} {{"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(decl.len(), 1, "{CLOCK_FILE} 里 `mod {MODULE}` 恰好一个");
    assert_eq!(
        lines[decl[0] - 1].trim(),
        format!("#[cfg(feature = \"{FEATURE}\")]"),
        "`mod {MODULE}` 紧跟在特性闸下面"
    );
    // 那个模块的体（到与它同缩进的 `}` 为止）。
    let indent = lines[decl[0]].len() - lines[decl[0]].trim_start().len();
    let end = (decl[0] + 1..lines.len())
        .find(|&i| lines[i].trim() == "}" && lines[i].len() - lines[i].trim_start().len() == indent)
        .expect("`mod shots_clock` 有收尾");
    let body = lines[decl[0]..=end].join("\n");
    assert!(body.contains(ENV), "环境变量 `{ENV}` 在 `mod {MODULE}` 里读");
    let rest = format!("{}\n{}", lines[..decl[0]].join("\n"), lines[end + 1..].join("\n"));
    assert!(!rest.contains(ENV), "{CLOCK_FILE} 里特性闸外读了 `{ENV}`");
    // 引用那一行（`shots_clock::now()`）也在特性闸下。
    let uses: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(i, l)| (*i < decl[0] || *i > end) && l.contains(&format!("{MODULE}::")))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(uses.len(), 1, "`{MODULE}::` 在模块外恰好用一次（`now()` 里）");
    assert_eq!(
        lines[uses[0] - 1].trim(),
        format!("#[cfg(feature = \"{FEATURE}\")]"),
        "`{MODULE}::now()` 那一句紧跟在特性闸下面"
    );

    // 生产源码别处不提它。
    let mut hits = Vec::new();
    for (rel, prod) in production_files() {
        if rel != CLOCK_FILE && (prod.contains(ENV) || guard_core::contains_word(&prod, MODULE)) {
            hits.push(rel);
        }
    }
    assert!(hits.is_empty(), "生产源码别处提了台架的拨钟：{hits:?}");
}

#[test]
fn c3_the_rig_builds_it_and_hands_a_moment() {
    let pool = read(&crate::guard_support::repo_root().join("tests/shots/real/pool.mjs"));
    let build = pool
        .lines()
        .find(|l| l.contains("cwd: path.join(repo, \"src/backend\")"))
        .expect("pool.mjs 里有编后端那一行");
    assert!(
        build.contains(&format!("\"--features\", \"{FEATURE}\"")),
        "台架按特性 `{FEATURE}` 编后端：{}",
        build.trim()
    );
    assert!(
        pool.contains(&format!("{ENV}:")),
        "台架给每台后端设 `{ENV}`"
    );
}

#[test]
fn c4_default_build_reads_the_system_clock() {
    assert!(!cfg!(feature = "shots"), "默认构建里特性 `shots` 没开");
    let sys = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let ours = crate::common::time::now_ms();
    assert!((ours - sys).abs() < 5_000, "默认构建里 `now_ms()` 就是系统钟：{ours} vs {sys}");
}
