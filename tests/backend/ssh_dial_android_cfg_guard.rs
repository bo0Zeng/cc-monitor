//! **拨号核心里的平台 cfg 不许悄悄丢下 android。**
//!
//! Rust 里 `target_os = "android"` **不算** `"linux"`：写 `#[cfg(target_os = "linux")]` 的那一臂在手机上不编，
//! 手机落进「别的平台」那一支 —— 编得过，行为却悄悄换了（`tcp_rtt` 那一处从前就是这样：手机上往返时间恒「不知道」，
//! 压缩判准因此恒判「压」，没有一个字说这是故意的）。本 crate 手机也链，平台的分叉要把 android 明写出来。
//!
//! - **它守的性质是**：本 crate 生产段里每一个点名 `target_os = "linux"` 的 cfg，android 那一支都是明写的 ——
//!   否定 / 兜底那一形（`not(…linux…)`）里同时点名 android（android 不许掉进「别的平台」），
//!   而且同一份文件里另有一处点名 android 的 cfg（那一支写了什么，由人读、由那一份的判据钉）。
//! - **它扫的人群是**：`src/backend/ssh-dial/` 下全部 `.rs` 的生产段（剥掉测试段与注释）里的 `#[cfg(…)]` / `#[cfg_attr(…)]` / `cfg!(…)`。
//!
//! 买不到：cfg 写成别的拼法（宏里拼出来、`build.rs` 设的自定义 cfg）认不出；android 那一支写得对不对判不了。

use std::collections::BTreeMap;

const LINUX: &str = "target_os = \"linux\"";
const ANDROID: &str = "target_os = \"android\"";

/// 一段代码里所有 cfg 谓词（`cfg(` 之后、配平的那一对括号里的文本，空白压成一个空格）。
fn predicates(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = code;
    while let Some(at) = rest.find("cfg") {
        let after = &rest[at + 3..];
        let tail = after
            .strip_prefix("_attr(")
            .or_else(|| after.strip_prefix("!("))
            .or_else(|| after.strip_prefix('('));
        rest = after;
        let Some(tail) = tail else { continue };
        let mut depth = 1usize;
        let mut end = None;
        for (i, c) in tail.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(e) = end {
            out.push(tail[..e].split_whitespace().collect::<Vec<_>>().join(" "));
        }
    }
    out
}

/// 一份文件里悄悄丢下 android 的那几处（空 = 干净）。
fn android_gaps(file: &str, code: &str) -> Vec<String> {
    let preds = predicates(code);
    let mut gaps = Vec::new();
    let names_linux = preds.iter().any(|p| p.contains(LINUX));
    let names_android = preds.iter().any(|p| p.contains(ANDROID));
    if names_linux && !names_android {
        gaps.push(format!(
            "{file}：点名了 linux 却没有一处 cfg 点名 android —— 手机走的是哪一支？"
        ));
    }
    for p in &preds {
        if p.contains("not(") && p.contains(LINUX) && !p.contains(ANDROID) {
            gaps.push(format!(
                "{file}：`cfg({p})` 是「别的平台」那一支，android 会悄悄掉进来 —— 把 android 明写进去"
            ));
        }
    }
    gaps
}

#[test]
fn no_platform_cfg_in_the_dial_core_silently_drops_android() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut gaps = Vec::new();
    for (path, src) in guard_core::scan_tree_excluding(root, &["rs"], &[]) {
        let rel = guard_core::module_address(root, &path);
        let code = crate::guard_support::production_code(&src);
        seen.insert(rel.clone(), predicates(&code).len());
        gaps.extend(android_gaps(&rel, &code));
    }
    assert!(
        seen.contains_key("tcp_rtt.rs") && seen.contains_key("connect.rs"),
        "人群里没有 tcp_rtt.rs / connect.rs —— 遍历坏了：{seen:?}"
    );
    assert!(
        seen.get("tcp_rtt.rs").copied().unwrap_or(0) >= 3,
        "tcp_rtt.rs 里认出的 cfg 不到 3 处 —— 识别器坏了：{seen:?}"
    );
    assert!(
        gaps.is_empty(),
        "拨号核心里有平台 cfg 悄悄丢下了 android：\n{}",
        gaps.join("\n")
    );
}

/// 正控：识别器认得出那两形（文件里只点名 linux · 兜底那一支漏了 android），也放得过写全了的那一形。
#[test]
fn the_android_judge_sees_both_shapes() {
    let only_linux =
        "#[cfg(target_os = \"linux\")]\nfn a() {}\n#[cfg(not(target_os = \"linux\"))]\nfn a() {}\n";
    let g = android_gaps("x.rs", only_linux);
    assert_eq!(g.len(), 2, "{g:?}");
    let spelled = "#[cfg(target_os = \"linux\")]\nfn a() {}\n#[cfg(target_os = \"android\")]\nfn a() {}\n\
                   #[cfg(not(any(target_os = \"linux\", target_os = \"android\", windows)))]\nfn a() {}\n";
    assert!(android_gaps("y.rs", spelled).is_empty());
    assert_eq!(
        predicates("if cfg!(windows) { } #[cfg_attr(not(windows), allow(dead_code))]"),
        vec![
            "windows".to_string(),
            "not(windows), allow(dead_code)".to_string()
        ]
    );
}
