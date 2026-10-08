//! **`pub` 项要有产品读者** —— 判据。
//!
//! - **人群**：后端（`src/backend`）· 壳（`src/frontend/shell/src`）· 共享 crate（`src/common/*/src`，`guard-core` 除外：
//!   它在每一份引它的 `Cargo.toml` 里都只在 `[dev-dependencies]`，整个 crate 是测试基础设施）的生产段里每一个 `pub` 项
//!   （`fn` · `const` · `static` · `struct` · `enum` · `type` · `trait`；`pub(crate)` 不在内 —— 那一档有编译器的 `dead_code`）。
//!   生产段 ＝ `guard_core::production_code` 剥过的那一份；整份被 `#[cfg(test)] mod x;` 收进测试构建的文件不算；
//!   带 `cfg(test)` / `cfg(any(test, …))` 属性的项不算。
//! - **读者面**：人群那几棵 ＋ 文件窗口（`src/frontend/filewin/src`，它链共享 crate）的生产段。
//! - **判**：项名作为整词在读者面里出现的次数减去定义那一次 ＝ 读者数；零 ⇒ 必须在 [`EXEMPT`] 里（写理由），否则红。
//! - **豁免表两向**：表里每一行都还在、而且还是零读者（有了读者就该摘掉 —— 自退役）。
//!
//! # 买不到
//!
//! - 按名字数：别处有同名的东西（同名方法 · 字符串里提到）就算有读者 —— 只会漏判，不会误判。
//! - 不看可见性该不该更窄（有读者的 `pub` 只在本包里读也不判）。

use crate::guard_support::repo_root;
use std::collections::{BTreeMap, BTreeSet};

/// 零读者而留着的那几项：`(文件, 项名, 理由)`。
const EXEMPT: &[(&str, &str, &str)] = &[
    (
        "src/backend/footprint/registry.rs",
        "NOT_MANAGED",
        "足迹申报表的「不归它管」那一半（逐条写为什么不属这张表）：读者是 footprint 的登记判据，理由文字进文案普查台账",
    ),
    (
        "src/backend/files/mod.rs",
        "FRESHNESS",
        "保鲜机制逐 target 的声明（刻意不判相等）：读者是 `capability_guard` 的反向判据，各格的说明文字进文案普查台账",
    ),
    (
        "src/backend/control/files_commit.rs",
        "COMMIT_COMMANDS",
        "第三层写模块的命令声明表：读者是 `readonly_guard`「门里够得到的命令」与 inbound 结构判据的对拍（另一侧是命令表的源码文本，两侧异源）",
    ),
    (
        "src/backend/control/files_extract.rs",
        "EXTRACT_COMMANDS",
        "第三层写模块的命令声明表：读者是 `readonly_guard`「门里够得到的命令」与 inbound 结构判据的对拍（另一侧是命令表的源码文本，两侧异源）",
    ),
    (
        "src/backend/control/files_write.rs",
        "MANAGE_COMMANDS",
        "第三层写模块的命令声明表：读者是 `readonly_guard`「门里够得到的命令」与 inbound 结构判据的对拍（另一侧是命令表的源码文本，两侧异源）",
    ),
    (
        "src/backend/agents/claudecode/paths.rs",
        "is_inside_tree",
        "`~/.claude*` 那个星号包含哪些树的唯一住址；零生产消费者由 `readonly_guard` 的 STAGED_ZERO 登记数着，接回生产路径那一刻那边红",
    ),
    (
        "src/backend/lib.rs",
        "capability_ledger",
        "能力账的汇总本体：从各族声明现推（写成函数而不是第二张表，免得声明与汇总漂开），读者是 `capability_ledger_guard` 那张异源点名表",
    ),
    (
        "src/backend/lib.rs",
        "capabilities_on",
        "每个 target 上做得到哪些的现推那一段（只读声明，`target_parity_guard` 从源码钉它不读差异表），读者是对等判据",
    ),
    (
        "src/backend/lib.rs",
        "CC_MONITOR_BUILD_STAMP",
        "`#[used]` ＋ `#[no_mangle]` 的身份戳：读者是二进制字节（部署 · 内嵌 · 远端见证按字节扫 `STAMP_OPEN`），不是源码里的名字",
    ),
    (
        "src/common/acct-core/src/lib.rs",
        "AUTH_KINDS",
        "鉴权方式的闭集：判定单家表 J16 拿它与界面那一份联合逐个对拍，产品逻辑按单个常量读",
    ),
    (
        "src/common/copy-core/src/lib.rs",
        "copy_matches",
        "共享 crate 给各包测试按文案键断言的口子：读者是别的包的测试，收不进本 crate 的 `cfg(test)`",
    ),
];

const ITEM_KINDS: &[&str] = &["fn", "const", "static", "struct", "enum", "type", "trait"];

/// 一行若是 `pub` 项的定义 ⇒ 它的名字。
fn pub_item_name(line: &str) -> Option<&str> {
    let mut rest = line.trim_start().strip_prefix("pub ")?;
    for q in ["unsafe ", "async ", "extern \"C\" "] {
        rest = rest.strip_prefix(q).unwrap_or(rest);
    }
    let rest = rest.strip_prefix("const fn ").map_or_else(
        || {
            ITEM_KINDS
                .iter()
                .find_map(|k| rest.strip_prefix(k).and_then(|r| r.strip_prefix(' ')))
        },
        Some,
    )?;
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

/// 定义前面紧挨着的属性里有没有 `cfg(test)` / `cfg(any(test, …))`。
fn test_gated(lines: &[&str], at: usize) -> bool {
    lines[..at]
        .iter()
        .rev()
        .map(|l| l.trim())
        .take_while(|l| l.starts_with("#[") || l.starts_with("///"))
        .any(|l| l.contains("cfg(test)") || l.contains("cfg(any(test"))
}

/// 一段生产代码里的整词计数。
fn words(text: &str, into: &mut BTreeMap<String, usize>) {
    for w in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if !w.is_empty() && !w.starts_with(|c: char| c.is_ascii_digit()) {
            *into.entry(w.to_string()).or_default() += 1;
        }
    }
}

/// 一棵源码树里被 `#[cfg(test)] mod x;` 整份收进测试构建的那几份（仓根相对路径）。
fn test_only_modules(files: &[(String, String)]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (path, raw) in files {
        let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
        let dir =
            if path.ends_with("/lib.rs") || path.ends_with("/mod.rs") || path.ends_with("/main.rs")
            {
                dir.to_string()
            } else {
                format!(
                    "{dir}/{}",
                    path.rsplit('/').next().unwrap().trim_end_matches(".rs")
                )
            };
        let lines: Vec<&str> = raw.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let t = l.split("//").next().unwrap_or("").trim();
            let Some(name) = t
                .strip_prefix("mod ")
                .or_else(|| t.strip_prefix("pub mod "))
                .or_else(|| t.strip_prefix("pub(crate) mod "))
                .and_then(|r| r.split(';').next())
                .filter(|n| {
                    t.ends_with(';') && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                })
            else {
                continue;
            };
            let gated = lines[..i]
                .iter()
                .rev()
                .map(|l| l.trim())
                .take_while(|l| l.starts_with("#[") || l.starts_with("//"))
                .any(|l| l == "#[cfg(test)]");
            if gated {
                out.insert(format!("{dir}/{name}.rs"));
                out.insert(format!("{dir}/{name}/mod.rs"));
            }
        }
    }
    out
}

fn rs_files(rel: &str) -> Vec<(String, String)> {
    let root = repo_root();
    guard_core::scan_tree_excluding(&root.join(rel), &["rs"], &[])
        .into_iter()
        .map(|(p, s)| {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, s)
        })
        .collect()
}

fn population_roots() -> Vec<String> {
    let mut v = vec![
        "src/backend".to_string(),
        "src/frontend/shell/src".to_string(),
    ];
    for (path, _) in rs_files("src/common") {
        let mut parts = path.split('/');
        let (_, _, krate) = (parts.next(), parts.next(), parts.next().unwrap_or(""));
        let root = format!("src/common/{krate}/src");
        if krate != "guard-core" && !v.contains(&root) {
            v.push(root);
        }
    }
    v
}

/// ★ 零读者的 `pub` 项（豁免表之外的）：`(文件, 项名)`。`population` 是要判的那几份，`readers` 是读者面（都已是生产段）。
fn zero_reader_items(population: &[(String, String)], readers: &[String]) -> Vec<(String, String)> {
    let mut count = BTreeMap::new();
    for r in readers {
        words(r, &mut count);
    }
    let mut out = Vec::new();
    for (path, prod) in population {
        let lines: Vec<&str> = prod.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let Some(name) = pub_item_name(l) else {
                continue;
            };
            if test_gated(&lines, i) {
                continue;
            }
            if count.get(name).copied().unwrap_or(0) <= 1 {
                out.push((path.clone(), name.to_string()));
            }
        }
    }
    out
}

fn production(files: Vec<(String, String)>) -> Vec<(String, String)> {
    let gated = test_only_modules(&files);
    files
        .into_iter()
        .filter(|(p, _)| !gated.contains(p))
        .map(|(p, s)| (p, guard_core::production_code(&s)))
        .collect()
}

#[test]
fn every_pub_item_has_a_product_reader_or_a_registered_reason() {
    let population: Vec<(String, String)> = population_roots()
        .iter()
        .flat_map(|r| production(rs_files(r)))
        .collect();
    let mut readers: Vec<String> = population.iter().map(|(_, s)| s.clone()).collect();
    readers.extend(
        production(rs_files("src/frontend/filewin/src"))
            .into_iter()
            .map(|(_, s)| s),
    );
    assert!(
        population.len() > 300,
        "人群只有 {} 份 —— 扫描坏了",
        population.len()
    );
    let items: usize = population
        .iter()
        .map(|(_, s)| s.lines().filter(|l| pub_item_name(l).is_some()).count())
        .sum();
    assert!(items > 500, "只认出 {items} 个 `pub` 项 —— 认法坏了");

    let zero = zero_reader_items(&population, &readers);
    let exempt: BTreeSet<(String, String)> = EXEMPT
        .iter()
        .map(|(f, n, _)| ((*f).to_string(), (*n).to_string()))
        .collect();
    let unregistered: Vec<String> = zero
        .iter()
        .filter(|z| !exempt.contains(*z))
        .map(|(f, n)| format!("{f} · {n}"))
        .collect();
    assert!(
        unregistered.is_empty(),
        "这几个 `pub` 项在生产代码里没有读者（只被测试读也算零）：删，或收进 `#[cfg(test)]`，或登记进豁免表写清为什么：\n  {}",
        unregistered.join("\n  ")
    );
    let zero: BTreeSet<(String, String)> = zero.into_iter().collect();
    let stale: Vec<String> = EXEMPT
        .iter()
        .filter(|(f, n, _)| !zero.contains(&((*f).to_string(), (*n).to_string())))
        .map(|(f, n, _)| format!("{f} · {n}"))
        .collect();
    assert!(
        stale.is_empty(),
        "豁免表里这几行已经有读者了 / 不在了 —— 摘掉：\n  {}",
        stale.join("\n  ")
    );
    assert!(
        EXEMPT.iter().all(|(_, _, why)| why.chars().count() >= 8),
        "豁免表每一行都要写理由"
    );
}

/// 正控：认法与判法在一段手写样本上各咬一次（有读者的不报、只被测试读的报、`cfg(test)` 的不算、`pub(crate)` 不在人群里）。
#[test]
fn the_detector_bites_on_a_hand_written_sample() {
    let lib = "pub fn used_once() {}\npub fn read_by_tests_only() {}\n#[cfg(any(test, feature = \"fixtures\"))]\npub fn fixture() {}\npub(crate) fn inner() {}\npub const SHOWN: u8 = 1;\npub struct Lonely;\n";
    let user = "fn main() { used_once(); let _ = SHOWN; }";
    let pop = vec![("x/lib.rs".to_string(), lib.to_string())];
    let got = zero_reader_items(&pop, &[lib.to_string(), user.to_string()]);
    assert_eq!(
        got,
        [
            ("x/lib.rs".to_string(), "read_by_tests_only".to_string()),
            ("x/lib.rs".to_string(), "Lonely".to_string())
        ]
    );
    let files = vec![
        (
            "a/src/lib.rs".to_string(),
            "#[cfg(test)]\nmod helpers;\nmod real;\n".to_string(),
        ),
        (
            "a/src/m.rs".to_string(),
            "#[cfg(test)]\npub(crate) mod deep;\n".to_string(),
        ),
    ];
    let gated = test_only_modules(&files);
    assert!(
        gated.contains("a/src/helpers.rs")
            && gated.contains("a/src/m/deep.rs")
            && !gated.contains("a/src/real.rs")
    );
    assert_eq!(
        pub_item_name("    pub const fn derive(x: u8) -> u8 {"),
        Some("derive")
    );
    assert_eq!(pub_item_name("pub(crate) fn no()"), None);
}
