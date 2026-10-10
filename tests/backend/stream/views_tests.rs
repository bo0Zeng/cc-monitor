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
