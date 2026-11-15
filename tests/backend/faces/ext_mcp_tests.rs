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

/// 一张表：本机 · 连着的 laptop · 没连上的那一台；`mcp_row` ⇒ 表里有一行 MCP。
fn list(mcp_row: bool) -> ExtList {
    let row = |kind: &str| json!({"kind": kind, "kindSaid": kind, "name": "x", "about": null, "detail": [], "new": false, "known": null, "note": null, "cells": []});
    let rows = if mcp_row {
        vec![row("skill"), row("mcp")]
    } else {
        vec![row("skill")]
    };
    serde_json::from_value(json!({
        "machines": [
            {"key": null, "here": true, "reachable": true, "name": "me"},
            {"key": "laptop", "here": false, "reachable": true, "name": "laptop"},
            {"key": null, "here": false, "reachable": false, "name": "gone"},
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
) -> impl Fn(
    String,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<serde_json::Value>> + Send>> {
    let answers: Vec<(String, serde_json::Value)> = answers
        .iter()
        .map(|(n, a)| (n.to_string(), a.clone()))
        .collect();
    let asked = asked.clone();
    move |m| {
        asked.lock().unwrap().push(m.clone());
        let got = answers
            .iter()
            .find(|(n, _)| *n == m)
            .map(|(_, a)| a.clone());
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
        "本机那一份就地读 · laptop 经它的常驻流问到 · 没连上的那台不问"
    );
    assert_eq!(
        *asked.lock().unwrap(),
        vec!["laptop".to_string()],
        "只问了够得着的那一台"
    );
}

#[tokio::test]
async fn a_machine_that_does_not_answer_leaves_its_cell_empty() {
    let asked: Asked = Arc::default();
    let got = with_mcp(
        list(true),
        || async { Some(reply("mine")) },
        there(&[], &asked),
    )
    .await;
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
    assert_eq!(
        *here_asked.lock().unwrap(),
        0,
        "表里没有 MCP 行 ⇒ 本机那一份也不读"
    );
}

/// 一条用户级 `mcp-read` 条目：名字 · 配置（`command` 那一格）· 不一致。那一句照真成品分两种：
/// 不一致 ⇒「〈配置 不一致〉」；一致 ⇒「〈在 配置〉」（只说在哪几个号里）。
fn held(name: &str, cfg: &str, mismatch: bool) -> serde_json::Value {
    let said = if mismatch {
        format!("〈{cfg} 不一致〉")
    } else {
        format!("〈在 {cfg}〉")
    };
    let mut e = json!({"scope": "user", "name": name, "server": {"command": cfg}, "held": {"said": said, "tip": null}});
    if mismatch {
        e["mismatch"] = json!(true);
    }
    e
}

/// 同 [`held`]，落在另一级（`local` · `project`）。
fn held_at(scope: &str, name: &str, cfg: &str, mismatch: bool) -> serde_json::Value {
    let mut e = held(name, cfg, mismatch);
    e["scope"] = json!(scope);
    e
}

fn entries(es: &[serde_json::Value]) -> serde_json::Value {
    json!({"entries": es, "dirs": [], "problems": []})
}

/// 表里 MCP 那几行投成 `(那一句, 各台挑第几条)`。
type Picked = (Option<String>, Option<Vec<Option<u32>>>);
fn mcp_rows(l: &ExtList) -> Vec<Picked> {
    l.rows
        .iter()
        .filter(|r| r.kind == crate::assets::asset_catalog::ExtKind::Mcp)
        .map(|r| (r.held_said.clone(), r.picks.clone()))
        .collect()
}

/// ★ 同名同配置两个号（`mcp-read` 已合成一条带两个号）⇒ 表里仍一行，不拆。
#[tokio::test]
async fn one_server_held_by_two_accounts_stays_one_row() {
    let asked: Asked = Arc::default();
    let mine = entries(&[held("x", "c1", false)]);
    let theirs = entries(&[held("x", "c2", false)]);
    let got = with_mcp(
        list(true),
        || async move { Some(mine) },
        there(&[("laptop", theirs)], &asked),
    )
    .await;
    assert_eq!(
        mcp_rows(&got),
        vec![(None, None)],
        "没有一台说不一致 ⇒ 不拆（各台各自的那一份照旧）"
    );
}

/// ★ 同名各号配置不一致 ⇒ 那一行拆成两行：各带那一份写好的那一句、各台挑哪一条（下标，没有那一份 ⇒ 空）；格子照抄。
#[tokio::test]
async fn differing_configs_split_into_one_row_each() {
    let asked: Asked = Arc::default();
    let mine = entries(&[held("x", "c1", true), held("x", "c2", true)]);
    let got = with_mcp(list(true), || async move { Some(mine) }, there(&[], &asked)).await;
    assert_eq!(
        mcp_rows(&got),
        vec![
            (
                Some("〈c1 不一致〉".into()),
                Some(vec![Some(0), None, None])
            ),
            (
                Some("〈c2 不一致〉".into()),
                Some(vec![Some(1), None, None])
            ),
        ]
    );
    let mcp: Vec<_> = got
        .rows
        .iter()
        .filter(|r| r.kind == crate::assets::asset_catalog::ExtKind::Mcp)
        .collect();
    assert_eq!(mcp[0].cells, mcp[1].cells);
    assert_ne!(mcp[0].variant, mcp[1].variant, "拆出的几行各有各的序号");
}

/// ★ 各台照（哪一级, 配置原文）对上同一行（不照各台读出的先后）：laptop 那份一致的 c2 落在用户级 c2 那一行；
/// 本机本地级的 c2 与用户级的 c2 各一行；行只照说不一致的那几份起（本地级 c2 在本机一致、在 laptop 不一致 ⇒ 起一行、两台都挑上），
/// 那一句取说不一致的那一条；项目级同名那一条不进拆出的行。
#[tokio::test]
async fn split_rows_pair_machines_by_scope_and_config() {
    let asked: Asked = Arc::default();
    let mine = entries(&[
        held("x", "c1", true),
        held("x", "c2", true),
        held_at("local", "x", "c2", false),
    ]);
    let theirs = entries(&[
        held_at("project", "x", "p", false),
        held("x", "c2", false),
        held_at("local", "x", "c2", true),
        held_at("local", "x", "c5", true),
    ]);
    let got = with_mcp(
        list(true),
        || async move { Some(mine) },
        there(&[("laptop", theirs)], &asked),
    )
    .await;
    assert_eq!(
        mcp_rows(&got),
        vec![
            (
                Some("〈c1 不一致〉".into()),
                Some(vec![Some(0), None, None])
            ),
            (
                Some("〈c2 不一致〉".into()),
                Some(vec![Some(1), Some(1), None])
            ),
            (
                Some("〈c2 不一致〉".into()),
                Some(vec![Some(2), Some(2), None])
            ),
            (
                Some("〈c5 不一致〉".into()),
                Some(vec![None, Some(3), None])
            ),
        ]
    );
}

/// ★ 只一级不一致：另一级那条一致的不起行、不被标成不一致（判只在 `mcp-read` 那一处，这里只照它说的拆）。
#[tokio::test]
async fn a_consistent_scope_gets_no_row_of_its_own() {
    let asked: Asked = Arc::default();
    let mine = entries(&[
        held("x", "c1", true),
        held("x", "c2", true),
        held_at("local", "x", "c9", false),
    ]);
    let got = with_mcp(list(true), || async move { Some(mine) }, there(&[], &asked)).await;
    assert_eq!(
        mcp_rows(&got),
        vec![
            (
                Some("〈c1 不一致〉".into()),
                Some(vec![Some(0), None, None])
            ),
            (
                Some("〈c2 不一致〉".into()),
                Some(vec![Some(1), None, None])
            ),
        ]
    );
}
