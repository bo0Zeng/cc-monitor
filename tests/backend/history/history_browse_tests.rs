//! 历史页那一列（`history-browse`）的判据。
//!
//! # 守的要求（住址）
//!
//! 几台的会话清单在本机后端并成一列（界面不并、不排）；界面只问看得见的那一窗，头里带各台几行 · 读没读到。
//!
//! # 判据
//!
//! 1. **并法与从前界面里的一致**：几台各给一份成品 ⇒ 并出来的一项一项（段头 · 组头 · 行 · 分叉挂父会话 · 孤儿 · 开合）
//!    与从前历史页在界面里并出来的逐条相同（对拍金样 `tests/__fixtures__/history-browse.golden.json`，由从前那份界面代码跑出来）。
//! 2. **只回那一窗**：`range` 之外的项不带；总数 · 段头照整列。
//! 3. **各台那一格**：读到了 ⇒ 几行 ＋ 注解那一句；读不到、手上有旧副本 ⇒ 行照副本出、头里标旧（为什么 ＋ 副本落下那一刻）；
//!    从没读到过 ⇒ 为什么、没有行；远端没同步过、也没带它的 `fresh` ⇒ 没读过（不去问）。
//! 4. **帧面**：`fresh` 只认 `remotes` 里的、要带期限；同步没成的那台下一问照样说读不到（不拿旧副本充数）；窗口 1–2000。
//! 5. **手机那一列同一份**：端核心并的段 · 行 · 分叉挂父 · 孤儿 · 组的先后与这里摊出的逐条相同（分叉树只住 `resident_client::column::nest`）。
//! 6. **手机那一列的分叉树**：父收着 ⇒ 子不出；展开 ⇒ 子紧跟父；父不在这一列（含摆进别处）⇒ 孤儿。
//!
//! # 买不到
//!
//! - 🔴 真远端（同步那一跳的判据在 `remote_listing_tests`）。

use super::*;
use serde_json::{json, Value};

fn golden() -> Value {
    let p = guard_core::addr::repo_root().join("tests/__fixtures__/history-browse.golden.json");
    serde_json::from_str(&std::fs::read_to_string(p).expect("金样")).expect("金样读不懂")
}

fn answered(g: &Value) -> Vec<Answered> {
    g["machines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| Answered {
            head: serde_json::from_value(m["head"].clone()).expect("头"),
            rows: serde_json::from_value(m["rows"].clone()).expect("行"),
        })
        .collect()
}

fn shape_of(c: &Value) -> Shape {
    let fold: Fold = serde_json::from_value(c["groups"].clone()).unwrap();
    let forks: Vec<RowId> = serde_json::from_value(c["forks"].clone()).unwrap();
    Shape {
        by: serde_json::from_value(c["view"].clone()).unwrap(),
        all_open: !c["query"].as_str().unwrap().is_empty(),
        fold,
        forks: forks.into_iter().map(|r| (r.origin, r.sid)).collect(),
    }
}

/// 摊好的一列照金样那一形写成一条一条（段头插在它落的那一项之前；孤儿那一句只在按时间那一形里画）。
fn tokens(laid: &Laid, by: By) -> Vec<String> {
    let mut out = Vec::new();
    let o = |m: &Option<cmd_wire::Machine>| m.as_ref().map_or(String::new(), ToString::to_string);
    for (i, line) in laid.lines.iter().enumerate() {
        for s in laid.sections.iter().filter(|s| s.at as usize == i) {
            out.push(format!("sec:{}", s.text.0));
        }
        match line {
            HistoryLine::Group { group, open } => {
                out.push(format!("g:{}|{}:{open}", o(&group.origin), group.key))
            }
            HistoryLine::Row {
                row,
                child,
                forks,
                forks_open,
                orphan,
            } => {
                let mut t = format!("r:{}|{}", o(&row.origin), row.session_id);
                if *child {
                    t += ":child";
                }
                if *forks > 0 {
                    t += &format!(":forks={forks}{}", if *forks_open { "(open)" } else { "" });
                }
                if *orphan && by == By::Time {
                    t += ":orphan";
                }
                out.push(t);
            }
        }
    }
    out
}

/// ★ 判据 1：几台各给一份 ⇒ 并出的先后与从前界面里并出的逐条一致。
#[test]
fn several_machines_merge_the_way_the_page_used_to() {
    let g = golden();
    let a = answered(&g);
    let cases = g["cases"].as_array().unwrap();
    assert!(cases.len() >= 6, "金样里的情形少了");
    for c in cases {
        let shape = shape_of(c);
        let want: Vec<String> = serde_json::from_value(c["expect"].clone()).unwrap();
        assert_eq!(tokens(&lay(&a, &shape), shape.by), want, "{}", c["name"]);
    }
}

fn machines_ok(g: &Value) -> Vec<MachineHead> {
    g["machines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| MachineHead {
            origin: m["origin"]
                .as_str()
                .map(|o| cmd_wire::Machine::named(o.to_string())),
            total: m["rows"].as_array().unwrap().len() as u32,
            read: Read::Ok { notice: None },
        })
        .collect()
}

/// ★ 判据 2：`range` 只回那一窗（键照摊好的先后）；总数 · 段头照整列。
#[test]
fn a_range_answers_only_that_window() {
    let g = golden();
    let a = answered(&g);
    let c = &g["cases"][1];
    let shape = shape_of(c);
    let laid = lay(&a, &shape);
    let all = laid.lines.len();
    let views = Views::new(2);
    let p = patch_from(
        &views,
        "f",
        machines_ok(&g),
        &a,
        &shape,
        Some(Range { from: 2, limit: 3 }),
    );
    let order = p.order.expect("整份带先后");
    assert_eq!(order.len(), 3);
    assert_eq!(p.up.len(), 3, "窗外的项也带来了");
    let want: Vec<String> = laid.lines[2..5].iter().map(key_of).collect();
    assert_eq!(order, want);
    let head = p.head.expect("整份带头");
    assert_eq!(head.total as usize, all, "总数得是整列的");
    assert_eq!(head.info.sections, laid.sections, "段头得是整列的");
    // 窗口在尾巴上 ⇒ 有几项回几项。
    let p = patch_from(
        &views,
        "f",
        machines_ok(&g),
        &a,
        &shape,
        Some(Range {
            from: all as u32 - 1,
            limit: 50,
        }),
    );
    assert_eq!(p.order.unwrap().len(), 1);
}

/// ★ 判据 3：各台那一格；读不到 / 没读过的那台行不出、几行记 0。
#[test]
fn each_machine_says_whether_it_was_read() {
    let local: cmd_wire::listing::Listing = Default::default();
    let ask = super::super::history_list::filters(None, None, None, None).unwrap();
    let (heads, answered) = per_machine(
        vec![
            (None, Got::Listing(local)),
            (Some("dev".into()), Got::Failed("dev 后端未在运行".into())),
            (Some("box".into()), Got::Unread),
            (
                Some("old".into()),
                Got::Stale {
                    listing: stale_listing(),
                    why: "old 后端未在运行".into(),
                    at_ms: 500,
                },
            ),
        ],
        &ask,
        1_000,
        &Default::default(),
    );
    assert_eq!(answered.len(), 2, "只有读到了的与有旧副本的出成品");
    assert_eq!(answered[1].rows.len(), 1, "旧副本的行要照出");
    assert_eq!(
        answered[1].rows[0]
            .origin
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("old")
    );
    let v = serde_json::to_value(&heads).unwrap();
    assert_eq!(
        v,
        json!([
            {"total": 0, "read": {"is": "ok", "notice": null}},
            {"origin": "dev", "total": 0, "read": {"is": "failed", "why": "dev 后端未在运行"}},
            {"origin": "box", "total": 0, "read": {"is": "unread"}},
            {"origin": "old", "total": 1, "read": {"is": "stale", "why": "old 后端未在运行", "at": v[3]["read"]["at"]}},
        ])
    );
    assert_eq!(
        v[3]["read"]["at"],
        json!(crate::common::time::history_at_text(500, 1_000, &Default::default()).0),
        "旧的那一刻照副本落下那一刻写"
    );
}

/// 一台旧副本里的一行。
fn stale_listing() -> cmd_wire::listing::Listing {
    let mut l = cmd_wire::listing::Listing::default();
    l.rows = vec![serde_json::from_value(json!({
        "agent": "claude", "sessionId": "s-old", "projectPath": "/w/p", "projectName": "p", "aiTitle": null,
        "firstUserExcerpt": "x", "title": "x", "startedAt": 1, "updatedAt": 2, "jsonlPath": "/x", "life": {"is": "unclear"},
        "messageCountApprox": 0, "isBg": false, "starred": false, "customTitle": null, "launchTab": null, "parent": null,
        "projectDir": "-w-p", "group": "claude:/w/p", "lastAccount": null
    }))
    .expect("旧副本那一行")];
    l
}

async fn browse(args: Value, until: bool) -> Result<Value, (&'static str, String)> {
    let req: HistoryBrowseReq =
        crate::wire::typed::take(&args).map_err(|f| ("bad_args", f.message))?;
    let until = until
        .then(|| crate::platform::child::Until::after(crate::platform::child::Deadline::secs(5)))
        .flatten();
    super::answer(req, Default::default(), until)
        .await
        .map(|r| serde_json::to_value(r).unwrap())
}

/// ★ 判据 4：帧面。远端那台没带 `fresh` ⇒ 不去问、说没读过；带了 ⇒ 去同步（这台够不着它 ⇒ 读不到，那一句照实）；
/// 同步没成的下一问不带 `fresh` 也说读不到。入参不对 ⇒ `bad_args`（先于任何 IO）。
#[tokio::test]
async fn the_frame_arm_reads_what_is_held_and_syncs_only_fresh_ones() {
    let m = "browse-never-reached";
    let ask = |fresh: bool| json!({"here": false, "remotes": [m], "fresh": if fresh { json!([m]) } else { json!([]) }, "by": "time", "groups": {"open": false, "except": []}, "forks": [], "range": {"from": 0, "limit": 40}});
    let v = browse(ask(false), false).await.unwrap();
    assert_eq!(
        v["head"]["info"]["machines"][0]["read"]["is"], "unread",
        "{v}"
    );
    let v = browse(ask(true), true).await.unwrap();
    assert_eq!(
        v["head"]["info"]["machines"][0]["read"]["is"], "failed",
        "{v}"
    );
    assert_eq!(v["head"]["total"], 0);
    let v = browse(ask(false), false).await.unwrap();
    assert_eq!(
        v["head"]["info"]["machines"][0]["read"]["is"], "failed",
        "同步没成的那台下一问又说成没读过：{v}"
    );
    let with = |over: Value| {
        let mut a = ask(false);
        a["here"] = json!(true);
        a["remotes"] = json!([]);
        for (k, v) in over.as_object().unwrap() {
            a[k] = v.clone();
        }
        if over.get("range") == Some(&Value::Null) {
            a.as_object_mut().unwrap().remove("range");
        }
        a
    };
    for (bad, until) in [
        (with(json!({"fresh": ["x"]})), true),
        (with(json!({"remotes": ["x"], "fresh": ["x"]})), false),
        (with(json!({"remotes": ["x", "x"]})), false),
        (with(json!({"remotes": [""]})), false),
        (with(json!({"range": {"from": 0, "limit": 0}})), false),
        (with(json!({"range": {"from": 0, "limit": 2001}})), false),
        (with(json!({"range": null})), false),
    ] {
        let e = browse(bad.clone(), until).await.unwrap_err();
        assert_eq!(e.0, "bad_args", "{bad} 应是 bad_args：{e:?}");
    }
}

/// 金样那几台交给端核心的那一问：每行带 `sid` · 父（那台写的 `forkedFromSessionId`）；`forks` 照这一形展开了的那几个。
fn phone_ask(
    a: &[Answered],
    shape: &Shape,
    placed: &dyn Fn(&HistoryRow) -> bool,
) -> resident_client::column::ChatsColumnAsk {
    use resident_client::column::{ChatsColumnAsk, ColumnFork, ColumnHost, ColumnRow};
    let host_of = |o: &Option<cmd_wire::Machine>| {
        a.iter()
            .position(|m| m.rows.first().is_some_and(|r| &r.origin == o))
            .map(|h| h as u32)
    };
    ChatsColumnAsk {
        hosts: a
            .iter()
            .map(|m| ColumnHost {
                rows: m
                    .rows
                    .iter()
                    .map(|r| ColumnRow {
                        at: r.at,
                        section: Some(r.section_text.0.clone()),
                        placed: placed(r),
                        sid: r.session_id.clone(),
                        parent: r.forked_from_session_id.clone(),
                    })
                    .collect(),
                groups: m.head.groups.iter().map(|g| g.order).collect(),
            })
            .collect(),
        forks: shape
            .forks
            .iter()
            .filter_map(|(o, sid)| {
                host_of(o).map(|host| ColumnFork {
                    host,
                    sid: sid.clone(),
                })
            })
            .collect(),
    }
}

/// 端核心交回的那一列照 [`tokens`] 那一形写成一条一条（段头 ＋ 行；孤儿那一句照按时间那一形）。
fn phone_tokens(a: &[Answered], c: &resident_client::column::ChatsColumn) -> Vec<String> {
    let o = |m: &Option<cmd_wire::Machine>| m.as_ref().map_or(String::new(), ToString::to_string);
    c.sections
        .iter()
        .flat_map(|s| {
            let head = format!("sec:{}", s.title.clone().unwrap_or_default());
            std::iter::once(head).chain(s.rows.iter().map(|l| {
                let r = &a[l.host as usize].rows[l.at as usize];
                let mut t = format!("r:{}|{}", o(&r.origin), r.session_id);
                if l.child {
                    t += ":child";
                }
                if l.forks > 0 {
                    t += &format!(
                        ":forks={}{}",
                        l.forks,
                        if l.forks_open { "(open)" } else { "" }
                    );
                }
                if l.orphan {
                    t += ":orphan";
                }
                t
            }))
        })
        .collect()
}

/// ★ 判据 5：手机会话屏那一列与本机后端并的同一份 —— 金样那三台（同一刻的行跨台都有 · 分叉挂父 · 孙辈 · 孤儿）交端核心 ⇒
/// 段与行的先后、分叉挂父（`child · forks · forksOpen · orphan`）、组的先后与这里摊出的逐条相同；按时间那几形各一遍。
#[test]
fn the_phone_column_merges_like_the_backend() {
    use resident_client::column::column;
    let g = golden();
    let a = answered(&g);
    let o = |m: &Option<cmd_wire::Machine>| m.as_ref().map_or(String::new(), ToString::to_string);
    let folded = Shape {
        by: By::Project,
        all_open: false,
        fold: Fold::default(),
        forks: BTreeSet::new(),
    };
    let mut times = 0;
    for c in g["cases"].as_array().unwrap() {
        let shape = shape_of(c);
        if shape.by != By::Time {
            continue;
        }
        times += 1;
        let want = tokens(&lay(&a, &shape), By::Time);
        assert!(want.len() > 10, "金样那几台的行少了：{want:?}");
        let got = column(&phone_ask(&a, &shape, &|_| false));
        assert_eq!(phone_tokens(&a, &got), want, "{}", c["name"]);
        let groups: Vec<String> = got
            .groups
            .iter()
            .map(|p| {
                let g = &a[p.host as usize].head.groups[p.at as usize];
                format!("g:{}|{}:false", o(&g.origin), g.key)
            })
            .collect();
        assert_eq!(groups, tokens(&lay(&a, &folded), By::Project), "组的先后");
    }
    assert!(times >= 2, "金样里按时间那几形少了");
}

/// ★ 判据 6：手机那一列的分叉树 —— 父收着 ⇒ 子不在任何一段里；父展开 ⇒ 子紧跟父、不另起段；
/// 父不在这一列（没读到 · 摆进固定或组那几段）⇒ 子自成一行、记孤儿。摆进别处的那几行等于不在这一列：与后端去掉它们摊出的相同。
#[test]
fn the_phone_column_nests_forks_under_their_parent() {
    use resident_client::column::column;
    let g = golden();
    let a = answered(&g);
    let shape_with = |forks: &[&str]| Shape {
        by: By::Time,
        all_open: false,
        fold: Fold::default(),
        forks: forks.iter().map(|s| (None, s.to_string())).collect(),
    };
    // 父收着：k1（a1 的分叉）· g1（k1 的分叉）· k2（cx 的分叉）都不出。
    let shut = phone_tokens(&a, &column(&phone_ask(&a, &shape_with(&[]), &|_| false)));
    for sid in ["k1", "g1", "k2"] {
        let me = format!("r:|{sid}");
        assert!(
            !shut
                .iter()
                .any(|t| t == &me || t.starts_with(&format!("{me}:"))),
            "父收着、{sid} 还出：{shut:?}"
        );
    }
    assert!(shut.contains(&"r:|a1:forks=1".to_string()), "{shut:?}");
    // 父不在这一列（o1 的父 nowhere 没有、x1 的父 d1 在另一台）⇒ 自成一行、记孤儿。
    for t in ["r:|o1:orphan", "r:|x1:orphan"] {
        assert!(shut.contains(&t.to_string()), "{t}：{shut:?}");
    }
    // 展开 a1 · k1：k1 · g1 紧跟 a1，在同一段里。
    let open = phone_tokens(
        &a,
        &column(&phone_ask(&a, &shape_with(&["a1", "k1"]), &|_| false)),
    );
    let at = open
        .iter()
        .position(|t| t == "r:|a1:forks=1(open)")
        .expect("a1 展开那一行");
    assert_eq!(
        open[at + 1..at + 3],
        [
            "r:|k1:child:forks=1(open)".to_string(),
            "r:|g1:child".to_string()
        ],
        "{open:?}"
    );
    // 摆进别处的 k1 · b1：g1 没了父 ⇒ 孤儿；a1 底下没有分叉。与后端去掉那两行摊出的相同。
    let away = |r: &HistoryRow| r.session_id == "k1" || r.session_id == "b1";
    let got = phone_tokens(&a, &column(&phone_ask(&a, &shape_with(&["a1"]), &away)));
    let rest: Vec<Answered> = a
        .iter()
        .map(|m| Answered {
            head: m.head.clone(),
            rows: m.rows.iter().filter(|r| !away(r)).cloned().collect(),
        })
        .collect();
    assert_eq!(
        got,
        tokens(&lay(&rest, &shape_with(&["a1"])), By::Time),
        "去掉摆进别处的"
    );
    assert!(got.contains(&"r:|g1:orphan".to_string()), "{got:?}");
    assert!(got.contains(&"r:|a1".to_string()), "{got:?}");
    // 分叉落在另一天（段头字不同）：展开了也跟着父，不另起段。
    let mut later = answered(&g);
    for r in later.iter_mut().flat_map(|m| m.rows.iter_mut()) {
        if r.session_id == "g1" {
            r.section_text = cmd_wire::cells::Words("九月".into());
        }
    }
    let shape = shape_with(&["a1", "k1"]);
    let got = phone_tokens(&later, &column(&phone_ask(&later, &shape, &|_| false)));
    let want = tokens(&lay(&later, &shape), By::Time);
    assert_eq!(got, want, "分叉落在另一天");
    let at = want.iter().position(|t| t == "r:|g1:child").expect("g1");
    assert_eq!(want[at - 1], "r:|k1:child:forks=1(open)", "{want:?}");
}
