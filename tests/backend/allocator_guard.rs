//! 远端后端（musl 静态版）的全局分配器是 mimalloc；本机 glibc 版与 Windows 版照旧用系统分配器。
//!
//! # 为什么换
//!
//! musl 自带的分配器放掉的内存不还给系统：680 MB 合成世界里热完全文搜索索引就停在 350–520 MB
//! （glibc 同一世界 125 MB），问过一次含工具的搜索之后再涨三四百 MB、不回落。远端那台跑的就是 musl 版。
//! 换成 mimalloc 之后的读数与选型理由（许可 · 交叉编译 · 为什么只换 musl）住 `src/backend/Cargo.toml` 那一段与提交说明。
//!
//! # 它判什么（三条，都读盘上原文；注释与测试段先用共享剥法剥掉）
//!
//! 1. 清单里 `mimalloc` 恰好声明一次，而且只在 `[target.'cfg(target_env = "musl")'.dependencies]` 那一段 ——
//!    写进 `[dependencies]` 就连 glibc / Windows 版一起换了（Windows 还要多编一棵 C）。
//! 2. `main.rs` 的生产段里恰好一处 `#[global_allocator]`，紧挨着它的属性是 `#[cfg(target_env = "musl")]`，类型是 `mimalloc::MiMalloc`。
//!    放在 `main.rs`（只有远端那个二进制有它）而不放库面：本机 GUI 进程把后端当库链进去，库里定全局分配器会连壳一起换。
//! 3. 库面（`src/backend` 下 `main.rs` 之外每一份 `.rs` 的生产段）不许出现 `#[global_allocator]`；
//!    测试档的量具 `alloc_probe.rs` 整份是 `cfg(test)`（挂载处带着），明写排除。
//!
//! # 买不到什么
//!
//! 判的是源码与清单，不是编出来的字节。字节那一半归门禁 `muslbuild` 那一格：两个 arch 编完各认一次
//! mimalloc 自带的报错前缀 `mimalloc: `（没换上 ⇒ 字节里没有这串 ⇒ 红）。常驻降没降不进判据（读数住提交说明）。

const MANIFEST: &str = include_str!("../../src/backend/Cargo.toml");
const MAIN_RS: &str = include_str!("../../src/backend/main.rs");

/// mimalloc 唯一该住的那一段。
const MUSL_SECTION: &str = "[target.'cfg(target_env = \"musl\")'.dependencies]";
const ATTR: &str = "#[global_allocator]";

/// 清单里一共几条依赖 ＋ 名为 `dep` 的那条住在哪几段（`#` 注释行由 `guard_core::strip_hash_comment_lines` 先剥掉）。
fn sections_declaring(manifest: &str, dep: &str) -> (usize, Vec<String>) {
    let mut total = 0usize;
    let mut found = Vec::new();
    let mut section = String::new();
    for line in guard_core::strip_hash_comment_lines(manifest).lines() {
        let t = line.trim();
        if t.starts_with('[') {
            section = t.to_string();
            continue;
        }
        if !section.ends_with("dependencies]") || line.starts_with(char::is_whitespace) {
            continue;
        }
        let Some((k, _)) = t.split_once('=') else {
            continue;
        };
        total += 1;
        if k.trim() == dep {
            found.push(section.clone());
        }
    }
    (total, found)
}

/// 生产段里逐行（去首尾空白、去空行；剥法是 `guard_core::production_code`）。
fn production_lines(src: &str) -> Vec<String> {
    guard_core::production_code(src)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

#[test]
fn mimalloc_is_declared_once_and_only_for_musl() {
    let (total, found) = sections_declaring(MANIFEST, "mimalloc");
    assert!(
        total >= 15,
        "清单里只抽出 {total} 条依赖 —— 抽取坏了，本条在空转"
    );
    assert_eq!(
        found,
        vec![MUSL_SECTION.to_string()],
        "`mimalloc` 应在 `src/backend/Cargo.toml` 里恰好声明一次、只在 `{MUSL_SECTION}` —— \
         写在别的段会连 glibc / Windows 版一起换分配器"
    );
}

#[test]
fn the_remote_binary_sets_mimalloc_as_global_allocator_only_on_musl() {
    let lines = production_lines(MAIN_RS);
    let at: Vec<usize> = (0..lines.len()).filter(|&i| lines[i] == ATTR).collect();
    assert_eq!(
        at.len(),
        1,
        "`main.rs` 的生产段里应恰好一处 `{ATTR}`（实得 {}）",
        at.len()
    );
    let i = at[0];
    let around = &lines[i.saturating_sub(2)..(i + 2).min(lines.len())];
    assert!(
        around.iter().any(|l| l == "#[cfg(target_env = \"musl\")]"),
        "`{ATTR}` 紧挨着要有 `#[cfg(target_env = \"musl\")]`（只换远端 musl 版）；实得：{around:?}"
    );
    let decl = lines.get(i + 1).map(String::as_str).unwrap_or_default();
    assert!(
        decl.starts_with("static ") && decl.ends_with(": mimalloc::MiMalloc = mimalloc::MiMalloc;"),
        "`{ATTR}` 下一行应是 `static …: mimalloc::MiMalloc = mimalloc::MiMalloc;`；实得：{decl:?}"
    );
}

#[test]
fn the_library_face_sets_no_global_allocator() {
    let root = crate::guard_support::src_root();
    let files = guard_core::scan_tree_excluding(
        &root,
        &["rs"],
        &["src/backend/main.rs", "src/backend/alloc_probe.rs"],
    );
    assert!(
        files.len() >= 100,
        "只扫到 {} 份 `.rs` —— 走树坏了，本条在空转",
        files.len()
    );
    let hits: Vec<String> = files
        .iter()
        .filter(|(_, src)| production_lines(src).iter().any(|l| l == ATTR))
        .map(|(p, _)| p.strip_prefix(&root).unwrap_or(p).display().to_string())
        .collect();
    assert!(
        hits.is_empty(),
        "库面里不许定全局分配器（本机 GUI 进程把后端当库链进去，会连壳一起换）：{hits:?}"
    );
}
