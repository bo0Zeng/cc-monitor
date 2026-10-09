//! 找 pb：认清单里的名字与入口脚本，次序照候选。

use super::*;
use crate::plan::fixture::{fake_pb, scratch};

#[test]
fn a_plugin_named_planned_build_with_its_entry_is_found() {
    let d = scratch("locate-ok");
    let entry = fake_pb(&d.join("pb"), PLUGIN_NAME, None);
    assert_eq!(entry_in(&d.join("pb")), Some(entry));
}

#[test]
fn another_plugin_or_a_missing_entry_is_not_pb() {
    let d = scratch("locate-no");
    fake_pb(&d.join("other"), "some-other-plugin", None);
    assert_eq!(entry_in(&d.join("other")), None);
    fake_pb(&d.join("hollow"), PLUGIN_NAME, None);
    std::fs::remove_file(d.join("hollow").join(ENTRY)).unwrap();
    assert_eq!(entry_in(&d.join("hollow")), None);
    assert_eq!(entry_in(&d.join("nowhere")), None);
}

#[test]
fn the_first_candidate_that_is_pb_wins() {
    let d = scratch("locate-order");
    fake_pb(&d.join("x"), "nope", None);
    let second = fake_pb(&d.join("y"), PLUGIN_NAME, None);
    fake_pb(&d.join("z"), PLUGIN_NAME, None);
    let got = entry_among(&[d.join("x"), d.join("y"), d.join("z")]);
    assert_eq!(got, Some(second));
}
