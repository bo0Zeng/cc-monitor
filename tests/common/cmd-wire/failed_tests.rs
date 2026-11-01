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

fn keys(v: &serde_json::Value) -> BTreeSet<String> {
    v.as_object().expect("对象").keys().cloned().collect()
}

/// 手机装后端没做成的那一行：标签 `end` ＋ 恰是 `Failed` 那几格（那一句叫 `said`）；读回同一份。
#[test]
fn deploy_end_failed_carries_failed() {
    let end = crate::phone::DeployEnd::Failed(sample());
    let v = serde_json::to_value(&end).expect("序列化");
    assert_eq!(
        keys(&v),
        ["end", "code", "said", "detail", "data"].map(String::from).into()
    );
    assert_eq!(v["end"], "failed");
    let back: crate::phone::DeployEnd = serde_json::from_value(v).expect("读回");
    assert_eq!(back, end);
}

/// 传输收场的失败那一形：标签 `state` ＋ 恰是 `Failed` 那几格（不出 `why`）；读回同一份。
#[test]
fn transfer_end_failed_carries_failed() {
    let end = crate::frame::TransferEnd::Failed(Failed {
        data: None,
        ..sample()
    });
    let v = serde_json::to_value(&end).expect("序列化");
    assert_eq!(
        keys(&v),
        ["state", "code", "said", "detail"].map(String::from).into()
    );
    let back: crate::frame::TransferEnd = serde_json::from_value(v).expect("读回");
    assert_eq!(back, end);
}

/// 拨号回执没开成的那一句也叫 `said`（不出 `error`）。
#[test]
fn dial_ack_says_said() {
    let ack = crate::dial::DialAck::of_fail(crate::dial::DialFail {
        said: "拨不通".into(),
        detail: None,
        fingerprint: None,
        reason: None,
        key_type: None,
        expected_fingerprint: None,
    });
    let v = serde_json::to_value(&ack).expect("序列化");
    assert_eq!(v["said"], "拨不通", "{v}");
    assert!(v.get("error").is_none(), "{v}");
}
