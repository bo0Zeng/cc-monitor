use super::*;
use serde_json::json;

// 判据照线上那一份比：入参经 typed 那一处收（同分派）再交本体，回包写成线上那一份（盖住 `super::*` 里的同名口）。
fn take<R: serde::de::DeserializeOwned>(v: &serde_json::Value) -> Result<R, Said> {
    crate::wire::typed::take(v).map_err(|f| Said::new("bad_args", f.said))
}
fn val<T: serde::Serialize>(v: T) -> serde_json::Value {
    serde_json::to_value(v).unwrap()
}
fn parse_inbox(v: &serde_json::Value) -> Result<(String, u64), Said> {
    super::parse_inbox(&take(v)?)
}
fn rows_of(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .expect("金样缺那几行")
        .iter()
        .map(|l| l.as_str().expect("一行不是字符串").to_string())
        .collect()
}

fn codes_of(op: &str) -> Vec<String> {
    let mut v: Vec<String> = crate::stream::inbound::REGISTRY
        .iter()
        .find(|s| s.name == op)
        .unwrap_or_else(|| panic!("后端登记表里没有 `{op}`"))
        .codes
        .iter()
        .map(|c| c.to_string())
        .collect();
    v.sort();
    v
}

fn golden_codes(g: &serde_json::Value) -> Vec<String> {
    let mut v: Vec<String> = g["codes"]
        .as_array()
        .expect("金样缺 codes")
        .iter()
        .map(|c| c.as_str().expect("码不是字符串").to_string())
        .collect();
    v.sort();
    v
}

/// 这一块一个子进程都不起：收件箱住后端，读写都在 `store` 那一处，不起 cc-bus 脚本、不认它的输出形。
#[test]
fn the_bus_backend_starts_no_cc_bus_script() {
    for (file, src) in [
        (
            "control/cc_bus.rs",
            include_str!("../../../src/backend/control/cc_bus.rs"),
        ),
        (
            "control/cc_bus/store.rs",
            include_str!("../../../src/backend/control/cc_bus/store.rs"),
        ),
        (
            "control/cc_bus_wire.rs",
            include_str!("../../../src/backend/control/cc_bus_wire.rs"),
        ),
    ] {
        let prod = guard_core::production_code(src);
        for needle in [
            "plugin::invoke",
            "plugin::discover",
            "Command::new",
            "#cc-list-tsv",
            "#cc-log",
        ] {
            assert!(!prod.contains(needle), "{file} 的生产段里还有 `{needle}`");
        }
    }
    // 正控：同一个认法在真起子进程的那一处认得出。
    assert!(
        guard_core::production_code(include_str!("../../../src/backend/plan/mod.rs"))
            .contains("plugin::invoke")
    );
}

/// **跨语言金样**：驾驶舱读收件箱的成品，两侧读同一份 `tests/__fixtures__/cc-bus-read.golden.json`。
/// 异源：输入那几行经**生产**解析器（`parse_inbox` / `inbox_reply`）现算，reply 是手写期望；码集合 == `inbound::REGISTRY`。
#[test]
fn the_cockpit_read_products_match_the_cross_language_golden() {
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/cc-bus-read.golden.json"))
            .expect("金样读不出来");
    let ib = &g["inbox"];
    assert_eq!(
        parse_inbox(&ib["request"]).expect("金样请求过不了生产解析器"),
        ("alpha_cc".to_string(), 200)
    );
    assert_eq!(
        parse_inbox(&ib["badRequest"])
            .expect_err("`--help` 当 id 必须拒")
            .code
            .as_deref(),
        Some("bad_id")
    );
    assert_eq!(
        val(super::inbox_reply(&rows_of(&ib["lines"]))),
        ib["reply"],
        "`bus-inbox` 成品与金样不相等"
    );
    assert_eq!(
        golden_codes(ib),
        codes_of("bus-inbox"),
        "金样 `bus-inbox` 的拒绝码与后端登记的不相等"
    );
}

/// 收件箱的回显超上限保尾、说 `truncated`；坏行跳过计数、不丢好行。
#[test]
fn the_inbox_tail_keeps_the_newest_and_says_so() {
    let good = serde_json::to_string(&BusEnvelope {
        id: "m19a1b2c3d4e0001".into(),
        ts: "2026-10-26T08:00:00Z".into(),
        from: "peer-a".into(),
        to: "alpha-b".into(),
        tier: 2,
        act: cmd_wire::bus::BusAct::Inform,
        title: "占位".into(),
        re: None,
        refs: vec![],
        text: "x".repeat(1024 * 1024),
    })
    .unwrap();
    let rows: Vec<String> = std::iter::repeat_n(good, 6)
        .chain(["not json".to_string()])
        .collect();
    let r = super::inbox_reply(&rows);
    assert!(r.truncated);
    assert_eq!(
        (r.messages.len(), r.skipped),
        (3, 1),
        "4 MiB 装得下末三封 ＋ 末行坏的那一行"
    );
    let r = super::inbox_reply(&rows[4..]);
    assert!(!r.truncated);
    assert_eq!((r.messages.len(), r.skipped), (2, 1));
}

/// 叫它什么 · 在不在只问后端那一处（判官由调用方交进来）；判不了 ⇒ `live` 是 null、标题照实就是会话 id。
#[test]
fn title_and_liveness_come_from_the_judge_and_unknown_stays_unknown() {
    let rows = vec![("s1".to_string(), 1), ("s2".to_string(), 0)];
    let got = roster_agents(&rows, &|sid| match sid {
        "s1" => Seen {
            title: Some("标题".into()),
            live: Some(false),
        },
        _ => Seen {
            title: None,
            live: None,
        },
    });
    assert_eq!(
        got,
        vec![
            BusAgent {
                id: "s1".into(),
                title: "标题".into(),
                unread: 1,
                live: Some(false)
            },
            BusAgent {
                id: "s2".into(),
                title: "s2".into(),
                unread: 0,
                live: None
            },
        ]
    );
}

/// 读面收的 id 在拼进收件箱路径之前先过形状判定（`§47`）。样本取跨语言金样 `cc-bus-control.golden.json` 的 `ids`。
#[test]
fn bus_ids_are_judged_here_before_they_reach_cc_bus() {
    let g: serde_json::Value = serde_json::from_str(include_str!(
        "../../__fixtures__/cc-bus-control.golden.json"
    ))
    .expect("金样读不出来");
    let take = |k: &str| -> Vec<String> {
        g["ids"][k]
            .as_array()
            .unwrap_or_else(|| panic!("金样缺 ids.{k}"))
            .iter()
            .map(|v| v.as_str().expect("id 不是字符串").to_string())
            .collect()
    };
    let (ok, bad) = (take("ok"), take("bad"));
    assert!(
        !ok.is_empty() && !bad.is_empty(),
        "金样的 ids 空了 —— 下面是空转"
    );
    for id in &ok {
        assert_eq!(
            parse_inbox(&json!({ "id": id }))
                .map(|(id, _)| id)
                .as_deref(),
            Ok(id.as_str()),
            "{id:?} 的收件箱该读得到"
        );
    }
    for id in bad.iter().filter(|id| !id.trim().is_empty()) {
        let e = parse_inbox(&json!({ "id": id })).expect_err(id);
        assert_eq!(e.code.as_deref(), Some("bad_id"), "{id:?}：读收件箱的拒码不对（{e:?}）");
        assert!(
            e.said.contains(&format!("{id:?}")),
            "{id:?}：那一句没点出是哪个值：{}",
            e.said
        );
        for checked in [
            peek_for_inbound(&BusPeekReq {
                id: id.clone(),
                max_lines: None,
            })
            .map(|_| ()),
            unread_for_inbound(&BusUnreadReq { id: id.clone() }).map(|_| ()),
        ] {
            assert_eq!(
                checked.unwrap_err().code.as_deref(),
                Some("bad_id"),
                "{id:?}：peek · unread 也先判形状"
            );
        }
    }
    // 空是「缺」（`bad_args`），不是「形状不对」—— 两件事的话不一样。
    assert_eq!(
        parse_inbox(&json!({ "id": "" })).unwrap_err().code.as_deref(),
        Some("bad_args")
    );
}

/// ★★**跨语言金样**：界面直接收的 `bus-list` 成品，两侧读同一份 `tests/__fixtures__/cc-bus-control.golden.json`。
/// 成品 == **生产**构造器（名单行由金样里那份收件箱样例经 `roster_agents` 现算）· 码集合 == `inbound::REGISTRY`。
#[test]
fn the_bus_products_match_the_cross_language_golden() {
    use serde_json::Value;
    let g: Value = serde_json::from_str(include_str!(
        "../../__fixtures__/cc-bus-control.golden.json"
    ))
    .expect("金样读不出来");
    assert_eq!(
        golden_codes(&g["bus-list"]),
        codes_of("bus-list"),
        "金样里 `bus-list` 的拒绝码与后端登记的不相等 —— 界面那张「码 → 一句话」的表就会漏一档或多一档"
    );
    let sessions = g["input"]["sessions"]
        .as_array()
        .expect("输入样例缺 `sessions`")
        .clone();
    let judge = |sid: &str| -> Seen {
        sessions.iter().find(|s| s["sid"] == sid).map_or(
            Seen {
                title: None,
                live: None,
            },
            |s| Seen {
                title: s["title"].as_str().map(str::to_string),
                live: s["live"].as_bool(),
            },
        )
    };
    let rows: Vec<(String, u64)> = g["input"]["inboxes"]
        .as_array()
        .expect("输入样例缺 `inboxes`")
        .iter()
        .map(|r| {
            (
                r["id"].as_str().expect("缺 id").to_string(),
                r["unread"].as_u64().expect("缺 unread"),
            )
        })
        .collect();
    assert_eq!(
        val(list_reply(roster_agents(&rows, &judge))),
        g["bus-list"]["reply"],
        "`bus-list` 成品与金样不相等"
    );
}

/// 给 agent 读的那一段：一封一段、只印线上码、有引用才印引用那一行、读不懂的行跳过并在末尾说跳过几条。
#[test]
fn the_peeked_text_is_one_block_per_letter_with_wire_codes_only() {
    let e = |id: &str, re: Option<&str>, refs: Vec<&str>| {
        serde_json::to_string(&BusEnvelope {
            id: id.into(),
            ts: "2026-10-27T00:00:00Z".into(),
            from: "aaaaaaaa-1111-2222-3333-444444444444".into(),
            to: "bbbbbbbb-1111-2222-3333-444444444444".into(),
            tier: 1,
            act: cmd_wire::bus::BusAct::Answer,
            title: "占位标题".into(),
            re: re.map(str::to_string),
            refs: refs.into_iter().map(str::to_string).collect(),
            text: "占位正文".into(),
        })
        .unwrap()
    };
    let (text, shown, skipped) = render(&[
        e(
            "m1aaaaaaaaaaaa",
            Some("m0aaaaaaaaaaaa"),
            vec!["commit:393349b8b"],
        ),
        "not json".to_string(),
        e("m2aaaaaaaaaaaa", None, vec![]),
    ]);
    assert_eq!((shown, skipped), (2, 1));
    assert_eq!(text.matches("[cc-bus] id=").count(), 2);
    assert!(text.contains(
        "[cc-bus] id=m1aaaaaaaaaaaa from=aaaaaaaa-1111-2222-3333-444444444444 ts=2026-10-27T00:00:00Z tier=1 act=answer re=m0aaaaaaaaaaaa\ntitle: 占位标题\nrefs: commit:393349b8b\n占位正文\n"
    ));
    assert_eq!(
        text.matches("refs: ").count(),
        1,
        "没有引用的那一封不印引用行"
    );
    assert!(text.contains(
        "(reply: cc-bus send aaaaaaaa-1111-2222-3333-444444444444 --act answer --re m2aaaaaaaaaaaa"
    ));
    assert!(
        !text.contains(&copy_core::copy_text("ccBus.act.answer", &[])),
        "不印显示用的词"
    );
    assert!(
        copy_core::copy_matches_with(
            "beCcBus.peek.skipped",
            &[("n", "1")],
            text.trim_end().lines().last().unwrap()
        ),
        "末行说跳过了一条：{text}"
    );
    assert_eq!(render(&[]), (String::new(), 0, 0));
}

/// 总线的帧命令：名单 · 读收件箱 · 发信 · 两阶段读两跳 · 未读数。界面不发信（人要说话就在会话里说）。
#[test]
fn the_bus_commands_are_the_inbox_and_its_ops() {
    let mut got: Vec<&str> = crate::stream::inbound::REGISTRY
        .iter()
        .map(|s| s.name)
        .filter(|n| n.starts_with("bus-"))
        .collect();
    got.sort_unstable();
    assert_eq!(
        got,
        [
            "bus-commit",
            "bus-inbox",
            "bus-list",
            "bus-peek",
            "bus-send",
            "bus-unread"
        ]
    );
}
