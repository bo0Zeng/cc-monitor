//! 旁名的形状：造出来的每一种，账号库的共享集都不收（共享集只认已知共享表里的名字）；记号的字面只住 `side_name.rs` 一处。

use super::*;
use crate::accounts::manage::layout::share_items;
use crate::accounts::manage::scan::DirState;
use crate::platform::acct_view::Item;

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

/// 共享库顶层摆着原名与它的每一种旁名（写到一半被快照拍到的那一刻）：原名照共享，旁名一个都不进共享集。
#[test]
fn no_side_name_is_ever_shared() {
    for name in ["settings.json", "CLAUDE.md", "plugins"] {
        let sides = every_side_name(name);
        let file = Item::File {
            mode: Some(0o600),
            size: 2,
            mtime: None,
        };
        let shared = DirState {
            me: Some(Item::Dir { mode: Some(0o700) }),
            entries: std::iter::once(name.to_string())
                .chain(sides.iter().cloned())
                .map(|n| (n, file.clone()))
                .collect(),
        };
        let items = share_items(&shared);
        assert!(
            items.iter().any(|i| i == name),
            "原名 {name:?} 不在共享集里：{items:?}"
        );
        for side in &sides {
            assert!(!items.contains(side), "旁名 {side:?} 进了共享集");
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
