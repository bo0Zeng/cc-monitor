//! 会话输入队那三条的帧面一层：入参照类型解（多一个键 · 缺 id 拒）· 码表 · 本机那一支落到这台的队上、回执带序号与排第几。
//!
//! ⚠ 家目录一律是临时目录（`Own::issue_at`），真 `~/.cc-monitor/` 一个字节都不碰；本机那一支不碰可达表与对面。

use super::*;
use cmd_wire::queue::QueuePutOutcome;
use serde_json::json;

fn home(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-squeue-face-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn put_req(v: serde_json::Value) -> Result<SessionQueuePutReq, serde_json::Error> {
    serde_json::from_value(v)
}

#[test]
fn put_args_decode_by_type() {
    let ok = put_req(json!({
        "machine": "<local>", "sid": "s1", "id": "m1", "end": "desk", "n": 1, "atMs": 5, "text": "hi"
    }))
    .expect("照类型解");
    assert_eq!((ok.id.as_str(), ok.n, ok.at_ms), ("m1", Some(1), 5));
    assert!(
        put_req(json!({"machine": "<local>", "sid": "s1", "end": "d", "atMs": 1, "text": "t"}))
            .is_err(),
        "缺 id ⇒ 拒"
    );
    assert!(
        put_req(json!({
            "machine": "<local>", "sid": "s1", "id": "m", "end": "d", "atMs": 1, "text": "t", "extra": 1
        }))
        .is_err(),
        "多一个键 ⇒ 拒"
    );
    assert!(
        put_req(json!({"machine": "<local>", "sid": "../x", "id": "m", "end": "d", "atMs": 1, "text": "t"}))
            .is_err(),
        "sid 不合值域 ⇒ 拒（落不成路径）"
    );
}

#[test]
fn every_command_declares_offline() {
    for all in [
        SessionQueuePutErr::ALL,
        SessionQueueCancelErr::ALL,
        SessionQueueReadErr::ALL,
    ] {
        assert!(all.contains(&"offline"), "{all:?}");
        assert!(all.contains(&"queue_unreadable"), "{all:?}");
    }
}

#[tokio::test]
async fn local_put_lands_on_this_machine() {
    let h = home("local");
    let own = crate::platform::fs::Own::issue_at(&h);
    let req = put_req(json!({
        "machine": "<local>", "sid": "s1", "id": "m1", "end": "desk", "atMs": 5, "text": "hi"
    }))
    .unwrap();
    let r = req.clone();
    let o = own.clone();
    let got = route::<SessionQueuePutCmd, _>(
        &req,
        move || q::put(&o, &r, 100),
        SessionQueuePutErr::ALL,
        &crate::common::time::Tz::default(),
        None,
        &crate::dial::remote_ask::test_peers::named(&[]),
        &DialRemote,
    )
    .await
    .expect("本机那一支");
    assert_eq!(
        (got.outcome, got.seq, got.place),
        (QueuePutOutcome::Queued, Some(1), Some(1))
    );
    let list = q::read(&own, "s1", 101).unwrap();
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].text, "hi");
}

/// 带附件投：有一份不在（或会话工作目录读不出）⇒ `attachment_gone`、那一句列出编号、队文件一个字节没写；都在 ⇒ 排进去、编号随那一条。
#[test]
fn attachments_are_checked_on_the_owner_before_anything_is_written() {
    let h = home("attach");
    let own = crate::platform::fs::Own::issue_at(&h);
    let cwd = h.join("proj");
    let at = cwd.join(".ccm-attach").join("1027-1200-a3f2");
    std::fs::create_dir_all(&at).unwrap();
    std::fs::write(at.join("a.png"), b"x").unwrap();
    let req = |ids: &[&str]| {
        put_req(json!({
            "machine": "<local>", "sid": "s1", "id": "m1", "end": "desk", "atMs": 5, "text": "看图", "attachments": ids
        }))
        .unwrap()
    };
    let file = h.join(".cc-monitor/session-queue/s1.json");
    let found = |_: &str| Some(cwd.clone());
    let no = put_here(
        &own,
        &req(&["1027-1200-a3f2", "1027-1201-e95b"]),
        100,
        &found,
    )
    .expect_err("有一份不在 ⇒ 拒");
    assert_eq!(no.code.as_deref(), Some("attachment_gone"));
    assert!(
        no.said.to_string().contains("1027-1201-e95b"),
        "{}",
        no.said
    );
    let no = put_here(&own, &req(&["1027-1200-a3f2"]), 100, &|_: &str| None)
        .expect_err("读不出工作目录 ⇒ 拒");
    assert_eq!(no.code.as_deref(), Some("attachment_gone"));
    assert!(!file.exists(), "拒了还写了队文件");
    let got = put_here(&own, &req(&["1027-1200-a3f2"]), 100, &found).expect("都在");
    assert_eq!(got.outcome, QueuePutOutcome::Queued);
    assert_eq!(
        q::read(&own, "s1", 101).unwrap().items[0].attachments,
        ["1027-1200-a3f2"]
    );
}

/// 主人那台拒了（附件不在 · 太长）⇒ 回的那一句就是发件端照摆的整句（未送达 · 原因 · 内容保留），码 · 原话不动；收下的照常回执。
#[test]
fn owner_refusal_is_the_whole_undelivered_sentence() {
    let h = home("refused");
    let own = crate::platform::fs::Own::issue_at(&h);
    let req = |text: &str, ids: &[&str]| {
        put_req(json!({
            "machine": "<local>", "sid": "s1", "id": "m1", "end": "phone", "atMs": 5, "text": text, "attachments": ids
        }))
        .unwrap()
    };
    let whole = |why: &str| copy_text("terminal.input.failed", &[("why", why)]);
    let no = owner_put(
        &own,
        &req("看图", &["1027-1201-e95b"]),
        100,
        &|_: &str| None,
    )
    .expect_err("附件不在 ⇒ 拒");
    assert_eq!(no.code.as_deref(), Some("attachment_gone"));
    assert_eq!(
        no.said,
        whole(&copy_text(
            "beQueue.put.attachmentGone",
            &[("ids", "1027-1201-e95b")]
        ))
    );
    // 比一句的上限（64 KiB）长。
    let long = "x".repeat(64 * 1024 + 1);
    let no = owner_put(&own, &req(&long, &[]), 100, &|_: &str| None).expect_err("太长 ⇒ 拒");
    assert_eq!(no.code.as_deref(), Some("text_too_long"));
    let bare = put_here(&own, &req(&long, &[]), 100, &|_: &str| None).unwrap_err();
    assert_eq!(no.said, whole(&bare.said));
    assert_eq!(no.raw, bare.raw, "原话不动");
    let got = owner_put(&own, &req("hi", &[]), 100, &|_: &str| None).expect("收下");
    assert_eq!(got.outcome, QueuePutOutcome::Queued);
}
