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

/// 这台设了 pb 路径 ⇒ 只认它（入口不在也不退回已装的那份，回「设了的那个不行」）；没设 ⇒ 照已装的插件。
#[test]
fn a_set_pb_dir_is_the_only_place_looked_at() {
    let d = scratch("locate-set");
    let installed = fake_pb(&d.join("installed"), PLUGIN_NAME, None);
    let mine = fake_pb(&d.join("mine"), "anything", None);
    let plugins = [(d.join("installed"), PLUGIN_NAME.to_string())];
    assert_eq!(pick(Some(&d.join("mine")), &plugins), Ok(mine));
    assert_eq!(
        pick(Some(&d.join("empty")), &plugins),
        Err(Missing::SetDir(d.join("empty")))
    );
    assert_eq!(pick(None, &plugins), Ok(installed));
    assert_eq!(pick(None, &[]), Err(Missing::NotInstalled));
}

/// 校验一个路径：不在 · 底下没有入口 · 起不来 / 退非零 · 能用；每种都带一句给人看的。
#[test]
fn checking_a_dir_names_what_is_wrong_with_it() {
    let d = scratch("locate-check");
    let got = check(&d.join("nowhere"));
    assert_eq!(got["state"], "missing");
    std::fs::create_dir_all(d.join("plain")).unwrap();
    assert_eq!(check(&d.join("plain"))["state"], "no_entry");
    fake_pb(&d.join("broken"), PLUGIN_NAME, Some(1));
    assert_eq!(check(&d.join("broken"))["state"], "not_runnable");
    fake_pb(&d.join("good"), PLUGIN_NAME, Some(0));
    assert_eq!(check(&d.join("good"))["state"], "ok");
    for dir in ["nowhere", "plain", "broken", "good"] {
        let said = check(&d.join(dir))["said"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert!(!said.is_empty(), "{dir}");
    }
    let _ = std::fs::remove_dir_all(&d);
}
