//! 文件窗口反查：`plan-files` 的回包按线上类型解成一张按名字查的表（期望手写，回包是合成的中性例子）。

use super::*;
use cmd_wire::plan::{PlanFileState, PlanFiles, PlanStatusCode};
use serde_json::json;

fn dir(v: serde_json::Value) -> Option<PlanDir> {
    PlanDir::of(serde_json::from_value::<PlanFiles>(v).unwrap())
}

fn entry(name: &str, id: &str, status: &str) -> serde_json::Value {
    json!({"name": name, "id": id, "label": id, "title": null, "statusCode": status, "status": null, "block": null,
           "signAtText": null, "fileState": "ok", "fileNote": null, "dup": []})
}

#[test]
fn a_dir_outside_every_slice_adds_nothing() {
    assert!(dir(json!({"workspace": null, "slice": null, "unreadable": null, "entries": [], "unowned": null, "unownedWhy": null})).is_none());
}

#[test]
fn entries_become_owners_by_name_with_label_status_block_sign_and_double_claims() {
    let d = dir(json!({"workspace": "/w", "slice": "alpha", "unreadable": null, "unowned": [], "unownedWhy": null, "entries": [
        {"name": "read.txt", "id": "A1-1", "label": "A1-1 甲的读入", "title": "甲的读入", "statusCode": "done", "status": null,
         "block": "甲功能", "signAtText": "02:08", "fileState": "ok", "fileNote": "12 字",
         "dup": [{"id": "A1-2", "label": "A1-2 甲的写出", "title": "甲的写出"}]},
        {"name": "write.txt", "id": "A1-2", "label": "A1-2", "title": null, "statusCode": "open", "status": null,
         "block": null, "signAtText": null, "fileState": "broken", "fileNote": "不是 UTF-8", "dup": []},
        {"name": "gone.txt", "id": "A2", "label": "A2 乙", "title": "乙", "statusCode": "dropped", "status": null,
         "block": null, "signAtText": null, "fileState": "missing", "fileNote": null, "dup": []}
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
    assert_eq!(r.sign_at_text.as_deref(), Some("02:08"));
    assert_eq!(broken(r), None);
    // 也声明它的那一格照「编号 标题」写。
    assert!(dup_text(r).starts_with("A1-2 甲的写出"), "{}", dup_text(r));
    let Claim::Owner(w) = d.claim("write.txt") else {
        panic!("write.txt 该有主")
    };
    assert_eq!(w.file_state, Some(PlanFileState::Broken));
    assert_eq!(broken(w), Some("不是 UTF-8"));
    let Claim::Owner(g) = d.claim("gone.txt") else {
        panic!("gone.txt 该有主")
    };
    assert_eq!(g.status_code, Some(PlanStatusCode::Dropped));
    assert_eq!(d.claim("nobody.txt"), Claim::Unknown);
}

/// ★ 凭据格（`statusCode: evidence`）不画成「没做完」：图标与没做完的不同。
#[test]
fn an_evidence_cell_is_not_drawn_as_open() {
    let d = dir(json!({"workspace": "/w", "slice": "alpha", "unreadable": null, "unowned": [], "unownedWhy": null,
        "entries": [entry("proof.txt", "A1-9", "evidence"), entry("todo.txt", "A1-8", "open")]}))
    .unwrap();
    let (Claim::Owner(e), Claim::Owner(o)) = (d.claim("proof.txt"), d.claim("todo.txt")) else {
        panic!("两份都该有主")
    };
    assert_eq!(e.status_code, Some(PlanStatusCode::Evidence));
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
