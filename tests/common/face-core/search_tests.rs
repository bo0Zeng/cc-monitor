//! 几台的搜索结果合一份（`face_core::search::merge`）的判据：本机后端与手机核心调的是这一份。
//!
//! 1. `updatedAt` 倒序、同一刻照台的先后；命中数 · 读不动的份数相加；任一行被砍 ⇒ `truncated`；不覆盖的那几家去重；
//!    远端的行带那台的名字、本机的不带；没答上的那几台原样带上。
//! 2. 真回包那一形（那台扫出来的行，金样 `history-search-row.golden.json`）照类型解得开、合完整数格原样是整数；
//!    整数格写成 `3000.0` ⇒ 解不开（交进来的一侧得原样写整数）。
//! 3. 结果头那一句在这里写好（搜的词 · 命中数 · 会话数；截过 ⇒ 截过那一句），两端照摆；一台都没答上 ⇒ 不写（没问到不当没有），
//!    答了一台、一处都没中 ⇒ 照写（命中 0）。
//! 4. 头底下那几句也在这里写好（读不动几份 · 只按标题的那几家），两端照摆。

use super::*;
use cmd_wire::search::{HistorySearchReply, Merged, SearchDown};
use cmd_wire::Machine;
use serde_json::{json, Value};

/// 金样里那几行（那台真扫出来的形：`updatedAt` · `face.order` · `hits[].offset` 这类整数格都在）。
fn golden_rows() -> Vec<Value> {
    let v: Value = serde_json::from_str(include_str!(
        "../../__fixtures__/history-search-row.golden.json"
    ))
    .expect("金样是 JSON");
    v["lines"]
        .as_array()
        .expect("lines")
        .iter()
        .map(|l| serde_json::from_str(l.as_str().expect("一行")).expect("一行 JSON"))
        .collect()
}

/// 一台的回包（真形）：行照金样、改掉会话 id 与时刻，带上那台写好的两格字。
fn reply_text(rows: &[(&str, i64, u32, bool)], unreadable: usize, skipped: &[&str]) -> String {
    let base = golden_rows()[0].clone();
    let sessions: Vec<Value> = rows
        .iter()
        .map(|(sid, at, more, cut)| {
            let mut r = base.clone();
            r["sessionId"] = json!(sid);
            r["updatedAt"] = json!(at);
            r["moreHits"] = json!(more);
            r["hitsTruncated"] = json!(cut);
            r["atText"] = json!(format!("at-{sid}"));
            r["spanText"] = json!(format!("span-{sid}"));
            r["label"] = json!(sid);
            r["starred"] = json!(false);
            r["hidden"] = json!(null);
            r
        })
        .collect();
    json!({ "sessions": sessions, "unreadable": unreadable, "skipped": skipped }).to_string()
}

fn reply(
    rows: &[(&str, i64, u32, bool)],
    unreadable: usize,
    skipped: &[&str],
) -> HistorySearchReply {
    serde_json::from_str(&reply_text(rows, unreadable, skipped)).expect("真形的回包照类型解得开")
}

#[test]
fn merging_sorts_newest_first_stably_and_sums_what_each_machine_said() {
    let local = Machine::local();
    let down = vec![SearchDown {
        origin: Machine::named("wsl"),
        said: "那台没在跑".into(),
    }];
    let m = merge(
        "q",
        vec![
            (
                local.clone(),
                reply(
                    &[("tie-b", 150, 0, false), ("local-old", 100, 2, false)],
                    1,
                    &["Codex"],
                ),
            ),
            (
                Machine::named("pi"),
                reply(
                    &[("rem-new", 300, 2, true), ("tie-a", 150, 0, false)],
                    2,
                    &["Codex", "Gemini"],
                ),
            ),
        ],
        down.clone(),
    );
    let order: Vec<(&str, String)> = m
        .sessions
        .iter()
        .map(|s| (s.row.row.session_id.as_str(), s.origin.to_string()))
        .collect();
    // 倒序；同一刻照台的先后（本机那一台先交）；本机那几行照实写 `"<local>"`。
    let (pi, here) = ("pi".to_string(), cmd_wire::LOCAL.to_string());
    assert_eq!(
        order,
        [
            ("rem-new", pi.clone()),
            ("tie-b", here.clone()),
            ("tie-a", pi),
            ("local-old", here)
        ]
    );
    // 金样那一行自己摆着 1 处命中：4 行 × 1 ＋ moreHits 2 ＋ 2。
    assert_eq!((m.total_hits, m.sessions.len(), m.truncated), (8, 4, true));
    assert_eq!(
        m.notes,
        [
            copy_core::copy_text("history.search.unreadable", &[("n", "3")]),
            copy_core::copy_text("history.search.skipped", &[("agent", "Codex")]),
            copy_core::copy_text("history.search.skipped", &[("agent", "Gemini")]),
        ],
        "读不动的份数相加写一句 · 不覆盖的那几家去重、照先说的那台的先后各一句"
    );
    assert_eq!(m.down, down);
    assert_eq!(
        m.sessions[0].row.at_text, "at-rem-new",
        "那台写好的字原样带过来"
    );
    // 「+N 处」那颗的字合的这一步写好（界面照抄）：还有没摆出来的命中才有。
    let more: Vec<Option<String>> = m.sessions.iter().map(|s| s.more_text.clone()).collect();
    let plus2 = Some(copy_core::copy_text(
        "history.searchSession.more",
        &[("n", "2")],
    ));
    assert_eq!(more, [plus2.clone(), None, None, plus2]);
    let empty = merge("q", vec![], vec![]);
    assert_eq!(
        (empty.total_hits, empty.sessions.len(), empty.truncated),
        (0, 0, false)
    );
}

#[test]
fn integer_cells_of_a_real_reply_pass_through_as_integers() {
    let m = merge(
        "q",
        vec![(
            Machine::named("pi"),
            reply(&[("s1", 1_767_225_600_123, 3, false)], 0, &[]),
        )],
        vec![],
    );
    let out = serde_json::to_value(&m).expect("写得出");
    let row = &out["sessions"][0];
    assert!(row["updatedAt"].is_i64(), "{row}");
    assert_eq!(row["updatedAt"], json!(1_767_225_600_123_i64));
    assert!(row["face"]["order"].is_u64(), "{row}");
    assert!(
        row["hits"][0]["offset"].is_u64() && row["hits"][0]["line"].is_u64(),
        "{row}"
    );
    assert!(
        !out.to_string().contains(".0,"),
        "整数格被写成了小数：{out}"
    );
    // 交进来的一侧把整数写成小数 ⇒ 解不开（手机那一侧原样写整数，判据在 `JsonTest`）。
    let floaty = reply_text(&[("s1", 3000, 0, false)], 0, &[])
        .replace("\"updatedAt\":3000", "\"updatedAt\":3000.0");
    assert!(floaty.contains("3000.0"));
    assert!(serde_json::from_str::<HistorySearchReply>(&floaty).is_err());
}

#[test]
fn the_head_line_is_written_here_once_with_the_word_and_the_counts() {
    let plain = merge(
        "报表",
        vec![(
            Machine::local(),
            reply(&[("s1", 10, 1, false), ("s2", 9, 0, false)], 0, &[]),
        )],
        vec![],
    );
    let args = |m: &Merged| (m.total_hits.to_string(), m.sessions.len().to_string());
    let (n, m) = args(&plain);
    assert_eq!(
        plain.summary,
        Some(copy_core::copy_text(
            "history.search.summary",
            &[("q", "报表"), ("n", &n), ("m", &m)]
        ))
    );
    let cut = merge(
        "报表",
        vec![(Machine::local(), reply(&[("s1", 10, 2, true)], 0, &[]))],
        vec![],
    );
    let (n, m) = args(&cut);
    assert!(cut.truncated);
    assert_eq!(
        cut.summary,
        Some(copy_core::copy_text(
            "history.search.summaryTruncated",
            &[("q", "报表"), ("n", &n), ("m", &m)]
        )),
        "截过 ⇒ 截过那一句"
    );
}

/// ★ 一台都没答上 ⇒ 不出结果头那一句（没问到不当「命中 0」）；答了一台、一处都没中 ⇒ 照出（命中 0 是真的）。
/// 两端只照这一格摆，不再自己数没答上的台数。
#[test]
fn no_machine_answered_writes_no_head_line() {
    let down = |name: &str| SearchDown {
        origin: Machine::named(name),
        said: format!("{name} 离线"),
    };
    let none = merge("报表", vec![], vec![down("pi"), down("nas")]);
    assert_eq!(none.summary, None, "一台都没答上 ⇒ 没有结果头");
    assert_eq!(none.down.len(), 2, "没答上的那几台原样带上");
    let empty = merge(
        "报表",
        vec![(Machine::local(), reply(&[], 0, &[]))],
        vec![down("pi")],
    );
    assert_eq!(
        empty.summary,
        Some(copy_core::copy_text(
            "history.search.summary",
            &[("q", "报表"), ("n", "0"), ("m", "0")]
        )),
        "答了一台、一处没中 ⇒ 照出命中 0"
    );
}

/// ★ 一行都没有时空态那一句也在这里写好，两端只照它摆：一台都没答上 ⇒「没有一台答上」那一句（没问到不当没有）；
/// 答了、一处没中 ⇒ 没有匹配那一句；有行 ⇒ 不写。
#[test]
fn the_empty_line_says_whether_nobody_answered_or_nothing_matched() {
    let down = |name: &str| SearchDown {
        origin: Machine::named(name),
        said: format!("{name} 离线"),
    };
    let none = merge("报表", vec![], vec![down("pi"), down("nas")]);
    assert_eq!(
        none.empty,
        Some(copy_core::copy_text("history.search.noneAnswered", &[])),
        "一台都没答上 ⇒ 搜不了那一句"
    );
    let missed = merge(
        "报表",
        vec![(Machine::local(), reply(&[], 0, &[]))],
        vec![down("pi")],
    );
    assert_eq!(
        missed.empty,
        Some(copy_core::copy_text("kit.menu.noMatch", &[("q", "报表")])),
        "答了、一处没中 ⇒ 没有匹配"
    );
    let hit = merge(
        "报表",
        vec![(Machine::local(), reply(&[("s1", 10, 0, false)], 0, &[]))],
        vec![],
    );
    assert_eq!(hit.empty, None, "有行 ⇒ 不写空态");
    assert_ne!(none.empty, missed.empty, "两种空不是同一句");
}

#[test]
fn nothing_unreadable_and_nothing_skipped_writes_no_note() {
    let m = merge(
        "q",
        vec![(Machine::local(), reply(&[("s1", 10, 0, false)], 0, &[]))],
        vec![],
    );
    assert!(
        m.notes.is_empty(),
        "读不动 0 份 · 没有只按标题的 ⇒ 一句都不写"
    );
}

/// ★ 没答上那台那一句由这里写好、叫出那台（同需手动那一份说法，`needs::missed`）：答了一个错 ⇒「<那台> · <那一句>」；
/// 断着 ⇒ 离线那一句；回包解不开 ⇒ 读不懂那一句；答了 ⇒ 那一份原样。
#[test]
fn a_machine_that_did_not_answer_is_named_in_its_down_sentence() {
    use cmd_wire::phone::CallEnd;
    let pi = || Machine::named("pi");
    let down = search_reply(
        pi(),
        "树莓派",
        CallEnd::Failed {
            call: 1,
            code: "failed".into(),
            said: "读不动".into(),
            detail: String::new(),
            data: None,
        },
    )
    .expect_err("答失败");
    assert_eq!(
        down,
        SearchDown {
            origin: pi(),
            said: copy_core::copy_text(
                "kit.text.machineSaid",
                &[("machine", "树莓派"), ("said", "读不动")]
            ),
        }
    );
    let off = search_reply(pi(), "树莓派", CallEnd::Down { call: 1 }).expect_err("断着");
    assert_eq!(off.said, copy_core::offline("树莓派"));
    let odd = search_reply(
        pi(),
        "树莓派",
        CallEnd::Ok {
            call: 1,
            data: json!({"sessions": 1}),
        },
    )
    .expect_err("解不开");
    assert_eq!(odd.said, copy_core::reply_unreadable("树莓派"));
    let ok = search_reply(
        pi(),
        "树莓派",
        CallEnd::Ok {
            call: 1,
            data: serde_json::from_str(&reply_text(&[("s1", 10, 0, false)], 0, &[])).unwrap(),
        },
    )
    .expect("答了");
    assert_eq!(ok, reply(&[("s1", 10, 0, false)], 0, &[]));
}
