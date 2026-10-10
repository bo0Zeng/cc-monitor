//! 通用投影的判据：路径写法 · 声明按格目录校验（认不出的不放过）· 挑格 / 去格 · 应答里逐处投影。

use super::*;
use serde_json::{json, Value};

fn view(v: Value) -> View {
    parse_view(&v).expect("声明应当认得").expect("不是空声明")
}

fn bad(v: Value) -> String {
    parse_view(&v).expect_err("声明应当被拒")
}

/// 真代码写出来的记录（跨语言金样，正文是占位）。
fn golden_records() -> Vec<Value> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/__fixtures__/record.golden.jsonl");
    std::fs::read_to_string(p)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).unwrap()["record"].clone())
        .collect()
}

/// 记录正文按需那一份（网络层调研 #2）：工具入参 · 工具结果正文 · 逐段改动。
fn body_on_demand() -> Value {
    json!({"omit": {"record": [
        "blocks[type=tool_use].input",
        "blocks[type=tool_result].content",
        "results.*.patch",
        "results.*.patchTruncated",
    ]}})
}

#[test]
fn paths_parse_in_the_catalog_spelling() {
    use Step::*;
    let k = |s: &str| Key(s.to_string());
    assert_eq!(parse_path("a.b"), Some(vec![k("a"), k("b")]));
    assert_eq!(
        parse_path("results.*.patch"),
        Some(vec![k("results"), All, k("patch")])
    );
    assert_eq!(parse_path("blocks[]"), Some(vec![k("blocks"), Each]));
    assert_eq!(
        parse_path("{t=said}.blocks[type=tool_result].content"),
        Some(vec![
            Where("t".into(), "said".into()),
            k("blocks"),
            EachWhere("type".into(), "tool_result".into()),
            k("content"),
        ])
    );
    assert_eq!(
        parse_path("answer{kind=picked}.options[]"),
        Some(vec![
            k("answer"),
            Where("kind".into(), "picked".into()),
            k("options"),
            Each
        ])
    );
    for broken in [
        "", ".a", "a.", "a..b", "[]", "a[b]", "a[=x]", "a[k=]", "a[]b", "a{k=v", "a[k=v",
    ] {
        assert_eq!(parse_path(broken), None, "{broken:?} 应当认不出");
    }
}

#[test]
fn every_catalog_cell_is_a_path_a_view_can_name() {
    // 目录里列出来的每一格，原样写进声明都得认（否则出口照目录写的声明会被拒）。
    for p in PRODUCTS {
        let paths: Vec<String> = cells_of(p).0.into_iter().map(|c| c.path).collect();
        let v = json!({"cells": {p.name: paths.clone()}, "omit": {p.name: paths}});
        assert!(
            parse_view(&v).is_ok(),
            "{}: {:?}",
            p.name,
            parse_view(&v).err()
        );
    }
}

#[test]
fn a_view_naming_what_the_catalog_lacks_is_refused_not_ignored() {
    // 认不出的词
    assert!(bad(json!({"where": {}})).contains("unknown word `where`"));
    // 认不出的成品
    assert!(bad(json!({"omit": {"recrod": ["id"]}})).contains("unknown product `recrod`"));
    // 认不出的格：拼错 · 往透传的一团里点 · 挑法的值目录里没见过 · 挑法对不上那一层 · 只有挑法没有格
    for p in [
        "blcoks",
        "blocks[type=tool_use].input.file_path",
        "blocks[type=tool_usage].input",
        "{t=title}.blocks",
        "{t=said}",
        "results[]",
    ] {
        let e = bad(json!({"omit": {"record": [p]}}));
        assert!(e.contains(&format!("unknown cell `{p}`")), "{p}: {e}");
    }
    // 一份声明里几处都错 ⇒ 每一处都说
    let e = bad(json!({"omit": {"record": ["nope1", "id", "nope2"]}}));
    assert!(
        e.contains("nope1") && e.contains("nope2") && !e.contains("`id`"),
        "{e}"
    );
    // 形状不对
    assert!(parse_view(&json!(["omit"])).is_err());
    assert!(parse_view(&json!({"omit": ["record"]})).is_err());
    assert!(parse_view(&json!({"omit": {"record": "id"}})).is_err());
    // 缺 ＝ 全量
    assert_eq!(parse_view(&Value::Null), Ok(None));
}

#[test]
fn a_path_without_the_pick_covers_every_kind_and_with_it_only_that_kind() {
    // `blocks` 在说的与回的两种记录里都有：不写挑法 ⇒ 两种都去；写了 ⇒ 只去那一种。
    let said = json!({"t": "said", "id": "u", "blocks": [{"type": "text", "text": "x"}]});
    let reply = json!({"t": "reply", "id": "a", "blocks": [{"type": "text", "text": "y"}]});
    let all = view(json!({"omit": {"record": ["blocks"]}}));
    let only_said = view(json!({"omit": {"record": ["{t=said}.blocks"]}}));
    let (mut s1, mut r1) = (said.clone(), reply.clone());
    project("record", &mut s1, &all);
    project("record", &mut r1, &all);
    assert_eq!(
        (s1, r1),
        (
            json!({"t": "said", "id": "u"}),
            json!({"t": "reply", "id": "a"})
        )
    );
    let (mut s2, mut r2) = (said, reply.clone());
    project("record", &mut s2, &only_said);
    project("record", &mut r2, &only_said);
    assert_eq!((s2, r2), (json!({"t": "said", "id": "u"}), reply));
}

#[test]
fn body_on_demand_drops_exactly_the_tool_bodies_and_keeps_the_rest() {
    let v = view(body_on_demand());
    let recs = golden_records();
    let mut hit = (0, 0, 0);
    for r in &recs {
        let mut got = r.clone();
        project("record", &mut got, &v);
        // 去掉的那几格真去掉了（删格，不是置空）
        for b in got["blocks"].as_array().into_iter().flatten() {
            match b["type"].as_str() {
                Some("tool_use") => assert!(b.get("input").is_none(), "{b}"),
                Some("tool_result") => assert!(b.get("content").is_none(), "{b}"),
                _ => {}
            }
        }
        for res in got["results"]
            .as_object()
            .into_iter()
            .flat_map(|o| o.values())
        {
            assert!(
                res.get("patch").is_none() && res.get("patchTruncated").is_none(),
                "{res}"
            );
        }
        // 别的一格不动：把去掉的几格从原样里也删掉，两边逐字相等
        let mut want = r.clone();
        if let Some(bs) = want.get_mut("blocks").and_then(Value::as_array_mut) {
            for b in bs {
                let o = b.as_object_mut().unwrap();
                match o.get("type").and_then(Value::as_str) {
                    Some("tool_use") => hit.0 += o.remove("input").is_some() as usize,
                    Some("tool_result") => hit.1 += o.remove("content").is_some() as usize,
                    _ => {}
                }
            }
        }
        if let Some(rs) = want.get_mut("results").and_then(Value::as_object_mut) {
            for res in rs.values_mut() {
                let o = res.as_object_mut().unwrap();
                hit.2 += o.remove("patch").is_some() as usize;
                o.remove("patchTruncated");
            }
        }
        assert_eq!(got, want);
    }
    // 金样里三样都真有（判据不是空跑）
    assert!(hit.0 > 0 && hit.1 > 0 && hit.2 > 0, "{hit:?}");
}

#[test]
fn cells_keeps_only_the_named_cells_and_the_layers_leading_to_them() {
    let v = view(
        json!({"cells": {"record": ["id", "t", "{t=said}.who.text", "blocks[type=text].text"]}}),
    );
    let mut said = json!({
        "t": "said", "id": "u", "at": "x", "timeText": "09:30",
        "who": {"text": "hi", "speaker": {"kind": "human"}},
        "blocks": [{"type": "text", "text": "hi"}, {"type": "tool_result", "for": "c", "content": []}],
    });
    project("record", &mut said, &v);
    assert_eq!(
        said,
        json!({"t": "said", "id": "u", "who": {"text": "hi"}, "blocks": [{"text": "hi"}]})
    );
    // 挑法对不上的那一种：只剩没挑法的那几格
    let mut title = json!({"t": "title", "id": "x", "text": "T", "by": "agent"});
    project("record", &mut title, &v);
    assert_eq!(title, json!({"t": "title", "id": "x"}));
    // 先挑后去
    let both = view(json!({"cells": {"record": ["id", "t"]}, "omit": {"record": ["t"]}}));
    let mut r = json!({"t": "title", "id": "x", "text": "T"});
    project("record", &mut r, &both);
    assert_eq!(r, json!({"id": "x"}));
}

#[test]
fn ending_on_a_list_or_table_drops_the_cell_itself_not_leaves_it_empty() {
    let v = view(json!({"omit": {"record": ["results.*", "blocks[]"]}}));
    let mut r = json!({"t": "said", "id": "u", "results": {"c": {"ok": true}}, "blocks": [{"type": "text", "text": "x"}]});
    project("record", &mut r, &v);
    assert_eq!(r, json!({"t": "said", "id": "u"}));
    // 按判别格挑的那几项：只删那几项
    let v = view(json!({"omit": {"record": ["blocks[type=thinking]"]}}));
    let mut r = json!({"t": "reply", "id": "a", "blocks": [{"type": "thinking", "text": "p"}, {"type": "text", "text": "x"}]});
    project("record", &mut r, &v);
    assert_eq!(r["blocks"], json!([{"type": "text", "text": "x"}]));
}

#[test]
fn a_reply_is_projected_at_every_place_its_command_registers() {
    // `history-read` 那一形：行摘要里装着记录。
    let places = [("rows[]", "read_row"), ("rows[].record", "record")];
    let mut reply = json!({"rows": [
        {"end": 10, "hash": 1, "cwd": "/w", "record": {"t": "reply", "id": "a", "blocks": [{"type": "tool_use", "id": "c", "name": "Bash", "input": {"command": "ls"}}]}},
        {"end": 20, "hash": 2},
    ], "next": 20, "eof": true});
    let v = view(
        json!({"cells": {"read_row": ["end", "record"]}, "omit": {"record": ["blocks[type=tool_use].input"]}}),
    );
    project_reply(&mut reply, &places, &v);
    assert_eq!(
        reply,
        json!({"rows": [
            {"end": 10, "record": {"t": "reply", "id": "a", "blocks": [{"type": "tool_use", "id": "c", "name": "Bash"}]}},
            {"end": 20},
        ], "next": 20, "eof": true})
    );
    // 整份应答就是那件成品（`history-facts`）
    let mut facts = json!({"end": 3, "touchedFiles": ["/a"], "permissionMode": null});
    project_reply(
        &mut facts,
        &[("", "facts")],
        &view(json!({"omit": {"facts": ["touchedFiles"]}})),
    );
    assert_eq!(facts, json!({"end": 3, "permissionMode": null}));
}
