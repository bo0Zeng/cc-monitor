//! 找 pb：名字对、入口脚本在才算；次序照交进来的。

use super::*;
use crate::plan::fixture::{fake_pb, scratch};

#[test]
fn the_first_plugin_named_planned_build_with_its_entry_wins() {
    let d = scratch("locate-order");
    fake_pb(&d.join("x"), "nope", None);
    let second = fake_pb(&d.join("y"), PLUGIN_NAME, None);
    fake_pb(&d.join("z"), PLUGIN_NAME, None);
    let got = entry_among(&[
        (d.join("x"), "nope".into()),
        (d.join("y"), PLUGIN_NAME.into()),
        (d.join("z"), PLUGIN_NAME.into()),
    ]);
    assert_eq!(got, Some(second));
}

#[test]
fn another_name_or_a_missing_entry_is_not_pb() {
    let d = scratch("locate-no");
    fake_pb(&d.join("other"), "some-other-plugin", None);
    assert_eq!(
        entry_among(&[(d.join("other"), "some-other-plugin".into())]),
        None
    );
    fake_pb(&d.join("hollow"), PLUGIN_NAME, None);
    std::fs::remove_file(d.join("hollow").join(ENTRY)).unwrap();
    assert_eq!(entry_among(&[(d.join("hollow"), PLUGIN_NAME.into())]), None);
}
