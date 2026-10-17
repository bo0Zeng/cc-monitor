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
    let del = copy_text("beFiles.batch.verbDelete", &[]);
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
    assert_eq!(undo_plan(&p, &chmod_reply(&[(Done, Some(0o644)), (Done, None)])), None);
    assert_eq!(undo_plan(&p, &chmod_reply(&[(Done, Some(0o644)), (Done, Some(0o70000))])), None, "不是权限位 ⇒ 当读不到");
    assert_eq!(undo_plan(&p, &chmod_reply(&[(Failed, None), (Skipped, None)])), None);
    assert_eq!(undo_plan(&plan(Verb::Delete, 1), &chmod_reply(&[(Done, Some(0o644))])), None);
}

/// 🔴 复制一组：同一个父目录 ⇒ 根是那个目录的上一级公共处；目标在别的分支 ⇒ 根是两边的公共上级，`from` / `to` 是根下相对段。
#[test]
fn a_copy_plan_roots_at_the_common_parent() {
    let p = copy_plan(b"/srv/a", &[b"x".to_vec(), b"d".to_vec()], b"/srv/a/sub", "devbox").unwrap();
    assert_eq!(
        p.args,
        serde_json::json!({"root": "/srv/a", "items": [{"from": "x", "to": "sub/x"}, {"from": "d", "to": "sub/d"}]})
    );
    assert_eq!(p.names, vec!["x", "d"]);
    assert_eq!(p.into.as_deref(), Some("/srv/a/sub"));
    let p = copy_plan(b"/srv/a/b", &[b"x".to_vec()], b"/srv/c/", "devbox").unwrap();
    assert_eq!(p.args, serde_json::json!({"root": "/srv", "items": [{"from": "a/b/x", "to": "c/x"}]}));
    let p = copy_plan(b"/a", &[b"x".to_vec()], b"/b", "devbox").unwrap();
    assert_eq!(p.args, serde_json::json!({"root": "/", "items": [{"from": "a/x", "to": "b/x"}]}), "只在根下相交");
    let p = copy_plan(b"/w", &[b"d".to_vec()], b"/w/d", "devbox").unwrap();
    assert_eq!(p.args, serde_json::json!({"root": "/w", "items": [{"from": "d", "to": "d/d"}]}), "复制进它自己底下：根按上一级算");
}

/// 🔴 同名那一问的答：整组一个答 ⇒ 每个键同一格；逐项选 ⇒ 点过的照点、没点的跳过；取消 ⇒ `None`。
#[test]
fn an_answer_has_one_cell_per_key_and_unpicked_rows_skip() {
    let c: Vec<Conflict> = serde_json::from_value(serde_json::json!([
        {"key": "0", "name": "a", "kind": "file", "there": "file", "answers": ["overwrite", "skip", "keep_both"]},
        {"key": "1", "name": "b", "kind": "file", "there": "file", "answers": ["overwrite", "skip", "keep_both"]},
    ]))
    .unwrap();
    let all = answer_of(&c, Some(Decide::KeepBoth), None).unwrap();
    assert_eq!(all.values().copied().collect::<Vec<_>>(), vec![Decide::KeepBoth; 2]);
    let picks = BTreeMap::from([("1".to_string(), Decide::Overwrite)]);
    let each = answer_of(&c, None, Some(&picks)).unwrap();
    assert_eq!(each, BTreeMap::from([("0".to_string(), Decide::Skip), ("1".to_string(), Decide::Overwrite)]));
    assert_eq!(answer_of(&c, None, None), None);
}
