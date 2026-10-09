//! `win-path-core` 的判据。要求：「交给 Win32 的路径带长路径前缀（过 260 字符的写不失败）；实现只一处」。

use super::*;

/// 拼出来的样子。Linux 上按 UTF-16 喂 Windows 形的路径。
#[test]
fn win32_paths_get_the_long_path_prefix() {
    let w = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
    let long_tail = "d".repeat(300);
    for (given, want) in [
        (
            r"C:\Users\u\.cc-monitor\bind.json".to_string(),
            r"\\?\C:\Users\u\.cc-monitor\bind.json".to_string(),
        ),
        (
            "C:/Users/u/x.json".to_string(),
            r"\\?\C:\Users\u\x.json".to_string(),
        ),
        (
            r"\\srv\share\a.json".to_string(),
            r"\\?\UNC\srv\share\a.json".to_string(),
        ),
        (r"\\?\C:\already".to_string(), r"\\?\C:\already".to_string()),
        (r"\\.\pipe\x".to_string(), r"\\.\pipe\x".to_string()),
        ("rel\\x.json".to_string(), "rel\\x.json".to_string()),
        (
            format!(r"D:\{long_tail}\b.json"),
            format!(r"\\?\D:\{long_tail}\b.json"),
        ),
    ] {
        assert_eq!(win32_long_path(w(&given)), w(&want), "{given}");
    }
}

/// 上面那条在执行链上：全仓 Rust 源码里把路径编成 UTF-16（`encode_wide`）的只有本 crate 一处 ——
/// 后端的不覆盖改名 · monitor 的配置换名 · 原子写 · 凭据文件的权限，都经 [`win32_path`] 拿那一串。
#[test]
fn only_this_crate_turns_a_path_into_utf16() {
    const HOME: &str = "src/common/win-path-core/src/lib.rs";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let home = std::fs::read_to_string(root.join(HOME)).expect("读本 crate 源码");
    assert!(
        home.contains("encode_wide"),
        "本 crate 里找不到 encode_wide（锚丢了）"
    );
    let stray: Vec<String> = guard_core::scan_tree_excluding(&root.join("src"), &["rs"], &[HOME])
        .into_iter()
        .filter(|(p, src)| {
            !p.components().any(|c| c.as_os_str() == "vendor") && src.contains("encode_wide")
        })
        .map(|(p, _)| p.display().to_string())
        .collect();
    assert!(
        stray.is_empty(),
        "这几处自己把路径编成 UTF-16 交给 Win32，没过长路径前缀 —— 改调 `win_path_core::win32_path`：{stray:?}"
    );
}
