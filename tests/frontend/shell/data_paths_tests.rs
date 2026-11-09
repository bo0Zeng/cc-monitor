//! # （数据位置：逐个文件、目录不算大小、说明不说内部词）＋ `INVARIANTS §2.1`（真相 / 缓存两类）
//!
//! 核原文：红线格逐字「③ 不递归算目录大小 ⇒ 目录行只显示「已创建」，是刻意的」——
//! `probe_dir_never_returns_size` 判这一句；同节「今天 / 改成」表逐字「去掉 `sid` / `HWND`」——
//! `no_entry_description_speaks_our_internal_words` 判这一句；每项的类与 `INVARIANTS §2.1` 那张表两向相等。
//! ⚠ 那条判据原先自称出自「差项 4」，里没有那一节（已改指 `§6.2`；生产侧同一处注释未改）。

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
    assert_eq!(info.size_text, Some(copy_core::size_text(5)));
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
    assert!(info.size_text.is_none());
}

#[test]
fn probe_dir_never_returns_size() {
    let d = TestDir::new("probe-dir");
    fs::write(d.path().join("a"), "abc").unwrap();
    let info = probe_dir(d.path().to_path_buf(), "x/", "test", DataClass::Cache);
    assert!(info.exists);
    assert!(info.size_text.is_none());
    assert_eq!(info.kind, "dir");
}

// 这里原来有「目录里认得出 `.ccm-backup-`」那一条 —— 那一格随 `$PROFILE` 备份搬到界面问本机后端
//   （`tests/frontend/ui/settings/profile-backups.vitest.ts`）。

// ── 〔真相 / 缓存列〕────────────────────────

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

/// ★★ 枚举里每一项的类 ＋ 本机后端家里那几样（契约 `relay_route_core::OWN_HOME_ENTRIES`，后端 `data-report` 照它列）的类
/// == `INVARIANTS §2.1` 那张表（两向集合相等，异源）。后端那几样的名字 ＝ 去掉家那一段、目录带尾 `/`（同界面上那一格）；
/// 后端那边另有一条只比它自家那几样（`footprint/data_tests.rs`），这一条管两边拼起来一样不多一样不少。
///
/// 反空真：表至少抽出 3 条真相、3 条缓存（少于这个数说明抽取器坏了，下面的相等在小人群上成立）。
#[test]
fn every_entry_carries_the_class_the_invariants_table_gives_it() {
    let md = include_str!("../../../src/doc/INVARIANTS.md");
    let mut want = invariants_classes(md);
    want.sort_by(|a, b| a.0.cmp(&b.0));
    let truths = want.iter().filter(|(_, c)| *c == DataClass::Truth).count();
    let caches = want.iter().filter(|(_, c)| *c == DataClass::Cache).count();
    assert!(
        truths >= 3 && caches >= 3,
        "从 §2.1 只抽出 {truths} 条真相 / {caches} 条缓存 —— 抽取器坏了"
    );
    let d = TestDir::new("classes");
    let backend = relay_route_core::OWN_HOME_ENTRIES
        .iter()
        .map(|&(_, rel, dir, truth)| {
            let tail = rel.trim_start_matches(".cc-monitor/");
            let name = if dir {
                format!("{tail}/")
            } else {
                tail.to_string()
            };
            (
                name,
                if truth {
                    DataClass::Truth
                } else {
                    DataClass::Cache
                },
            )
        });
    let mut got: Vec<(String, DataClass)> = monitor_entries(d.path())
        .into_iter()
        .map(|e| (e.label, e.class))
        .chain(backend)
        .collect();
    got.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        got, want,
        "数据位置的类与 `INVARIANTS §2.1` 那张表对不上。\n\
         ⇒ 新加的文件要**同拍**两处：`data_paths.rs::monitor_entries`（界面这一侧）或 `relay_route_core::OWN_HOME_ENTRIES`（本机后端家里）选类，§2.1 那张表加一行。"
    );
}

/// 落盘声明表里故意不在「文件与数据」那一页列的：`(声明的落点, 为什么)`。
const NOT_LISTED: &[(&str, &str)] = &[(
    "<account-config-dir>/<credentials-file>",
    "住每个号自己的配置目录（落点随号）· 不在 cc-monitor 的家里 · 号由账号页管",
)];

/// 落盘声明表（`relay_route_core` 的 `OWN_FILES`，只在那个 crate 的测试档里有）每一行的落点：读那份源码原文，
/// 照 `OWN_FILES` 列的名字找各自的 `Decl` 声明，取第一个参数（字面量，或 `&str` 常量 ⇒ 再取那个常量的字面量）。
fn declared_rels(src: &str) -> Vec<String> {
    // 常量写成字面量，或由布局宏给（`pub const X: &str = x_rel!();` ⇒ 到 `macro_rules! x_rel` 里取那一个字面量）。
    let first_lit = |s: &str| -> Option<String> {
        let q = s.find('"')? + 1;
        Some(s[q..q + s[q..].find('"')?].to_string())
    };
    let lit_of = |name: &str| -> Option<String> {
        let head = format!("pub const {name}: &str = ");
        let rhs = &src[src.find(&head)? + head.len()..];
        if rhs.starts_with('"') {
            return first_lit(rhs);
        }
        let mac = rhs.split("!()").next()?.trim();
        first_lit(&src[src.find(&format!("macro_rules! {mac} {{"))?..])
    };
    let start = src
        .find("const OWN_FILES: &[Row] = &[")
        .expect("声明表 `OWN_FILES` 不在了 —— 改了名就来改这条");
    let body = &src[start..start + src[start..].find("];").unwrap()];
    body.lines()
        .filter_map(|l| l.trim().strip_suffix(".row,"))
        .map(|name| {
            let at = src
                .find(&format!("pub const {name}: Decl<"))
                .unwrap_or_else(|| panic!("{name}：找不到它的 Decl 声明"));
            let rest = &src[at..];
            // 头一个构造口（`::json(` · `::foreign(` 里先出现的那个，免得跨到下一份声明）。
            let open = ["::json(", "::foreign("]
                .iter()
                .filter_map(|k| rest.find(k).map(|i| i + k.len()))
                .min();
            let open = open.unwrap_or_else(|| panic!("{name}：认不出 Decl 的写法"));
            let arg = rest[open..rest[open..].find(',').unwrap() + open].trim();
            match arg.strip_prefix('"') {
                Some(q) => q.trim_end_matches('"').to_string(),
                None => {
                    lit_of(arg).unwrap_or_else(|| panic!("{name}：落点常量 {arg} 取不到字面量"))
                }
            }
        })
        .collect()
}

/// ★★ 「文件与数据」那一页列全落盘声明表（`relay_route_core` 的 `OWN_FILES`）：每一份声明要么被界面这一侧（`monitor_entries`）或
/// 本机后端家里那几样（`OWN_HOME_ENTRIES`）列着 —— 同名，或落在列着的那个目录底下 —— 要么在 [`NOT_LISTED`] 里写明为什么。
/// 名单里的每一行都得真在声明表里、又真没被列（死名单红）。
#[test]
fn every_declared_file_is_listed_on_the_data_page() {
    const HOME: &str = ".cc-monitor/";
    let own_files = declared_rels(include_str!(
        "../../../src/common/relay-route-core/src/lib.rs"
    ));
    let d = TestDir::new("listed");
    let listed: Vec<String> = monitor_entries(d.path())
        .into_iter()
        .map(|e| e.label)
        .chain(
            relay_route_core::OWN_HOME_ENTRIES
                .iter()
                .map(|&(_, rel, dir, _)| {
                    let tail = rel.trim_start_matches(HOME);
                    if dir {
                        format!("{tail}/")
                    } else {
                        tail.to_string()
                    }
                }),
        )
        .collect();
    let covered = |rel: &str| {
        rel.strip_prefix(HOME).is_some_and(|tail| {
            listed
                .iter()
                .any(|n| n == tail || (n.ends_with('/') && tail.starts_with(n.as_str())))
        })
    };
    assert!(
        own_files.len() >= 20 && own_files.iter().all(|r| !r.is_empty()),
        "声明表只抽出 {} 份：抽取器坏了，下面的比较是空真",
        own_files.len()
    );
    let missing: Vec<&str> = own_files
        .iter()
        .map(String::as_str)
        .filter(|rel| !covered(rel) && !NOT_LISTED.iter().any(|(n, _)| n == rel))
        .collect();
    assert_eq!(
        missing,
        Vec::<&str>::new(),
        "这几份声明了却不在「文件与数据」那一页 ⇒ 列进 `OWN_HOME_ENTRIES` 或 `monitor_entries`，或写进 NOT_LISTED 并写明为什么"
    );
    for (rel, why) in NOT_LISTED {
        assert!(!why.is_empty(), "{rel}：名单里没写为什么");
        assert!(
            own_files.iter().any(|r| r == rel),
            "{rel}：名单里这一行不在声明表里（死名单）"
        );
        assert!(!covered(rel), "{rel}：已经列着了，名单里这一行是死的");
    }
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

/// ★ 「今天 / 改成」表那一行：条目说明里不许再有 R1 那两个词（`sid` / `HWND`）。带正控。
/// 原写「差项 4」—— 里已没有那一节，改指今天写着这件事的那一节。
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

/// ★ 本机后端写的那几份（历史注解 · API key 表连同家里那几样）不归这里列，由后端照 `OWN_HOME_ENTRIES` 列、逐样 stat
/// （界面照本机后端 `data-report` 的 `own` 画；远端那台也列）。数据目录里与后端相关、却只住界面这一侧的只剩宿主交给后端的错误输出
/// 所在的目录，路径取 `logging::backend_stderr_log_path`。期望逐条手写，不从被测函数派生。
#[test]
fn the_backend_rows_on_this_side_point_where_they_are_written() {
    let data = TestDir::new("backend-side");
    let entries = monitor_entries(data.path());
    let got: Vec<(String, PathBuf, String)> = entries
        .iter()
        .map(|e| (e.label.clone(), PathBuf::from(&e.path), e.kind.clone()))
        .filter(|(l, _, _)| l == "logs/backend/")
        .collect();
    let row = |l: &str, p: PathBuf, k: &str| (l.to_string(), p, k.to_string());
    assert_eq!(
        got,
        vec![row(
            "logs/backend/",
            crate::logging::backend_stderr_log_path(data.path())
                .parent()
                .unwrap()
                .to_path_buf(),
            "dir",
        )]
    );
    for backend_owned in ["history-metadata.json", "apikey-credentials.json"] {
        assert!(
            entries.iter().all(|e| e.label != backend_owned),
            "`{backend_owned}` 归本机后端列（`OWN_HOME_ENTRIES`），壳又替它 stat 了"
        );
    }
}

/// ★ 日志目录那一行在这份枚举里（设置面板按 `LOGS_DIR_LABEL` 认它、改指「日志」那一页；修在界面层，这一行不从枚举里删）。
#[test]
fn the_logs_dir_row_stays_in_the_list() {
    let data = TestDir::new("logs-row");
    let got: Vec<(String, PathBuf)> = monitor_entries(data.path())
        .into_iter()
        .filter(|e| e.label == crate::data_paths::LOGS_DIR_LABEL)
        .map(|e| (e.label, PathBuf::from(e.path)))
        .collect();
    assert_eq!(got, vec![("logs/".to_string(), data.path().join("logs"))]);
}
