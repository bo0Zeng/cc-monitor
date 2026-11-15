//! 几台的需手动合成一份（`face_core::needs::merge_needs`）的判据：本机后端 `sessions-needs-merge` 与手机核心 `NeedsFace.merge` 调的是这一份。
//!
//! 1. 跨台只照各项的 `rank` 比；并列 ⇒ 本机在前、再照给的台序与台内原来的先后；每项带回它那台；`total` 是那一列各台加起来几项。
//! 2. 会外读不出的那几台各带那一句 ＋ 写好的那一行（本机在前）；没问到的那几台原样带上、不计。
//! 3. 头那一句 · 说得准说不准由这里写：说得准才带数；会外读不出只让需手动那一列说不准，没问到的一台让两列都说不准。
//! 4. 回包那一形照类型解得开（手机核心交 Kotlin 的就是这一形）。
//! 5. 一问的结局（生成的 `CallEnd`）⇒ 答了那一份 / 没问到那一格（`needs_reply`）：那一句叫出那台、码说已叫出的照原样；没问到的都让数说不准。
//!
//! 6. 分两列只在 `column_of` 一处：会话且挂着答得了的请求（`needs.ask` 在）⇒ 待答请求，其余（答不了的会话 · 计划 · 会外）⇒ 需手动。
//! 7. 窗口标题由这里整句写好：非零那几列的「待答请求 n」「需手动 n」照分隔接（待答请求在前），都 0 ⇒ 只有名字。
//!
//! 1–5 的几项多写成计划那一臂（合法不看是哪一臂，只看 `rank`；计划落需手动那一列），期望手写。

use super::*;
use cmd_wire::cells::Words;
use cmd_wire::needs::{NeedsAt, NeedsDown, NeedsItem, NeedsList, NeedsMerged};
use cmd_wire::phone::CallEnd;
use cmd_wire::Machine;
use serde_json::json;

fn item(name: &str, rank: u64) -> NeedsItem {
    NeedsItem::Plan {
        rank,
        workspace: "/w".into(),
        slice: name.into(),
        cell: None,
    }
}

fn name(x: &NeedsAt) -> String {
    match &x.item {
        NeedsItem::Plan { slice, .. } => slice.clone(),
        other => panic!("夹具里只有计划那一臂：{other:?}"),
    }
}

fn machine(m: &str) -> Machine {
    if m == "<local>" {
        Machine::local()
    } else {
        Machine::named(m.to_string())
    }
}

/// `(台, 项名, rank)` 照给的先后拆进各台（一台第一次出现的那一刻定台序）。
fn split(rows: &[(&str, &str, u64)]) -> Vec<(Machine, NeedsList)> {
    let mut out: Vec<(Machine, NeedsList)> = Vec::new();
    for (m, n, rank) in rows {
        let it = item(n, *rank);
        match out.iter_mut().find(|x| x.0.to_string() == *m) {
            Some(x) => x.1.items.push(it),
            None => out.push((
                machine(m),
                NeedsList {
                    items: vec![it],
                    ended: vec![],
                    manual_unreadable: None,
                },
            )),
        }
    }
    out
}

#[test]
fn items_are_ordered_across_machines_by_rank_alone() {
    type Case<'a> = (&'a str, &'a [(&'a str, &'a str, u64)], &'a [&'a str]);
    let cases: &[Case] = &[
        (
            "小的先答，不看是哪台",
            &[("devbox", "a", 9), ("<local>", "b", 3), ("gpu", "c", 1)],
            &["c", "b", "a"],
        ),
        (
            "并列 ⇒ 本机在前，再照给的台序与台内的先后",
            &[
                ("devbox", "c", 7),
                ("devbox", "d", 7),
                ("<local>", "a", 7),
                ("<local>", "b", 7),
                ("gpu", "e", 7),
            ],
            &["a", "b", "c", "d", "e"],
        ),
        ("一台都没有 ⇒ 空", &[], &[]),
    ];
    for (title, rows, want) in cases {
        let got = merge_needs(split(rows), vec![], false, label);
        let names: Vec<String> = got.manual.items.iter().map(name).collect();
        assert_eq!(&names, want, "{title}");
        assert_eq!(
            got.manual.total as usize,
            rows.len(),
            "{title}：总数是各台加起来几项"
        );
        assert!(got.asks.items.is_empty(), "{title}：计划不进待答请求");
        for x in &got.manual.items {
            let (m, ..) = rows.iter().find(|r| r.1 == name(x)).unwrap();
            assert_eq!(
                x.origin.to_string(),
                *m,
                "{title}：{} 带回的台不对",
                name(x)
            );
        }
    }
}

#[test]
fn unreadable_and_down_machines_are_carried_with_their_sentence() {
    let mut answered = split(&[("devbox", "r", 1), ("<local>", "l", 2)]);
    answered[0].1.manual_unreadable = Some(Words("读不懂".into()));
    let got = merge_needs(
        answered,
        vec![NeedsDown {
            origin: Machine::named("pi"),
            said: "那台没在跑".into(),
            detail: "细节".into(),
        }],
        false,
        label,
    );
    assert_eq!(got.manual.total, 2, "没问到的那台不计");
    let v = serde_json::to_value(&got).unwrap();
    assert_eq!(
        v["unreadable"],
        json!([{"origin": "devbox", "said": "读不懂",
            "line": copy_core::copy_text("needs.panel.unreadable", &[("machine", "开发机")])}]),
        "那一行由合的那一下写好、叫出那台（照给的称呼）"
    );
    assert_eq!(
        v["down"],
        json!([{"origin": "pi", "said": "那台没在跑", "detail": "细节"}]),
        "没问到的那几台原样带上"
    );
}

/// 称呼替身：devbox ⇒「开发机」，别的照名字。
fn label(m: &Machine) -> String {
    match m.to_string().as_str() {
        "devbox" => "开发机".into(),
        other => other.into(),
    }
}

/// ★ 头那一句 · 说得准说不准在合的那一下写好：说得准 ⇒ 带数；调用方说有哪台还在读 · 没问到的一台 ⇒ 两列都不带数；
/// 有哪台会外读不出 ⇒ 只有需手动那一列不带数（待答请求不缺那一截）。两列各照自己那一键。
#[test]
fn each_column_head_carries_its_count_only_when_that_column_is_sure() {
    let needs = |n: &str| copy_core::copy_text("beGroups.count.needs", &[("n", n)]);
    let asks = |n: &str| copy_core::copy_text("beGroups.count.asks", &[("n", n)]);
    let needs_unsure = copy_core::copy_text("beSession.needs.unknown", &[]);
    let asks_unsure = copy_core::copy_text("needs.asks.title", &[]);
    let two = || {
        let mut a = split(&[("devbox", "a", 1), ("<local>", "b", 2)]);
        a[0].1.items.push(session("s1", 0, true));
        a
    };
    let heads = |m: &NeedsMerged| {
        (
            m.asks.sure,
            m.asks.head.0.clone(),
            m.manual.sure,
            m.manual.head.0.clone(),
        )
    };

    let got = merge_needs(two(), vec![], false, label);
    assert_eq!(
        heads(&got),
        (true, asks("1"), true, needs("2")),
        "都说得准 ⇒ 两列各带自己的数"
    );

    let got = merge_needs(two(), vec![], true, label);
    assert_eq!(
        heads(&got),
        (false, asks_unsure.clone(), false, needs_unsure.clone()),
        "调用方说有哪台还在读 ⇒ 两列都不准"
    );
    assert_eq!(
        (got.asks.total, got.manual.total),
        (1, 2),
        "说不准也照数（只是头那一句不带）"
    );

    let mut odd = two();
    odd[0].1.manual_unreadable = Some(Words("读不懂".into()));
    let got = merge_needs(odd, vec![], false, label);
    assert_eq!(
        heads(&got),
        (true, asks("1"), false, needs_unsure.clone()),
        "会外读不出 ⇒ 只缺需手动那一截"
    );

    let got = merge_needs(
        two(),
        vec![NeedsDown {
            origin: Machine::named("pi"),
            said: "pi 离线".into(),
            detail: String::new(),
        }],
        false,
        label,
    );
    assert_eq!(
        heads(&got),
        (false, asks_unsure, false, needs_unsure),
        "没问到的一台（断着也算）⇒ 两列都不准"
    );
}

/// ★ 窗口标题由合的那一下整句写好（出口照抄）：非零那几列照「待答请求 n」「需手动 n」用分隔接、待答请求在前；都 0 ⇒ 只有名字。不看准不准。
#[test]
fn the_window_title_names_the_nonzero_columns_asks_first() {
    let asks = |n: &str| copy_core::copy_text("beGroups.count.asks", &[("n", n)]);
    let needs = |n: &str| copy_core::copy_text("beGroups.count.needs", &[("n", n)]);
    let sep = copy_core::copy_text("kit.text.sep", &[]);
    let titled =
        |counts: &str| copy_core::copy_text("needs.windowTitle.count", &[("counts", counts)]);
    let with = |a: usize, m: usize| {
        let mut items: Vec<NeedsItem> = (0..a)
            .map(|i| session(&format!("a{i}"), i as u64, true))
            .collect();
        items.extend((0..m).map(|i| item(&format!("m{i}"), 10 + i as u64)));
        vec![(
            Machine::local(),
            NeedsList {
                items,
                ended: vec![],
                manual_unreadable: None,
            },
        )]
    };
    let cases = [
        ((0, 0), copy_core::copy_text("needs.windowTitle.base", &[])),
        ((2, 0), titled(&asks("2"))),
        ((0, 1), titled(&needs("1"))),
        ((2, 1), titled(&format!("{}{sep}{}", asks("2"), needs("1")))),
    ];
    for ((a, m), want) in cases {
        let got = merge_needs(with(a, m), vec![], false, label);
        assert_eq!(got.window_title.0, want, "待答请求 {a} · 需手动 {m}");
        let unsure = merge_needs(with(a, m), vec![], true, label);
        assert_eq!(
            unsure.window_title.0, want,
            "说不准也只看数：待答请求 {a} · 需手动 {m}"
        );
    }
}

/// ★ 两台：一条答得了的会话 · 一条答不了的会话 · 一条计划 · 一条会外 ⇒ 待答请求恰那一条，需手动恰另三条（各照 rank、各带回它那台）。
#[test]
fn only_answerable_sessions_go_to_the_asks_column() {
    let answered = vec![
        (
            Machine::named("devbox"),
            NeedsList {
                items: vec![session("可答", 4, true), item("计划项", 7)],
                ended: vec![],
                manual_unreadable: None,
            },
        ),
        (
            Machine::local(),
            NeedsList {
                items: vec![session("不可答", 2, false), outside("会外", 9)],
                ended: vec![],
                manual_unreadable: None,
            },
        ),
    ];
    let got = merge_needs(answered, vec![], false, label);
    let col = |c: &cmd_wire::needs::NeedsColumn| -> Vec<(String, String)> {
        c.items
            .iter()
            .map(|x| (x.origin.to_string(), any_name(x)))
            .collect()
    };
    assert_eq!(col(&got.asks), [("devbox".to_string(), "可答".to_string())]);
    assert_eq!(
        col(&got.manual),
        [
            (cmd_wire::LOCAL.to_string(), "不可答".to_string()),
            ("devbox".to_string(), "计划项".to_string()),
            (cmd_wire::LOCAL.to_string(), "会外".to_string()),
        ]
    );
    assert_eq!((got.asks.total, got.manual.total), (1, 3));
}

/// 一条在等人的会话：`answerable` ⇒ 挂着界面答得了的请求（`ask` 在），否则那一句摆在答的位置上（`ask` 缺）。两格只经两个写口改。
fn session(sid: &str, rank: u64, answerable: bool) -> NeedsItem {
    use cmd_wire::needs::{Ask, AskKind, Needs, NeedsKind, NeedsRow};
    let mut needs = Needs {
        kind: NeedsKind::Approve,
        tool: Some("Bash".into()),
        call: None,
        what: None,
        text: Words("核心·等批准".into()),
        head: Words("等批准 · Bash".into()),
        head_code: None,
        rank,
        waited_ms: None,
        since_ms: None,
        reply: cmd_wire::needs::NeedsReply::Manual(Words("核心·只能在终端里答".into())),
    };
    if answerable {
        needs.answer_with(
            Ask {
                id: format!("ask-{sid}"),
                kind: AskKind::Approve,
                tool: "Bash".into(),
                run: None,
                what: None,
                detail: "ls".into(),
                choices: vec![],
                questions: vec![],
            },
            Words("等批准 · Bash".into()),
            None,
        );
    } else {
        needs.manual(Words("核心·只能在终端里答".into()));
    }
    NeedsItem::Session {
        rank,
        clock: None,
        session: NeedsRow {
            sid: sid.into(),
            label: Words(sid.into()),
            needs,
            hidden: None,
        },
    }
}

/// 会话外的一项（权限太宽的秘密文件那一种，`said` 当名字）。
fn outside(said: &str, rank: u64) -> NeedsItem {
    NeedsItem::Manual {
        rank,
        manual: cmd_wire::needs::ManualItem::SecretFile {
            said: said.into(),
            fix: None,
            why: None,
        },
    }
}

/// 三臂的名字：会话 ⇒ sid，计划 ⇒ 那一片，会外 ⇒ 那一句。
fn any_name(x: &NeedsAt) -> String {
    match &x.item {
        NeedsItem::Session { session, .. } => session.sid.clone(),
        NeedsItem::Plan { slice, .. } => slice.clone(),
        NeedsItem::Manual {
            manual: cmd_wire::needs::ManualItem::SecretFile { said, .. },
            ..
        } => said.clone(),
        other => panic!("夹具里没有这一种：{other:?}"),
    }
}

/// 一问的结局（生成的 `CallEnd`）⇒ 答了那一份 / 没问到那一格；夹具只交结局，句子照文案表算出期望。
fn end_of(end: CallEnd) -> Result<NeedsList, NeedsDown> {
    needs_reply(Machine::named("devbox"), "开发机", end)
}

/// ★ 没问到那台那一句：断着 ⇒ 离线那一句（叫出那台）；到期 · 撤了 ⇒ 核心写好的原样；那台答了一个错 ⇒「<那台> · <那一句>」，
/// 码说两端对不上 · 不认这条命令 ⇒ 句里已叫出那台、照原样；回包解不开 ⇒ 读不懂那一句。没问到的那一格一并进合的那一下 ⇒ 说不准。
#[test]
fn a_missed_machine_becomes_one_down_with_its_sentence_and_makes_the_count_unsure() {
    let down = end_of(CallEnd::Down { call: 1 }).expect_err("断着");
    assert_eq!(down.said, copy_core::offline("开发机"));
    assert_eq!(down.origin, Machine::named("devbox"));
    let got = merge_needs(vec![], vec![down], false, label);
    assert!(
        !got.asks.sure && !got.manual.sure,
        "一台断着 ⇒ 两列都说不准"
    );

    let timeout = end_of(CallEnd::Timeout {
        call: 1,
        said: "开发机 没回".into(),
        detail: "d".into(),
    })
    .expect_err("到期");
    assert_eq!(
        (timeout.said.as_str(), timeout.detail.as_str()),
        ("开发机 没回", "d")
    );

    let failed = |code: &str| {
        end_of(CallEnd::Failed {
            call: 1,
            code: code.into(),
            said: "坏了".into(),
            detail: "细节".into(),
            data: None,
        })
        .expect_err("答失败")
    };
    let f = failed("failed");
    assert_eq!(
        f.said,
        copy_core::copy_text(
            "kit.text.machineSaid",
            &[("machine", "开发机"), ("said", "坏了")]
        )
    );
    assert_eq!(f.detail, "细节");
    assert_eq!(
        failed(cmd_wire::SHAPE_DIFFERS).said,
        "坏了",
        "码 backend_differs ⇒ 不再套机器名"
    );
    assert_eq!(failed(cmd_wire::UNKNOWN_COMMAND).said, "坏了");

    let odd = end_of(CallEnd::Ok {
        call: 1,
        data: json!({"items": 3}),
    })
    .expect_err("回包解不开");
    assert_eq!(odd.said, copy_core::reply_unreadable("开发机"));
    assert!(
        odd.detail.starts_with("command: sessions-needs\nreply: "),
        "{}",
        odd.detail
    );
}

#[test]
fn an_answered_machine_passes_its_list_through() {
    let list = NeedsList {
        items: vec![item("a", 1)],
        ended: vec![],
        manual_unreadable: None,
    };
    let got = end_of(CallEnd::Ok {
        call: 1,
        data: serde_json::to_value(&list).unwrap(),
    });
    assert_eq!(got, Ok(list));
}

#[test]
fn the_merged_shape_reads_back() {
    let got = merge_needs(
        split(&[("<local>", "a", 1), ("devbox", "b", 2)]),
        vec![],
        false,
        label,
    );
    let text = serde_json::to_string(&got).unwrap();
    let back: NeedsMerged = serde_json::from_str(&text).expect("回包那一形解得开");
    assert_eq!(back, got);
}
