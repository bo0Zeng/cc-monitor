//! 分派那两层（`route` · `apply`）没有兜底臂、臂集合就是帧种集合：后端加一种帧、这一侧忘了处理时编不过，
//! 而谁加一条 `_ =>`，穷尽性当场消失、新帧从此被静默丢弃（「后端明明发了，界面没反应」）—— 这条守它。

/// 生产段里 `fn {name}(` 那个函数体（到下一个顶格 `}` 为止）。
fn body_of<'a>(prod: &'a str, name: &str) -> &'a str {
    let at = prod
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("生产段里找不到 `fn {name}(`"));
    let end = prod[at..]
        .find("\n}\n")
        .unwrap_or_else(|| panic!("`{name}` 没有收尾"));
    &prod[at..at + end]
}

/// 一段里的臂（`{prefix}X(` 起头、同一行或后几行有 `=>`）的 `X`；`a | b` 连写的各算一个。
fn arms(body: &str, prefix: &str) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for l in body.lines() {
        let s = l.trim_start();
        let s = s.strip_prefix("f @ (").unwrap_or(s);
        if !s.starts_with(prefix) {
            continue;
        }
        for part in s.split(" | ") {
            if let Some(r) = part.trim_start().strip_prefix(prefix) {
                let name: String = r
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect();
                if !name.is_empty() {
                    out.insert(name);
                }
            }
        }
    }
    out
}

/// `{head}` 那个 enum 的变体名（`X(…)` 那一形）。
fn variants(src: &str, head: &str) -> std::collections::BTreeSet<String> {
    let at = src.find(head).unwrap_or_else(|| panic!("找不到 `{head}`"));
    let body = &src[at..at + src[at..].find("\n}").expect("enum 没有收尾")];
    body.lines()
        .skip(1)
        .filter_map(|l| l.trim().split_once('(').map(|(n, _)| n.to_string()))
        .collect()
}

#[test]
fn the_two_dispatch_layers_cover_every_frame_kind_without_a_catch_all() {
    let prod = guard_core::production_code(include_str!(
        "../../../../src/frontend/shell/src/stream_source/dispatch.rs"
    ));
    let route = body_of(&prod, "route");
    let apply = body_of(&prod, "apply");
    for (name, body) in [("route", route), ("apply", apply)] {
        assert!(
            !body.lines().any(|l| l.trim_start().starts_with("_ =>")),
            "`{name}` 里出现了兜底臂 —— 新帧会被静默丢弃；确实不处理也请写成具名臂"
        );
    }
    // 帧种由 `cmd-wire` 的类型说（另一份源）。
    let kinds = variants(
        include_str!("../../../../src/common/cmd-wire/src/frame.rs"),
        "pub enum Frame<R = Record> {",
    );
    assert_eq!(kinds.len(), 24, "帧种表读空了 / 读错了：{kinds:?}");
    assert_eq!(arms(route, "Frame::"), kinds, "`route` 的臂不是全部帧种");
    let ordered = variants(&prod, "pub(crate) enum Ordered {");
    assert!(
        ordered.is_subset(&kinds) && !ordered.is_empty(),
        "有序的那几种里有不是帧种的：{ordered:?}"
    );
    assert_eq!(
        arms(apply, "Ordered::"),
        ordered,
        "`apply` 的臂不是全部有序的那几种"
    );
}

/// 交活卡的地方恰好一处：分派第一层那一臂（本机远端同一处，origin 是这条流的）。
#[test]
fn live_cards_are_handed_over_in_one_place_with_the_stream_origin() {
    let prod = guard_core::production_code(include_str!(
        "../../../../src/frontend/shell/src/stream_source/dispatch.rs"
    ));
    guard_core::find_pinned(body_of(&prod, "route"), "crate::live_card::deliver(at, l)")
        .unwrap_or_else(|e| panic!("活卡那一臂不是恰好一处、或 origin 不是这条流的：{e}"));
    let mut n = 0;
    for (_, raw) in guard_core::scan_tree!(
        &guard_core::addr::repo_root().join("src/frontend/shell/src"),
        &["rs"]
    ) {
        n += guard_core::production_code(&raw)
            .matches("live_card::deliver(")
            .count();
    }
    assert_eq!(n, 1, "交活卡的地方不是恰好一处");
}
