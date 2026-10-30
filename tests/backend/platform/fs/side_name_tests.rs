//! 旁名的形状：造出来的每一种，账号库的排除名都认；记号的字面只住 `side_name.rs` 一处。

use super::*;
use crate::accounts::manage::layout::is_excluded;

/// 每一种拼法各造几个（不同的原名 · pid · 序号）。
fn every_side_name(name: &str) -> Vec<String> {
    let n = OsStr::new(name);
    let me = std::process::id();
    let thread = std::thread::current().id();
    let mut out = Vec::new();
    for (pid, seq) in [(1u32, 0u64), (me, 7), (u32::MAX, u64::MAX)] {
        out.push(swap(n, pid, seq).to_string_lossy().into_owned());
        out.push(
            backup(n, u128::from(seq), seq)
                .to_string_lossy()
                .into_owned(),
        );
        out.push(exec_tmp(name, pid));
        out.push(own_tmp(n, pid, thread, seq).to_string_lossy().into_owned());
    }
    out
}

#[test]
fn every_side_name_is_excluded_from_the_share() {
    for name in ["settings.json", ".credentials.json", "CLAUDE.md", "plugins"] {
        assert!(!is_excluded(name), "原名 {name:?} 自己不该被排除");
        for side in every_side_name(name) {
            assert!(is_excluded(&side), "旁名 {side:?} 没被排除名认出来");
        }
    }
}

#[test]
fn the_marks_are_spelled_only_in_side_name() {
    let root = guard_core::addr::backend_src_root();
    let home = root.join("platform/fs/side_name.rs");
    let mut hits = Vec::new();
    for (path, src) in guard_core::walk_tree(&root, &["rs"]) {
        if guard_core::lexically_normalized(&path) == guard_core::lexically_normalized(&home) {
            continue;
        }
        let code = guard_core::production_code(&src);
        for m in MARKS {
            if code.contains(m) {
                hits.push(format!("{} 写了 {m:?}", guard_core::addr::rel_of(&path)));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "旁名记号只许经 platform::fs::side_name 拼：\n{}",
        hits.join("\n")
    );
}
