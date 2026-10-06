//! # 要求住址：`INVARIANTS §28`（`origin` 边界：label 是那台机器唯一的 key）
//!
//! 核原文：`INVARIANTS §28` 逐字「`RemoteConfig.label`（`stream_source/config.rs`，空则回退 `host`）是唯一
//! 「持久（config.json）+ 上 wire（每条远端行带 `origin`）+ 名字派生」的 key」—— 本族判
//! `lib.rs::parse_remote_entries` 缺 label 回退 host、重复 label 加后缀保住唯一。缺必填跳过 / `jump` 空串 / 空数组三条没有逐字住址。
//! 🔴 `legacy_single_object_one_host` 钉的是旧单对象配置的兼容支，与「不为任何盘上旧状态留兼容层」相抵 —— 待定（断言不动）。〔散文墓碑〕
//! 已删：那一支与那条测试删了，旧形状今天是「认不出」（`a_remote_section_without_a_hosts_array_is_refused_not_emptied`；不为旧形状留兼容）。

use super::parse_remote_entries;

/// 只看机器（不看「连接这台」）。
fn parse_remote_hosts(
    remote: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<crate::stream_source::RemoteConfig>, &'static str> {
    parse_remote_entries(remote).map(|v| v.into_iter().map(|(c, _)| c).collect())
}
use serde_json::json;

fn remote_obj(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    v.as_object().expect("test json 必须是对象").clone()
}

/// 要求：「不为旧配置留兼容」· D4（不许静默当成空）。
///
/// 旧单对象形态（没有 `hosts` 数组）从前被当成 1 台（测试 `legacy_single_object_one_host` 钉着那一支）〔散文墓碑〕，
/// 那一支删了 ⇒ 它现在是 **`Err`（认不出）**：既不悄悄当成一台，也不装作「没配远端」。
/// `hosts` 写成了别的类型同一格。对照：空的 `hosts` 数组是合法的「零台」，`Ok(空)`。
#[test]
fn a_remote_section_without_a_hosts_array_is_refused_not_emptied() {
    for shape in [
        json!({ "enabled": true, "host": "pi.local", "user": "pi" }),
        json!({ "enabled": true, "hosts": { "host": "pi.local", "user": "pi" } }),
        json!({ "enabled": true }),
    ] {
        let got = parse_remote_hosts(&remote_obj(shape.clone()));
        assert_eq!(
            got.as_ref().map(Vec::len).map_err(|e| *e),
            Err(super::REMOTE_HOSTS_UNRECOGNIZED),
            "{shape} 该是「认不出」，不是一个列表"
        );
    }
    assert!(super::REMOTE_HOSTS_UNRECOGNIZED.starts_with("认不出"));
    // 对照：空数组是「零台」，合法。
    assert_eq!(
        parse_remote_hosts(&remote_obj(json!({ "hosts": [] }))).map(|v| v.len()),
        Ok(0)
    );
}

/// F56：jump 字段解析——有值 → Some;缺省/空串 → None（str_field 过滤空）。
#[test]
fn jump_field_parsed() {
    let remote = remote_obj(json!({
        "hosts": [
            {"host": "internal", "user": "u", "jump": "bastion"},
            {"host": "direct", "user": "u"},
            {"host": "empty", "user": "u", "jump": ""}
        ]
    }));
    let cfgs = parse_remote_hosts(&remote).expect("hosts 数组该认得");
    assert_eq!(cfgs.len(), 3);
    assert_eq!(cfgs[0].jump.as_deref(), Some("bastion"));
    assert_eq!(cfgs[1].jump, None, "缺省 → None");
    assert_eq!(cfgs[2].jump, None, "空串 → None");
}

/// hosts 数组多台；缺 label 的台 label 回退 host；port 透传。
#[test]
fn hosts_array_multi() {
    let remote = remote_obj(json!({
        "hosts": [
            {"label": "pi", "host": "pi.local", "user": "pi"},
            {"host": "nano.local", "user": "u", "port": 2222}
        ]
    }));
    let cfgs = parse_remote_hosts(&remote).expect("hosts 数组该认得");
    assert_eq!(cfgs.len(), 2);
    assert_eq!(cfgs[0].label, "pi");
    assert_eq!(cfgs[1].label, "nano.local", "缺 label → 默认 host");
    assert_eq!(cfgs[1].port, 2222);
}

/// 缺必填字段(user)的台被跳过，不影响其他台。`backendPath` 不再是字段（盘上旧值不读）。
#[test]
fn missing_required_field_skipped() {
    let remote = remote_obj(json!({
        "hosts": [
            {"host": "ok.local", "user": "u"},
            {"host": "bad.local"}
        ]
    }));
    let cfgs = parse_remote_hosts(&remote).expect("hosts 数组该认得");
    assert_eq!(cfgs.len(), 1);
    assert_eq!(cfgs[0].host, "ok.local");
}

/// label 重复 → 第二台后缀化，保证 by_label 选台唯一。
#[test]
fn duplicate_label_suffixed() {
    let remote = remote_obj(json!({
        "hosts": [
            {"label": "box", "host": "a", "user": "u"},
            {"label": "box", "host": "b", "user": "u"}
        ]
    }));
    let cfgs = parse_remote_hosts(&remote).expect("hosts 数组该认得");
    assert_eq!(cfgs.len(), 2);
    assert_eq!(cfgs[0].label, "box");
    assert_eq!(cfgs[1].label, "box (#2)");
}

/// 空 hosts 数组 → 空（无可用远端，等同本地）。
#[test]
fn empty_hosts_array_is_empty() {
    let remote = remote_obj(json!({ "hosts": [] }));
    assert!(parse_remote_hosts(&remote)
        .expect("hosts 数组该认得")
        .is_empty());
}

/// 每台的「连接这台」：`"connect": false` 的那台照样认得（卸载 · 测试连接要它的参数），只是标成不连；缺省 = 连。
#[test]
fn connect_false_is_kept_but_marked_not_to_connect() {
    let remote = remote_obj(json!({ "hosts": [
        { "host": "a", "user": "u" },
        { "host": "b", "user": "u", "connect": false },
        { "host": "c", "user": "u", "connect": true },
    ] }));
    let got: Vec<(String, bool)> = parse_remote_entries(&remote)
        .expect("hosts 数组该认得")
        .into_iter()
        .map(|(c, k)| (c.origin_label(), k))
        .collect();
    assert_eq!(
        got,
        [
            ("a".to_string(), true),
            ("b".to_string(), false),
            ("c".to_string(), true)
        ]
    );
}
