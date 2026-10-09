//! 下层 IO 错 → 原因词：常见几种说人话（都在原因词闭集里），其余「原因不明」；原话不在这里（交给复制详情）。

use super::*;
use std::io::ErrorKind as K;

#[test]
fn each_common_io_kind_becomes_a_word_in_the_closed_set_and_the_rest_is_unknown() {
    let rules: serde_json::Value =
        serde_json::from_str(include_str!("../../../src/shared/copy/rules.json")).unwrap();
    let words: Vec<String> = rules["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "C-W8")
        .unwrap()["words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap().to_string())
        .collect();
    let cases = [
        (K::PermissionDenied, "reason.io.denied"),
        (K::NotFound, "reason.io.notFound"),
        (K::StorageFull, "reason.io.full"),
        (K::QuotaExceeded, "reason.io.full"),
        (K::ResourceBusy, "reason.io.busy"),
        (K::AlreadyExists, "reason.io.exists"),
        (K::Other, "reason.io.unknown"),
        (K::Interrupted, "reason.io.unknown"),
    ];
    for (kind, key) in cases {
        let got = io_reason(kind);
        assert_eq!(got, crate::copy_text(key, &[]), "{kind:?}");
        assert!(words.contains(&got), "{kind:?} ⇒「{got}」不在原因词闭集里");
    }
}

#[test]
fn each_standard_sftp_status_becomes_a_word_in_the_closed_set_and_the_rest_is_unknown() {
    let rules: serde_json::Value =
        serde_json::from_str(include_str!("../../../src/shared/copy/rules.json")).unwrap();
    let words: Vec<String> = rules["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "C-W8")
        .unwrap()["words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap().to_string())
        .collect();
    // SFTP v3 标准码：2 无此文件 · 3 无权限 · 4 通用失败 · 5 坏报文 · 6 无连接 · 7 连接断 · 8 不支持。
    let cases = [
        (2, "reason.io.notFound"),
        (3, "reason.io.denied"),
        (4, "reason.io.unknown"),
        (5, "reason.sftp.badMessage"),
        (6, "reason.sftp.connectionLost"),
        (7, "reason.sftp.connectionLost"),
        (8, "reason.sftp.unsupported"),
        (99, "reason.io.unknown"),
    ];
    for (code, key) in cases {
        let got = sftp_status_reason(code);
        assert_eq!(got, crate::copy_text(key, &[]), "码 {code}");
        assert!(words.contains(&got), "码 {code} ⇒「{got}」不在原因词闭集里");
    }
}
