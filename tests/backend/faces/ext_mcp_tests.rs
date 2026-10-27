//! `ext-list` 带各台 MCP 那一层（[`super::with_mcp`]）的判据：界面只问本机后端一条 `ext-list`，
//! 各台那一格 `mcp` 由本机后端问到（这里喂假的「本机这一份」与「问那台」，记下问了谁）。

use super::with_mcp;
use crate::assets::ext::ExtList;
use serde_json::json;
use std::sync::{Arc, Mutex};

type Asked = Arc<Mutex<Vec<String>>>;

fn reply(name: &str) -> serde_json::Value {
    json!({"entries": [{"name": name}], "dirs": [], "problems": []})
}

/// 一张表：本机 · 连着的 laptop · 连不上的那一台；`mcp_row` ⇒ 表里有一行 MCP。
fn list(mcp_row: bool) -> ExtList {
    let row = |kind: &str| {
        json!({"kind": kind, "kindSaid": kind, "name": "x", "about": null, "detail": [], "new": false, "known": null, "note": null, "cells": []})
    };
    let rows = if mcp_row { vec![row("skill"), row("mcp")] } else { vec![row("skill")] };
    serde_json::from_value(json!({
        "machines": [
            {"key": null, "here": true, "reachable": true, "name": "me", "projects": []},
            {"key": "laptop", "here": false, "reachable": true, "name": "laptop", "projects": []},
            {"key": null, "here": false, "reachable": false, "name": "gone", "projects": []},
        ],
        "rows": rows,
        "problems": [],
        "legend": [],
        "knownTitle": "",
    }))
    .unwrap()
}

/// 假的「问那台」：`answers` 里有那台 ⇒ 答那一份，没有 ⇒ 问不到。问了谁记进 `asked`。
fn there(
    answers: &[(&str, serde_json::Value)],
    asked: &Asked,
) -> impl Fn(String) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<serde_json::Value>> + Send>>
{
    let answers: Vec<(String, serde_json::Value)> =
        answers.iter().map(|(n, a)| (n.to_string(), a.clone())).collect();
    let asked = asked.clone();
    move |m| {
        asked.lock().unwrap().push(m.clone());
        let got = answers.iter().find(|(n, _)| *n == m).map(|(_, a)| a.clone());
        Box::pin(async move { got })
    }
}

fn mcp_of(l: &ExtList) -> Vec<Option<serde_json::Value>> {
    l.machines.iter().map(|m| m.mcp.clone()).collect()
}

#[tokio::test]
async fn each_reachable_machine_gets_its_own_mcp_read_and_this_one_reads_its_own() {
    let asked: Asked = Arc::default();
    let got = with_mcp(
        list(true),
        || async { Some(reply("mine")) },
        there(&[("laptop", reply("theirs"))], &asked),
    )
    .await;
    assert_eq!(
        mcp_of(&got),
        vec![Some(reply("mine")), Some(reply("theirs")), None],
        "本机那一份就地读 · laptop 经它的常驻流问到 · 连不上的那台不问"
    );
    assert_eq!(*asked.lock().unwrap(), vec!["laptop".to_string()], "只问了够得着的那一台");
}

#[tokio::test]
async fn a_machine_that_does_not_answer_leaves_its_cell_empty() {
    let asked: Asked = Arc::default();
    let got = with_mcp(list(true), || async { Some(reply("mine")) }, there(&[], &asked)).await;
    assert_eq!(mcp_of(&got), vec![Some(reply("mine")), None, None]);
}

#[tokio::test]
async fn no_mcp_row_asks_nobody() {
    let asked: Asked = Arc::default();
    let here_asked = Arc::new(Mutex::new(0));
    let seen = here_asked.clone();
    let got = with_mcp(
        list(false),
        move || {
            *seen.lock().unwrap() += 1;
            async { Some(reply("mine")) }
        },
        there(&[("laptop", reply("theirs"))], &asked),
    )
    .await;
    assert_eq!(mcp_of(&got), vec![None, None, None]);
    assert!(asked.lock().unwrap().is_empty());
    assert_eq!(*here_asked.lock().unwrap(), 0, "表里没有 MCP 行 ⇒ 本机那一份也不读");
}
