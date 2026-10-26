//! 文件窗口反查：一个目录落在哪个仓库里 · 那里每份文件归哪一格（标题 · 状态 · 哪个权力范围）· 被两格声明 ⇒ 另一格 · 无主的那几份。

use super::*;
use crate::plan::fixture::{as_json, dump, dump_broken, who};
use serde_json::{json, Value};

/// 反查的回包，线上那一形（判据按键看）。
fn dir_of(doc: &PlanRead, dir: &str) -> Value {
    as_json(&of_dir(doc, dir))
}

fn doc(d: Value) -> PlanRead {
    let b = crate::plan::book::Book::default();
    let raw = d.to_string().into_bytes();
    let mut v = b
        .ingest(
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

/// 夹具那个仓库登记在 `repos/alpha`：src/ 底下 read（A1-1，已签收）· shared（A2 划出的根节点先声明，A2-1 也声明）·
/// third（A7，缺）· write（A1-2，空）声明了，stray 没人声明。
#[test]
fn files_in_a_repo_carry_their_cell_scope_and_state() {
    let v = dir_of(&doc(dump("/w")), "/w/repos/alpha/src");
    assert_eq!(v["workspace"], "/w");
    assert_eq!(v["slice"], "alpha");
    assert_eq!(v["unreadable"], Value::Null);
    let e = v["entries"].as_array().unwrap();
    let names: Vec<&str> = e.iter().map(|x| x["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["read.txt", "shared.txt", "third.txt", "write.txt"]);
    assert_eq!(e[0]["id"], "A1-1");
    assert_eq!(e[0]["title"], "甲的读入");
    // 「编号 标题」后端拼好（窗口照它画）。
    assert_eq!(e[0]["label"], "A1-1 甲的读入");
    assert_eq!(e[0]["statusCode"], "done");
    assert_eq!(e[0]["status"], "已签收 2");
    assert_eq!(e[0]["block"], Value::Null, "住 project");
    assert_eq!(e[0]["fileState"], "ok");
    assert!(e[0].get("signAtText").is_none(), "pb 0.5 不给签收时刻");
    assert_eq!(e[1]["id"], "A2");
    assert_eq!(e[1]["block"], "乙功能");
    assert_eq!(
        e[1]["dup"],
        json!([{"id": "A2-1", "label": "A2-1 乙的一半", "title": "乙的一半"}])
    );
    assert_eq!(e[2]["fileState"], "missing");
    assert_eq!(e[3]["id"], "A1-2");
    assert_eq!(e[3]["statusCode"], "open");
    assert_eq!(e[3]["status"], Value::Null);
    assert_eq!(e[3]["fileState"], "empty");
    // 无主：pb 给整个仓库的，这里只留落在这个目录里的文件名。
    assert_eq!(v["unowned"], json!(["stray.txt"]));
    assert_eq!(v["unownedWhy"], Value::Null);
    let t = dir_of(&doc(dump("/w")), "/w/repos/alpha/tests");
    assert_eq!(t["unowned"], json!([]));
    assert_eq!(t["entries"][0]["id"], "A1-3");
    assert_eq!(t["entries"][0]["statusCode"], Value::Null, "测试不算活");
    assert_eq!(t["entries"][0]["fileState"], "broken");
}

#[test]
fn a_file_declared_by_two_cells_names_the_other_one() {
    let mut d = dump("/w");
    d["repos"][0]["nodes"][2]["files"] = json!([{"path": "src/read.txt", "file_check": "present", "note": "3 字", "chars": 3, "bytes": 3}]);
    let v = dir_of(&doc(d), "/w/repos/alpha/src");
    assert_eq!(v["entries"][0]["id"], "A1-1");
    assert_eq!(
        v["entries"][0]["dup"],
        json!([{"id": "A1-2", "label": "A1-2 甲的写出", "title": "甲的写出"}])
    );
}

/// 仓库读不出（pb 给 `paths: null`）⇒ 判不了，不是一份都没有：`unowned` 为 null、`unownedWhy` 是 pb 那一句。
#[test]
fn an_unreadable_repo_says_why_unowned_cannot_be_told() {
    let mut d = dump("/w");
    d["repos"][0]["unowned"] = json!({"paths": null, "error": "仓库不是 git 仓"});
    let v = dir_of(&doc(d), "/w/repos/alpha/src");
    assert_eq!(v["unowned"], Value::Null);
    assert_eq!(v["unownedWhy"], "仓库不是 git 仓");
}

#[test]
fn the_repo_root_and_other_dirs_are_told_apart() {
    let d = doc(dump("/w"));
    let root = dir_of(&d, "/w/repos/alpha");
    assert_eq!(root["slice"], "alpha");
    assert_eq!(root["entries"], json!([]));
    assert_eq!(
        dir_of(&d, "/w/repos/alpha/src/")["entries"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    // 工作区根 · 计划仓 · 仓库名那个目录（仓库不在那儿）· 名字只是前缀相同的目录 ⇒ 不在仓库里。
    for dir in [
        "/w",
        "/w/.planned-build/alpha",
        "/w/alpha/src",
        "/w/repos",
        "/w/repos/alphabet/src",
        "/elsewhere",
    ] {
        assert_eq!(dir_of(&d, dir)["slice"], Value::Null, "{dir}");
    }
}

#[test]
fn an_unreadable_slice_says_so_and_lists_nothing() {
    let v = dir_of(&doc(dump_broken("/w")), "/w/repos/alpha/src");
    assert_eq!(v["slice"], "alpha");
    assert_eq!(v["unreadable"], "图.md 第 3 行：元数据缺 id");
    assert_eq!(v["entries"], json!([]));
    assert_eq!(v["unowned"], Value::Null);
}

/// ★ `plan-files` 的应答（线上类型 `PlanFiles` 的序列化，帧面照原样交出去）== 金样 `tests/__fixtures__/plan-files.golden.json`；
/// 出参对拍判据（`inbound_structure_guards`）拿这一份对注册表登的出参。
#[test]
fn the_reply_through_the_wire_type_is_the_golden_sample() {
    let got = dir_of(&doc(dump("/w")), "/w/repos/alpha/src");
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
    assert_eq!(dir_of(&d, "/w/elsewhere/alpha/src")["slice"], "alpha");
    assert_eq!(dir_of(&d, "/w/repos/alpha/src")["slice"], Value::Null);
}
