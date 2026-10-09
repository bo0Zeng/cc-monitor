//! 线上形状：夹具出的成品（标过认可 · 写过时刻）过一遍类型，不丢一格也不添一格 ⇒ 生成给界面的类型说的就是线上那一份。

use super::*;
use crate::plan::dump::Ran;
use crate::plan::fixture::{dump, dump_broken, who};
use std::path::Path;

fn ran(doc: &Value) -> Ran {
    Ran::Dump {
        doc: doc.clone(),
        raw: doc.to_string().into_bytes(),
    }
}

/// 帧面出口那一份：本子出的 ＋ 认可与退回标过 ＋ 时刻写成字。
fn out(v: Value) -> Value {
    let mut v = v;
    crate::plan::review::current(None).annotate(&mut v);
    crate::plan::product::with_time_texts(&mut v, 2_000, 0);
    v
}

fn lossless<T: Serialize + serde::de::DeserializeOwned>(v: &Value) {
    assert_eq!(
        &checked::<T>(v.clone()).unwrap(),
        v,
        "过一遍类型丢了或添了格"
    );
}

#[test]
fn a_full_read_a_stale_slice_and_a_bare_slice_all_pass_through_the_types_unchanged() {
    let b = crate::plan::book::Book::default();
    let dir = Path::new("/w");
    let first = out(b.take(ran(&dump("/w")), dir, &who, 1_000).unwrap());
    assert!(first["slices"][0]["cells"].as_array().unwrap().len() == 4);
    lossless::<PlanRead>(&first);
    let stale = out(b.take(ran(&dump_broken("/w")), dir, &who, 2_000).unwrap());
    assert!(!stale["slices"][0]["stale"].is_null());
    lossless::<PlanRead>(&stale);
    let bare = out(crate::plan::book::Book::default()
        .take(ran(&dump_broken("/w")), dir, &who, 1_000)
        .unwrap());
    assert_eq!(bare["slices"][0]["bare"], true);
    lossless::<PlanRead>(&bare);
    // `plan-list` 的一行（帧面拼的就是这个函数）。
    lossless::<PlanListWorkspace>(&crate::plan::product::list_row(&first));
}

#[test]
fn a_shape_the_types_do_not_know_is_refused_not_passed_on() {
    assert!(checked::<PlanCmdReply>(serde_json::json!({"said": null, "path": null})).is_err());
    assert!(checked::<PlanWhyCode>(serde_json::json!({"kind": "later"})).is_err());
    assert_eq!(
        checked::<PlanCmdReply>(serde_json::json!({"rc": 0, "said": "auto 开", "path": null}))
            .unwrap(),
        serde_json::json!({"rc": 0, "said": "auto 开", "path": null})
    );
}
