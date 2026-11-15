//! 手机 JNI 一个口：Rust 导出只按 `Core` 那一个类起名，Kotlin 的 `external` 与 `System.loadLibrary` 只在 `core-rs` 的 `Core.kt`。
//! 别的模块要核心，经 `Core`（同模块）或经交进来的口（core-claude 的 `ResidentCore`），不自己认识那份 `.so`。
//! 本文件住 `tests/`、由 lib.rs 以 `#[path]` 挂进来：两棵扫的树（`rust/` · `src/mobile`）都不含它，不会在自己身上找到针。

use std::path::{Path, PathBuf};

/// 导出名的唯一前缀：`com.ccmonitor.mobile.core.rs.Core` 那一个类。
const PREFIX: &str = "Java_com_ccmonitor_mobile_core_rs_Core_";
/// Kotlin 里唯一认识那份 `.so` 的文件（相对 `src/mobile`）。
const HOME: &str = "core-rs/src/main/kotlin/com/ccmonitor/mobile/core/rs/Core.kt";

fn mobile() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 一行去掉 `//` 注释以后的代码。
fn code(line: &str) -> &str {
    line.split("//").next().unwrap_or("")
}

/// `rust/` 里每个 `extern "system" fn Java_…`：都以 [`PREFIX`] 起名。
#[test]
fn every_rust_export_is_named_after_core() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut seen = 0;
    let mut odd = Vec::new();
    for (f, text) in guard_core::scan_tree!(root, &["rs"]) {
        for (i, line) in text.lines().enumerate() {
            let c = code(line);
            let Some(at) = c.find("fn Java_") else {
                continue;
            };
            seen += 1;
            if !c[at + 3..].starts_with(PREFIX) {
                odd.push(format!("{}:{}", f.display(), i + 1));
            }
        }
    }
    assert!(seen > 0, "一个 Java_ 导出都没扫到：扫的目录不对");
    assert!(
        odd.is_empty(),
        "导出名不以 {PREFIX} 起头 {} 处：{odd:?}",
        odd.len()
    );
}

/// `src/mobile/**/src/main/**/*.kt` 里 `System.loadLibrary` 与 `external fun` 只在 [`HOME`]。
#[test]
fn only_core_kt_knows_the_native_library() {
    let root = mobile();
    let mut home_has = (false, false);
    let mut odd = Vec::new();
    for (f, text) in guard_core::scan_tree!(&root, &["kt"]) {
        if !f.to_string_lossy().contains("/src/main/") {
            continue;
        }
        for (i, line) in text.lines().enumerate() {
            let c = code(line);
            let load = c.contains("System.loadLibrary(");
            let ext = c
                .split_whitespace()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|w| w == ["external", "fun"]);
            if !(load || ext) {
                continue;
            }
            if f.ends_with(HOME) {
                home_has.0 |= load;
                home_has.1 |= ext;
            } else {
                odd.push(format!(
                    "{}:{}",
                    f.strip_prefix(&root).unwrap_or(&f).display(),
                    i + 1
                ));
            }
        }
    }
    assert!(
        home_has.0 && home_has.1,
        "{HOME} 里没扫到 loadLibrary / external：扫法不对"
    );
    assert!(
        odd.is_empty(),
        "Core.kt 之外认识那份 .so 的 {} 处：{odd:?}",
        odd.len()
    );
}
