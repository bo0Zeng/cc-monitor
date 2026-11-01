//! 旁名记号的字面只住 `side_name.rs` 一处（旁名进不了账号库共享集那一条住 `tests/backend/accounts/manage/side_name_exclusion_tests.rs`，要读账号库）。

use super::*;

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
