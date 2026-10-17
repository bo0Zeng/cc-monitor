//! 轮换的写（第五版）：设置整份交回、照声明校验 · 存只重写头（代码逐字节不变）· 另存 · 冲突 · 换一份 · 新会话用哪份 ·
//! 拖出来的顺序 · 现在用（只拿事实拦）· 策略文件加 / 复制 / 改名 / 删 · 24 小时条的算术。临时家目录。

use super::*;
use crate::faces::rotation_view_face::tests::{Home, S1, S2};
use crate::wire::typed::Handle;
use serde_json::json;

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

/// ★ 会话里改设置交意图 ⇒ 改完存成草稿（跟文件相同 ⇒ 草稿丢掉）；不合声明 ⇒ `refused` 带那一格、草稿不动；行号越界 ⇒ `bad_args`。
/// 设置页 ⇒ 照交来的那份草稿改、只回改完的那份。
#[test]
fn session_settings_become_a_draft_and_are_checked() {
    let home = Home::new("edit-draft");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    let target = RotWhere::Session { sid: S1.into() };
    let edit = |edit: RotEdit| {
        settings_with(
            &ctx,
            &home.own(),
            &RotationSettingsReq {
                target: target.clone(),
                settings: None,
                edit,
                if_rev: None,
            },
        )
    };
    let draft = || ctx.setup.uses.now().sessions[S1].draft.clone();
    let got = edit(RotEdit::Add {
        account: "b".into(),
    })
    .expect("ok");
    assert!(got.dirty);
    assert_eq!(draft(), Some(got.settings.clone()));
    edit(RotEdit::Set {
        key: "delay".into(),
        row: Some(1),
        v: json!(30),
    })
    .expect("ok");
    let got = edit(RotEdit::Day {
        key: "caps".into(),
        row: None,
        window: "5h".into(),
        from: 0,
        to: 0,
        v: Some(80.0),
    })
    .expect("ok");
    assert_eq!(got.settings, mine(), "一步步改出来的就是那一份");
    assert_eq!(draft(), Some(mine()));
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
    assert_eq!(draft(), Some(mine()), "没收下的不动草稿");
    edit(RotEdit::Day {
        key: "caps".into(),
        row: None,
        window: "5h".into(),
        from: 0,
        to: 0,
        v: None,
    })
    .expect("ok");
    edit(RotEdit::Remove { row: 1 }).expect("ok");
    assert!(!edit(RotEdit::Keep).expect("ok").dirty);
    assert_eq!(draft(), None, "跟文件相同没丢草稿");

    let page = settings_with(
        &ctx,
        &home.own(),
        &RotationSettingsReq {
            target: RotWhere::Strategy {
                strategy: "by-list".into(),
            },
            settings: Some(mine()),
            edit: RotEdit::Move { from: 1, to: 0 },
            if_rev: None,
        },
    )
    .expect("ok");
    assert_eq!(
        page.settings["accounts"],
        json!([{"id": "b", "delay": 30}, {"start": true}])
    );
    assert!(page.dirty);
}

/// ★ 存：只重写头、代码逐字节不变，会话的草稿丢掉；版本对不上 ⇒ `conflict` 带此刻的版本；另存 ⇒ 新的一份、会话改用它。
#[test]
fn saving_rewrites_only_the_head_and_detects_conflicts() {
    let home = Home::new("save");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    let before = file(&home, "by-list");
    settings_with(
        &ctx,
        &home.own(),
        &RotationSettingsReq {
            target: RotWhere::Session { sid: S1.into() },
            settings: None,
            edit: RotEdit::Add {
                account: "b".into(),
            },
            if_rev: None,
        },
    )
    .expect("草稿");
    let stale = match save_with(
        &ctx,
        &home.own(),
        &RotationSaveReq {
            target: RotWhere::Session { sid: S1.into() },
            settings: None,
            if_rev: "h:000000000000".into(),
            save_as: None,
        },
    ) {
        Err(RotationSaveErr::Conflict(_, c)) => c.rev,
        other => panic!("该冲突：{other:?}"),
    };
    assert_eq!(stale, rev(&home, "by-list"));
    let got = save_with(
        &ctx,
        &home.own(),
        &RotationSaveReq {
            target: RotWhere::Session { sid: S1.into() },
            settings: None,
            if_rev: stale,
            save_as: None,
        },
    )
    .expect("存");
    let after = file(&home, "by-list");
    assert_eq!(code_of(&after), code_of(&before), "代码动了");
    assert_eq!(got.rev, shelf::rev_of(&after));
    assert_eq!(
        script::parse(&after).expect("读得懂").head.settings,
        json!({"caps": {}, "capKind": "soft", "accounts": [{"start": true}, {"id": "b"}]})
    );
    assert_eq!(ctx.setup.uses.now().sessions[S1].draft, None);

    let as_new = save_with(
        &ctx,
        &home.own(),
        &RotationSaveReq {
            target: RotWhere::Session { sid: S1.into() },
            settings: Some(json!({"caps": {}, "capKind": "hard", "accounts": [{"start": true}]})),
            if_rev: String::new(),
            save_as: Some(RotSaveAs {
                name: "夜里省着用".into(),
            }),
        },
    )
    .expect("另存");
    assert_eq!(as_new.id, "夜里省着用", "标题就是文件名");
    assert_eq!(ctx.setup.uses.now().sessions[S1].id, as_new.id);
    assert_eq!(code_of(&file(&home, &as_new.id)), code_of(&before));
    assert!(
        matches!(
            save_with(
                &ctx,
                &home.own(),
                &RotationSaveReq {
                    target: RotWhere::Strategy {
                        strategy: "by-list".into()
                    },
                    settings: None,
                    if_rev: String::new(),
                    save_as: Some(RotSaveAs {
                        name: "夜里省着用".into()
                    }),
                },
            ),
            Err(RotationSaveErr::BadArgs(_))
        ),
        "同名的已有"
    );
}

/// 还原 · 下拉换一份（有草稿要 `discard`）· 不再轮换（`null`）· 新会话用哪份（那一份要在；`null` ⇒ 新会话不轮换）。
#[test]
fn revert_use_and_for_new() {
    let home = Home::new("use");
    let ctx = home.ctx();
    home.saw(&ctx, S1);
    ctx.setup
        .change(&home.own(), |b| {
            b.sessions.get_mut(S1).expect("s1").draft = Some(mine());
        })
        .expect("w");
    let to = |discard: bool| RotationUseReq {
        sid: S1.into(),
        strategy: Some("third".into()),
        discard,
    };
    assert!(matches!(
        use_with(&ctx, &home.own(), &to(false)),
        Err(RotationUseErr::Dirty(_))
    ));
    use_with(&ctx, &home.own(), &to(true)).expect("换");
    let s = ctx.setup.uses.now().sessions[S1].clone();
    assert_eq!((s.id.as_str(), s.draft.is_none()), ("third", true));
    use_with(
        &ctx,
        &home.own(),
        &RotationUseReq {
            sid: S1.into(),
            strategy: None,
            discard: false,
        },
    )
    .expect("不轮换");
    assert_eq!(ctx.setup.uses.now().sessions[S1].id, "");
    revert_with(&ctx, &home.own(), &RotationRevertReq { sid: S1.into() }).expect("还原");
    assert!(matches!(
        revert_with(&ctx, &home.own(), &RotationRevertReq { sid: S2.into() }),
        Err(RotationRevertErr::NoSession(_))
    ));
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
    ctx.setup
        .change(&home.own(), |b| {
            b.sessions.get_mut(S1).expect("s1").id = copied.id.clone();
            b.for_new = copied.id.clone();
        })
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
    let b = ctx.setup.uses.now();
    assert_eq!(
        (b.sessions[S1].id.as_str(), b.for_new.as_str()),
        ("改了", "改了"),
        "指它的跟着改"
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
