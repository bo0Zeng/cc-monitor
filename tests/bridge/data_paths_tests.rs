//! # 要求住址：`设计/70 §6.2`（数据位置：逐个文件、目录不算大小、说明不说内部词）＋ `INVARIANTS §2.1`（真相 / 缓存两类）
//!
//! 核原文：`设计/70 §6.2` 红线格逐字「③ 不递归算目录大小 ⇒ 目录行只显示「已创建」，是刻意的」——
//! `probe_dir_never_returns_size` 判这一句；同节「今天 / 改成」表逐字「去掉 `sid` / `HWND`」——
//! `no_entry_description_speaks_our_internal_words` 判这一句；每项的类与 `INVARIANTS §2.1` 那张表两向相等。
//! ⚠ 那条判据原先自称出自「`70 §10.2` 差项 4」，`设计/70` 里没有那一节（已改指 `§6.2`；生产侧同一处注释在写区外，未改）。〔JA1 点址 2026-09-24〕

use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

struct TestDir(PathBuf);
impl TestDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!(
            "ccm-data-paths-test-{}-{tag}-{n}",
            std::process::id(),
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        TestDir(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn probe_file_returns_exists_and_size_when_present() {
    let d = TestDir::new("probe-file");
    let p = d.path().join("x.json");
    fs::write(&p, "hello").unwrap();
    let info = probe_file(p.clone(), "x.json", "test", DataClass::Truth);
    assert!(info.exists);
    assert_eq!(info.size_bytes, Some(5));
    assert_eq!(info.kind, "file");
}

#[test]
fn probe_file_returns_not_exists_when_absent() {
    let d = TestDir::new("probe-file-absent");
    let info = probe_file(
        d.path().join("absent.json"),
        "absent.json",
        "test",
        DataClass::Cache,
    );
    assert!(!info.exists);
    assert!(info.size_bytes.is_none());
}

#[test]
fn probe_dir_never_returns_size() {
    let d = TestDir::new("probe-dir");
    fs::write(d.path().join("a"), "abc").unwrap();
    let info = probe_dir(d.path().to_path_buf(), "x/", "test", DataClass::Cache);
    assert!(info.exists);
    assert!(info.size_bytes.is_none());
    assert_eq!(info.kind, "dir");
}

// 〔OSA · 主会话 09-28 裁〕这里原来有「目录里认得出 `.ccm-backup-`」那一条 —— 那一格随 `$PROFILE` 备份搬到界面问本机后端
//   （`tests/settings/profile-backups.vitest.ts`）。

// ── 〔第四波 ST2 · 用户 09-24 裁「真相 / 缓存列提前做」〕────────────────────────

/// 把 `INVARIANTS.md §2.1` 那张散文表读成 `(名字, 类)`。**异源**：那张表是人写的散文，
/// 枚举是代码 —— 两边各自漂了才会不等。「混（良性）」按真相记（理由见 `DataClass` 头注）。
fn invariants_classes(md: &str) -> Vec<(String, DataClass)> {
    let start = md
        .find("### 2.1 真相 vs 缓存")
        .expect("INVARIANTS.md 里找不到 §2.1 —— 标题改了就来改这条");
    let sec = &md[start..];
    let end = sec[4..].find("\n## ").map_or(sec.len(), |i| i + 4);
    let mut out = Vec::new();
    for line in sec[..end].lines() {
        let cols: Vec<&str> = line.split('|').map(str::trim).collect();
        if cols.len() < 4 || !cols[1].starts_with('`') {
            continue;
        }
        let class = if cols[2].contains("真相") || cols[2].contains("混") {
            DataClass::Truth
        } else if cols[2].contains("缓存") {
            DataClass::Cache
        } else {
            panic!("§2.1 那一行的类认不出来：{line}")
        };
        for name in cols[1].split('`').skip(1).step_by(2) {
            out.push((name.to_string(), class));
        }
    }
    out
}

/// ★★ 枚举里每一项的类 == `INVARIANTS §2.1` 那张表（两向集合相等，异源）。
///
/// 反空真：表至少抽出 3 条真相、3 条缓存（少于这个数说明抽取器坏了，下面的相等在小人群上成立）。
#[test]
fn every_entry_carries_the_class_the_invariants_table_gives_it() {
    let md = include_str!("../../src/doc/INVARIANTS.md");
    let mut want = invariants_classes(md);
    want.sort_by(|a, b| a.0.cmp(&b.0));
    let truths = want.iter().filter(|(_, c)| *c == DataClass::Truth).count();
    let caches = want.iter().filter(|(_, c)| *c == DataClass::Cache).count();
    assert!(
        truths >= 3 && caches >= 3,
        "从 §2.1 只抽出 {truths} 条真相 / {caches} 条缓存 —— 抽取器坏了"
    );
    let d = TestDir::new("classes");
    let mut got: Vec<(String, DataClass)> = monitor_entries(d.path())
        .into_iter()
        .map(|e| (e.label, e.class))
        .collect();
    got.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        got, want,
        "data dir 枚举里的类与 `INVARIANTS §2.1` 那张表对不上。\n\
         ⇒ 新加的文件要**同拍**两处：`data_paths.rs::monitor_entries` 选类，§2.1 那张表加一行。"
    );
}

/// ★ 线上形状：`class` 是**必有**的一格，值只有 `truth` / `cache` 两种（TS 那侧按这两个值分档）。
#[test]
fn the_class_goes_over_the_wire_as_two_lowercase_words() {
    let d = TestDir::new("wire");
    let mut seen = std::collections::BTreeSet::new();
    for e in monitor_entries(d.path()) {
        let v = serde_json::to_value(&e).unwrap();
        seen.insert(v["class"].as_str().expect("class 这一格缺席了").to_string());
    }
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        vec!["cache".to_string(), "truth".to_string()]
    );
}

/// ★ `设计/70 §6.2`「今天 / 改成」表那一行：条目说明里不许再有 R1 那两个词（`sid` / `HWND`）。带正控。
/// 〔JA1 2026-09-24〕原写「`70 §10.2` 差项 4」—— `设计/70` 里已没有那一节，改指今天写着这件事的那一节。
#[test]
fn no_entry_description_speaks_our_internal_words() {
    let hits = |s: &str| {
        let low = s.to_lowercase();
        low.split(|c: char| !c.is_ascii_alphanumeric())
            .any(|w| w == "sid" || w == "hwnd")
    };
    assert!(
        hits("cc 集成的 sid → 终端 HWND 持久绑定"),
        "正控没逮到 —— 尺子瞎了"
    );
    let d = TestDir::new("r1");
    let bad: Vec<String> = monitor_entries(d.path())
        .into_iter()
        .filter(|e| hits(&e.description))
        .map(|e| format!("{}：{}", e.label, e.description))
        .collect();
    assert_eq!(bad, Vec::<String>::new(), "条目说明里又出现了 sid / HWND");
}
