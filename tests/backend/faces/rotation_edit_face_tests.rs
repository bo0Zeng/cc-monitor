//! 轮换的写（第五版）：改设置照声明校验、当场存成新版（只重写头，代码逐字节不变）· 冲突 · 载入一份 · 新会话用哪份 ·
//! 策略文件加 / 复制 / 改名 / 删 · 设时区。临时家目录。

use super::*;
use crate::faces::rotation_view_face::tests::{Home, S1, S2};
use serde_json::{json, Value};

fn code_of(src: &str) -> String {
    src.split_once("\n---*/")
        .map(|x| x.1.to_string())
        .unwrap_or_default()
}

fn file(home: &Home, id: &str) -> String {
    std::fs::read_to_string(home.root.join(format!("strategies/{id}.rhai"))).expect("那一份")
}

fn rev(home: &Home, id: &str) -> String {
    shelf::rev_of(&file(home, id))
}

fn mine() -> Value {
    json!({"caps": {"5h": [{"from": 0, "to": 1440, "v": 80}]}, "capKind": "soft",
           "accounts": [{"start": true}, {"id": "b", "delay": 30}]})
}

/// ★ 改设置交意图 ⇒ 照声明改完、当场存成那份的新版（只重写头、代码逐字节不变；没变 ⇒ 不写、版本照旧）；
/// 不合声明 ⇒ `refused` 带那一格、文件不动；行号越界 ⇒ `bad_args`；版本对不上 ⇒ `conflict` 带此刻的版本。
#[test]
fn settings_edits_are_saved_into_the_file_as_a_new_version() {
    let home = Home::new("edit-save");
    let ctx = home.ctx();
    let before = file(&home, "by-list");
    let edit = |edit: RotEdit| {
        settings_with(
            &ctx,
            &home.own(),
            &RotationSettingsReq {
                strategy: "by-list".into(),
                edit,
                if_rev: rev(&home, "by-list"),
            },
        )
    };
    let settings = || script::parse(&file(&home, "by-list")).expect("读得懂").head.settings;
    let got = edit(RotEdit::Add {
        account: "b".into(),
    })
    .expect("ok");
    assert_eq!(got.rev, rev(&home, "by-list"), "回的是写成以后的版本");
    edit(RotEdit::Set {
        key: "delay".into(),
        row: Some(1),
        v: json!(30),
    })
    .expect("ok");
    edit(RotEdit::Day {
        key: "caps".into(),
        row: None,
        window: "5h".into(),
        from: 0,
        to: 0,
        v: Some(80.0),
    })
    .expect("ok");
    assert_eq!(settings(), mine(), "一步步改出来的就是那一份");
    assert_eq!(code_of(&file(&home, "by-list")), code_of(&before), "代码动了");
    let saved = file(&home, "by-list");
    match edit(RotEdit::Set {
        key: "delay".into(),
        row: Some(1),
        v: json!(-1),
    }) {
        Err(RotationSettingsErr::Refused(_, r)) => assert_eq!(r.cells[0].cell, "accounts[1].delay"),
        other => panic!("该拒：{other:?}"),
    }
    assert!(matches!(
        edit(RotEdit::Remove { row: 9 }),
        Err(RotationSettingsErr::BadArgs(_))
    ));
    assert_eq!(file(&home, "by-list"), saved, "没收下的不动文件");
    assert_eq!(edit(RotEdit::Keep).expect("ok").rev, shelf::rev_of(&saved));
    assert_eq!(file(&home, "by-list"), saved, "没变不写");
    match settings_with(
        &ctx,
        &home.own(),
        &RotationSettingsReq {
            strategy: "by-list".into(),
            edit: RotEdit::Move { from: 1, to: 0 },
            if_rev: "h:000000000000".into(),
        },
    ) {
        Err(RotationSettingsErr::Conflict(_, c)) => assert_eq!(c.rev, shelf::rev_of(&saved)),
        other => panic!("该冲突：{other:?}"),
    }
}

/// 下拉选一份 ＝ 载入它此刻的版本（memo · 叫醒清掉）· 不再轮换（`null`）· 读不懂的载入不了 · 新会话用哪份（那一份要在；`null` ⇒ 新会话不轮换）。
#[test]
fn use_and_for_new() {
    let home = Home::new("use");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    crate::accounts::quota::rotation::face_change(&ctx.hop.store, &home.own(), |b| {
        b.sessions.get_mut(S1).expect("s1").memo = Some(json!({"n": 1}));
    })
    .expect("w");
    let to = |id: Option<&str>, sid: &str| {
        use_with(
            &ctx,
            &home.own(),
            &RotationUseReq {
                sid: sid.into(),
                strategy: id.map(str::to_string),
            },
        )
    };
    to(Some("third"), S1).expect("换");
    let s = ctx.hop.store.now().sessions[S1].clone();
    let c = s.uses.expect("载入了");
    assert_eq!((c.id.as_str(), c.rev.clone()), ("third", rev(&home, "third")));
    assert!(s.memo.is_none(), "上一份记的 memo 该清");
    to(None, S1).expect("不轮换");
    assert!(ctx.hop.store.now().sessions[S1].uses.is_none());
    shelf::write_file(&home.own(), "broken", "fn decide(").expect("写");
    assert!(matches!(to(Some("broken"), S1), Err(RotationUseErr::NoStrategy(_))));
    assert!(matches!(to(Some("third"), S2), Err(RotationUseErr::NoSession(_))));
    for_new_with(
        &ctx,
        &home.own(),
        &RotationForNewReq {
            strategy: Some("by-reset".into()),
        },
    )
    .expect("设");
    assert_eq!(ctx.setup.uses.now().for_new, "by-reset");
    for_new_with(&ctx, &home.own(), &RotationForNewReq { strategy: None }).expect("设回不轮换");
    assert_eq!(ctx.setup.uses.now().for_new, "");
    assert!(matches!(
        for_new_with(
            &ctx,
            &home.own(),
            &RotationForNewReq {
                strategy: Some("nope".into())
            }
        ),
        Err(RotationForNewErr::NoStrategy(_))
    ));
}

/// ★ 策略文件：加（读不懂 ⇒ `bad_strategy` 带行；文件名就是给的名字）· 复制（另起一个文件名，内容一字不改）·
/// 改名（换文件名，指它的会话与「新会话用」跟着改；版本对不上 ⇒ `conflict`；名字占了或写法不对 ⇒ `bad_args`）·
/// 删（活会话在用 · 新会话用的 ⇒ `in_use`）。
#[test]
fn strategy_files_are_added_copied_renamed_and_deleted() {
    let home = Home::new("files");
    let ctx = home.ctx();
    let src = file(&home, "by-reset");
    match add_with(
        &ctx,
        &home.own(),
        &RotationStrategyAddReq {
            source: "fn decide(ctx) {}".into(),
            name: "坏的".into(),
        },
    ) {
        Err(RotationStrategyAddErr::BadStrategy(_, b)) => assert_eq!(b.line, Some(1)),
        other => panic!("该拒：{other:?}"),
    }
    let added = add_with(
        &ctx,
        &home.own(),
        &RotationStrategyAddReq {
            source: src.clone(),
            name: " 我的 ".into(),
        },
    )
    .expect("加");
    assert_eq!(added.id, "我的");
    assert_eq!(file(&home, &added.id), src);
    for bad in ["我的", "a/b", "..", "", "CON"] {
        assert!(
            matches!(
                add_with(
                    &ctx,
                    &home.own(),
                    &RotationStrategyAddReq {
                        source: src.clone(),
                        name: bad.into()
                    }
                ),
                Err(RotationStrategyAddErr::BadArgs(_))
            ),
            "{bad:?}"
        );
    }
    let copied = copy_with(
        &ctx,
        &home.own(),
        &RotationStrategyCopyReq {
            id: added.id.clone(),
            name: "我的 2".into(),
        },
    )
    .expect("复制");
    assert_eq!(
        (copied.id.as_str(), file(&home, &copied.id)),
        ("我的 2", src.clone())
    );
    assert!(matches!(
        rename_with(
            &ctx,
            &home.own(),
            &RotationStrategyRenameReq {
                id: copied.id.clone(),
                name: "改了".into(),
                if_rev: "h:x".into()
            }
        ),
        Err(RotationStrategyRenameErr::Conflict(..))
    ));
    home.saw(&ctx, S1);
    home.load(&ctx, S1, Some(&copied.id));
    ctx.setup
        .change(&home.own(), |b| b.for_new = copied.id.clone())
        .expect("w");
    assert!(
        matches!(
            rename_with(
                &ctx,
                &home.own(),
                &RotationStrategyRenameReq {
                    id: copied.id.clone(),
                    name: "我的".into(),
                    if_rev: rev(&home, &copied.id),
                }
            ),
            Err(RotationStrategyRenameErr::BadArgs(_))
        ),
        "名字占了"
    );
    let renamed = rename_with(
        &ctx,
        &home.own(),
        &RotationStrategyRenameReq {
            id: copied.id.clone(),
            name: "改了".into(),
            if_rev: rev(&home, &copied.id),
        },
    )
    .expect("改名");
    assert_eq!(renamed.id, "改了");
    assert_eq!(file(&home, "改了"), src, "内容一字不改");
    assert!(
        !home
            .root
            .join(format!("strategies/{}.rhai", copied.id))
            .exists(),
        "旧文件名没了"
    );
    assert_eq!(
        (
            crate::faces::rotation_view_face::tests::used(&ctx, S1).as_deref(),
            ctx.setup.uses.now().for_new.as_str()
        ),
        (Some("改了"), "改了"),
        "载入它的副本与「新会话用」跟着改"
    );
    ctx.setup
        .change(&home.own(), |b| b.for_new = "by-list".into())
        .expect("w");
    let copied = renamed;
    let del = |id: &str| {
        delete_with(
            &ctx,
            &home.own(),
            &RotationStrategyDeleteReq { id: id.into() },
            &home.alive(),
        )
    };
    match del(&copied.id) {
        Err(RotationStrategyDeleteErr::InUse(_, u)) => assert_eq!(u.sids, [S1]),
        other => panic!("该拒：{other:?}"),
    }
    assert!(
        matches!(del("by-list"), Err(RotationStrategyDeleteErr::InUse(..))),
        "新会话用的那份"
    );
    del(&added.id).expect("删");
    assert!(!home
        .root
        .join(format!("strategies/{}.rhai", added.id))
        .exists());
}

/// ★ 设时区：只把设置块里的 `tz` 换掉（说明 · 代码逐字节不变、设置照旧）；之后不再提示「时区不明」；
/// 版本对不上 ⇒ `conflict`；不是 IANA 名 ⇒ `bad_args`。
#[test]
fn setting_the_zone_rewrites_only_tz() {
    let home = Home::new("edit-tz");
    let ctx = home.ctx();
    let before = file(&home, "by-list");
    let head_before = script::parse(&before).expect("读得懂").head;
    let req = |tz: &str, if_rev: String| RotationStrategyTzReq {
        id: "by-list".into(),
        tz: tz.into(),
        if_rev,
    };
    assert!(matches!(
        tz_with(
            &ctx,
            &home.own(),
            &req("Asia/Shanghai", "h:000000000000".into())
        ),
        Err(RotationStrategyTzErr::Conflict(..))
    ));
    assert!(matches!(
        tz_with(
            &ctx,
            &home.own(),
            &req("Mars/Olympus", rev(&home, "by-list"))
        ),
        Err(RotationStrategyTzErr::BadArgs(_))
    ));
    assert_eq!(file(&home, "by-list"), before, "拒了还动了文件");
    let got = tz_with(
        &ctx,
        &home.own(),
        &req("Asia/Shanghai", rev(&home, "by-list")),
    )
    .expect("设上");
    let after = file(&home, "by-list");
    assert_eq!(got.rev, shelf::rev_of(&after));
    assert_eq!(code_of(&after), code_of(&before), "代码动了");
    assert_eq!(
        after.split_once("/*--- rotation").map(|x| x.0),
        before.split_once("/*--- rotation").map(|x| x.0),
        "说明动了"
    );
    let head = script::parse(&after).expect("读得懂").head;
    assert_eq!(head.tz.as_deref(), Some("Asia/Shanghai"));
    assert!(!head.tz_unknown);
    assert_eq!(head.settings, head_before.settings);
    assert_eq!(head.form, head_before.form);
    let item =
        crate::faces::rotation_view_face::strategies_with(&ctx, &Default::default(), &home.alive())
            .strategies
            .into_iter()
            .find(|s| s.id == "by-list")
            .expect("那一份");
    assert!(!item.tz_unknown, "设了还说时区不明");
}
