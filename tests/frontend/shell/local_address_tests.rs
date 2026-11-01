//! 壳的生产码里本机地址只经 `cmd_wire::LOCAL`。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`the_shell_spells_the_local_address_only_as_cmd_wire_local`] | 壳包的生产码里本机地址只经 `cmd_wire::LOCAL` | 扫整个包 |

#[test]
fn the_shell_spells_the_local_address_only_as_cmd_wire_local() {
    // 本机后端的地址只有一个家：`cmd_wire::LOCAL`。壳包里不许另起一份常量、不许经别的包转一份名字、不许手写那个串。
    let files = guard_core::scan_tree_excluding(&guard_core::addr::shell_src_root(), &["rs"], &[]);
    assert!(!files.is_empty(), "壳包一份 .rs 都没扫到 —— 住址坏了");
    let prod: Vec<(String, String)> = files
        .iter()
        .map(|(p, raw)| {
            (
                guard_core::addr::rel_of(p),
                guard_core::production_code(raw),
            )
        })
        .collect();
    let quoted = format!("\"{}\"", cmd_wire::LOCAL);
    let own: Vec<&str> = prod
        .iter()
        .filter(|(_, code)| {
            // 不再用的那个常量名拼开写：全仓 grep 旧名要零命中。
            guard_core::contains_word(code, concat!("LOCAL", "_ORIGIN"))
                || guard_core::contains_word(code, &quoted)
        })
        .map(|(rel, _)| rel.as_str())
        .collect();
    assert!(
        own.is_empty(),
        "这几份自己写了本机地址（改读 `cmd_wire::LOCAL`）：{own:?}"
    );
    // 反空真：读的那一侧真在（不然上面零命中只是没人提本机）。
    assert!(
        prod.iter()
            .any(|(_, code)| guard_core::contains_word(code, "cmd_wire::LOCAL")),
        "壳包里一处 `cmd_wire::LOCAL` 都没有 —— 本机那几个判断去哪了"
    );
}
