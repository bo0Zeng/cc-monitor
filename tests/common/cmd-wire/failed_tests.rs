use super::*;
use std::collections::BTreeSet;

fn sample() -> Failed {
    Failed {
        code: "busy".into(),
        said: "那台在忙".into(),
        detail: "码 busy".into(),
        data: Some(serde_json::json!({"n": 1})),
    }
}

/// 交出去那一形在 TS 那边就叫 `Failed`（界面 · 手机生成物都照这个名）。
#[test]
fn exported_as_failed() {
    let cfg = ts_rs::Config::default();
    assert_eq!(<Failed as ts_rs::TS>::name(&cfg), "Failed");
}

/// 线上恰是四格：码 · 那一句 · 复制详情 · 按码定形的 `data`；那一句的格叫 `said`。
#[test]
fn four_cells_said_not_message() {
    let v = serde_json::to_value(sample()).expect("序列化");
    let keys: BTreeSet<&str> = v
        .as_object()
        .expect("对象")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, BTreeSet::from(["code", "said", "detail", "data"]));
}

/// 没有按码定形的 `data` ⇒ 那一格不出；读回来是同一份。
#[test]
fn data_absent_is_not_written_and_round_trips() {
    let f = Failed {
        data: None,
        ..sample()
    };
    let v = serde_json::to_value(&f).expect("序列化");
    assert!(v.get("data").is_none(), "{v}");
    let back: Failed = serde_json::from_value(v).expect("读回");
    assert_eq!(back, f);
}

/// 这一侧自己说的那一句：兜底码 · 没有详情。
#[test]
fn local_has_fallback_code_and_no_detail() {
    let f = Failed::local("没走通".into());
    assert_eq!(
        (f.code.as_str(), f.said.as_str(), f.detail.as_str(), f.data),
        (copy_core::said::FALLBACK_CODE, "没走通", "", None)
    );
}
