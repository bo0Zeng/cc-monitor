//! `filewin/cross_copy.rs` 的判据 —— **复制到另一台机器**那一问。做的那一半住本机后端（`transfer-copy`，判据
//! `tests/backend/control/transfer_copy_tests.rs`）；窗口发一条的那一层（带票 · 冲突框 · 断了再问 · 撤）判据在 `batch_tests.rs`。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`the_window_spells_the_local_address_only_as_cmd_wire_local`] | 窗口包的生产码里本机地址只经 `cmd_wire::LOCAL` | 扫整个包 |
//! | [`the_box_sends_to_the_key_it_keeps_and_titles_it_by_machine_word`] | 框里那台的键 ⇒ 发出去的 `to` 就是那个键，落点是选择器上那一处；「到哪」那一段是 `machine_word(那台):落点` | 合成对端记下的入参 · 看板上的计划 |
//! | [`the_box_keeps_the_key_and_only_machine_word_names_it`] | 第二道：包里不自取本机那一词 | 扫整个包 |

/// 框里存的是那台的键：选了 `gpu-01`、落点 `/home/u/in` ⇒ 通道上那一条 `transfer-copy` 发到 `gpu-01`，
/// 「进度」那一行的「到哪」是 `machine_word(gpu-01):/home/u/in`。
/// ⚠ 框里只列远端，远端的 `machine_word` 就是键 ⇒ 这一条分不出「经 machine_word」与「直接拼键」；
/// 本机那一词不自取由下面那道字面扫描管。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_box_sends_to_the_key_it_keeps_and_titles_it_by_machine_word() {
    let (mut w, log) = crate::transfer::tests::one_line_window(Default::default());
    *w.listing.rows.lock().unwrap() = vec![crate::source::Listed::from(crate::source::Row {
        name: "a.bin".into(),
        path: "/srv/a.bin".into(),
        is_dir: false,
        size: 3,
        lossy_name: false,
    })];
    w.machines = vec!["gpu-01".into()];
    assert!(w.begin_cross_rows(&[0]), "那一问没摆出来");
    let (h, line) = (w.rt.clone().unwrap(), w.line.clone().unwrap());
    // 选择器停在那台的 `/home/u/in`（下拉选中 gpu-01 那一下做的同一件）。
    w.cross_pick.go(&h, &line, "gpu-01", "/home/u/in", None);
    assert!(w.confirm_cross(None), "答完那一问没起来");
    let (_, args) = crate::transfer::tests::step_of(&log, crate::transfer::CMD_TRANSFER).await;
    assert_eq!(args["to"], "gpu-01", "发出去的不是框里那台的键：{args}");
    assert_eq!(
        args["to_dir"], "/home/u/in",
        "落点不是选择器上那一处：{args}"
    );
    let into: Vec<Option<String>> = w
        .progress
        .jobs()
        .into_iter()
        .map(|j| match j.trip {
            crate::progress::Trip::Batch { board, .. } => board.plan().into,
            _ => None,
        })
        .collect();
    let word = copy_core::machine_word(&cmd_wire::Machine::named("gpu-01"));
    assert_eq!(
        into,
        [Some(format!("{word}:/home/u/in"))],
        "「到哪」那一段不是那台的名字 ＋ 落点"
    );
}

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

/// 框里存的是机器键，给人看的字只经 `copy_core::machine_word`：文件窗口包里不自取「本机」那一词，
/// 也就没有「照显示出来的字反推是哪台」那一支。
#[test]
fn the_box_keeps_the_key_and_only_machine_word_names_it() {
    let files =
        guard_core::scan_tree_excluding(&guard_core::addr::filewin_src_root(), &["rs"], &[]);
    assert!(!files.is_empty(), "文件窗口包一份 .rs 都没扫到 —— 住址坏了");
    let own: Vec<String> = files
        .iter()
        .filter(|(_, raw)| guard_core::production_code(raw).contains("\"detail.label.local\""))
        .map(|(p, _)| guard_core::addr::rel_of(p))
        .collect();
    assert!(
        own.is_empty(),
        "这几份自己取了本机那一词（改调 `copy_core::machine_word`）：{own:?}"
    );
}
