//! 〔TL3 · 审计 F 🔴-6〕**读路径的越界围栏在 observe 里只有一个家**（`observe/fence.rs`），两个读者都经它。
//!
//! # 守的要求（住址）
//!
//! `设计/15 §4.2`，逐字：「`history_query::fence_under_projects` 是具名围栏，头注逐字『**别再造一份**』；
//! `search_query::search` 里又内联了一份（头注『复刻 history_query』）。⇒ 判据的人群是『本文件』，而性质说的是
//! 『别再造一份』—— 又一个守卫范围 ≠ 性质范围，而这次护的是**安全判定**」；「⇒ 正确处置是在 **observe 内部**
//! 给它一个家（`observe/fence.rs`，`§5.3 C5`……）；**不是改 `common` 的门槛**」。
//!
//! # 三条
//!
//! - **F1 一个家**：人群 = `src/backend/observe/**.rs` 生产段（`guard_core::scan_tree_excluding` 现扫）；
//!   含 `canonicalize(` 的文件集合 == `{fence.rs}`（两向相等），且 `fence.rs` 里恰好两处、`Fence::at` 与
//!   `Fence::admit` 各一处（解开根 · 解开目标）。
//!   ⇒ 名字接自 `history_query_tests.rs` 里原来那一条（audit-0805 E3 立的，人群只有 `history_query.rs` 一份 ——
//!   正是 `15 §4.2` 说的「守卫范围 ≠ 性质范围」那一形，`search_query` 那份内联的它看不见）：性质没变，人群换成 observe 全树。
//! - **F2 两个读者都经它**：observe 生产段里点名围栏（`fence::` · `Fence::` · `fence_under_projects(`）的文件集合
//!   == `{history_query.rs, search_query.rs}`（除家本身）。F1 在「内联那份删了、围栏也没接上」那一形下照绿 ——
//!   这一条就是为那一形立的（`search` 那一处围栏是纵深防御：它那一趟目录遍历默认不跟 symlink，行为判据打不到它）。
//! - **F3 行为**：放行根下（相对 · 绝对）· 拒 `..` 越界 · 拒 symlink 逃逸（unix）· 根不存在 / 目标不存在的报错原话
//!   （与收口前逐字相同）。
//!
//! # 买不到
//!
//! - `observe/` 之外的路径解析（`files/` 一族是 V119「无数据围栏」的写面，不是这道读围栏；`control/` 的写口各有自己的判据）。
//! - 不认 `canonicalize(` 这个词、换一种写法自己判越界（例如手搓 `components()` 消 `..`）的第二份看不见。

use super::*;
use std::collections::BTreeSet;
use std::path::Path;

/// observe 生产段：`(相对 observe/ 的路径, 生产代码)`。
fn observe_sources() -> Vec<(String, String)> {
    let root = crate::guard_support::src_root().join("observe");
    let out: Vec<(String, String)> = guard_core::scan_tree_excluding(&root, &["rs"], &[])
        .into_iter()
        .map(|(p, text)| {
            let rel = p
                .strip_prefix(&root)
                .expect("扫到的文件不在 observe/ 下")
                .to_string_lossy()
                .replace('\\', "/");
            (rel, crate::guard_support::production_side_of(&p, &text))
        })
        .collect();
    assert!(
        out.iter()
            .any(|(f, code)| f == "history_query.rs" && code.len() > 3_000),
        "扫描面里没有剥好的 `history_query.rs` —— 根取歪了或剥法过剥，本文件此刻无效"
    );
    out
}

/// 一个 `fn` 头到下一个 `fn ` 之间那一段（够用：本文件只切 `fence.rs` 里的两个方法）。
fn fn_chunk<'a>(code: &'a str, head: &str) -> &'a str {
    let at = guard_core::find_pinned(code, head)
        .unwrap_or_else(|e| panic!("`{head}` 不是恰好一处：{e}"));
    let rest = &code[at + head.len()..];
    let end = rest.find(" fn ").map_or(rest.len(), |k| k);
    &code[at..at + head.len() + end]
}

/// ★ F1：`canonicalize(` 在 observe 生产段只住 `fence.rs`，而且恰好是 `Fence::at` / `Fence::admit` 各一处。
#[test]
fn path_resolution_has_exactly_one_home() {
    let src = observe_sources();
    let homes: BTreeSet<&str> = src
        .iter()
        .filter(|(_, code)| code.contains("canonicalize("))
        .map(|(f, _)| f.as_str())
        .collect();
    assert_eq!(
        homes,
        BTreeSet::from(["fence.rs"]),
        "\n🔴 observe 里自己解析路径再判越界的不只一处（`设计/15 §4.2`「别再造一份」）：{homes:?}\n\
         ⇒ 经 `observe/fence.rs::Fence`（或 `fence_under_projects`）放行，别再写一份 canonicalize ＋ `starts_with`。\n\
         少了 `fence.rs` ⇒ 围栏本身被改掉了，去看它是不是还挡得住 symlink 逃逸。"
    );
    let fence = &src
        .iter()
        .find(|(f, _)| f == "fence.rs")
        .expect("上面刚断言过")
        .1;
    assert_eq!(
        fence.matches("canonicalize(").count(),
        2,
        "`fence.rs` 里不是恰好两处解开（根 · 目标）"
    );
    for head in ["fn at(", "fn admit("] {
        assert_eq!(
            fn_chunk(fence, head).matches("canonicalize(").count(),
            1,
            "`{head}` 里不是恰好一处解开 —— 计数凑对了，位置没对"
        );
    }
}

/// ★ F2：observe 里点名围栏的读者 == `{history_query.rs, search_query.rs}`（两向相等）。
#[test]
fn both_readers_in_observe_go_through_the_fence() {
    let users: BTreeSet<String> = observe_sources()
        .into_iter()
        .filter(|(f, code)| {
            f != "fence.rs"
                && (code.contains("fence::")
                    || code.contains("Fence::")
                    || code.contains("fence_under_projects("))
        })
        .map(|(f, _)| f)
        .collect();
    assert_eq!(
        users,
        BTreeSet::from([
            "history_query.rs".to_string(),
            "search_query.rs".to_string()
        ]),
        "\n经围栏的 observe 读者变了：{users:?}\n\
         ⇒ 少了一个 ⇒ 那一份的越界防护没了（F1 在这一形下照绿）；\n\
         ⇒ 多了一个新读者、也经围栏 ⇒ 好事，按实数加进来并写清它读什么。"
    );
}

/// 一棵 `<home>/projects/p/a.jsonl` ＋ 旁边 `<home>/outside/x.jsonl` 的小树（结构，不采任何真会话内容）。
struct Tree(std::path::PathBuf);

impl Tree {
    fn new(tag: &str) -> Self {
        let home = std::env::temp_dir().join(format!("ccm-fence-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("projects").join("p")).unwrap();
        std::fs::create_dir_all(home.join("outside")).unwrap();
        std::fs::write(home.join("projects").join("p").join("a.jsonl"), "{}\n").unwrap();
        std::fs::write(home.join("outside").join("x.jsonl"), "{}\n").unwrap();
        Self(home)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// ★ F3：放行与拒绝的几格，报错原话与收口前逐字相同。
#[test]
fn the_fence_admits_inside_and_refuses_escapes_with_the_old_words() {
    let t = Tree::new("adm");
    let fence = Fence::at(&t.0.join("projects")).expect("projects/ 在，围栏该立得起来");
    let inside =
        t.0.join("projects")
            .join("p")
            .join("a.jsonl")
            .canonicalize()
            .unwrap();
    // 放行：相对（按根拼）与绝对，回的都是解开之后的那条。
    assert_eq!(fence.admit(Path::new("p/a.jsonl")).unwrap(), inside);
    assert_eq!(fence.admit(&inside).unwrap(), inside);
    // `..` 越界 ⇒ 拒，原话点名 projects。
    let e = fence.admit(Path::new("../outside/x.jsonl")).unwrap_err();
    assert!(
        e.starts_with("refusing to access outside projects dir: "),
        "{e}"
    );
    // 目标不在 ⇒ 解不开。
    let e = fence.admit(Path::new("p/nope.jsonl")).unwrap_err();
    assert!(e.starts_with("path unavailable: "), "{e}");
    // 根不在 ⇒ 立不起来。
    let bare = t.0.join("outside");
    let e = Fence::at(&bare.join("projects"))
        .err()
        .expect("没有 projects/ 却立起来了");
    assert!(e.starts_with("projects root unavailable: "), "{e}");
    // 别的根说它自己的名字（LOC1b 那种「各家合成历史面的记录根」用得上）。
    let e = Fence::at(&t.0.join("sessions"))
        .err()
        .expect("根不在却立起来了");
    assert!(e.starts_with("sessions root unavailable: "), "{e}");
}

/// ★ F3（unix）：根下的 symlink 指到根外 ⇒ 拒（文件与目录两形）。
#[cfg(unix)]
#[test]
fn the_fence_refuses_a_symlink_that_escapes_the_root() {
    use std::os::unix::fs::symlink;
    let t = Tree::new("sym");
    symlink(
        t.0.join("outside").join("x.jsonl"),
        t.0.join("projects").join("p").join("evil.jsonl"),
    )
    .unwrap();
    symlink(t.0.join("outside"), t.0.join("projects").join("away")).unwrap();
    let fence = Fence::at(&t.0.join("projects")).unwrap();
    for c in ["p/evil.jsonl", "away/x.jsonl"] {
        let e = fence.admit(Path::new(c)).unwrap_err();
        assert!(
            e.starts_with("refusing to access outside projects dir: "),
            "{c}: {e}"
        );
    }
    // 阴性对照：根下的 symlink 指回根下 ⇒ 放行（围栏判的是「解开之后在不在界内」，不是「是不是 symlink」）。
    symlink(
        t.0.join("projects").join("p").join("a.jsonl"),
        t.0.join("projects").join("p").join("same.jsonl"),
    )
    .unwrap();
    assert!(fence.admit(Path::new("p/same.jsonl")).is_ok());
}
