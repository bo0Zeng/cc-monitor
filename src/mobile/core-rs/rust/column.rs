//! **手机会话屏那一列**：勾上的那几台各一列 ⇒ 并成一列、分叉挂父、按段头切段（[`column`]）。
//! 怎么并（[`merge_by`]）· 分叉怎么挂（[`nest`]）是本机后端并历史页那一列同一份（`face_core::column`），这里只切手机那一屏的段：
//! 段头是那台写好的字，换了字就起新的一段；挂在父下的分叉跟着父，不另起段。形状导给 Kotlin（`ColumnFace`）。

use std::collections::BTreeSet;

use cmd_wire::cells::Words;
use cmd_wire::listing::CanDelete;
use face_core::column::{merge_by, nest};
use serde::{Deserialize, Serialize};

/// 手机会话屏交来的：勾上的那几台，照机器表先后。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ChatsColumnAsk {
    pub hosts: Vec<ColumnHost>,
    /// 展开了分叉的那几行。
    pub forks: Vec<ColumnFork>,
}

/// 第几台（[`ChatsColumnAsk::hosts`] 里的位置）上的一个会话。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ColumnFork {
    pub host: u32,
    pub sid: String,
}

/// 一台：给人看的称呼（手机机器表里那台的，删除确认那一句点名用）· 它的行（那台排好的先后）· 它的组的 `order`（那台排好的先后）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ColumnHost {
    pub label: String,
    pub rows: Vec<ColumnRow>,
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub groups: Vec<i64>,
}

/// 一行：并的键（那台给的 `at`）· 段头（那台写的；没有 ⇒ `null`）· 已摆进固定 · 组那几段（不再进按天那几段，挂在它下面的分叉也就没了父）·
/// 会话 id · 它分叉自哪个会话（那台写的；不是分叉 ⇒ `null`）· 能不能删（那台判的 `can.delete`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ColumnRow {
    #[cfg_attr(test, ts(type = "number"))]
    pub at: i64,
    pub section: Option<String>,
    pub placed: bool,
    pub sid: String,
    pub parent: Option<String>,
    pub delete: CanDelete,
}

/// 并好的：按天那几段 · 按目录时各台的组并成的一列 · 各台删除确认那几句（照 [`ChatsColumnAsk::hosts`] 先后）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ChatsColumn {
    pub sections: Vec<ColumnSection>,
    pub groups: Vec<ColumnAt>,
    pub asks: Vec<ColumnAsks>,
}

/// 一台的删除确认那几句，都照那台的称呼点名（手机这一端没有自己的记录；那台后端写的那一句不点名，不用）：
/// 那台的行一行一句（照 [`ColumnHost::rows`] 先后，摆进固定 · 组那几段的也有）· 组里有、清单里没有的那一条（能不能删说不准）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ColumnAsks {
    pub rows: Vec<Words>,
    pub unlisted: Words,
}

/// 按天的一段：段头（那台写的）· 那几行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ColumnSection {
    pub title: Option<String>,
    pub rows: Vec<ColumnLine>,
}

/// 段里的一行：第几台的第几项 ＋ 它在分叉树里的样子（`face_core::column::Nest`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ColumnLine {
    pub host: u32,
    pub at: u32,
    pub child: bool,
    pub forks: u32,
    pub forks_open: bool,
    pub orphan: bool,
    /// 「分叉 N」那颗的字（这一列里没有分叉自它的 ⇒ 缺；桌面历史页那一行同一句），手机照抄。
    pub forks_text: Option<String>,
}

/// 第几台（[`ChatsColumnAsk::hosts`] 里的位置）的第几项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ColumnAt {
    pub host: u32,
    pub at: u32,
}

/// 第几台的那一列 ⇒ 带上位置的那一列（并好之后还认得出是哪台第几项）。
fn placed_in<'a, T: 'a>(lists: impl Iterator<Item = &'a [T]>) -> Vec<Vec<(ColumnAt, &'a T)>> {
    let n = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
    lists
        .enumerate()
        .map(|(h, l)| {
            l.iter()
                .enumerate()
                .map(|(i, x)| {
                    (
                        ColumnAt {
                            host: n(h),
                            at: n(i),
                        },
                        x,
                    )
                })
                .collect()
        })
        .collect()
}

/// 手机会话屏那一列：行按 `at` 并、摆进固定 · 组的那几行不进；分叉挂父（`face_core::column::nest`，同一台）；
/// 顶层的行按段头连着的切成几段，挂在父下的跟着父；组按 `order` 并。
pub fn column(ask: &ChatsColumnAsk) -> ChatsColumn {
    let rows = placed_in(ask.hosts.iter().map(|h| h.rows.as_slice()));
    let row_lists: Vec<&[(ColumnAt, &ColumnRow)]> = rows.iter().map(Vec::as_slice).collect();
    let merged: Vec<&(ColumnAt, &ColumnRow)> = merge_by(&row_lists, |(_, r)| r.at)
        .into_iter()
        .filter(|(_, r)| !r.placed)
        .collect();
    let open: BTreeSet<(u32, String)> = ask.forks.iter().map(|f| (f.host, f.sid.clone())).collect();
    let mut sections: Vec<ColumnSection> = Vec::new();
    for ((at, r), n) in nest(
        &merged,
        |(at, r)| (at.host, r.sid.clone()),
        |(at, r)| r.parent.clone().map(|p| (at.host, p)),
        &open,
    ) {
        let line = ColumnLine {
            host: at.host,
            at: at.at,
            child: n.child,
            forks: n.forks,
            forks_open: n.forks_open,
            orphan: n.orphan,
            forks_text: (n.forks > 0)
                .then(|| copy_core::copy_text("history.row.forks", &[("n", &n.forks.to_string())])),
        };
        match sections.last_mut() {
            Some(s) if n.child || s.title == r.section => s.rows.push(line),
            _ => sections.push(ColumnSection {
                title: r.section.clone(),
                rows: vec![line],
            }),
        }
    }
    let groups = placed_in(ask.hosts.iter().map(|h| h.groups.as_slice()));
    let group_lists: Vec<&[(ColumnAt, &i64)]> = groups.iter().map(Vec::as_slice).collect();
    ChatsColumn {
        sections,
        groups: merge_by(&group_lists, |(_, o)| **o)
            .into_iter()
            .map(|(at, _)| *at)
            .collect(),
        asks: ask.hosts.iter().map(asks_of).collect(),
    }
}

/// 一台的删除确认那几句：照那台的称呼点名（`face_core::history_asks::delete_ask_named`，本机后端写清单行同一份）。
fn asks_of(h: &ColumnHost) -> ColumnAsks {
    let named = |d| face_core::history_asks::delete_ask_named(Some(&h.label), d);
    ColumnAsks {
        rows: h.rows.iter().map(|r| named(r.delete)).collect(),
        unlisted: named(CanDelete::Unsure),
    }
}

#[cfg(test)]
mod tests {
    //! 手机会话屏那一列与本机后端历史页并的同一份：照 `history-browse` 金样那三台（同一刻的行跨台都有 · 分叉挂父 · 孙辈 · 孤儿）
    //! 交 [`column`]，段与行的先后、分叉挂父、组的先后与金样里后端摊出的那一列（`expect`）逐条相同。
    use super::*;
    use serde_json::Value;

    fn golden() -> Value {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../tests/__fixtures__/history-browse.golden.json");
        serde_json::from_str(&std::fs::read_to_string(p).expect("金样")).expect("金样读不懂")
    }

    fn machines(g: &Value) -> Vec<Value> {
        g["machines"].as_array().expect("machines").clone()
    }

    fn s(v: &Value) -> String {
        v.as_str().unwrap_or_default().to_string()
    }

    /// 金样那几台交给手机核心的那一问：每行带 `sid` · 父（那台写的 `forkedFromSessionId`）；`forks` 照这几个展开（`(那台, 会话)`）；
    /// `away` 那几行摆进了固定 · 组那几段。
    fn ask(m: &[Value], forks: &[(String, String)], away: &dyn Fn(&str) -> bool) -> ChatsColumnAsk {
        let host_of = |o: &str| m.iter().position(|h| h["origin"] == o).map(|h| h as u32);
        ChatsColumnAsk {
            hosts: m
                .iter()
                .map(|h| ColumnHost {
                    label: s(&h["origin"]),
                    rows: h["rows"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|r| ColumnRow {
                            at: r["at"].as_i64().unwrap(),
                            section: r["sectionText"].as_str().map(str::to_string),
                            placed: away(r["sessionId"].as_str().unwrap()),
                            sid: s(&r["sessionId"]),
                            parent: r["forkedFromSessionId"].as_str().map(str::to_string),
                            delete: serde_json::from_value(r["can"]["delete"].clone()).unwrap(),
                        })
                        .collect(),
                    groups: h["head"]["groups"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|g| g["order"].as_i64().unwrap())
                        .collect(),
                })
                .collect(),
            forks: forks
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

    /// 并好的一列照金样 `expect` 那一形写成一条一条（段头 ＋ 行）。
    fn tokens(m: &[Value], c: &ChatsColumn) -> Vec<String> {
        c.sections
            .iter()
            .flat_map(|sec| {
                let head = format!("sec:{}", sec.title.clone().unwrap_or_default());
                std::iter::once(head).chain(sec.rows.iter().map(|l| {
                    let h = &m[l.host as usize];
                    let r = &h["rows"][l.at as usize];
                    let mut t = format!("r:{}|{}", s(&h["origin"]), s(&r["sessionId"]));
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
                    // 「分叉 N」那颗的字由核心写好（手机照抄）：有分叉 ⇒ 那一句，没有 ⇒ 缺。
                    assert_eq!(
                        l.forks_text,
                        (l.forks > 0).then(|| copy_core::copy_text(
                            "history.row.forks",
                            &[("n", &l.forks.to_string())]
                        )),
                        "{t}"
                    );
                    t
                }))
            })
            .collect()
    }

    fn local(sid: &str) -> (String, String) {
        (cmd_wire::LOCAL.to_string(), sid.to_string())
    }

    fn here(t: &str) -> String {
        format!("r:{}|{t}", cmd_wire::LOCAL)
    }

    /// ★ 按时间那几形逐条同金样（后端 `history-browse` 照它对拍）；组的先后同「按项目 · 全收着」那一形。
    #[test]
    fn the_phone_column_merges_like_the_backend() {
        let g = golden();
        let m = machines(&g);
        let cases = g["cases"].as_array().unwrap();
        let mut times = 0;
        for c in cases.iter().filter(|c| c["view"] == "time") {
            times += 1;
            let forks: Vec<(String, String)> = c["forks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| (s(&f["origin"]), s(&f["sid"])))
                .collect();
            let want: Vec<String> = serde_json::from_value(c["expect"].clone()).unwrap();
            assert!(want.len() > 10, "金样那几台的行少了：{want:?}");
            let got = column(&ask(&m, &forks, &|_| false));
            assert_eq!(tokens(&m, &got), want, "{}", c["name"]);
        }
        assert!(times >= 2, "金样里按时间那几形少了");
        let folded = cases
            .iter()
            .find(|c| {
                c["view"] == "project"
                    && c["forks"].as_array().unwrap().is_empty()
                    && c["groups"]["open"] == false
                    && c["groups"]["except"].as_array().unwrap().is_empty()
                    && c["query"] == ""
            })
            .expect("按项目 · 全收着");
        let want: Vec<String> = serde_json::from_value(folded["expect"].clone()).unwrap();
        let got = column(&ask(&m, &[], &|_| false));
        let groups: Vec<String> = got
            .groups
            .iter()
            .map(|p| {
                let h = &m[p.host as usize];
                format!(
                    "g:{}|{}:false",
                    s(&h["origin"]),
                    s(&h["head"]["groups"][p.at as usize]["key"])
                )
            })
            .collect();
        assert_eq!(groups, want, "组的先后");
    }

    /// ★ 分叉树：父收着 ⇒ 子不在任何一段里；父展开 ⇒ 子紧跟父、不另起段（分叉落在另一天也一样）；
    /// 父不在这一列（没读到 · 摆进固定或组那几段）⇒ 子自成一行、记孤儿。
    #[test]
    fn the_phone_column_nests_forks_under_their_parent() {
        let g = golden();
        let m = machines(&g);
        let shut = tokens(&m, &column(&ask(&m, &[], &|_| false)));
        for sid in ["k1", "g1", "k2"] {
            let me = here(sid);
            assert!(
                !shut
                    .iter()
                    .any(|t| t == &me || t.starts_with(&format!("{me}:"))),
                "父收着、{sid} 还出：{shut:?}"
            );
        }
        assert!(shut.contains(&here("a1:forks=1")), "{shut:?}");
        for t in [here("o1:orphan"), here("x1:orphan")] {
            assert!(shut.contains(&t), "{t}：{shut:?}");
        }
        let open = tokens(
            &m,
            &column(&ask(&m, &[local("a1"), local("k1")], &|_| false)),
        );
        let at = open
            .iter()
            .position(|t| *t == here("a1:forks=1(open)"))
            .expect("a1 展开那一行");
        assert_eq!(
            open[at + 1..at + 3],
            [here("k1:child:forks=1(open)"), here("g1:child")],
            "{open:?}"
        );
        let away = |sid: &str| sid == "k1" || sid == "b1";
        let got = tokens(&m, &column(&ask(&m, &[local("a1")], &away)));
        // k1 · b1 摆进别处 ⇒ 不在这一列；g1 没了父 ⇒ 孤儿、照它的 at 落；a1 底下没有分叉了（与后端去掉那两行摊出的同）。
        let want: Vec<String> = [
            "sec:今天",
            &here("a1"),
            "r:dev|d1",
            &here("g1:orphan"),
            &here("a2"),
            "sec:昨天",
            &here("o1:orphan"),
            &here("x1:orphan"),
            "r:dev|d2",
            "sec:九月",
            &here("a3"),
            &here("cx:forks=1"),
            "r:dev|d3",
        ]
        .iter()
        .map(|t| t.to_string())
        .collect();
        assert_eq!(got, want, "摆进别处的那两行");
        let mut later = m.clone();
        for h in later.iter_mut() {
            for r in h["rows"].as_array_mut().unwrap() {
                if r["sessionId"] == "g1" {
                    r["sectionText"] = "九月".into();
                }
            }
        }
        let got = tokens(
            &later,
            &column(&ask(&later, &[local("a1"), local("k1")], &|_| false)),
        );
        let want: Vec<String> = [
            "sec:今天",
            &here("a1:forks=1(open)"),
            &here("k1:child:forks=1(open)"),
            &here("g1:child"),
            "r:dev|d1",
            "r:box|b1",
            &here("a2"),
            "sec:昨天",
            &here("o1:orphan"),
            &here("x1:orphan"),
            "r:dev|d2",
            "sec:九月",
            &here("a3"),
            &here("cx:forks=1"),
            "r:dev|d3",
        ]
        .iter()
        .map(|t| t.to_string())
        .collect();
        assert_eq!(
            got, want,
            "分叉落在另一天：整列（与后端 history-browse 那一形摊出的同）"
        );
        let at = got.iter().position(|t| *t == here("g1:child")).expect("g1");
        assert_eq!(
            got[at - 1],
            here("k1:child:forks=1(open)"),
            "分叉落在另一天也跟着父：{got:?}"
        );
        let nine = got
            .iter()
            .position(|t| t == "sec:九月")
            .expect("九月那一段");
        assert!(at < nine, "g1 不进九月那一段：{got:?}");
    }

    /// ★ 删除确认那一句照那台的称呼点名（那台后端写的那一句不点名，手机不用它）：远端的行逐行点名，
    /// 摆进固定 · 组那几段的也有；说不准在不在跑的多一句；组里有、清单里没有的那一条照说不准写。
    #[test]
    fn the_phone_column_names_the_machine_in_the_delete_ask() {
        let row = |sid: &str, placed: bool, delete: CanDelete| ColumnRow {
            at: 1,
            section: None,
            placed,
            sid: sid.to_string(),
            parent: None,
            delete,
        };
        let host = |label: &str, rows: Vec<ColumnRow>| ColumnHost {
            label: label.to_string(),
            rows,
            groups: vec![],
        };
        let got = column(&ChatsColumnAsk {
            hosts: vec![
                host(
                    "gpu",
                    vec![
                        row("a", false, CanDelete::Yes),
                        row("b", true, CanDelete::Unsure),
                    ],
                ),
                host("盒子", vec![row("c", false, CanDelete::Live)]),
            ],
            forks: vec![],
        });
        let named = |m: &str, c| face_core::history_asks::delete_ask_named(Some(m), c);
        assert_eq!(
            got.asks,
            vec![
                ColumnAsks {
                    rows: vec![
                        named("gpu", CanDelete::Yes),
                        named("gpu", CanDelete::Unsure)
                    ],
                    unlisted: named("gpu", CanDelete::Unsure),
                },
                ColumnAsks {
                    rows: vec![named("盒子", CanDelete::Live)],
                    unlisted: named("盒子", CanDelete::Unsure),
                },
            ]
        );
        assert!(got.asks[0].rows[0].0.contains("gpu"), "{:?}", got.asks);
        assert_ne!(
            got.asks[0].rows[0],
            face_core::history_asks::delete_ask_named(None, CanDelete::Yes),
            "不点名那一句不进手机"
        );
    }
}
