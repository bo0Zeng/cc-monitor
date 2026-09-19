use super::parse_remote_hosts;
use serde_json::json;

fn remote_obj(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    v.as_object().expect("test json 必须是对象").clone()
}

/// 向后兼容：旧单对象（无 hosts 键）→ 1 台，label 默认 = host，port 默认 22。
#[test]
fn legacy_single_object_one_host() {
    let remote = remote_obj(json!({
        "host": "pi.local", "user": "pi", "daemonPath": "/x"
    }));
    let cfgs = parse_remote_hosts(&remote);
    assert_eq!(cfgs.len(), 1);
    assert_eq!(cfgs[0].host, "pi.local");
    assert_eq!(cfgs[0].label, "pi.local", "label 默认 = host");
    assert_eq!(cfgs[0].port, 22, "port 默认 22");
}

/// F56：jump 字段解析——有值 → Some;缺省/空串 → None（str_field 过滤空）。
#[test]
fn jump_field_parsed() {
    let remote = remote_obj(json!({
        "hosts": [
            {"host": "internal", "user": "u", "daemonPath": "/x", "jump": "bastion"},
            {"host": "direct", "user": "u", "daemonPath": "/y"},
            {"host": "empty", "user": "u", "daemonPath": "/z", "jump": ""}
        ]
    }));
    let cfgs = parse_remote_hosts(&remote);
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
            {"label": "pi", "host": "pi.local", "user": "pi", "daemonPath": "/x"},
            {"host": "nano.local", "user": "u", "daemonPath": "/y", "port": 2222}
        ]
    }));
    let cfgs = parse_remote_hosts(&remote);
    assert_eq!(cfgs.len(), 2);
    assert_eq!(cfgs[0].label, "pi");
    assert_eq!(cfgs[1].label, "nano.local", "缺 label → 默认 host");
    assert_eq!(cfgs[1].port, 2222);
}

/// 缺必填字段(daemonPath)的台被跳过，不影响其他台。
#[test]
fn missing_required_field_skipped() {
    let remote = remote_obj(json!({
        "hosts": [
            {"host": "ok.local", "user": "u", "daemonPath": "/x"},
            {"host": "bad.local", "user": "u"}
        ]
    }));
    let cfgs = parse_remote_hosts(&remote);
    assert_eq!(cfgs.len(), 1);
    assert_eq!(cfgs[0].host, "ok.local");
}

/// label 重复 → 第二台后缀化，保证 by_label 选台唯一。
#[test]
fn duplicate_label_suffixed() {
    let remote = remote_obj(json!({
        "hosts": [
            {"label": "box", "host": "a", "user": "u", "daemonPath": "/x"},
            {"label": "box", "host": "b", "user": "u", "daemonPath": "/y"}
        ]
    }));
    let cfgs = parse_remote_hosts(&remote);
    assert_eq!(cfgs.len(), 2);
    assert_eq!(cfgs[0].label, "box");
    assert_eq!(cfgs[1].label, "box (#2)");
}

/// 空 hosts 数组 → 空（无可用远端，等同本地）。
#[test]
fn empty_hosts_array_is_empty() {
    let remote = remote_obj(json!({ "hosts": [] }));
    assert!(parse_remote_hosts(&remote).is_empty());
}
