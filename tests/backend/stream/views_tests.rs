//! 请求信封的 `view`：登记的成品住处对得上注册表与格目录 · 点了这条命令不出的成品就拒 · 帧面那一跳真的拒、真的裁。

use super::*;
use serde_json::json;
use tokio::sync::mpsc;

#[test]
fn every_place_names_a_registered_command_and_a_catalogued_product() {
    use crate::faces::cells_catalog::PRODUCTS;
    let mut seen = std::collections::BTreeSet::new();
    for (cmd, places) in PLACES {
        assert!(seen.insert(*cmd), "`{cmd}` 登记了两遍");
        assert!(
            super::super::REGISTRY.iter().any(|s| s.name == *cmd),
            "`{cmd}` 不在注册表里"
        );
        assert!(!places.is_empty(), "`{cmd}` 登记了空住处");
        for (at, product) in *places {
            assert!(
                PRODUCTS.iter().any(|p| p.name == *product),
                "`{cmd}` 的 `{at}` 登记的成品 `{product}` 不在格目录里"
            );
            assert!(
                at.is_empty() || crate::faces::project::parse_path(at).is_some(),
                "`{cmd}` 的住处 `{at}` 写坏了"
            );
        }
    }
}

#[test]
fn a_view_naming_a_product_the_command_does_not_carry_is_refused() {
    let v = json!({"omit": {"facts": ["touchedFiles"]}});
    let f = plan_for("history-read", &v).expect_err("history-read 不出 facts");
    assert_eq!(f.code, "bad_args");
    assert!(
        f.raw.as_deref().is_some_and(|r| r.contains("facts")),
        "{f:?}"
    );
    // 没登记住处的命令：带什么都拒（不当没说）
    let f = plan_for("ping", &json!({"omit": {"record": ["id"]}})).expect_err("ping 不出成品");
    assert_eq!(f.code, "bad_args");
    // 认不出的格：拒，那一格写进复制详情
    let f = plan_for("history-page", &json!({"omit": {"record": ["blcoks"]}})).unwrap_err();
    assert_eq!(f.code, "bad_args");
    assert!(
        f.raw.as_deref().is_some_and(|r| r.contains("blcoks")),
        "{f:?}"
    );
    // 缺 ＝ 全量（任何命令都收）
    let p = plan_for("ping", &serde_json::Value::Null).unwrap();
    assert_eq!(p.apply(Some(json!({"a": 1}))), Some(json!({"a": 1})));
    // 认得、也出的 ⇒ 收
    assert!(plan_for(
        "history-facts",
        &json!({"omit": {"facts": ["touchedFiles"]}})
    )
    .is_ok());
}

/// 帧面那一跳：坏声明在开跑之前就回 `bad_args`（带着那条请求的 id），好声明照常分派。
#[tokio::test]
async fn the_frame_face_refuses_a_bad_view_before_running_the_command() {
    let (tx, mut rx) = mpsc::channel(super::super::REPLY_CHANNEL_CAPACITY);
    let input = concat!(
        r#"{"id":"v1","cmd":"ping","view":{"omit":{"record":["id"]}}}"#,
        "\n",
        r#"{"id":"v2","cmd":"history-read","args":{},"view":{"omit":{"record":["nope"]}}}"#,
        "\n",
        r#"{"id":"v3","cmd":"ping","view":null}"#,
        "\n",
    );
    let h = super::super::spawn(
        std::io::Cursor::new(input.as_bytes().to_vec()),
        tx,
        crate::stream::inbound::WatchDesk::new(|_| {
            tokio::sync::oneshot::channel::<Vec<crate::stream::wire::WatchFrom>>().1
        }),
        crate::stream::wire::HelloFlushed::for_tests(),
    );
    h.await.unwrap();
    let mut got = std::collections::BTreeMap::new();
    while let Some(f) = rx.recv().await {
        let v: serde_json::Value =
            serde_json::from_str(crate::stream::wire::to_line(&f).unwrap().trim()).unwrap();
        got.insert(v["id"].as_str().unwrap().to_string(), v);
    }
    for id in ["v1", "v2"] {
        assert_eq!(got[id]["ok"], false, "{id}: {}", got[id]);
        assert_eq!(got[id]["code"], "bad_args", "{id}: {}", got[id]);
    }
    assert_eq!(got["v3"]["ok"], true, "{}", got["v3"]);
}

/// 帧面那一跳：成功的应答照声明裁好再回（`spawn_handler` 接上了 `Plan`）。
#[tokio::test]
async fn the_frame_face_projects_a_successful_reply() {
    let (tx, mut rx) = mpsc::channel(super::super::REPLY_CHANNEL_CAPACITY);
    let running = super::super::Running::default();
    let plan = plan_for(
        "history-read",
        &json!({"omit": {"record": ["blocks[type=tool_use].input"]}, "cells": {"read_row": ["end", "record"]}}),
    )
    .unwrap();
    let mut req = crate::stream::wire::Request {
        id: "p1".into(),
        cmd: "history-read".into(),
        args: serde_json::Value::Null,
        within_ms: None,
        view: serde_json::Value::Null,
        tz: Default::default(),
        until: None,
    };
    req.args = json!({});
    super::super::spawn_handler(
        req,
        tx.clone(),
        running,
        |_r| async move {
            Ok(Some(json!({"rows": [{"end": 9, "hash": 1, "record": {"t": "reply", "id": "a", "blocks": [
                {"type": "tool_use", "id": "c", "name": "Bash", "input": {"command": "ls"}},
                {"type": "text", "text": "x"},
            ]}}], "next": 9, "eof": true})))
        },
        true,
        plan,
    )
    .await;
    drop(tx);
    let f = rx.recv().await.unwrap();
    let v: serde_json::Value =
        serde_json::from_str(crate::stream::wire::to_line(&f).unwrap().trim()).unwrap();
    assert_eq!(
        v["data"],
        json!({"rows": [{"end": 9, "record": {"t": "reply", "id": "a", "blocks": [
            {"type": "tool_use", "id": "c", "name": "Bash"},
            {"type": "text", "text": "x"},
        ]}}], "next": 9, "eof": true})
    );
}

// ═══ 流的声明（attach 行的 `view` · 起流 `--view`）═══════════════════════════════

/// 流的住处表：每个 kind 是真有的推送帧、每件成品在格目录里。
#[test]
fn every_stream_place_names_a_pushed_frame_and_a_catalogued_product() {
    use crate::faces::cells_catalog::PRODUCTS;
    for (kind, places) in STREAM_PLACES {
        assert!(
            crate::EMITS.contains(kind),
            "流的住处表点了 `{kind}`，后端不发这种帧"
        );
        for (_, product) in *places {
            assert!(
                PRODUCTS.iter().any(|p| p.name == *product),
                "`{kind}` 那一处的成品 `{product}` 不在格目录里"
            );
        }
    }
}

/// 推送帧照流的声明裁：`session_added` 去掉 `pid`、`line.record` 去掉正文块的字；`kind` 恒在；声明没点的帧与应答字节不变。
#[test]
fn the_stream_view_trims_pushed_frames_and_leaves_the_rest_alone() {
    use crate::stream::wire::{to_line, to_line_viewed, Frame};
    let view = StreamView::parse(&json!({
        "omit": {"session_added": ["pid"], "record": ["blocks[type=text].text"]},
    }))
    .unwrap()
    .expect("认得的声明");
    let added = Frame::SessionAdded {
        sid: "s".into(),
        agent_kind: None,
        liveness_confidence: None,
        background: false,
        attachable: None,
        cwd: None,
        project_dir: None,
        name: None,
        path: None,
        lines: None,
        activity: None,
        activity_text: crate::stream::wire::activity_cells(None).0,
        activity_tone: crate::stream::wire::activity_cells(None).1,
        waiting_for: None,
        container: None,
        pid: 42,
    };
    let full: serde_json::Value = serde_json::from_str(to_line(&added).unwrap().trim()).unwrap();
    assert_eq!(full["pid"], 42, "没声明的时候该有 pid");
    let cut: serde_json::Value =
        serde_json::from_str(to_line_viewed(&added, Some(&view)).unwrap().trim()).unwrap();
    assert!(cut.get("pid").is_none(), "声明去掉了 pid 却还在：{cut}");
    assert_eq!(cut["kind"], "session_added");
    assert_eq!(cut["sid"], "s");

    use crate::agents::record::{Block, Body, Record};
    let record = Record {
        agent: "claude".into(),
        id: "u1".into(),
        at: None,
        time_text: None,
        at_ms: None,
        body: Body::Said {
            who: crate::agents::UserText {
                speaker: crate::agents::Speaker::Human,
                text: "x".into(),
                pasted: Vec::new(),
            },
            blocks: vec![Block::Text { text: "x".into() }],
            results: Default::default(),
            cwd: None,
        },
    };
    let line = Frame::Line {
        session_id: "s".into(),
        path: "/p".into(),
        seq: 0,
        record: Some(record),
        cwd: None,
        byte_offset: 9,
        rid: None,
    };
    let cut: serde_json::Value =
        serde_json::from_str(to_line_viewed(&line, Some(&view)).unwrap().trim()).unwrap();
    assert_eq!(cut["kind"], "line");
    assert_eq!(cut["byte_offset"], 9);
    assert_eq!(cut["record"]["blocks"][0]["type"], "text");
    assert!(
        cut["record"]["blocks"][0].get("text").is_none(),
        "声明去掉了正文却还在：{cut}"
    );
    assert_eq!(cut["record"]["who"]["text"], "x", "没点到的格照发");

    let other = Frame::SessionsReplayed;
    assert_eq!(
        to_line_viewed(&other, Some(&view)).unwrap(),
        to_line(&other).unwrap()
    );
    assert_eq!(
        to_line_viewed(&added, None).unwrap(),
        to_line(&added).unwrap(),
        "没声明 ⇒ 字节与 to_line 一样"
    );
}

/// 流的声明只许点推送帧里住着的成品；认不出的词 · 格同请求信封那一处拒。
#[test]
fn the_stream_view_refuses_products_the_stream_does_not_carry() {
    assert_eq!(StreamView::parse(&serde_json::Value::Null), Ok(None));
    assert!(StreamView::parse(&json!({"omit": {"facts": ["usage"]}})).is_err());
    assert!(StreamView::parse(&json!({"omit": {"session_added": ["nope"]}})).is_err());
    assert!(StreamView::parse(&json!({"pick": {}})).is_err());
    assert!(
        StreamView::parse(&json!({"cells": {"session_status": ["sid"]}}))
            .unwrap()
            .is_some()
    );
}
