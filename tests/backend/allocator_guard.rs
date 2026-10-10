//! 远端后端（musl 静态版）的全局分配器是 mimalloc；本机 glibc 版与 Windows 版照旧用系统分配器。
//!
//! # 为什么换
//!
//! musl 自带的分配器放掉的内存不还给系统：680 MB 合成世界里热完全文搜索索引就停在 350–490 MB
//! （glibc 同一世界 117 MB），问过一次含工具的搜索之后再涨两三百 MB、不回落。远端那台跑的就是 musl 版。
//! 换成 mimalloc 之后的读数与选型理由（许可 · 交叉编译 · 为什么只换 musl）住本仓提交说明。
//!
//! # 它判什么（三条，都读盘上原文）
//!
//! 1. 清单里 `mimalloc` 恰好声明一次，而且只在 `[target.'cfg(target_env = "musl")'.dependencies]` 那一段 ——
//!    写进 `[dependencies]` 就连 glibc / Windows 版一起换了（Windows 还要多编一棵 C）。
//! 2. `main.rs` 里恰好一处 `#[global_allocator]`，紧挨着它的属性是 `#[cfg(target_env = "musl")]`，类型是 `mimalloc::MiMalloc`。
//!    放在 `main.rs`（只有远端那个二进制有它）而不放 `lib.rs`：本机 GUI 进程把后端当库链进去，库里定全局分配器会连壳一起换。
//! 3. 库面（`lib.rs` 与它下面的生产段）不许再出现 `#[global_allocator]` —— 测试档的量具 `alloc_probe` 是 `cfg(test)`，不算。
//!
//! # 买不到什么
//!
//! 判的是源码与清单，不是编出来的字节。字节那一半归门禁 `muslbuild` 那一格：两个 arch 编完各认一次
//! mimalloc 自带的报错前缀 `mimalloc: `（没换上 ⇒ 字节里没有这串 ⇒ 红）。常驻降没降不进判据（读数住提交说明）。

const MANIFEST: &str = include_str!("../../src/backend/Cargo.toml");
const MAIN_RS: &str = include_str!("../../src/backend/main.rs");
const LIB_RS: &str = include_str!("../../src/backend/lib.rs");

/// mimalloc 唯一该住的那一段。
const MUSL_SECTION: &str = "[target.'cfg(target_env = \"musl\")'.dependencies]";

/// 清单里每条依赖的（段, 名）；`#` 注释行先剥掉。
fn manifest_deps(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut section = String::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('#') || t.is_empty() {
            continue;
        }
        if t.starts_with('[') {
            section = t.to_string();
            continue;
        }
        if !section.ends_with("dependencies]") || line.starts_with(char::is_whitespace) {
            continue;
        }
        if let Some((k, _)) = t.split_once('=') {
            out.push((section.clone(), k.trim().to_string()));
        }
    }
    out
}

/// 不在 `//` 注释里的那几行（去掉首尾空白）。
fn code_lines(src: &str) -> Vec<&str> {
    src.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .collect()
}

#[test]
fn mimalloc_is_declared_once_and_only_for_musl() {
    let deps = manifest_deps(MANIFEST);
    assert!(
        deps.len() >= 15,
        "清单里只抽出 {} 条依赖 —— 抽取坏了：{deps:?}",
        deps.len()
    );
    let mi: Vec<_> = deps.iter().filter(|(_, n)| n == "mimalloc").collect();
    assert_eq!(
        mi.len(),
        1,
        "`mimalloc` 应在 `src/backend/Cargo.toml` 里恰好声明一次（实得 {}：{mi:?}）",
        mi.len()
    );
    assert_eq!(
        mi[0].0, MUSL_SECTION,
        "`mimalloc` 只许住 `{MUSL_SECTION}` —— 写在别的段会连 glibc / Windows 版一起换分配器"
    );
}

#[test]
fn the_remote_binary_sets_mimalloc_as_global_allocator_only_on_musl() {
    let lines = code_lines(MAIN_RS);
    let at: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| **l == "#[global_allocator]")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        at.len(),
        1,
        "`main.rs` 里应恰好一处 `#[global_allocator]`（实得 {}）",
        at.len()
    );
    let i = at[0];
    let around = &lines[i.saturating_sub(2)..(i + 2).min(lines.len())];
    assert!(
        around.contains(&"#[cfg(target_env = \"musl\")]"),
        "`#[global_allocator]` 紧挨着要有 `#[cfg(target_env = \"musl\")]`（只换远端 musl 版）；实得：{around:?}"
    );
    let decl = lines.get(i + 1).copied().unwrap_or_default();
    assert!(
        decl.starts_with("static ") && decl.contains(": mimalloc::MiMalloc = mimalloc::MiMalloc;"),
        "`#[global_allocator]` 下一行应是 `static …: mimalloc::MiMalloc = mimalloc::MiMalloc;`；实得：{decl:?}"
    );
}

#[test]
fn the_library_face_sets_no_global_allocator() {
    // 库面：`lib.rs` 本身 ＋ 生产段每一份 `.rs`（`src/backend` 下、`main.rs` 之外）。
    let mut hits = Vec::new();
    if code_lines(LIB_RS).contains(&"#[global_allocator]") {
        hits.push("lib.rs".to_string());
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut n = 0usize;
    for e in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
    {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let rel = p
            .strip_prefix(root)
            .unwrap_or(p)
            .to_string_lossy()
            .into_owned();
        if rel == "main.rs" || rel == "lib.rs" || rel == "alloc_probe.rs" {
            continue;
        }
        n += 1;
        let Ok(src) = std::fs::read_to_string(p) else {
            continue;
        };
        if code_lines(&src).contains(&"#[global_allocator]") {
            hits.push(rel);
        }
    }
    assert!(n >= 100, "只扫到 {n} 份 `.rs` —— 走树坏了，本条在空转");
    assert!(
        hits.is_empty(),
        "库面里不许定全局分配器（本机 GUI 进程把后端当库链进去，会连壳一起换）：{hits:?}"
    );
}
