//! 文件窗口反查（稿 06）：一个目录落在哪一片的仓库里 · 那里每份文件归哪一格（标题 · 状态 · 哪一块 · 签收时刻）· 被两格声明 ⇒ 另一格。

use super::*;
use crate::plan::fixture::{dump, dump_broken, who};
use serde_json::json;

fn doc(d: Value) -> Value {
    let b = crate::plan::book::Book::default();
    let raw = d.to_string().into_bytes();
    let mut v = b
        .take(
            crate::plan::dump::Ran::Dump { doc: d, raw },
            std::path::Path::new("/w"),
            &who,
            1,
        )
        .unwrap();
    // 时刻成字与生产同一处（`plan_face::annotated` 也调它）。
    crate::plan::product::with_time_texts(&mut v, 1_767_400_000_000, &Default::default());
    v
}

#[test]
fn files_in_a_slice_repo_carry_their_cell_and_a_double_claim_names_the_other() {
    let v = of_dir(&doc(dump("/w")), "/w/alpha/src");
    assert_eq!(v["workspace"], "/w");
    assert_eq!(v["slice"], "alpha");
    assert_eq!(v["unreadable"], Value::Null);
    let e = v["entries"].as_array().unwrap();
    let names: Vec<&str> = e.iter().map(|x| x["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["read.txt", "write.txt"]);
    // read.txt 两格都声明：先声明的那一格作主（排期先后），另一格进 `dup`。
    assert_eq!(e[0]["id"], "A1-1");
    assert_eq!(e[0]["title"], "甲的读入");
    assert_eq!(e[0]["statusCode"], "done");
    assert_eq!(e[0]["block"], "甲功能");
    assert_eq!(e[0]["fileState"], "ok");
    assert_eq!(e[0]["dup"], json!([{"id": "A1-2", "title": "甲的写出"}]));
    // 作数的那一条签收（后签的），时刻成了字。
    assert!(e[0]["signAtText"].as_str().is_some_and(|s| !s.is_empty()));
    assert_eq!(e[1]["id"], "A1-2");
    assert_eq!(e[1]["statusCode"], "open");
    assert_eq!(e[1]["fileState"], "missing");
    assert_eq!(e[1]["dup"], json!([]));
    assert_eq!(e[1]["signAtText"], Value::Null);
    // 无主要 pb 给 `unowned`（请求单 3）；没给 ⇒ `null`，不是空表。
    assert_eq!(v["unowned"], Value::Null);
}

#[test]
fn the_repo_root_and_other_dirs_are_told_apart() {
    let d = doc(dump("/w"));
    // 仓库根：那里没有声明的文件，但它是那一片的仓库 ⇒ 有 `slice`、条目空。
    let root = of_dir(&d, "/w/alpha");
    assert_eq!(root["slice"], "alpha");
    assert_eq!(root["entries"], json!([]));
    // 尾斜杠照样认。
    assert_eq!(
        of_dir(&d, "/w/alpha/src/")["entries"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // 工作区根 · 计划仓 · 不在任何一片的目录 · 名字只是前缀相同的目录 ⇒ 不在片里。
    for dir in [
        "/w",
        "/w/.planned-build/alpha",
        "/w/beta",
        "/w/alphabet/src",
        "/elsewhere",
    ] {
        assert_eq!(of_dir(&d, dir)["slice"], Value::Null, "{dir}");
    }
}

#[test]
fn an_unreadable_slice_says_so_and_lists_nothing() {
    let v = of_dir(&doc(dump_broken("/w")), "/w/alpha/src");
    assert_eq!(v["slice"], "alpha");
    assert_eq!(v["unreadable"], "图.md 第 3 行：元行缺 id");
    assert_eq!(v["entries"], json!([]));
}

/// ★ `plan-files` 的应答（过一遍线上类型 `PlanFiles`，帧面出口那一道）== 金样 `tests/__fixtures__/plan-files.golden.json`；
/// 出参对拍判据（`inbound_structure_guards`）拿这一份对注册表登的出参。
#[test]
fn the_reply_through_the_wire_type_is_the_golden_sample() {
    let got = crate::plan::wire::checked::<crate::plan::wire::PlanFiles>(of_dir(
        &doc(dump("/w")),
        "/w/alpha/src",
    ))
    .unwrap();
    let golden: Value =
        serde_json::from_str(include_str!("../../__fixtures__/plan-files.golden.json")).unwrap();
    assert_eq!(
        got,
        golden,
        "金样该是：\n{}",
        serde_json::to_string_pretty(&got).unwrap()
    );
}
