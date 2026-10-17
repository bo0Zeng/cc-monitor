//! `Hand::apply` 照金样逐条：每一行是「手上那份 ＋ 后端那一答 ⇒ 后端此刻那一窗」，金样由后端 `common/views.rs` 的判据真答出来写。
//! 手机那一份 `apply`（`SyncHand`）读同一份。

use super::*;
use serde_json::Value;

const GOLDEN: &str = "tests/__fixtures__/sync-patch.golden.jsonl";

#[derive(Deserialize)]
struct Case {
    case: String,
    hand: Hand<Value, Value>,
    patch: Patch<Value, Value>,
    want: Hand<Value, Value>,
}

fn cases() -> Vec<Case> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(GOLDEN);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读 {}：{e}", path.display()));
    text.lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("金样这一行解不开（{e}）：{l}")))
        .collect()
}

#[test]
fn apply_follows_every_golden_line() {
    let cases = cases();
    // 正控：三种答（整份 · 没变 · 只给变了的）金样里都有，本条不在空转。
    let kinds: std::collections::BTreeSet<&str> = cases
        .iter()
        .map(|c| match (c.patch.whole, c.patch.same) {
            (true, _) => "whole",
            (_, true) => "same",
            _ => "part",
        })
        .collect();
    assert_eq!(kinds, ["part", "same", "whole"].into_iter().collect());
    for c in cases {
        let mut hand = c.hand;
        hand.apply(c.patch);
        assert_eq!(hand, c.want, "{}", c.case);
    }
}
