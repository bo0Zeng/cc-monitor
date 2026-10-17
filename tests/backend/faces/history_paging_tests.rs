//! 8a b：分页的那四条（`history-index` · `-user-inputs` · `-turns` · `-branch`）一条应答至多 [`REPLY_PAGE_BYTES`] 字节，
//! 照 `next` 一页一页接着问，拼起来与一页装下时逐项相同；`limit` 生效；一项单独比上限大 ⇒ 那一页只有它。
//!
//! 夹具只造结构（uuid · 父子链 · 行长），正文是占位字。

use super::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("r8ab-paging-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("projects").join("-p")).unwrap();
    d
}

/// 一份 `turns` 轮的会话：每轮一句 user ＋ 一句 assistant（`end_turn`），每隔一轮先说一句再回退（同一个父亲另起一枝）
/// ⇒ 主线外清单每隔一轮多两条。`pad` ＝ 每句正文的占位长度。
fn fixture(home: &Path, sid: &str, turns: usize, pad: usize) -> String {
    let mut out = String::new();
    let mut parent: Option<String> = None;
    let mut t = 0u64;
    let mut stamp = || {
        t += 1;
        format!("2026-10-12T00:{:02}:{:02}.000Z", (t / 60) % 60, t % 60)
    };
    let body = "x".repeat(pad);
    let mut line = |v: Value| {
        out.push_str(&v.to_string());
        out.push('\n');
    };
    for k in 0..turns {
        if k % 2 == 1 {
            // 先说一句、答一句，再回退到同一个父亲另起一枝。
            let (u, a) = (format!("dead-u-{k}"), format!("dead-a-{k}"));
            line(
                json!({"type":"user","uuid":u,"parentUuid":parent,"timestamp":stamp(),"message":{"role":"user","content":format!("q{k} {body}")}}),
            );
            line(
                json!({"type":"assistant","uuid":a,"parentUuid":u,"timestamp":stamp(),"message":{"role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":body}]}}),
            );
        }
        let (u, a) = (format!("u-{k}"), format!("a-{k}"));
        line(
            json!({"type":"user","uuid":u,"parentUuid":parent,"timestamp":stamp(),"message":{"role":"user","content":format!("p{k} {body}")}}),
        );
        line(
            json!({"type":"assistant","uuid":a,"parentUuid":u,"timestamp":stamp(),"message":{"role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":body}]}}),
        );
        parent = Some(a);
    }
    let p = home
        .join("projects")
        .join("-p")
        .join(format!("{sid}.jsonl"));
    std::fs::write(&p, &out).unwrap();
    p.to_string_lossy().into_owned()
}

/// 一条应答那一行的字节数（同生产写出去的那一形：回包正文 ＋ 信封余量）。
fn line_len<T: serde::Serialize>(v: &T) -> usize {
    serde_json::to_vec(v).unwrap().len() + ENVELOPE_BYTES
}

/// 照 `next` 一页一页问到底；回：每一页（序列化成值）。
fn pages<T: serde::Serialize>(
    cap: usize,
    mut ask: impl FnMut(Option<u64>) -> T,
    next_of: impl Fn(&T) -> Option<u64>,
) -> Vec<Value> {
    let mut out = Vec::new();
    let mut at = None;
    loop {
        let r = ask(at);
        assert!(
            line_len(&r) <= cap,
            "一条应答 {} 字节，过了上限 {cap}",
            line_len(&r)
        );
        at = next_of(&r);
        out.push(serde_json::to_value(&r).unwrap());
        if at.is_none() || out.len() > 10_000 {
            break;
        }
    }
    out
}

const CAP: usize = 4 << 10;

#[test]
fn every_paged_reply_fits_the_cap_and_the_pages_add_up_to_the_whole() {
    let home = scratch("whole");
    let path = fixture(&home, "s-whole", 120, 300);
    let tz = crate::Tz::default();

    // 骨架索引：从 0 起（扫描图）· 照 next 接着问（图留着 ⇒ 切）；拼起来的逐行 == 一页装下的那一份。
    let whole = index_page(
        &home,
        HistoryIndexReq {
            path: path.clone(),
            offset: None,
            until: None,
            limit: None,
        },
        usize::MAX,
    )
    .unwrap();
    assert!(whole.next.is_none());
    let ps = pages(
        CAP,
        |at| {
            index_page(
                &home,
                HistoryIndexReq {
                    path: path.clone(),
                    offset: at,
                    until: None,
                    limit: None,
                },
                CAP,
            )
            .unwrap()
        },
        |r| r.next,
    );
    assert!(ps.len() > 3, "夹具要够分好几页：{} 页", ps.len());
    let mut rows = Vec::new();
    for (i, p) in ps.iter().enumerate() {
        if i + 1 < ps.len() {
            assert_eq!(p["end"], p["next"], "不是最后一页 ⇒ end 就是下一页的起点");
            assert_eq!(ps[i + 1]["from"], p["next"]);
        }
        rows.extend(crate::faces::read_face::tests::rows_of_index(p));
    }
    assert_eq!(
        rows,
        crate::faces::read_face::tests::rows_of_index(&serde_json::to_value(&whole).unwrap())
    );
    assert_eq!(ps.last().unwrap()["end"].as_u64(), Some(whole.end));

    // 用户输入。
    let whole = user_inputs_page(
        &home,
        UserInputsReq {
            path: path.clone(),
            from: None,
            limit: None,
        },
        usize::MAX,
    )
    .unwrap();
    let ps = pages(
        CAP,
        |at| {
            user_inputs_page(
                &home,
                UserInputsReq {
                    path: path.clone(),
                    from: at,
                    limit: None,
                },
                CAP,
            )
            .unwrap()
        },
        |r| r.next,
    );
    assert!(ps.len() > 1);
    let got: Vec<Value> = ps
        .iter()
        .flat_map(|p| p["entries"].as_array().unwrap().clone())
        .collect();
    assert_eq!(
        Value::Array(got),
        serde_json::to_value(&whole.entries).unwrap()
    );

    // 轮次。
    let whole = turns_page(
        &home,
        HistoryTurnsReq {
            path: path.clone(),
            from: None,
            limit: None,
        },
        &tz,
        usize::MAX,
    )
    .unwrap();
    let ps = pages(
        CAP,
        |at| {
            turns_page(
                &home,
                HistoryTurnsReq {
                    path: path.clone(),
                    from: at,
                    limit: None,
                },
                &tz,
                CAP,
            )
            .unwrap()
        },
        |r| r.next,
    );
    assert!(ps.len() > 1);
    let got: Vec<Value> = ps
        .iter()
        .flat_map(|p| p["turns"].as_array().unwrap().clone())
        .collect();
    assert_eq!(
        Value::Array(got),
        serde_json::to_value(&whole.turns).unwrap()
    );

    // 主线外清单（续点是条数）。
    let whole = branch_page(
        &home,
        BranchReq {
            path: path.clone(),
            from: None,
            limit: None,
        },
        usize::MAX,
    )
    .unwrap();
    assert!(
        whole.off.len() >= 100,
        "夹具的主线外清单该有一百多条：{}",
        whole.off.len()
    );
    let ps = pages(
        CAP / 8,
        |at| {
            branch_page(
                &home,
                BranchReq {
                    path: path.clone(),
                    from: at,
                    limit: None,
                },
                CAP / 8,
            )
            .unwrap()
        },
        |r| r.next,
    );
    assert!(ps.len() > 1);
    let got: Vec<Value> = ps
        .iter()
        .flat_map(|p| p["off"].as_array().unwrap().clone())
        .collect();
    assert_eq!(Value::Array(got), serde_json::to_value(&whole.off).unwrap());
}

/// 从中间起、记录又长了（留着的那张图对不上）⇒ 接着扫追加的那一截再切：拼起来照样等于整份。
#[test]
fn pages_from_the_middle_after_the_record_grew_still_add_up() {
    let home = scratch("file");
    let path = fixture(&home, "s-file", 80, 200);
    let tz = crate::Tz::default();
    let first = index_page(
        &home,
        HistoryIndexReq {
            path: path.clone(),
            offset: None,
            until: None,
            limit: None,
        },
        CAP,
    )
    .unwrap();
    let mid = first.next.expect("要分页");
    // 文件长一行 ⇒ 留着的图对不上 ⇒ 第 2 页起接着扫追加的那一行再切。
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    std::io::Write::write_all(
        &mut f,
        b"{\"type\":\"system\",\"uuid\":\"tail\",\"timestamp\":\"2026-10-12T01:00:00.000Z\"}\n",
    )
    .unwrap();
    let rest = pages(
        CAP,
        |at| {
            index_page(
                &home,
                HistoryIndexReq {
                    path: path.clone(),
                    offset: Some(at.unwrap_or(mid)),
                    until: None,
                    limit: None,
                },
                CAP,
            )
            .unwrap()
        },
        |r| r.next,
    );
    let whole = index_page(
        &home,
        HistoryIndexReq {
            path: path.clone(),
            offset: Some(mid),
            until: None,
            limit: None,
        },
        usize::MAX,
    )
    .unwrap();
    let mut rows = Vec::new();
    for p in &rest {
        rows.extend(crate::faces::read_face::tests::rows_of_index(p));
    }
    assert_eq!(
        rows,
        crate::faces::read_face::tests::rows_of_index(&serde_json::to_value(&whole).unwrap())
    );
    let t = turns_page(
        &home,
        HistoryTurnsReq {
            path: path.clone(),
            from: Some(mid),
            limit: Some(3),
        },
        &tz,
        usize::MAX,
    )
    .unwrap();
    assert_eq!(t.turns.len(), 3, "limit 管一页几轮");
    assert_eq!(t.next, Some(t.end));
}

/// 从中间续取时扫描图在不在，答案都一样：两份逐字节相同的记录，一份冷开过（图在）、一份没开过（图不在），
/// 从同一处起问用户输入与轮次 —— 回退掉的那几句（主线外）两边都不列。
#[test]
fn a_page_from_the_middle_is_the_same_whether_the_scan_map_is_held_or_not() {
    let home = scratch("held");
    let held = fixture(&home, "s-held", 40, 50);
    let cold = fixture(&home, "s-cold", 40, 50);
    let tz = crate::Tz::default();
    let inputs = |path: &str, from: Option<u64>| {
        user_inputs_page(
            &home,
            UserInputsReq {
                path: path.to_string(),
                from,
                limit: None,
            },
            usize::MAX,
        )
        .unwrap()
    };
    let turns = |path: &str, from: Option<u64>| {
        turns_page(
            &home,
            HistoryTurnsReq {
                path: path.to_string(),
                from,
                limit: None,
            },
            &tz,
            usize::MAX,
        )
        .unwrap()
    };
    let whole = inputs(&held, None);
    let _ = turns(&held, None);
    let mid = whole.entries[whole.entries.len() / 2].at;
    let (a, b) = (inputs(&held, Some(mid)), inputs(&cold, Some(mid)));
    assert!(
        a.entries.iter().all(|r| !r.uuid.starts_with("dead-")),
        "图在：回退掉的不列"
    );
    assert_eq!(
        serde_json::to_value(&b.entries).unwrap(),
        serde_json::to_value(&a.entries).unwrap(),
        "图不在时续取的用户输入与图在时不一样"
    );
    assert_eq!((b.end, b.next), (a.end, a.next));
    let (a, b) = (turns(&held, Some(mid)), turns(&cold, Some(mid)));
    assert_eq!(
        serde_json::to_value(&b.turns).unwrap(),
        serde_json::to_value(&a.turns).unwrap(),
        "图不在时续取的轮次与图在时不一样"
    );
    assert_eq!((b.end, b.next), (a.end, a.next));
}

#[test]
fn limit_caps_the_rows_of_one_page() {
    let home = scratch("limit");
    let path = fixture(&home, "s-limit", 20, 10);
    let r = index_page(
        &home,
        HistoryIndexReq {
            path: path.clone(),
            offset: None,
            until: None,
            limit: Some(7),
        },
        usize::MAX,
    )
    .unwrap();
    assert_eq!(r.count, 7);
    let r2 = index_page(
        &home,
        HistoryIndexReq {
            path: path.clone(),
            offset: r.next,
            until: None,
            limit: Some(7),
        },
        usize::MAX,
    )
    .unwrap();
    assert_eq!(r2.from, r.end);
    let b = branch_page(
        &home,
        BranchReq {
            path,
            from: Some(2),
            limit: Some(3),
        },
        usize::MAX,
    )
    .unwrap();
    assert_eq!((b.off.len(), b.next), (3, Some(5)));
}

/// 一项单独就比上限大 ⇒ 这一页只装它（不切一项），下一页接着走；别的页照旧 ≤ 上限。
#[test]
fn an_item_bigger_than_the_cap_goes_alone_on_its_page() {
    let home = scratch("alone");
    let path = fixture(&home, "s-alone", 6, 10);
    let tz = crate::Tz::default();
    let cap = ENVELOPE_BYTES + 64; // 比任何一轮都小
    let mut at = None;
    let mut n = 0;
    loop {
        let r = turns_page(
            &home,
            HistoryTurnsReq {
                path: path.clone(),
                from: at,
                limit: None,
            },
            &tz,
            cap,
        )
        .unwrap();
        assert_eq!(r.turns.len(), 1, "装不下一项的上限下，一页恰好一项");
        n += 1;
        at = r.next;
        if at.is_none() {
            break;
        }
    }
    assert_eq!(n, 6);
}
