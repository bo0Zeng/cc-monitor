//! 文件窗口反查：一个目录落在哪一片的仓库里 · 那里每份文件归哪一格（标题 · 状态 · 哪一块 · 签收时刻）· 被两格声明 ⇒ 另一格 · 无主的那几份。

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

/// 夹具那一片的仓库登记在 `repos/alpha`：src/ 底下 read（A1-1）· shared（A2，派出去那一块的根格）· write（A1-2，缺）声明了，stray 没人声明。
#[test]
fn files_in_a_slice_repo_carry_their_cell_block_and_sign() {
    let v = of_dir(&doc(dump("/w")), "/w/repos/alpha/src");
    assert_eq!(v["workspace"], "/w");
    assert_eq!(v["slice"], "alpha");
    assert_eq!(v["unreadable"], Value::Null);
    let e = v["entries"].as_array().unwrap();
    let names: Vec<&str> = e.iter().map(|x| x["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["read.txt", "shared.txt", "write.txt"]);
    assert_eq!(e[0]["id"], "A1-1");
    assert_eq!(e[0]["title"], "甲的读入");
    assert_eq!(e[0]["statusCode"], "done");
    assert_eq!(e[0]["block"], Value::Null, "住顶块");
    assert_eq!(e[0]["fileState"], "ok");
    assert!(e[0]["signAtText"].as_str().is_some_and(|s| !s.is_empty()));
    assert_eq!(e[1]["id"], "A2");
    assert_eq!(e[1]["block"], "乙功能");
    assert_eq!(e[2]["id"], "A1-2");
    assert_eq!(e[2]["fileState"], "missing");
    assert_eq!(e[2]["signAtText"], Value::Null);
    assert_eq!(e[2]["dup"], json!([]));
    // 无主：pb 给整片的，这里只留落在这个目录里的文件名。
    assert_eq!(v["unowned"], json!(["stray.txt"]));
    assert_eq!(v["unownedWhy"], Value::Null);
    let t = of_dir(&doc(dump("/w")), "/w/repos/alpha/tests");
    assert_eq!(t["unowned"], json!([]));
    assert_eq!(t["entries"][0]["id"], "A1-3");
}

#[test]
fn a_file_declared_by_two_cells_names_the_other_one() {
    let mut d = dump("/w");
    d["slices"][0]["cells"][2]["files"] =
        json!([{"path": "src/read.txt", "state": "在", "note": "3 字"}]);
    let v = of_dir(&doc(d), "/w/repos/alpha/src");
    assert_eq!(v["entries"][0]["id"], "A1-1");
    assert_eq!(
        v["entries"][0]["dup"],
        json!([{"id": "A1-2", "title": "甲的写出"}])
    );
}

/// 仓库读不出（pb 给 `paths: null`）⇒ 判不了，不是一份都没有：`unowned` 为 null、`unownedWhy` 是 pb 那一句。
#[test]
fn an_unreadable_repo_says_why_unowned_cannot_be_told() {
    let mut d = dump("/w");
    d["slices"][0]["unowned"] = json!({"paths": null, "why": "仓库不是 git 仓"});
    let v = of_dir(&doc(d), "/w/repos/alpha/src");
    assert_eq!(v["unowned"], Value::Null);
    assert_eq!(v["unownedWhy"], "仓库不是 git 仓");
}

#[test]
fn the_repo_root_and_other_dirs_are_told_apart() {
    let d = doc(dump("/w"));
    let root = of_dir(&d, "/w/repos/alpha");
    assert_eq!(root["slice"], "alpha");
    assert_eq!(root["entries"], json!([]));
    assert_eq!(
        of_dir(&d, "/w/repos/alpha/src/")["entries"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    // 工作区根 · 计划仓 · 片名那个目录（仓库不在那儿）· 名字只是前缀相同的目录 ⇒ 不在片里。
    for dir in [
        "/w",
        "/w/.planned-build/alpha",
        "/w/alpha/src",
        "/w/repos",
        "/w/repos/alphabet/src",
        "/elsewhere",
    ] {
        assert_eq!(of_dir(&d, dir)["slice"], Value::Null, "{dir}");
    }
}

#[test]
fn an_unreadable_slice_says_so_and_lists_nothing() {
    let v = of_dir(&doc(dump_broken("/w")), "/w/repos/alpha/src");
    assert_eq!(v["slice"], "alpha");
    assert_eq!(v["unreadable"], "图.md 第 3 行：元行缺 id");
    assert_eq!(v["entries"], json!([]));
    assert_eq!(v["unowned"], Value::Null);
}

/// ★ `plan-files` 的应答（过一遍线上类型 `PlanFiles`，帧面出口那一道）== 金样 `tests/__fixtures__/plan-files.golden.json`；
/// 出参对拍判据（`inbound_structure_guards`）拿这一份对注册表登的出参。
#[test]
fn the_reply_through_the_wire_type_is_the_golden_sample() {
    let got = crate::plan::wire::tests::checked::<crate::plan::wire::PlanFiles>(of_dir(
        &doc(dump("/w")),
        "/w/repos/alpha/src",
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

/// 仓库登记在别处 ⇒ 按登记的位置认；片名那个目录不再算。
#[test]
fn a_slice_registered_elsewhere_is_found_at_its_registered_path() {
    let d = doc(crate::plan::fixture::dump_at("/w", "elsewhere/alpha"));
    assert_eq!(of_dir(&d, "/w/elsewhere/alpha/src")["slice"], "alpha");
    assert_eq!(of_dir(&d, "/w/repos/alpha/src")["slice"], Value::Null);
}
