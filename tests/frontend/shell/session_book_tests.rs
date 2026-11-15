//! 「monitor 只收成品帧」· 「说不清不许被显示成已结束」—— `session_book` 只记账、不裁决。
//!
//! 钉两件：① 交进来的成品原样交出去（顺序不变），连接断了 monitor 一句都不自己说（说不清是本机后端逐条发的 `session_left`）；
//! ② F5 重放计划：骨架在前、终局在后；有行却没成品的不说（去向由那台后端答）。
use super::*;

fn meta(cwd: &str) -> cmd_wire::frame::SessionAdded {
    crate::frame_fixtures::session_added("x", serde_json::json!({ "cwd": cwd }))
}

fn told(o: &str, frame: crate::stream_source::Frame) -> In {
    In::Told {
        origin: o.into(),
        frame,
    }
}

fn live(o: &str, s: &str) -> In {
    let mut m = meta("/w");
    m.sid = s.into();
    told(o, Frame::SessionAdded(m))
}

/// 那台说的去向：`reconnectable` · `ended`（线上 `face.life.is` 那一形）。
fn left(o: &str, s: &str, life: &str) -> In {
    told(
        o,
        Frame::SessionLeft(crate::frame_fixtures::session_left(s, life, "T")),
    )
}

fn listed(o: &str) -> In {
    told(
        o,
        Frame::SessionsReplayed(cmd_wire::frame::SessionsReplayed {}),
    )
}

/// 本机后端那一侧的替身：每台（连本机）的一帧会话帧原样交进来，紧跟那台那一本折好的那一格（与生产同一个 `SessionBook::step`：
/// 改整条的那几种只带那一条、不带 `whole`；本机那一台 `observe/watcher.rs`，别台 `control/feeds.rs`）。
#[derive(Default)]
struct Peer {
    books: std::collections::HashMap<String, face_core::book::SessionBook>,
}

impl Peer {
    fn step(&mut self, b: &mut Book, ev: In) -> Vec<Out> {
        let seats = match &ev {
            In::Told { origin, frame } => {
                let book = self.books.entry(origin.clone()).or_default();
                book.step(frame).map(|s| (origin.clone(), s))
            }
            In::Closed { origin } => {
                if let Some(book) = self.books.get_mut(origin) {
                    book.listed = false;
                }
                None
            }
            _ => None,
        };
        let out = b.step(ev);
        if let Some((origin, seats)) = seats {
            assert!(
                b.step(In::Seats { origin, seats }).is_empty(),
                "成品不交界面"
            );
        }
        out
    }
}

/// ★ 先红的那一条：别台的会话帧原样交界面、壳不折 —— 没跟着 `seats` 的不进缓存（F5 不重放、不算活）；
/// 跟着了照 `seats` 那一条重放（整条以它为准，不是壳从会话帧里拼的）。
#[test]
fn a_machines_seats_are_the_only_thing_replayed_for_it() {
    let mut b = Book::default();
    let out = b.step(live("pi", "a"));
    assert!(
        matches!(&out[..], [Out::Live { sid, .. }] if sid == "a"),
        "会话帧原样交界面：{out:?}"
    );
    let pi = crate::origin::Origin::named("pi");
    assert!(!b.is_live(&pi, "a"), "壳自己折了别台的会话帧");
    assert!(b.replay(&[]).before.is_empty(), "没有成品也重放了");
    let mut m = meta("/from-seats");
    m.sid = "a".into();
    b.step(In::Seats {
        origin: "pi".into(),
        seats: cmd_wire::frame::SessionSeats {
            seats: vec![cmd_wire::frame::SessionSeat::of(
                "a",
                Some(&m),
                None,
                None,
                None,
            )],
            listed: true,
            whole: false,
        },
    });
    assert!(b.is_live(&pi, "a"));
    // 一条不带 `seat` ⇒ 那一本里不再有它（别的照留）。
    let mut m2 = meta("/w");
    m2.sid = "b".into();
    let one = |seat: cmd_wire::frame::SessionSeat| In::Seats {
        origin: "pi".into(),
        seats: cmd_wire::frame::SessionSeats {
            seats: vec![seat],
            listed: true,
            whole: false,
        },
    };
    b.step(one(cmd_wire::frame::SessionSeat::of(
        "b",
        Some(&m2),
        None,
        None,
        None,
    )));
    assert!(b.is_live(&pi, "b"));
    b.step(one(cmd_wire::frame::SessionSeat::of(
        "b", None, None, None, None,
    )));
    assert!(!b.is_live(&pi, "b"), "不带 seat 的那一条还在");
    assert!(b.is_live(&pi, "a"), "不带 seat 的那一条摘错了别的");
    let r = b.replay(&[]);
    assert!(
        matches!(&r.before[..], [Out::Live { meta, .. }] if meta.cwd == m.cwd),
        "{:?}",
        r.before
    );
    assert!(matches!(&r.after[..], [Out::Listed { origin }] if origin == "pi"));
    // 整本换：不在里面的就是不在了。
    b.step(In::Seats {
        origin: "pi".into(),
        seats: cmd_wire::frame::SessionSeats {
            seats: Vec::new(),
            listed: false,
            whole: true,
        },
    });
    assert!(!b.is_live(&pi, "a"));
}

/// ★ 先红的那一条：本机那一台也不折 —— 本机的会话帧原样交界面，不带成品的不进缓存（F5 不重放、不算活）；
/// 本机后端紧跟的 `session_seats`（[`In::Seats`]，`<local>`）照它换那一格。
#[test]
fn the_local_machine_is_cached_only_from_its_seats_too() {
    let mut b = Book::default();
    let local = crate::origin::Origin::local();
    let out = b.step(live("<local>", "x"));
    assert!(
        matches!(&out[..], [Out::Live { sid, .. }] if sid == "x"),
        "会话帧原样交界面：{out:?}"
    );
    assert!(!b.is_live(&local, "x"), "壳自己折了本机的会话帧");
    assert!(b.replay(&[]).before.is_empty(), "没有成品也重放了");
    let mut m = meta("/from-seats");
    m.sid = "x".into();
    b.step(In::Seats {
        origin: "<local>".into(),
        seats: cmd_wire::frame::SessionSeats {
            seats: vec![cmd_wire::frame::SessionSeat::of(
                "x",
                Some(&m),
                None,
                None,
                None,
            )],
            listed: true,
            whole: false,
        },
    });
    assert!(b.is_live(&local, "x"));
    let r = b.replay(&[]);
    assert!(
        matches!(&r.before[..], [Out::Live { meta, .. }] if meta.cwd == m.cwd),
        "{:?}",
        r.before
    );
}

#[test]
fn products_pass_through_in_order_and_a_closed_link_adds_nothing() {
    let mut b = Book::default();
    let mut p = Peer::default();
    let mut out = Vec::new();
    for ev in [
        live("pi", "a"),
        live("pi", "b"),
        live("pi", "c"),
        left("pi", "b", "reconnectable"),
        left("pi", "c", "ended"),
        listed("pi"),
        live("<local>", "x"),
        In::Closed {
            origin: "pi".into(),
        },
        In::Closed {
            origin: "nowhere".into(),
        },
    ] {
        out.extend(p.step(&mut b, ev));
    }
    let kinds: Vec<String> = out
        .iter()
        .map(|o| match o {
            Out::Live { sid, .. } => format!("live {sid}"),
            Out::Status { status, .. } => format!("status {}", status.sid),
            Out::Left { left, .. } => format!("left {} {}", left.sid, went(left)),
            Out::Listed { origin } => format!("listed {origin}"),
            Out::Runs { runs, .. } => format!("runs {}", runs.sid),
            Out::Branch { sid, .. } => format!("branch {sid}"),
            Out::Unread { unread, .. } => format!("unread {}", unread.sid),
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
            // 连接收了：一格都不交（说不清由本机后端逐条发，这一侧不合成）。
            "live x",
        ]
    );
}

#[test]
fn the_f5_plan_puts_skeletons_first_and_judges_bufferless_nothing() {
    let mut b = Book::default();
    let mut p = Peer::default();
    for ev in [
        live("pi", "a"),
        left("pi", "r", "reconnectable"),
        left("pi", "e", "ended"),
        left("pi", "u", "unclear"),
        listed("pi"),
        live("mu", "m"),
    ] {
        p.step(&mut b, ev);
    }
    let buffered: Vec<(String, String)> = [
        ("a", "pi"),
        ("e", "pi"),
        ("u", "pi"),
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
                Out::Left { origin, left } => {
                    format!("left {origin}/{} {}", left.sid, went(left))
                }
                Out::Listed { origin } => format!("listed {origin}"),
                Out::Status { status, .. } => format!("status {}", status.sid),
                Out::Runs { origin, runs } => format!("runs {origin}/{}", runs.sid),
                Out::Branch { origin, sid, .. } => format!("branch {origin}/{sid}"),
                Out::Unread { origin, unread } => format!("unread {origin}/{}", unread.sid),
            })
            .collect()
    };
    assert_eq!(said(&r.before), vec!["live mu/m", "live pi/a"]);
    assert_eq!(
        said(&r.after),
        vec![
            "left pi/r Reconnectable",
            "left pi/e Ended",
            // 本机后端在断线那一刻说过的说不清照原样再说一次。
            "left pi/u Unclear",
            // 有行、没被说过（`gone` · 那台没报完的 `mu/n` · 根本没连上的 `zz/far`）⇒ 不说：去向由那台后端答（这里补一句就是 monitor 自己在合成）。
            "listed pi",
        ]
    );
}

/// ★ 说不清是本机后端发的那一帧 `session_left`（`face.life = unclear`）：原样转成会话流一格 `left`；之后连接收了，这一侧一格都不补。
#[test]
fn the_backends_unclear_left_passes_through_and_a_closed_link_adds_nothing() {
    let mut b = Book::default();
    let mut p = Peer::default();
    p.step(&mut b, live("pi", "a"));
    let said = p.step(&mut b, left("pi", "a", "unclear"));
    let frames: Vec<serde_json::Value> = said
        .iter()
        .map(|o| serde_json::to_value(o.frame()).unwrap())
        .collect();
    assert_eq!(frames.len(), 1, "{frames:?}");
    assert_eq!(
        frames[0]["left"]["face"]["life"],
        serde_json::json!({"is": "unclear"})
    );
    assert!(b
        .step(In::Closed {
            origin: "pi".into(),
        })
        .is_empty());
    assert_eq!(
        b.of("pi").map(|p| p
            .seats
            .values()
            .filter(|s| matches!(s, Seat::Left { .. }))
            .count()),
        Some(1),
        "账照留：F5 重放照它再说"
    );
}

/// ★ 活会话一格 `added`，就是那台的 `session_added`：骨架那几格 · 此刻的灯与名字 · 容器 · 隐藏那一格一起带（不再拆成三格、不重拼）。期望值手写。
#[test]
fn the_live_cell_is_the_backends_session_added_frame() {
    let o = Out::Live {
        origin: "pi".into(),
        sid: "a".into(),
        meta: crate::frame_fixtures::session_added(
            "a",
            serde_json::json!({
                "project_dir": "/a/proj",
                "background": true,
                "hidden": {"by": "bg", "said": "后端那一句"},
                "container": {"host": "tmux", "terminal": "tmux-1-2"},
                "face": {
                    "dot": "running", "life": {"is": "live", "activity": "working"}, "settled": false,
                    "name": "跑着", "tip": "跑着", "line": "跑着", "tone": "now", "order": 1,
                    "can": {"close": {"can": "no", "why": "W"}, "resume": {"can": "no", "why": "W"},
                            "front": {"can": "yes"}, "attach": {"can": "yes"}, "view": {"can": "yes", "said": "S", "open": {"kind": "attach", "terminal": "tmux-1-2"}}, "reattach": {"can": "no", "why": "W"},
                            "model": {"can": "yes"}, "permission": {"can": "yes"}, "stop": {"can": "yes"}, "say": {"can": "yes", "hint": "H"}, "end": {"can": "yes"}},
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
        (&a["face"]["life"]["activity"], &a["face"]["name"]),
        (&serde_json::json!("working"), &serde_json::json!("跑着"))
    );
}

/// 主线外清单：照「前 `at` 条照旧，其后换成这些」拼成整份转出去（8a b：那台只发变了的那一截）；换了一份记录 ⇒ 从空的起拼；
/// F5 重放跟在那个活会话的宣告后面再说一次（拼好的最新那一份）；会话离开 ⇒ 不再重放。
#[test]
fn a_sessions_branch_list_passes_through_and_replays_after_its_announcement() {
    let off = |ids: &[&str]| -> Vec<String> { ids.iter().map(|i| i.to_string()).collect() };
    let branch = |s: &str, path: &str, at: u64, ids: &[&str]| In::Branch {
        origin: crate::origin::Origin::named("pi"),
        sid: s.into(),
        path: path.into(),
        at,
        off: ids.iter().map(|i| i.to_string()).collect(),
        runs: Vec::new(),
        sum: format!("s{at}+{}", ids.len()),
    };
    let mut b = Book::default();
    let mut p = Peer::default();
    p.step(&mut b, live("pi", "a"));
    p.step(&mut b, live("pi", "b"));
    assert_eq!(
        p.step(&mut b, branch("a", "/a.jsonl", 0, &["u1"])),
        vec![Out::Branch {
            origin: crate::origin::Origin::named("pi"),
            sid: "a".into(),
            off: off(&["u1"]),
            runs: Vec::new(),
        }]
    );
    // 只来变了的那一截 ⇒ 交出去的是拼好的整份。
    assert_eq!(
        p.step(&mut b, branch("a", "/a.jsonl", 1, &["u2", "u3"])),
        vec![Out::Branch {
            origin: crate::origin::Origin::named("pi"),
            sid: "a".into(),
            off: off(&["u1", "u2", "u3"]),
            runs: Vec::new(),
        }]
    );
    // 前 2 条照旧、其后没有了 ⇒ 截短。
    p.step(&mut b, branch("a", "/a.jsonl", 2, &[]));
    // 另一份记录 ⇒ 从空的起拼（不接在上一份后面）。
    let got = p.step(&mut b, branch("b", "/b1.jsonl", 0, &["x"]));
    assert!(matches!(&got[..], [Out::Branch { off: o, .. }] if o == &off(&["x"])));
    let got = p.step(&mut b, branch("b", "/b2.jsonl", 1, &["y"]));
    assert!(
        matches!(&got[..], [Out::Branch { off: o, .. }] if o == &off(&["y"])),
        "{got:?}"
    );
    p.step(&mut b, left("pi", "b", "ended"));
    // 同一个 sid 又活了（续接）：它离开前那份清单不算数，等那台再说。
    p.step(&mut b, live("pi", "b"));
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

/// 段跟着清单拼：起在 `at` 之前的段照旧，其后换成这一帧带的（那台保证 `at` 是段首，账不并段、不数数）；F5 重放带同一份段。
#[test]
fn branch_runs_are_spliced_at_the_frames_head_and_replayed() {
    let run = |at: u64, n: u64| cmd_wire::frame::BranchRun {
        at,
        n,
        said: cmd_wire::cells::Words(format!("x{n}")),
    };
    let branch = |at: u64, ids: &[&str], runs: Vec<cmd_wire::frame::BranchRun>| In::Branch {
        origin: crate::origin::Origin::named("pi"),
        sid: "a".into(),
        path: "/a.jsonl".into(),
        at,
        off: ids.iter().map(|i| i.to_string()).collect(),
        runs,
        sum: format!("s{at}"),
    };
    let runs_of = |out: &[Out]| match out {
        [Out::Branch { runs, .. }] => runs.clone(),
        other => panic!("{other:?}"),
    };
    let mut b = Book::default();
    let mut p = Peer::default();
    p.step(&mut b, live("pi", "a"));
    p.step(
        &mut b,
        branch(0, &["u1", "u2", "u3"], vec![run(0, 2), run(2, 1)]),
    );
    // 第 2 条起那一段长了一条 ⇒ 那台从它的段首起整段重发。
    let out = p.step(&mut b, branch(2, &["u3", "u4"], vec![run(2, 2)]));
    assert_eq!(runs_of(&out), [run(0, 2), run(2, 2)]);
    let r = b.replay(&[]);
    let replayed: Vec<_> = r
        .before
        .iter()
        .filter_map(|o| match o {
            Out::Branch { runs, .. } => Some(runs.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(replayed, [vec![run(0, 2), run(2, 2)]]);
}

/// read8c：报名单时交回手上那份主线外清单（条数 ＋ 那台给的最后一帧的摘要）—— 断线不摘（重接之后交回，那台只补缺的）；
/// 拼的时候中间缺过一截（`at` 比手上的长）⇒ 说不准，交 `at: 0`；没有的、会话离开了的 ⇒ `at: 0`；那台报完清单时不在了 ⇒ 摘。
#[test]
fn the_branch_list_on_hand_is_handed_back_across_a_lost_link_said_unclear() {
    use cmd_wire::watch::BranchHeld;
    let nothing = |sid: &str| BranchHeld {
        sid: sid.into(),
        ..Default::default()
    };
    let pi = crate::origin::Origin::named("pi");
    let branch = |s: &str, at: u64, ids: &[&str], sum: &str| In::Branch {
        origin: pi.clone(),
        sid: s.into(),
        path: format!("/{s}.jsonl"),
        at,
        off: ids.iter().map(|i| i.to_string()).collect(),
        runs: Vec::new(),
        sum: sum.into(),
    };
    let sids = |v: &[&str]| {
        v.iter()
            .map(|s| s.to_string())
            .collect::<std::collections::BTreeSet<_>>()
    };
    let mut b = Book::default();
    let mut p = Peer::default();
    p.step(&mut b, live("pi", "a"));
    p.step(&mut b, live("pi", "b"));
    p.step(&mut b, branch("a", 0, &["u1", "u2"], "s2"));
    p.step(&mut b, branch("a", 2, &["u3"], "s3"));
    p.step(&mut b, branch("b", 0, &["x"], "sx"));
    p.step(&mut b, branch("b", 5, &["y"], "sy")); // 前 5 条照旧，手上只有 1 条 ⇒ 中间缺过
                                                  // 断的那一刻本机后端先说两条说不清，再收这一条连接。
    p.step(&mut b, left("pi", "a", "unclear"));
    p.step(&mut b, left("pi", "b", "unclear"));
    p.step(
        &mut b,
        In::Closed {
            origin: "pi".into(),
        },
    );
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
    p.step(&mut b, live("pi", "b"));
    p.step(&mut b, listed("pi"));
    assert_eq!(b.branch_held(&pi, &sids(&["a"])), vec![nothing("a")]);
}

/// 名单并进 attach 行之后，手上那份清单在写 attach 行时就读：旧连接队列里还压着一帧 `session_branch` 没落账时读到的是更早一刻的
/// （`at` ＋ `sum` 出自同一帧，仍是一份自洽的前缀）。那台照它补缺的那一截（从更早的 `at` 起）；旧那帧之后再落账、新补来的那一帧再覆盖 ⇒
/// 落完账手上的与那台一致，只是多补了一点。
#[test]
fn a_branch_list_read_before_the_old_links_last_frame_still_ends_where_the_peer_is() {
    use cmd_wire::watch::BranchHeld;
    let pi = crate::origin::Origin::named("pi");
    let branch = |at: u64, ids: &[&str], sum: &str| In::Branch {
        origin: pi.clone(),
        sid: "a".into(),
        path: "/a.jsonl".into(),
        at,
        off: ids.iter().map(|i| i.to_string()).collect(),
        runs: Vec::new(),
        sum: sum.into(),
    };
    let only_a: std::collections::BTreeSet<String> = ["a".to_string()].into();
    let mut b = Book::default();
    let mut p = Peer::default();
    p.step(&mut b, live("pi", "a"));
    p.step(&mut b, branch(0, &["u1", "u2"], "s2"));
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
    p.step(&mut b, branch(2, &["u3"], "s3"));
    // 新连接上那台照读早的那份补：此刻它的清单是 u1..u4，从第 2 条起补。
    let out = p.step(&mut b, branch(2, &["u3", "u4"], "s4"));
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

/// 离开那一帧说的去向（同改形之前的写法：`Reconnectable` · `Ended`）。
fn went(l: &cmd_wire::frame::SessionLeft) -> &'static str {
    match l.face.life {
        cmd_wire::frame::Life::Reconnectable => "Reconnectable",
        cmd_wire::frame::Life::Unclear => "Unclear",
        _ => "Ended",
    }
}

/// 交出去的那几格里未读那几格（会话 · 写好的字）。
fn unread_outs(outs: &[Out]) -> Vec<(String, Option<String>)> {
    outs.iter()
        .filter_map(|o| match o {
            Out::Unread { unread, .. } => Some((
                unread.sid.clone(),
                unread.said.as_ref().map(|w| w.0.clone()),
            )),
            _ => None,
        })
        .collect()
}

fn un(sid: &str, said: Option<&str>) -> (String, Option<String>) {
    (sid.to_string(), said.map(str::to_string))
}

/// ★ 未读：整条里那一格变了才交界面一格（照整条换，这一侧不数）；整本换掉、手上有而整本里没有的交「没有了」；
/// 那条连接收了 ⇒ 那台的未读整份作废、各交一格「没有了」（说不清不留数）；F5 重放末尾再说一次手上的。
#[test]
fn unread_moves_with_the_seats_and_goes_with_the_connection() {
    use cmd_wire::cells::Words;
    use cmd_wire::frame::{SessionSeat, SessionSeats};
    let mut b = Book::default();
    let seat = |sid: &str, said: Option<&str>| {
        SessionSeat::of(
            sid,
            None,
            None,
            None,
            said.map(|s| Words(s.into())).as_ref(),
        )
    };
    let seats = |v: Vec<SessionSeat>, whole: bool| In::Seats {
        origin: "pi".into(),
        seats: SessionSeats {
            seats: v,
            listed: false,
            whole,
        },
    };
    assert_eq!(
        unread_outs(&b.step(seats(vec![seat("a", Some("2"))], false))),
        [un("a", Some("2"))]
    );
    assert!(
        unread_outs(&b.step(seats(vec![seat("a", Some("2"))], false))).is_empty(),
        "没变不交"
    );
    b.step(seats(vec![seat("b", Some("1"))], false));
    assert_eq!(
        unread_outs(&b.step(seats(vec![seat("b", Some("3"))], true))),
        [un("a", None), un("b", Some("3"))]
    );
    assert_eq!(unread_outs(&b.replay(&[]).after), [un("b", Some("3"))]);
    assert_eq!(
        unread_outs(&b.step(In::Closed {
            origin: "pi".into()
        })),
        [un("b", None)]
    );
    assert!(unread_outs(&b.replay(&[]).after).is_empty());
}
