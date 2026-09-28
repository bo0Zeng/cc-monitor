//! 设计/99 §2.1 ⑬「monitor 只收成品帧」· `设计/30 §3.5.7a`「`Unseen` 不许被显示成已结束」—— `session_book` 只记账、不裁决。
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
            Out::Listed { origin } => format!("listed {origin}"),
            Out::Unseen { origin, sids } => format!("unseen {origin} {sids:?}"),
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
    assert_eq!(b.local_listed(), None, "本机还没报完清单 ⇒ 不交半截的");
    b.step(In::Listed {
        origin: "<local>".into(),
    });
    assert_eq!(b.local_listed().map(|v| v.len()), Some(1));
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
                Out::Left { origin, sid, fate } => format!("left {origin}/{sid} {fate:?}"),
                Out::Listed { origin } => format!("listed {origin}"),
                Out::Unseen { origin, sids } => format!("unseen {origin} {sids:?}"),
                Out::Status { sid, .. } => format!("status {sid}"),
            })
            .collect()
    };
    assert_eq!(said(&r.before), vec!["live mu/m", "live pi/a"]);
    assert_eq!(
        said(&r.after),
        vec![
            "left pi/r Reconnectable",
            "left pi/e Ended",
            // 有行、这条连接上没说过：那台报完了清单 ⇒ 已结束；没报完（`mu`）/ 根本没连上（`zz`）⇒ 说不清。
            "left pi/gone Ended",
            r#"unseen mu ["n"]"#,
            r#"unseen zz ["far"]"#,
            "listed pi",
        ]
    );
}
