//! 文件窗口反查：`plan-files` 的回包按线上类型解成一张按名字查的表（期望手写，回包是合成的中性例子）。

use super::*;
use cmd_wire::plan::{PlanFileState, PlanFiles, PlanStatusCode};
use serde_json::json;

fn dir(v: serde_json::Value) -> Option<PlanDir> {
    PlanDir::of(serde_json::from_value::<PlanFiles>(v).unwrap())
}

fn entry(name: &str, id: &str, status: &str) -> serde_json::Value {
    let code = if status == "none" {
        serde_json::Value::Null
    } else {
        json!(status)
    };
    json!({"name": name, "id": id, "label": id, "title": null, "statusCode": code, "status": null, "block": null,
           "fileState": "ok", "fileNote": null, "dup": []})
}

/// ★ pb 0.5 不给签收时刻：回包里再带 `signAtText` 就是形状对不上（线上类型拒多出来的键）。
#[test]
fn a_sign_time_is_no_longer_part_of_the_shape() {
    let mut e = entry("read.txt", "A1-1", "done");
    e["signAtText"] = json!("02:08");
    let v = json!({"workspace": "/w", "slice": "alpha", "unreadable": null, "unowned": [], "unownedWhy": null, "entries": [e]});
    assert!(serde_json::from_value::<PlanFiles>(v).is_err());
}

#[test]
fn a_dir_outside_every_slice_adds_nothing() {
    assert!(dir(json!({"workspace": null, "slice": null, "unreadable": null, "entries": [], "unowned": null, "unownedWhy": null})).is_none());
}

#[test]
fn entries_become_owners_by_name_with_label_status_block_and_double_claims() {
    let d = dir(json!({"workspace": "/w", "slice": "alpha", "unreadable": null, "unowned": [], "unownedWhy": null, "entries": [
        {"name": "read.txt", "id": "A1-1", "label": "A1-1 甲的读入", "title": "甲的读入", "statusCode": "done", "status": "已签收",
         "block": "甲功能", "fileState": "ok", "fileNote": "12 字",
         "dup": [{"id": "A1-2", "label": "A1-2 甲的写出", "title": "甲的写出"}]},
        {"name": "write.txt", "id": "A1-2", "label": "A1-2", "title": null, "statusCode": "open", "status": null,
         "block": null, "fileState": "broken", "fileNote": "不是 UTF-8", "dup": []},
        {"name": "gone.txt", "id": "A2", "label": "A2 乙", "title": "乙", "statusCode": null, "status": null,
         "block": null, "fileState": "missing", "fileNote": null, "dup": []}
    ]}))
    .unwrap();
    assert_eq!((d.workspace.as_str(), d.slice.as_str()), ("/w", "alpha"));
    assert_eq!(d.unreadable, None);
    let Claim::Owner(r) = d.claim("read.txt") else {
        panic!("read.txt 该有主")
    };
    assert_eq!((r.id.as_str(), r.label.as_str()), ("A1-1", "A1-1 甲的读入"));
    assert_eq!(r.status_code, Some(PlanStatusCode::Done));
    assert_eq!(r.block.as_deref(), Some("甲功能"));
    assert_eq!(r.status.as_deref(), Some("已签收"));
    // 预览头那一行：仓库 · 权力范围，不带签收时刻（pb 0.5 不给）。
    assert_eq!(
        owner_line("alpha", r)
            .matches(&copy_text("kit.text.sep", &[]))
            .count(),
        1
    );
    assert_eq!(broken(r), None);
    // 被几格声明 ⇒ 灰色读数「共用 · 也在 {编号 标题}」，「编号 标题」照抄后端拼好的那一格。
    assert_eq!(dup_text(r), "共用 · 也在 A1-2 甲的写出");
    let Claim::Owner(w) = d.claim("write.txt") else {
        panic!("write.txt 该有主")
    };
    assert_eq!(w.file_state, Some(PlanFileState::Broken));
    assert_eq!(broken(w), Some("不是 UTF-8"));
    let Claim::Owner(g) = d.claim("gone.txt") else {
        panic!("gone.txt 该有主")
    };
    assert_eq!(g.status_code, None);
    assert_eq!(d.claim("nobody.txt"), Claim::Unknown);
}

/// ★ 类不许记收的值的节点（`statusCode: null`，如测试）不画成「没签收」：图标与没签收的不同。
#[test]
fn a_node_outside_the_work_is_not_drawn_as_open() {
    let d = dir(json!({"workspace": "/w", "slice": "alpha", "unreadable": null, "unowned": [], "unownedWhy": null,
        "entries": [entry("proof.txt", "A1-9", "none"), entry("todo.txt", "A1-8", "open")]}))
    .unwrap();
    let (Claim::Owner(e), Claim::Owner(o)) = (d.claim("proof.txt"), d.claim("todo.txt")) else {
        panic!("两份都该有主")
    };
    assert_eq!(e.status_code, None);
    assert_eq!(o.status_code, Some(PlanStatusCode::Open));
    assert_ne!(status_icon(e.status_code), status_icon(o.status_code));
}

/// ★ 无主照回包画：列在 `unowned` 里的那一份是无主；判不了（`unowned: null`）⇒ 没主的那几份带 pb 那一句原因；有主的照旧。
#[test]
fn unowned_files_are_told_and_an_untold_dir_carries_the_reason() {
    let told = dir(json!({"workspace": "/w", "slice": "alpha", "unreadable": null, "unowned": ["stray.txt"], "unownedWhy": null,
        "entries": [entry("read.txt", "A1-1", "done")]}))
    .unwrap();
    assert_eq!(told.claim("stray.txt"), Claim::Unowned);
    assert!(matches!(told.claim("read.txt"), Claim::Owner(_)));
    assert_eq!(told.claim("other.txt"), Claim::Unknown);
    let untold = dir(json!({"workspace": "/w", "slice": "alpha", "unreadable": null, "unowned": null, "unownedWhy": "仓库不是 git 仓",
        "entries": [entry("read.txt", "A1-1", "done")]}))
    .unwrap();
    assert_eq!(untold.claim("stray.txt"), Claim::Untold("仓库不是 git 仓"));
    assert!(matches!(untold.claim("read.txt"), Claim::Owner(_)));
}

#[test]
fn an_unreadable_slice_keeps_its_said_and_no_owners() {
    let d = dir(json!({"workspace": "/w", "slice": "alpha", "unreadable": "图.md 第 3 行：元行缺 id", "entries": [], "unowned": null, "unownedWhy": null})).unwrap();
    assert_eq!(d.unreadable.as_deref(), Some("图.md 第 3 行：元行缺 id"));
    assert_eq!(d.claim("read.txt"), Claim::Unknown);
}
