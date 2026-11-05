//! 几台的需手动合成一份（`face_core::needs::merge_needs`）的判据：本机后端 `sessions-needs-merge` 与手机核心 `NeedsFace.merge` 调的是这一份。
//!
//! 1. 跨台只照各项的 `rank` 比；并列 ⇒ 本机在前、再照给的台序与台内原来的先后；每项带回它那台；`total` 是各台加起来几项。
//! 2. 会外读不出的那几台各带那一句（本机在前）；没问到的那几台原样带上、不计。
//! 3. 回包那一形照类型解得开（手机核心交 Kotlin 的就是这一形）。
//!
//! 每一例的几项写成计划那一臂（合法不看是哪一臂，只看 `rank`），期望手写。

use super::*;
use cmd_wire::cells::Words;
use cmd_wire::needs::{NeedsAt, NeedsDown, NeedsItem, NeedsList, NeedsMerged};
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
        let got = merge_needs(split(rows), vec![]);
        let names: Vec<String> = got.items.iter().map(name).collect();
        assert_eq!(&names, want, "{title}");
        assert_eq!(got.total as usize, rows.len(), "{title}：总数是各台加起来几项");
        for x in &got.items {
            let (m, ..) = rows.iter().find(|r| r.1 == name(x)).unwrap();
            assert_eq!(x.origin.to_string(), *m, "{title}：{} 带回的台不对", name(x));
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
        }],
    );
    assert_eq!(got.total, 2, "没问到的那台不计");
    let v = serde_json::to_value(&got).unwrap();
    assert_eq!(v["unreadable"], json!([{"origin": "devbox", "said": "读不懂"}]));
    assert_eq!(
        v["down"],
        json!([{"origin": "pi", "said": "那台没在跑"}]),
        "没问到的那几台原样带上"
    );
}

#[test]
fn the_merged_shape_reads_back() {
    let got = merge_needs(split(&[("<local>", "a", 1), ("devbox", "b", 2)]), vec![]);
    let text = serde_json::to_string(&got).unwrap();
    let back: NeedsMerged = serde_json::from_str(&text).expect("回包那一形解得开");
    assert_eq!(back, got);
}
