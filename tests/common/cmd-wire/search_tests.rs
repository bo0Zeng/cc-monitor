//! 几台的搜索结果合一份（`cmd_wire::search::merge`）的判据：本机后端与手机核心调的是这一份。
//!
//! 1. `updatedAt` 倒序、同一刻照台的先后；命中数 · 读不动的份数相加；任一行被砍 ⇒ `truncated`；不覆盖的那几家去重；
//!    远端的行带那台的名字、本机的不带；没答上的那几台原样带上。
//! 2. 真回包那一形（那台扫出来的行，金样 `history-search-row.golden.json`）照类型解得开、合完整数格原样是整数；
//!    整数格写成 `3000.0` ⇒ 解不开（交进来的一侧得原样写整数）。

use super::*;
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
        origin: Machine("wsl".into()),
        said: "那台没在跑".into(),
    }];
    let m = merge(
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
                Machine("pi".into()),
                reply(
                    &[("rem-new", 300, 2, true), ("tie-a", 150, 0, false)],
                    2,
                    &["Codex", "Gemini"],
                ),
            ),
        ],
        down.clone(),
    );
    let order: Vec<(&str, Option<&str>)> = m
        .sessions
        .iter()
        .map(|s| {
            (
                s.row.row.session_id.as_str(),
                s.origin.as_ref().map(|o| o.0.as_str()),
            )
        })
        .collect();
    // 倒序；同一刻照台的先后（本机那一台先交）。
    assert_eq!(
        order,
        [
            ("rem-new", Some("pi")),
            ("tie-b", None),
            ("tie-a", Some("pi")),
            ("local-old", None)
        ]
    );
    // 金样那一行自己摆着 1 处命中：4 行 × 1 ＋ moreHits 2 ＋ 2。
    assert_eq!((m.total_hits, m.session_count, m.truncated), (8, 4, true));
    assert_eq!(
        (m.unreadable, m.skipped.clone()),
        (3, vec!["Codex".to_string(), "Gemini".to_string()])
    );
    assert_eq!(m.down, down);
    assert_eq!(
        m.sessions[0].row.at_text, "at-rem-new",
        "那台写好的字原样带过来"
    );
    let empty = merge(vec![], vec![]);
    assert_eq!(
        (empty.total_hits, empty.session_count, empty.truncated),
        (0, 0, false)
    );
}

#[test]
fn integer_cells_of_a_real_reply_pass_through_as_integers() {
    let m = merge(
        vec![(
            Machine("pi".into()),
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
