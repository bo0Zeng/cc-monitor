//! 「monitor 只收成品帧」· 「`Unseen` 不许被显示成已结束」—— `session_book` 只记账、不裁决。
//!
//! 钉两件：① 交进来的成品原样交出去（顺序不变），monitor 自己只说「连接断了 ⇒ 当时活的 / 可重连的说不清」；
//! ② F5 重放计划：骨架在前、终局在后，有行却没成品的按「那台报完清单没有」分已结束 / 说不清。
use super::*;

fn meta(cwd: &str) -> LiveMeta {
    LiveMeta {
        cwd: Some(cwd.into()),
        ..Default::default()
    }
}

fn live(o: &str, s: &str) -> In {
    In::Live {
        origin: o.into(),
        sid: s.into(),
        meta: meta("/w"),
    }
}

fn left(o: &str, s: &str, fate: Fate) -> In {
    In::Left {
        origin: o.into(),
        sid: s.into(),
        fate,
        words: None,
    }
}

#[test]
fn products_pass_through_in_order_and_only_a_lost_link_is_the_monitors_own_word() {
    let mut b = Book::default();
    let mut out = Vec::new();
    for ev in [
        live("pi", "a"),
        live("pi", "b"),
        live("pi", "c"),
        left("pi", "b", Fate::Reconnectable),
        left("pi", "c", Fate::Ended),
        In::Listed {
            origin: "pi".into(),
        },
        live("<local>", "x"),
        In::LinkLost {
            origin: "pi".into(),
        },
        In::LinkLost {
            origin: "pi".into(),
        },
    ] {
        out.extend(b.step(ev));
    }
    let kinds: Vec<String> = out
        .iter()
        .map(|o| match o {
            Out::Live { sid, .. } => format!("live {sid}"),
            Out::Status { sid, .. } => format!("status {sid}"),
            Out::Left { sid, fate, .. } => format!("left {sid} {fate:?}"),
            Out::Listed { origin, all } => {
                format!("listed {origin}{}", if *all { " all" } else { "" })
            }
            Out::Unseen { origin, sids } => format!("unseen {origin} {sids:?}"),
            Out::Runs { sid, .. } => format!("runs {sid}"),
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            "live a",
            "live b",
            "live c",
            "left b Reconnectable",
            "left c Ended",
            "listed pi",
            "live x",
            // 断连：活的 ∪ 可重连的说不清；已结束的不动；那台的账整份作废（再断一次无话可说）；别的机器不受牵连。
            r#"unseen pi ["a", "b"]"#,
        ]
    );
}

#[test]
fn the_f5_plan_puts_skeletons_first_and_judges_bufferless_nothing() {
    let mut b = Book::default();
    for ev in [
        live("pi", "a"),
        left("pi", "r", Fate::Reconnectable),
        left("pi", "e", Fate::Ended),
        In::Listed {
            origin: "pi".into(),
        },
        live("mu", "m"),
    ] {
        b.step(ev);
    }
    let buffered: Vec<(String, String)> = [
        ("a", "pi"),
        ("e", "pi"),
        ("gone", "pi"),
        ("m", "mu"),
        ("n", "mu"),
        ("far", "zz"),
    ]
    .iter()
    .map(|(s, o)| (s.to_string(), o.to_string()))
    .collect();
    let r = b.replay(&buffered);
    let said = |v: &[Out]| -> Vec<String> {
        v.iter()
            .map(|o| match o {
                Out::Live { origin, sid, .. } => format!("live {origin}/{sid}"),
                Out::Left {
                    origin, sid, fate, ..
                } => format!("left {origin}/{sid} {fate:?}"),
                Out::Listed { origin, all } => {
                    format!("listed {origin}{}", if *all { " all" } else { "" })
                }
                Out::Unseen { origin, sids } => format!("unseen {origin} {sids:?}"),
                Out::Status { sid, .. } => format!("status {sid}"),
                Out::Runs { origin, sid, .. } => format!("runs {origin}/{sid}"),
            })
            .collect()
    };
    assert_eq!(said(&r.before), vec!["live mu/m", "live pi/a"]);
    assert_eq!(
        said(&r.after),
        vec![
            // 有行、这条连接上没说过、那台没报完（`mu`）/ 根本没连上（`zz`）⇒ **机器级**说不清，排在终局最前；
            //   紧跟着那台说过的活会话再宣告一次（前端按机器落说不清会把它一并落下，这一格翻回来）。
            r#"unseen mu ["n"]"#,
            "live mu/m",
            r#"unseen zz ["far"]"#,
            "left pi/r Reconnectable",
            "left pi/e Ended",
            // 那台报完了清单 ⇒ 不在清单里 = 已结束。
            "left pi/gone Ended",
            "listed pi",
        ]
    );
}

/// 〔「说不清」是**那台**的〕线上一格机器级 `unseen {origin}`（不再逐会话一格）；旁路快照被撤时
/// 没说过的那一条：那台报完了清单 ⇒ 已结束，没报完 ⇒ 机器级说不清 ＋ 那台说过的活 / 可重连的再说一次（前端按机器落，紧跟着翻回来）。
#[test]
fn unseen_is_one_machine_level_cell_and_settle_again_follows_the_list() {
    let mut b = Book::default();
    b.step(live("pi", "a"));
    b.step(left("pi", "r", Fate::Reconnectable));
    let lost = b.step(In::LinkLost {
        origin: "pi".into(),
    });
    let frames: Vec<String> = lost
        .iter()
        .flat_map(|o| o.frames())
        .map(|f| serde_json::to_string(&f).unwrap())
        .collect();
    assert_eq!(frames, vec![r#"{"unseen":{"origin":"pi"}}"#.to_string()]);

    b.step(live("pi", "a"));
    let said = |v: Vec<Out>| -> Vec<String> {
        v.iter()
            .map(|o| match o {
                Out::Live { sid, .. } => format!("live {sid}"),
                Out::Left { sid, fate, .. } => format!("left {sid} {fate:?}"),
                Out::Unseen { origin, .. } => format!("unseen {origin}"),
                other => format!("{other:?}"),
            })
            .collect()
    };
    assert_eq!(said(b.settle_again("pi", "q")), vec!["unseen pi", "live a"]);
    b.step(In::Listed {
        origin: "pi".into(),
    });
    assert_eq!(said(b.settle_again("pi", "q")), vec!["left q Ended"]);
    assert!(b.settle_again("pi", "a").is_empty(), "活着的不说");
}

/// 活会话那一格：项目目录原样带给前端（缺席的那几格按 `null` 出）。期望值手写。
#[test]
fn the_live_cell_carries_the_project_dir() {
    let with = Out::Live {
        origin: "pi".into(),
        sid: "a".into(),
        meta: LiveMeta {
            project_dir: Some("/a/proj".into()),
            ..Default::default()
        },
    };
    let first = |o: &Out| serde_json::to_string(&o.frames()[0]).unwrap();
    assert_eq!(
        first(&with),
        r#"{"live":{"session_id":"a","origin":"pi","background":false,"attachable":null,"cwd":null,"project_dir":"/a/proj","name":null}}"#
    );
}

/// ★ 「各台都报完」那一拍：机器表里每一台都来过 `listed` 才立（少一台 ⇒ 不立；表还不知道 ⇒ 不立）；
/// 机器表换了按新表重算（摘掉的正是唯一没报完的那台 ⇒ 当场再说一次「报完了」带 `all`）；F5 重放带同一份。
#[test]
fn all_listed_holds_only_when_every_machine_in_the_table_has_listed() {
    let listed = |o: &str| In::Listed { origin: o.into() };
    let all_of = |v: Vec<Out>| -> Vec<(String, bool)> {
        v.into_iter()
            .filter_map(|o| match o {
                Out::Listed { origin, all } => Some((origin, all)),
                _ => None,
            })
            .collect()
    };
    let mut b = Book::default();
    assert_eq!(
        all_of(b.step(listed("<local>"))),
        vec![("<local>".into(), false)],
        "表还不知道 ⇒ 不立"
    );
    assert!(
        b.set_machines(vec!["<local>".into(), "gpu-01".into(), "pi".into()])
            .is_empty(),
        "还有两台没来"
    );
    assert_eq!(
        all_of(b.step(listed("pi"))),
        vec![("pi".into(), false)],
        "少一台（gpu-01）⇒ 不立"
    );
    // gpu-01 从机器表里摘掉：剩下的都报完了 ⇒ 那一拍不错过。
    assert_eq!(
        all_of(b.set_machines(vec!["<local>".into(), "pi".into()])),
        vec![("<local>".into(), true)]
    );
    assert_eq!(
        all_of(b.replay(&[]).after),
        vec![("<local>".into(), true), ("pi".into(), true)],
        "F5 重放同一份"
    );
    // 又加回一台、它还没来 ⇒ 不立；它来了 ⇒ 立。
    assert!(b
        .set_machines(vec!["<local>".into(), "gpu-01".into(), "pi".into()])
        .is_empty());
    assert_eq!(
        all_of(b.step(listed("gpu-01"))),
        vec![("gpu-01".into(), true)]
    );
    // 那台断了（账作废）⇒ 再报一次之前不立。
    b.step(In::LinkLost {
        origin: "pi".into(),
    });
    assert_eq!(
        all_of(b.replay(&[]).after)
            .iter()
            .filter(|(_, a)| *a)
            .count(),
        0
    );
}
