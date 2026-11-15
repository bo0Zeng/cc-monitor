//! 一组的字与几组派生的计划：结局那一句（后端 · 手机下载同一句）· 断开那一行 · 进度 · 改权限的撤销 · 复制一组的公共根 · 同名那一问的答。
//! 一组的全过程（[`run`]）由桌面那一侧的判据带着看板一起测（`tests/frontend/filewin/batch_tests.rs`）。

use super::*;
use cmd_wire::batch::BatchOutcome;

fn outs(states: &[ItemState]) -> Vec<BatchOutcome> {
    states
        .iter()
        .enumerate()
        .map(|(i, s)| BatchOutcome::bare(i.to_string(), *s))
        .collect()
}

/// ★ 是 0 的那一段不写：「删除 5 项 · 完成 3 · 失败 ×2」，不出「跳过 0」「失败 ×0」「已取消 0」「完成 0」；全成 ⇒「已删除 5 项」。
#[test]
fn the_ending_line_leaves_out_the_parts_that_are_zero() {
    use ItemState::*;
    let seg = |k: &str, n: &str| match k {
        "ok" => copy_text("beFiles.batch.segOk", &[("ok", n)]),
        "skip" => copy_text("beFiles.batch.segSkip", &[("skip", n)]),
        "bad" => copy_text("beFiles.batch.segBad", &[("bad", n)]),
        _ => copy_text("beFiles.batch.segLeft", &[("left", n)]),
    };
    let sep = copy_text("kit.text.sep", &[]);
    let del = copy_text("rsFilewinWriteops.label.delete", &[]);
    let head = |n: &str| copy_text("beFiles.batch.head", &[("action", &del), ("n", n)]);
    let line = |parts: Vec<String>| parts.join(&sep);
    assert_eq!(
        said_of(&del, &outs(&[Done, Done, Done, Failed, Exists]), None),
        line(vec![head("5"), seg("ok", "3"), seg("bad", "2")])
    );
    assert_eq!(
        said_of(&del, &outs(&[Done, Skipped, Cancelled]), None),
        line(vec![
            head("3"),
            seg("ok", "1"),
            seg("skip", "1"),
            seg("left", "1")
        ])
    );
    assert_eq!(
        said_of(&del, &outs(&[Failed, Failed]), None),
        line(vec![head("2"), seg("bad", "2")])
    );
    assert_eq!(
        said_of(&del, &outs(&[Done, Done]), None),
        copy_text("beFiles.batch.done", &[("action", &del), ("n", "2")])
    );
    for s in [
        said_of(&del, &outs(&[Done, Failed]), None),
        said_of(&del, &outs(&[Skipped, Cancelled]), None),
    ] {
        assert!(
            !s.contains(" 0") && !s.contains("×0"),
            "写了是 0 的那一段：{s}"
        );
    }
}

fn plan(verb: Verb, n: usize) -> Plan {
    Plan {
        verb,
        cmd: "files-chmod",
        args: serde_json::json!({"root": "/r", "items": (0..n).map(|i| serde_json::json!({"rel": format!("f{i}")})).collect::<Vec<_>>(), "mode": "600"}),
        names: (0..n).map(|i| format!("f{i}")).collect(),
        into: None,
        machine: "devbox".into(),
    }
}

#[test]
fn the_cut_line_says_the_rest_is_unknown() {
    let s = cut_line(
        &plan(Verb::Copy, 2),
        &Tick {
            done: 1,
            total: 2,
            bytes: None,
        },
    );
    assert!(s.starts_with("devbox"), "{s}");
    assert!(s.contains("其余待核"), "{s}");
}

/// 下载那一圈断开那一行：做完几项、没下完几项（连回来那几项整份重下，不是「待核」）。
#[test]
fn the_download_cut_line_counts_what_is_left() {
    assert_eq!(
        cut_left_line(&plan(Verb::Download, 3), 1, 2),
        "devbox 连接已断开 · 下载 3 项 · 完成 1 · 中断 2"
    );
}

#[test]
fn progress_goes_by_bytes_when_the_backend_reports_them() {
    let t = Tick::of_cell(
        &serde_json::json!({"done": 0, "total": 1, "bytes_done": 512, "bytes_total": 2048}),
    )
    .unwrap();
    assert_eq!(t.frac(), Some(0.25), "一份大文件传到四分之一：照字节画");
    let t = Tick::of_cell(&serde_json::json!({"done": 1, "total": 4})).unwrap();
    assert_eq!(t.frac(), Some(0.25));
    assert!(Tick::of_cell(&serde_json::json!({"done": 1})).is_none());
}

fn chmod_reply(befores: &[(ItemState, Option<u32>)]) -> BatchReply {
    BatchReply {
        outcomes: befores
            .iter()
            .enumerate()
            .map(|(i, (s, b))| {
                let mut o = BatchOutcome::bare(i.to_string(), *s);
                o.before = *b;
                o
            })
            .collect(),
        said: String::new(),
    }
}

/// 🔴 改权限的撤销：做成的三项、改之前两种值 ⇒ 两组（各自那几项、各自那一值）；没做成的那一项不撤。
#[test]
fn a_chmod_undo_groups_the_done_items_by_what_they_were_before() {
    use ItemState::*;
    let p = plan(Verb::Chmod, 4);
    let r = chmod_reply(&[
        (Done, Some(0o644)),
        (Done, Some(0o755)),
        (Failed, None),
        (Done, Some(0o644)),
    ]);
    let got: Vec<(String, Vec<String>)> = undo_plan(&p, &r)
        .expect("读得到每一项改之前的值 ⇒ 给撤销")
        .into_iter()
        .map(|g| (g.args["mode"].as_str().unwrap().to_string(), g.names))
        .collect();
    assert_eq!(
        got,
        vec![
            ("644".to_string(), vec!["f0".to_string(), "f3".to_string()]),
            ("755".to_string(), vec!["f1".to_string()]),
        ]
    );
}

/// 🔴 有一项做成了却读不到改之前的值 ⇒ 整组不给撤销（撤不全就不撤，不猜）；不是改权限 · 一项都没做成 ⇒ 也不给。
#[test]
fn a_chmod_undo_is_all_or_nothing() {
    use ItemState::*;
    let p = plan(Verb::Chmod, 2);
    assert_eq!(
        undo_plan(&p, &chmod_reply(&[(Done, Some(0o644)), (Done, None)])),
        None
    );
    assert_eq!(
        undo_plan(
            &p,
            &chmod_reply(&[(Done, Some(0o644)), (Done, Some(0o70000))])
        ),
        None,
        "不是权限位 ⇒ 当读不到"
    );
    assert_eq!(
        undo_plan(&p, &chmod_reply(&[(Failed, None), (Skipped, None)])),
        None
    );
    assert_eq!(
        undo_plan(&plan(Verb::Delete, 1), &chmod_reply(&[(Done, Some(0o644))])),
        None
    );
}

/// 🔴 同名那一问的答：整组一个答 ⇒ 每个键同一格；逐项选 ⇒ 点过的照点、没点的跳过；取消 ⇒ `None`。
#[test]
fn an_answer_has_one_cell_per_key_and_unpicked_rows_skip() {
    let c: Vec<Conflict> = serde_json::from_value(serde_json::json!([
        {"key": "0", "name": "a", "kind": "file", "there": "file", "answers": ["overwrite", "skip", "keep_both"]},
        {"key": "1", "name": "b", "kind": "file", "there": "file", "answers": ["overwrite", "skip", "keep_both"]},
    ]))
    .unwrap();
    let all = answer_of(
        c.iter().map(|c| c.key.as_str()),
        Some(Decide::KeepBoth),
        None,
    )
    .unwrap();
    assert_eq!(
        all.values().copied().collect::<Vec<_>>(),
        vec![Decide::KeepBoth; 2]
    );
    let picks = BTreeMap::from([("1".to_string(), Decide::Overwrite)]);
    let each = answer_of(c.iter().map(|c| c.key.as_str()), None, Some(&picks)).unwrap();
    assert_eq!(
        each,
        BTreeMap::from([
            ("0".to_string(), Decide::Skip),
            ("1".to_string(), Decide::Overwrite)
        ])
    );
    assert_eq!(
        answer_of(c.iter().map(|c| c.key.as_str()), None, None),
        None
    );
}

/// 🔴 同名那一问的按钮字只住这里：一项给了几种答 ⇒ 几颗，先后恒为跳过 · 保留两份 · 覆盖（不照那台给的先后）；
/// 每颗带整组按钮那一句与逐项那一格的字。整组那几颗是各项给了的答并起来，同样的先后。
#[test]
fn the_clash_choices_carry_their_own_words_in_one_order() {
    let three = choices(&[Decide::Overwrite, Decide::Skip, Decide::KeepBoth]);
    assert_eq!(
        three.iter().map(|c| c.decide).collect::<Vec<_>>(),
        vec![Decide::Skip, Decide::KeepBoth, Decide::Overwrite]
    );
    assert_eq!(three[0].label, copy_text("rsFilewinBatch.btn.skip", &[]));
    assert_eq!(three[0].pick, copy_text("rsFilewinBatch.pick.skip", &[]));
    assert_eq!(
        three[2].label,
        copy_text("rsFilewinBatch.btn.overwrite", &[])
    );
    assert_eq!(three[1].pick, copy_text("rsFilewinBatch.btn.keepBoth", &[]));
    assert_ne!(
        three[0].label, three[0].pick,
        "整组那一句与逐项那一格是两句"
    );

    let c: Vec<Conflict> = serde_json::from_value(serde_json::json!([
        {"key": "0", "name": "d", "kind": "dir", "there": "dir", "answers": ["keep_both"]},
        {"key": "1", "name": "b", "kind": "file", "there": "file", "answers": ["overwrite", "skip", "keep_both"]},
    ]))
    .unwrap();
    assert_eq!(
        whole(&c).iter().map(|c| c.decide).collect::<Vec<_>>(),
        vec![Decide::Skip, Decide::KeepBoth, Decide::Overwrite],
        "并起来照同一个先后，不照哪一项先出现"
    );
    assert!(choices(&[]).is_empty());
}

/// 🔴 本机那一侧的同名清单与后端那一形同一份：要放的名字撞了已有的（分不分大小写由调用方说）⇒ 一项一格 `Conflict`，
/// 键是序号、两头都是文件、答法照 [`answers_of`]（文件之间：覆盖 · 跳过 · 保留两份）。保留两份起名「名 (2).ext」起、跳过已占的。
#[test]
fn a_local_name_list_clashes_like_the_backend_and_keeps_both_from_two() {
    let names = ["a.txt".to_string(), "b.md".to_string(), "c".to_string()];
    let here = ["A.TXT".to_string(), "c".to_string(), "c (2)".to_string()];
    let got = local_clashes(&names, &here, true);
    let want: Vec<Conflict> = serde_json::from_value(serde_json::json!([
        {"key": "0", "name": "a.txt", "kind": "file", "there": "file", "answers": ["overwrite", "skip", "keep_both"]},
        {"key": "2", "name": "c", "kind": "file", "there": "file", "answers": ["overwrite", "skip", "keep_both"]},
    ]))
    .unwrap();
    assert_eq!(got, want);
    assert_eq!(
        answers_of(false, true),
        vec![Decide::Overwrite, Decide::Skip, Decide::KeepBoth]
    );
    assert_eq!(
        answers_of(true, false),
        vec![Decide::KeepBoth],
        "目录不合并、提交不收跳过"
    );
    assert_eq!(
        local_clashes(&names, &here, false).len(),
        1,
        "分大小写 ⇒ A.TXT 不撞 a.txt"
    );
    assert_eq!(taken_as("a.txt", &here, true), Some("A.TXT"));

    assert_eq!(minted("notes.tar.gz", 2), "notes.tar (2).gz");
    assert_eq!(minted(".env", 3), ".env (3)");
    assert_eq!(
        keep_both_name("a.txt", &here, true).as_deref(),
        Some("a (2).txt")
    );
    assert_eq!(
        keep_both_name("c", &here, true).as_deref(),
        Some("c (3)"),
        "(2) 已占 ⇒ 往后试"
    );
    assert!(MINT_TRIES >= 2);
}

/// 🔴 删除 · 改权限一组的入参照线上类型拼：args 解得回 `FilesDeleteReq` / `FilesChmodReq`，目录项带 `recursive`，
/// 命令名是那两条命令的 NAME。
#[test]
fn delete_and_chmod_plans_are_typed_requests() {
    use cmd_wire::files::{FilesChmodReq, FilesDeleteReq};
    let root = WirePath::from("/srv/p");
    let p = delete_plan(
        root.clone(),
        &[
            (WirePath::from("a.txt"), "a.txt".to_string(), false),
            (WirePath::from("d"), "d".to_string(), true),
        ],
        "devbox",
    );
    assert_eq!(p.verb, Verb::Delete);
    assert_eq!(p.cmd, <FilesDeleteReq as Call>::NAME);
    assert_eq!(p.names, vec!["a.txt", "d"]);
    assert_eq!(p.machine, "devbox");
    let req: FilesDeleteReq = serde_json::from_value(p.args.clone()).unwrap();
    assert_eq!(req.root, root);
    assert_eq!(req.items[0].recursive, None);
    assert_eq!(req.items[1].recursive, Some(true));
    assert_eq!(req.ticket, None);

    let c = chmod_plan(
        root.clone(),
        &[(WirePath::from("a.txt"), "a.txt".to_string())],
        "755",
        "devbox",
    );
    assert_eq!(c.verb, Verb::Chmod);
    assert_eq!(c.cmd, <FilesChmodReq as Call>::NAME);
    let req: FilesChmodReq = serde_json::from_value(c.args.clone()).unwrap();
    assert_eq!(req.mode, "755");
    assert_eq!(req.items.len(), 1);
}

/// 🔴 改权限那一框的标题与现值行：一项 ⇒ 带名字；几项 ⇒ 按件数。现值全同 ⇒ 那一值、预填；不同 ⇒ mixed、不预填；
/// 有一项读不到 · 空 ⇒ 读不到、不预填（不猜）。
#[test]
fn the_chmod_box_title_and_line_come_from_here() {
    assert_eq!(
        chmod_title(&["a.txt".to_string()]),
        copy_text("rsFilewinWriteops.heading.chmodOne", &[("name", "a.txt")])
    );
    assert_eq!(
        chmod_title(&["a".to_string(), "b".to_string()]),
        copy_text("rsFilewinSelect.label.chmodMany", &[("n", "2")])
    );
    let now = mode_line(&[Some(0o644), Some(0o644)]);
    assert_eq!(
        now.line,
        copy_text("rsFilewinWriteops.mode.now", &[("mode", "644")])
    );
    assert_eq!(now.prefill.as_deref(), Some("644"));
    let mixed = mode_line(&[Some(0o644), Some(0o755)]);
    assert_eq!(mixed.line, copy_text("rsFilewinWriteops.mode.mixed", &[]));
    assert_eq!(mixed.prefill, None);
    for m in [&[Some(0o644), None][..], &[][..]] {
        let u = mode_line(m);
        assert_eq!(u.line, copy_text("rsFilewinWriteops.mode.unreadable", &[]));
        assert_eq!(u.prefill, None);
    }
}

/// 复制一组的公共根照 `WirePath::parent` 切：源带好几个尾 `/` 也是它的上一级。
#[test]
fn copy_root_drops_every_trailing_slash_of_a_source() {
    let req = copy_req(&[(b"/x/y//".to_vec(), b"/x/y/b".to_vec())]).unwrap();
    assert_eq!(req.root.0, b"/x");
}
