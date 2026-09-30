//! 〔FILES2 · 第四波 · 2026-09-27〕`filewin/cross_copy.rs` 的判据 —— **复制到另一台机器**。
//!
//! 要求住址：`设计/60 §6.2`「跨机复制」· `§7` 第 9 条 Q2；主会话 09-27 按通行做法裁：「一个任务『从 A 下到本机暂存 → 传到 B 的暂存 →
//! B 那台提交』（经本机中转），一条进度、可撤、半路失败清暂存」。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`a_cross_copy_pulls_from_a_pushes_to_b_and_cleans_the_local_staging`] | 线上每一步去**哪台**（A / 本机 / B）与载荷：本机暂存落点 == 下的落点 == 传的源、B 提交落在 B 的目录、本机暂存件删掉 | 实得是合成对端按 `origin` 记下的 |
//! | [`a_failed_push_removes_the_staging_on_b_too`] | 传 B 那一腿失败 ⇒ 结局是失败、B 那头开过单的暂存件删掉、本机的也删掉 | 同上 |
//! | [`the_target_dir_defaults_to_home_and_must_be_absolute`] | 目标目录：空 ⇒ B 的 home；不是 `/` 起头 ⇒ 拒 | 期望手写 |
//! | [`the_local_origin_is_the_app_one`] | 窗口那份 `<local>` == app 侧 `inbound_client::LOCAL_ORIGIN` | 读两侧源码 |
//! | [`the_staging_dir_is_the_backend_one`] | 窗口那份暂存区（清 B 那头暂存件用）== 后端 `files_commit::STAGING_DIR` | 读两侧源码 |

use super::*;
use chan_core::chan::wire::{Body, By, CallError, CancelToken, Cursor, Item, Kind, Op, Origin};

type Log = std::sync::Arc<std::sync::Mutex<Vec<(String, String, serde_json::Value)>>>;

struct Rig {
    log: Log,
    push_fails: bool,
}

fn refused(code: &str) -> CallError {
    let body = serde_json::to_vec(&serde_json::json!({ "code": code, "message": "合成" }))
        .unwrap_or_default();
    chan_core::chan::wire::err_from_wire(chan_core::chan::wire::WireErr::Refused, body)
}

impl chan_core::chan::router::Backends for Rig {
    fn call(
        &self,
        origin: Origin,
        op: Op,
        payload: Body,
        _left: std::time::Duration,
        _cancel: CancelToken,
    ) -> futures::future::BoxFuture<'static, Result<Body, CallError>> {
        let args: serde_json::Value = serde_json::from_slice(&payload.0).unwrap_or_default();
        self.log
            .lock()
            .unwrap()
            .push((origin.0.clone(), op.0.clone(), args));
        let answer = match (origin.0.as_str(), op.0.as_str()) {
            (o, "files-home") => Ok(
                serde_json::json!({ "path": match o { "A" => "/home/a", "B" => "/home/b", _ => "/home/me" } }),
            ),
            ("B", "files-stat") => Err(refused("not_found")),
            (_, "transfer-download") => Ok(serde_json::json!({ "id": "x-down" })),
            (_, "transfer-upload") => {
                Ok(serde_json::json!({ "id": "x-up", "key": "k".repeat(32) }))
            }
            _ => Ok(serde_json::json!({ "path": "/x", "bytes": 5 })),
        };
        Box::pin(async move { answer.map(|v| Body(serde_json::to_vec(&v).unwrap_or_default())) })
    }

    fn subscribe(
        &self,
        origin: Origin,
        kind: Kind,
        _from: Option<Cursor>,
    ) -> futures::stream::BoxStream<'static, Item> {
        use futures::StreamExt as _;
        self.log.lock().unwrap().push((
            origin.0.clone(),
            "subscribe".into(),
            serde_json::json!(kind.0),
        ));
        let end = if kind.0.ends_with("x-up") && self.push_fails {
            serde_json::json!({ "state": "failed", "why": "断网（合成）" })
        } else {
            serde_json::json!({ "state": "done", "bytes": 5, "sha256": "5a".repeat(32) })
        };
        futures::stream::iter([
            Item::Frame {
                seq: 1,
                body: Body(br#"{"got":5,"total":5}"#.to_vec()),
            },
            Item::Closed {
                by: By::Peer(Body(serde_json::to_vec(&end).unwrap_or_default())),
            },
        ])
        .boxed()
    }
}

async fn rig(push_fails: bool) -> (crate::source::Line, Log) {
    let log: Log = Default::default();
    let h = chan_core::chan::handoff::start_with(
        std::sync::Arc::new(Rig {
            log: log.clone(),
            push_fails,
        }),
        chan_core::chan::handoff::mint_key(),
        1 << 20,
        std::time::Duration::from_secs(5),
    )
    .await
    .expect("回环口绑得上");
    let line = chan_core::chan::dial::dial(
        &h,
        chan_core::chan::wire::Budget {
            until: std::time::Instant::now() + std::time::Duration::from_secs(5),
            cancel: CancelToken::new(),
        },
    )
    .await
    .expect("拨得通");
    (line, log)
}

fn steps(log: &Log) -> Vec<(String, String)> {
    log.lock()
        .unwrap()
        .iter()
        .map(|(o, s, _)| (o.clone(), s.clone()))
        .collect()
}

fn args_of(log: &Log, origin: &str, op: &str) -> Vec<serde_json::Value> {
    log.lock()
        .unwrap()
        .iter()
        .filter(|(o, s, _)| o == origin && s == op)
        .map(|(_, _, a)| a.clone())
        .collect()
}

async fn go(push_fails: bool) -> (Outcome, Log) {
    go_to("B", push_fails).await
}

async fn go_to(machine: &str, push_fails: bool) -> (Outcome, Log) {
    let (line, log) = rig(push_fails).await;
    let board = CrossBoard::default();
    let o = run(
        &line,
        &Origin("A".into()),
        &crate::source::RemotePath::plain("/srv/a.bin"),
        "a.bin",
        machine,
        "/data",
        &board,
        |_| async { true },
    )
    .await;
    (o, log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cross_copy_pulls_from_a_pushes_to_b_and_cleans_the_local_staging() {
    let (o, log) = go(false).await;
    assert!(
        matches!(&o, Outcome::Done { path, machine, .. } if path == "/data/a.bin" && machine == "B"),
        "{o:?}"
    );
    let s = steps(&log);
    let pos = |o: &str, op: &str| {
        s.iter()
            .position(|(a, b)| a == o && b == op)
            .unwrap_or_else(|| panic!("没有 {o} {op}：{s:?}"))
    };
    assert!(
        pos("B", "files-home") < pos("<local>", "files-home"),
        "先问 B 再备本机暂存"
    );
    assert!(
        pos("A", "transfer-download") < pos("B", "transfer-upload"),
        "先下后传"
    );
    assert!(
        pos("B", "files-commit-upload") < pos("<local>", "files-delete"),
        "提交之后才清本机暂存"
    );
    let dl = &args_of(&log, "A", "transfer-download")[0];
    assert_eq!(dl["remote_path"], "/srv/a.bin");
    let staged = dl["local_path"].as_str().expect("落点").to_string();
    assert!(
        staged.starts_with("/home/me/.cc-monitor/staging/") && staged.ends_with(".part"),
        "本机暂存落点不在本机后端的暂存区：{staged}"
    );
    assert_eq!(
        args_of(&log, "B", "transfer-upload")[0]["local_path"],
        staged.as_str(),
        "传的不是下的那一份"
    );
    let commit = &args_of(&log, "B", "files-commit-upload")[0];
    assert_eq!(
        (commit["root"].as_str(), commit["rel"].as_str()),
        (Some("/data"), Some("a.bin"))
    );
    let dels = args_of(&log, "<local>", "files-delete");
    let key_part = staged.rsplit('/').next().unwrap().to_string();
    assert!(
        dels.iter().any(|d| d["rel"] == key_part.as_str()),
        "本机暂存件没删：{dels:?}"
    );
    assert!(
        args_of(&log, "B", "files-delete").is_empty(),
        "成了还去删 B 的暂存件"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_push_removes_the_staging_on_b_too() {
    let (o, log) = go(true).await;
    assert!(
        matches!(&o, Outcome::Failed { why, .. } if why.contains("断网")),
        "{o:?}"
    );
    assert_eq!(
        args_of(&log, "B", "files-delete"),
        vec![
            serde_json::json!({ "root": "/home/b/.cc-monitor/staging", "rel": format!("{}.part", "k".repeat(32)) })
        ],
        "B 那头开过单的暂存件没删"
    );
    assert!(
        !args_of(&log, "<local>", "files-delete").is_empty(),
        "本机暂存件没删"
    );
    assert!(
        args_of(&log, "B", "files-commit-upload").is_empty(),
        "传失败了还去提交"
    );
}

#[test]
fn the_target_dir_defaults_to_home_and_must_be_absolute() {
    assert_eq!(target_dir("", "/home/b"), Ok("/home/b".to_string()));
    assert_eq!(target_dir(" /data/ ", "/home/b"), Ok("/data".to_string()));
    assert_eq!(target_dir("/", "/home/b"), Ok("/".to_string()));
    assert!(target_dir("data", "/home/b").is_err());
}

#[test]
fn the_local_origin_is_the_app_one() {
    let app = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/inbound_client.rs"
    ));
    let needle = format!("pub const LOCAL_ORIGIN: &str = \"{LOCAL_ORIGIN}\";");
    assert_eq!(
        app.matches(needle.as_str()).count(),
        1,
        "窗口那份 <local> 与 app 侧不是同一个值"
    );
}

#[test]
fn the_staging_dir_is_the_backend_one() {
    // 〔P3〕后端那一份的值住契约 crate（`relay_route_core::STAGING_DIR_REL`；数据位置页也按它列），
    //   后端 `files_commit::STAGING_DIR` 引它 ⇒ 这里钉「后端引的是契约那一份」＋「窗口这份 == 契约那一份」。
    let backend =
        guard_core::production_code(include_str!("../../../src/backend/control/files_commit.rs"));
    assert_eq!(
        backend
            .matches("pub const STAGING_DIR: &str = relay_route_core::STAGING_DIR_REL;")
            .count(),
        1,
        "后端 `files_commit::STAGING_DIR` 不再引契约那一份了"
    );
    assert_eq!(
        STAGING_DIR,
        relay_route_core::STAGING_DIR_REL,
        "窗口那份暂存区与后端的不是同一个值"
    );
}

/// 〔FILES2 · V152〕目标选的是本机（下拉里的 `<local>`）⇒ 没有第二腿：直接从 A 下到落点，不开上传的单、不碰本机暂存区。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn copying_to_this_machine_is_just_a_download_to_the_target() {
    let (o, log) = go_to(LOCAL_ORIGIN, false).await;
    assert!(
        matches!(&o, Outcome::Done { path, .. } if path == "/data/a.bin"),
        "{o:?}"
    );
    assert_eq!(
        args_of(&log, "A", "transfer-download")[0]["local_path"],
        "/data/a.bin",
        "没直接下到落点"
    );
    let s = steps(&log);
    assert!(
        !s.iter()
            .any(|(_, op)| op == "transfer-upload" || op == "files-mkdir"),
        "目标是本机却走了第二腿：{s:?}"
    );
}
