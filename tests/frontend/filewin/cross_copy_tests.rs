//! `filewin/cross_copy.rs` 的判据 —— **复制到另一台机器**那一问。做的那一半住本机后端（`transfer-copy`，判据
//! `tests/backend/control/transfer_copy_tests.rs`）；窗口发一条的那一层（带票 · 冲突框 · 断了再问 · 撤）判据在 `batch_tests.rs`。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`the_window_spells_the_local_address_only_as_cmd_wire_local`] | 窗口包的生产码里本机地址只经 `cmd_wire::LOCAL` | 扫整个包 |
//! | [`the_local_machine_is_shown_as_a_word_and_sent_as_its_address`] | 本机在下拉里写成字、发出去是地址 | 期望手写 |

use super::*;

#[test]
fn the_window_spells_the_local_address_only_as_cmd_wire_local() {
    // 本机后端的地址只有一个家：`cmd_wire::LOCAL`。文件窗口包里不许另起一份常量、不许手写那个串。
    let files =
        guard_core::scan_tree_excluding(&guard_core::addr::filewin_src_root(), &["rs"], &[]);
    assert!(!files.is_empty(), "文件窗口包一份 .rs 都没扫到 —— 住址坏了");
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
            // 删掉的那份旧常量名拼开写：全仓 grep 旧名要零命中。
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
        "文件窗口包里一处 `cmd_wire::LOCAL` 都没有 —— 本机那几个判断去哪了"
    );
}

#[test]
fn the_local_machine_is_shown_as_a_word_and_sent_as_its_address() {
    let shown = shown_machine(cmd_wire::LOCAL);
    assert_eq!(
        shown,
        copy_text("detail.label.local", &[])
    );
    assert!(!shown.contains('<'), "下拉里出现了字面的本机地址");
    assert_eq!(origin_of(&shown), cmd_wire::LOCAL);
    assert_eq!(origin_of(&format!(" {shown} ")), cmd_wire::LOCAL);
    assert_eq!(
        (shown_machine("gpu-01"), origin_of(" gpu-01 ")),
        ("gpu-01".to_string(), "gpu-01".to_string())
    );
}
