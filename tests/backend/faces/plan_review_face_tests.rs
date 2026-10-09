//! 计划认可：只收此刻在的、不是问人那一种的键。

use super::*;
use crate::plan::fixture::{dump, SUB};

fn doc_with(who: &dyn Fn(&str) -> crate::plan::Whose) -> Value {
    let b = crate::plan::book::Book::default();
    let d = dump("/w");
    let raw = d.to_string().into_bytes();
    b.take(
        crate::plan::dump::Ran::Dump { doc: d, raw },
        std::path::Path::new("/w"),
        who,
        1,
    )
    .unwrap()
}

#[test]
fn only_a_present_key_that_is_not_an_ask_can_be_acked() {
    let doc = doc_with(&crate::plan::fixture::who);
    let sl = &doc["slices"][0];
    let red = "red:悬空@B";
    assert!(check_ack(sl, red).is_ok());
    assert_eq!(
        check_ack(sl, "red:别的@B").unwrap_err().code,
        "no_such_need"
    );
    let ask = format!("ask:B@{SUB}");
    assert_eq!(check_ack(sl, &ask).unwrap_err().code, "not_ackable");
}
