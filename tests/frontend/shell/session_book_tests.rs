//! 「monitor 只收成品帧」· 「`Unseen` 不许被显示成已结束」—— `session_book` 只记账、不裁决。
//!
//! 钉两件：① 交进来的成品原样交出去（顺序不变），monitor 自己只说「连接断了 ⇒ 当时活的 / 可重连的说不清」；
//! ② F5 重放计划：骨架在前、终局在后；有行却没成品的：那台没报完清单 ⇒ 说不清，报完了 ⇒ 不说（去向由那台后端答）。
use super::*;

fn meta(cwd: &str) -> cmd_wire::frame::SessionAdded {
    crate::guard_support::session_added("x", serde_json::json!({ "cwd": cwd }))
}

fn live(o: &str, s: &str) -> In {
    In::Live {
        origin: o.into(),
        sid: s.into(),
        meta: meta("/w"),
    }
}

fn left(o: &str, s: &str, fate: SessionFate) -> In {
    let state = match fate {
        SessionFate::Reconnectable => "reconnectable",
        SessionFate::Ended => "ended",
    };
    In::Left {
        origin: o.into(),
        state: crate::guard_support::session_state(s, state, "T"),
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
        left("pi", "b", SessionFate::Reconnectable),
        left("pi", "c", SessionFate::Ended),
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
            Out::Status { status, .. } => format!("status {}", status.sid),
            Out::Left { state, .. } => format!("left {} {:?}", state.sid, state.state),
            Out::Listed { origin, all } => {
                format!("listed {origin}{}", if *all { " all" } else { "" })
            }
            Out::Unseen { origin, sids } => format!("unseen {origin} {sids:?}"),
            Out::Runs { runs, .. } => format!("runs {}", runs.sid),
            Out::Branch { sid, .. } => format!("branch {sid}"),
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
        left("pi", "r", SessionFate::Reconnectable),
        left("pi", "e", SessionFate::Ended),
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
                Out::Left { origin, state } => {
                    format!("left {origin}/{} {:?}", state.sid, state.state)
                }
                Out::Listed { origin, all } => {
                    format!("listed {origin}{}", if *all { " all" } else { "" })
                }
                Out::Unseen { origin, sids } => format!("unseen {origin} {sids:?}"),
                Out::Status { status, .. } => format!("status {}", status.sid),
                Out::Runs { origin, runs } => format!("runs {origin}/{}", runs.sid),
                Out::Branch { origin, sid, .. } => format!("branch {}/{sid}", origin.as_wire_str()),
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
            // 那台报完了清单、这条没被说过（`gone`）⇒ 不说：去向由那台后端答（这里补一句就是 monitor 自己在裁）。
            "listed pi",
        ]
    );
}

/// 〔「说不清」是**那台**的〕线上一格机器级 `unseen {origin}`（不再逐会话一格）。
#[test]
fn unseen_is_one_machine_level_cell() {
    let mut b = Book::default();
    b.step(live("pi", "a"));
    b.step(left("pi", "r", SessionFate::Reconnectable));
    let lost = b.step(In::LinkLost {
        origin: "pi".into(),
    });
    let frames: Vec<String> = lost
        .iter()
        .map(|o| serde_json::to_string(&o.frame()).unwrap())
        .collect();
    assert_eq!(frames, vec![r#"{"unseen":{"origin":"pi"}}"#.to_string()]);
}

/// ★ 活会话一格 `added`，就是那台的 `session_added`：骨架那几格 · 此刻的灯与名字 · 容器 · 隐藏那一格一起带（不再拆成三格、不重拼）。期望值手写。
#[test]
fn the_live_cell_is_the_backends_session_added_frame() {
    let o = Out::Live {
        origin: "pi".into(),
        sid: "a".into(),
        meta: crate::guard_support::session_added(
            "a",
            serde_json::json!({
                "project_dir": "/a/proj",
                "background": true,
                "hidden": {"by": "bg", "said": "后端那一句"},
                "container": {"host": "tmux", "terminal": "tmux-1-2"},
                "face": {
                    "dot": "running", "live": {"is": "yes", "activity": "working"}, "settled": false,
                    "name": "跑着", "tip": "跑着", "line": "跑着", "tone": "now", "order": 1,
                    "can": {"close": {"can": "no", "why": "W"}, "resume": {"can": "no", "why": "W"},
                            "term": {"can": "yes", "tmux": true}, "reattach": {"can": "no", "why": "W"},
                            "model": {"can": "yes"}, "permission": {"can": "yes"}},
                },
            }),
        ),
    };
    let v = serde_json::to_value(o.frame()).unwrap();
    let a = &v["added"];
    assert_eq!(v.as_object().unwrap().len(), 1, "{v}");
    assert_eq!(
        (&a["sid"], &a["project_dir"], &a["background"]),
        (
            &serde_json::json!("a"),
            &serde_json::json!("/a/proj"),
            &serde_json::json!(true)
        )
    );
    assert_eq!(
        a["hidden"],
        serde_json::json!({"by": "bg", "said": "后端那一句"})
    );
    assert_eq!(
        a["container"],
        serde_json::json!({"host": "tmux", "terminal": "tmux-1-2"})
    );
    assert_eq!(
        (&a["face"]["live"]["activity"], &a["face"]["name"]),
        (&serde_json::json!("working"), &serde_json::json!("跑着"))
    );
}

/// ★ 「各台都报完」那一刻：机器表里每一台都来过 `listed` 才立（少一台 ⇒ 不立；表还不知道 ⇒ 不立）；
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
    // gpu-01 从机器表里摘掉：剩下的都报完了 ⇒ 那一刻不错过。
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

/// 主线外清单：照「前 `at` 条照旧，其后换成这些」拼成整份转出去（8a b：那台只发变了的那一截）；换了一份记录 ⇒ 从空的起拼；
/// F5 重放跟在那个活会话的宣告后面再说一次（拼好的最新那一份）；会话离开 ⇒ 不再重放。
#[test]
fn a_sessions_branch_list_passes_through_and_replays_after_its_announcement() {
    let off = |ids: &[&str]| -> Vec<String> { ids.iter().map(|i| i.to_string()).collect() };
    let branch = |s: &str, path: &str, at: u64, ids: &[&str]| In::Branch {
        origin: crate::origin::Origin("pi".into()),
        sid: s.into(),
        path: path.into(),
        at,
        off: ids.iter().map(|i| i.to_string()).collect(),
        sum: format!("s{at}+{}", ids.len()),
    };
    let mut b = Book::default();
    b.step(live("pi", "a"));
    b.step(live("pi", "b"));
    assert_eq!(
        b.step(branch("a", "/a.jsonl", 0, &["u1"])),
        vec![Out::Branch {
            origin: crate::origin::Origin("pi".into()),
            sid: "a".into(),
            off: off(&["u1"])
        }]
    );
    // 只来变了的那一截 ⇒ 交出去的是拼好的整份。
    assert_eq!(
        b.step(branch("a", "/a.jsonl", 1, &["u2", "u3"])),
        vec![Out::Branch {
            origin: crate::origin::Origin("pi".into()),
            sid: "a".into(),
            off: off(&["u1", "u2", "u3"])
        }]
    );
    // 前 2 条照旧、其后没有了 ⇒ 截短。
    b.step(branch("a", "/a.jsonl", 2, &[]));
    // 另一份记录 ⇒ 从空的起拼（不接在上一份后面）。
    let got = b.step(branch("b", "/b1.jsonl", 0, &["x"]));
    assert!(matches!(&got[..], [Out::Branch { off: o, .. }] if o == &off(&["x"])));
    let got = b.step(branch("b", "/b2.jsonl", 1, &["y"]));
    assert!(
        matches!(&got[..], [Out::Branch { off: o, .. }] if o == &off(&["y"])),
        "{got:?}"
    );
    b.step(left("pi", "b", SessionFate::Ended));
    // 同一个 sid 又活了（续接）：它离开前那份清单不算数，等那台再说。
    b.step(live("pi", "b"));
    let r = b.replay(&[]);
    let got: Vec<String> = r
        .before
        .iter()
        .map(|o| match o {
            Out::Live { sid, .. } => format!("live {sid}"),
            Out::Branch { sid, off, .. } => format!("branch {sid} {off:?}"),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(got, vec!["live a", r#"branch a ["u1", "u2"]"#, "live b"]);
}

/// read8c：报名单时交回手上那份主线外清单（条数 ＋ 那台给的最后一帧的摘要）—— 断线不摘（重接之后交回，那台只补缺的）；
/// 拼的时候中间缺过一截（`at` 比手上的长）⇒ 说不准，交 `at: 0`；没有的、会话离开了的 ⇒ `at: 0`；那台报完清单时不在了 ⇒ 摘。
#[test]
fn the_branch_list_on_hand_is_handed_back_across_a_lost_link() {
    use cmd_wire::watch::BranchHeld;
    let nothing = |sid: &str| BranchHeld {
        sid: sid.into(),
        ..Default::default()
    };
    let pi = crate::origin::Origin("pi".into());
    let branch = |s: &str, at: u64, ids: &[&str], sum: &str| In::Branch {
        origin: pi.clone(),
        sid: s.into(),
        path: format!("/{s}.jsonl"),
        at,
        off: ids.iter().map(|i| i.to_string()).collect(),
        sum: sum.into(),
    };
    let sids = |v: &[&str]| {
        v.iter()
            .map(|s| s.to_string())
            .collect::<std::collections::BTreeSet<_>>()
    };
    let mut b = Book::default();
    b.step(live("pi", "a"));
    b.step(live("pi", "b"));
    b.step(branch("a", 0, &["u1", "u2"], "s2"));
    b.step(branch("a", 2, &["u3"], "s3"));
    b.step(branch("b", 0, &["x"], "sx"));
    b.step(branch("b", 5, &["y"], "sy")); // 前 5 条照旧，手上只有 1 条 ⇒ 中间缺过
    b.step(In::LinkLost {
        origin: "pi".into(),
    });
    assert_eq!(
        b.branch_held(&pi, &sids(&["a", "b", "c"])),
        vec![
            BranchHeld {
                sid: "a".into(),
                path: Some("/a.jsonl".into()),
                at: 3,
                sum: Some("s3".into())
            },
            nothing("b"),
            nothing("c"),
        ],
        "断线之后交回的手上那份不对"
    );
    // 重接：那台报完清单时 a 不在了 ⇒ 摘。
    b.step(live("pi", "b"));
    b.step(In::Listed {
        origin: "pi".into(),
    });
    assert_eq!(b.branch_held(&pi, &sids(&["a"])), vec![nothing("a")]);
}

/// 名单并进 attach 行之后，手上那份清单在写 attach 行时就读：旧连接队列里还压着一帧 `session_branch` 没落账时读到的是更早一刻的
/// （`at` ＋ `sum` 出自同一帧，仍是一份自洽的前缀）。那台照它补缺的那一截（从更早的 `at` 起）；旧那帧之后再落账、新补来的那一帧再覆盖 ⇒
/// 落完账手上的与那台一致，只是多补了一点。
#[test]
fn a_branch_list_read_before_the_old_links_last_frame_still_ends_where_the_peer_is() {
    use cmd_wire::watch::BranchHeld;
    let pi = crate::origin::Origin("pi".into());
    let branch = |at: u64, ids: &[&str], sum: &str| In::Branch {
        origin: pi.clone(),
        sid: "a".into(),
        path: "/a.jsonl".into(),
        at,
        off: ids.iter().map(|i| i.to_string()).collect(),
        sum: sum.into(),
    };
    let only_a: std::collections::BTreeSet<String> = ["a".to_string()].into();
    let mut b = Book::default();
    b.step(live("pi", "a"));
    b.step(branch(0, &["u1", "u2"], "s2"));
    // 写 attach 行那一刻读的：旧连接那一帧（前 2 条照旧，其后 u3）还压在队列里。
    let read_early = b.branch_held(&pi, &only_a);
    assert_eq!(
        read_early,
        vec![BranchHeld {
            sid: "a".into(),
            path: Some("/a.jsonl".into()),
            at: 2,
            sum: Some("s2".into()),
        }]
    );
    b.step(branch(2, &["u3"], "s3"));
    // 新连接上那台照读早的那份补：此刻它的清单是 u1..u4，从第 2 条起补。
    let out = b.step(branch(2, &["u3", "u4"], "s4"));
    let Some(Out::Branch { off, .. }) = out.last() else {
        panic!("{out:?}");
    };
    assert_eq!(off, &["u1", "u2", "u3", "u4"].map(String::from).to_vec());
    assert_eq!(
        b.branch_held(&pi, &only_a),
        vec![BranchHeld {
            sid: "a".into(),
            path: Some("/a.jsonl".into()),
            at: 4,
            sum: Some("s4".into()),
        }]
    );
}
