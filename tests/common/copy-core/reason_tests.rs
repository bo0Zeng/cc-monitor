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
