//! 传输台**中继**（`sftp_pool.rs`）的判据：开单 / 起跑 / 停订 / 流断了 都原样转给本机常驻后端，
//! 后端推上来的 `transfer` 帧翻成窗口认的那几格。
//!
//! 台架：一条内存管道两头 —— monitor 这头是**真的** `InboundClient` ＋ **真的**本机吸收点
//! （`local_backend::absorb_local_frame`），登记成本机那条流；对面是一个会说传输四条的小假后端
//! （记下每条请求，按用例的剧本回应答、塞帧）。后端那一半的真实现另有 `tests/backend/control/transfer_tests.rs`，
//! 两半接在一起的真进程读数见 `tests/evidence/SR1b-sftp-loopback.py`。
//!
//! ⚠ 本机那条流是进程内全局登记 ⇒ 本文件的用例一律先拿 `local_origin_test_lock`。

use super::*;
use crate::inbound_client::{park, register, unregister, BackendHello, LOCAL_ORIGIN};
use crate::stream_source::{parse_frame, InboundFrame};
use futures::stream::StreamExt;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

/// 台架：真 client（登记成本机）＋ 真吸收点 ＋ 假后端。
pub(crate) struct Rig {
    pub(crate) client: Arc<InboundClient>,
    seen: mpsc::UnboundedReceiver<Value>,
    to_monitor: mpsc::UnboundedSender<String>,
}

impl Drop for Rig {
    fn drop(&mut self) {
        unregister(LOCAL_ORIGIN, &self.client);
    }
}

/// `commands` = 假后端在 hello 里声明认的命令。开单那一条回 `{id, key}`，其余回 `ok`。
pub(crate) fn rig(commands: &[&str]) -> Rig {
    let (mon_w, be_r) = tokio::io::duplex(1 << 20);
    let (be_w, mon_r) = tokio::io::duplex(1 << 20);
    let hello = InboundFrame::Hello {
        v: 1,
        build_id: "t".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/tmp".into(),
        homes: vec![],
        capabilities: vec![],
        commands: commands.iter().map(|s| s.to_string()).collect(),
        unavailable: vec![],
        uncancellable: vec![],
    };
    let witness = BackendHello::from_hello_frame(&hello).expect("是 hello");
    let client = park(mon_w).into_client(witness);
    register(LOCAL_ORIGIN, Arc::clone(&client));
    let c2 = Arc::clone(&client);
    tokio::spawn(async move {
        let mut lines = tokio::io::BufReader::new(mon_r).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            if let Ok(f) = parse_frame(&l) {
                crate::local_backend::absorb_local_frame(f, Some(&c2));
            }
        }
    });
    let (to_monitor, mut outq) = mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        let mut w = be_w;
        while let Some(l) = outq.recv().await {
            if w.write_all(format!("{l}\n").as_bytes()).await.is_err() {
                break;
            }
        }
    });
    let (seen_tx, seen) = mpsc::unbounded_channel::<Value>();
    let reply = to_monitor.clone();
    tokio::spawn(async move {
        let mut n = 0u32;
        let mut lines = tokio::io::BufReader::new(be_r).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            let v: Value = serde_json::from_str(&l).expect("请求不是 JSON");
            let id = v["id"].as_str().unwrap_or_default().to_string();
            let line = match v["cmd"].as_str() {
                Some("transfer-upload") | Some("transfer-download") => {
                    n += 1;
                    format!(
                        r#"{{"kind":"reply","id":{id:?},"ok":true,"data":{{"id":"xfer-{n}","key":"00112233445566778899aabbccddeeff"}}}}"#
                    )
                }
                _ => format!(r#"{{"kind":"reply","id":{id:?},"ok":true}}"#),
            };
            let _ = reply.send(line);
            let _ = seen_tx.send(v);
        }
    });
    Rig {
        client,
        seen,
        to_monitor,
    }
}

impl Rig {
    pub(crate) async fn next(&mut self, cmd: &str) -> Value {
        loop {
            let v = tokio::time::timeout(Duration::from_secs(5), self.seen.recv())
                .await
                .unwrap_or_else(|_| panic!("5s 没等到 `{cmd}`"))
                .expect("假后端的请求口关了");
            if v["cmd"] == cmd {
                return v;
            }
        }
    }
    pub(crate) fn frame(&self, line: &str) {
        let _ = self.to_monitor.send(line.to_string());
    }
}

pub(crate) const ALL: [&str; 4] = [
    "transfer-upload",
    "transfer-download",
    "transfer-start",
    "transfer-stop",
];

pub(crate) fn cfg(label: &str) -> RemoteConfig {
    RemoteConfig {
        host: "example.invalid".into(),
        label: label.into(),
        port: 22,
        user: "nobody".into(),
        key_path: Some("/k".into()),
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    }
}

/// 收一条快照流直到收场（或 10 s）。
async fn drain(mut s: futures::stream::BoxStream<'static, Snap>) -> Vec<Snap> {
    let mut out = Vec::new();
    let r = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(x) = s.next().await {
            let over = x.end.is_some();
            out.push(x);
            if over {
                break;
            }
        }
    })
    .await;
    assert!(r.is_ok(), "10 s 内流没收场：{out:?}");
    out
}

/// 🔴🔴 **M2：开单 → 订阅即起跑 → 帧 → 终局**，而且转给后端的是**本机**那条流、带着那台远端的拨号请求。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_upload_is_relayed_to_the_local_backend_and_its_frames_come_back_as_snaps() {
    let _g = crate::inbound_client::local_origin_test_lock();
    let mut rig = rig(&ALL);
    let v = transfer_call(
        cfg("中继·上传"),
        TRANSFER_UPLOAD,
        // `home` 原样转给传输台（它连上之后比 SFTP 起始目录）。
        &serde_json::json!({ "local_path": "/tmp/x.bin", "home": "/home/u" }),
    )
    .await
    .expect("开单");
    assert_eq!(v["id"], "xfer-1");
    assert_eq!(
        v["key"].as_str().map(str::len),
        Some(32),
        "键没原样带回窗口"
    );
    let req = rig.next("transfer-upload").await;
    assert_eq!(req["args"]["local_path"], "/tmp/x.bin");
    assert_eq!(
        req["args"]["home"], "/home/u",
        "后端的 home 没原样转给传输台"
    );
    assert_eq!(
        req["args"]["dial"]["machine"]["host"], "example.invalid",
        "拨号请求不是那台远端的"
    );
    assert_eq!(
        req["args"]["dial"]["machine"]["keyPath"], "/k",
        "拨号请求只该带私钥路径（且要带）"
    );

    let origin = crate::origin::Origin("中继·上传".to_string());
    let s = watch_ticket(&origin, "xfer-1").expect("订阅");
    let start = rig.next("transfer-start").await;
    assert_eq!(start["args"]["id"], "xfer-1");
    rig.frame(r#"{"kind":"transfer","id":"xfer-1","got":10,"total":30}"#);
    rig.frame(
        r#"{"kind":"transfer","id":"xfer-1","got":30,"total":30,"end":{"state":"done","bytes":30,"sha256":"ab"}}"#,
    );
    let snaps = drain(s).await;
    assert_eq!(
        snaps.last(),
        Some(&Snap {
            got: 30,
            total: 30,
            // 上传那一路的整份摘要原样带着（形状由远端后端提交那一关判，中继不判）。
            end: Some(End::Done {
                bytes: 30,
                sha256: Some("ab".into())
            })
        })
    );
    // 收场之后中继摘掉（再订阅 = 没有这一趟）。
    assert_eq!(
        watch_ticket(&origin, "xfer-1").err().map(|(c, _)| c),
        Some("no-such-transfer")
    );
}

/// 🔴 **停订就是撤**：流在收场之前被丢 ⇒ 后端收到 `transfer-stop {id}`。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_the_stream_sends_a_stop_to_the_local_backend() {
    let _g = crate::inbound_client::local_origin_test_lock();
    let mut rig = rig(&ALL);
    transfer_call(
        cfg("中继·撤"),
        TRANSFER_UPLOAD,
        &serde_json::json!({ "local_path": "/tmp/y.bin" }),
    )
    .await
    .expect("开单");
    let origin = crate::origin::Origin("中继·撤".to_string());
    let mut s = watch_ticket(&origin, "xfer-1").expect("订阅");
    rig.next("transfer-start").await;
    let first = s.next().await.expect("第一格是此刻");
    assert!(first.end.is_none());
    drop(s);
    let stop = rig.next("transfer-stop").await;
    assert_eq!(stop["args"]["id"], "xfer-1");
}

/// 本机那条流断了 ⇒ 经它开的中继一律收场（`failed`，原因说清），不让看的人干等。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dead_local_stream_ends_every_relay_it_opened() {
    let _g = crate::inbound_client::local_origin_test_lock();
    let mut rig = rig(&ALL);
    transfer_call(
        cfg("中继·断"),
        TRANSFER_DOWNLOAD,
        &serde_json::json!({ "remote_path": "/srv/a", "local_path": "/tmp/ccm-sr1b-dl-target" }),
    )
    .await
    .expect("开单");
    let origin = crate::origin::Origin("中继·断".to_string());
    let s = watch_ticket(&origin, "xfer-1").expect("订阅");
    rig.next("transfer-start").await;
    fail_owned_by(&rig.client, "本机后端的流断了（判据）");
    let snaps = drain(s).await;
    assert_eq!(
        snaps.last().and_then(|x| x.end.clone()),
        Some(End::Failed("本机后端的流断了（判据）".to_string()))
    );
}

/// 订阅口的三种坏形：没有这一趟 · 不是这台机器的 · 第二次订阅 —— 原位说清楚。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bad_subscriptions_say_why_in_place() {
    let _g = crate::inbound_client::local_origin_test_lock();
    let mut rig = rig(&ALL);
    transfer_call(
        cfg("中继·坏"),
        TRANSFER_UPLOAD,
        &serde_json::json!({ "local_path": "/tmp/z.bin" }),
    )
    .await
    .expect("开单");
    let mine = crate::origin::Origin("中继·坏".to_string());
    let other = crate::origin::Origin("别的机器".to_string());
    assert_eq!(
        watch_ticket(&mine, "xfer-404").err().map(|(c, _)| c),
        Some("no-such-transfer")
    );
    assert_eq!(
        watch_ticket(&other, "xfer-1").err().map(|(c, _)| c),
        Some("no-such-transfer")
    );
    let s = watch_ticket(&mine, "xfer-1").expect("第一次订阅");
    rig.next("transfer-start").await;
    assert_eq!(
        watch_ticket(&mine, "xfer-1").err().map(|(c, _)| c),
        Some("already-watched")
    );
    drop(s);
}

/// 🔴 **M1**：本机后端在、但不认 `transfer-upload`（它比界面老）⇒ 报「太旧」，**一条请求都不发**（`D11`，不回落）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_old_local_backend_is_named_too_old_and_nothing_is_sent() {
    let _g = crate::inbound_client::local_origin_test_lock();
    let mut rig = rig(&["ping"]);
    let (code, e) = transfer_call(
        cfg("中继·旧"),
        TRANSFER_UPLOAD,
        &serde_json::json!({ "local_path": "/tmp/q.bin" }),
    )
    .await
    .expect_err("老后端不该开得了单");
    assert_eq!(code, "backend_unavailable");
    // 内部命令名（transfer-upload）不再上屏；「太旧」与出路（重开 monitor）留着。
    assert!(e.contains("太旧"), "{e}");
    assert!(rig.seen.try_recv().is_err(), "对老后端发了请求");
}

/// 🔴 **M1**：本机后端那条流不在 ⇒ 报「本机后端不在」（有界地等一会儿之后），不进程内开 SFTP。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_local_backend_the_transfer_is_refused_out_loud() {
    let _g = crate::inbound_client::local_origin_test_lock();
    assert!(crate::inbound_client::client_for(LOCAL_ORIGIN).is_none());
    let (code, e) = transfer_call(
        cfg("中继·无"),
        TRANSFER_UPLOAD,
        &serde_json::json!({ "local_path": "/tmp/q.bin" }),
    )
    .await
    .expect_err("本机后端不在也开得了单？");
    assert_eq!(code, "backend_unavailable");
    assert!(e.contains("本机后端不在"), "{e}");
}

/// 🔴 本机落点是一份会话记录的形状 ⇒ **照样转给本机后端开单**，落点原样带过去。
///
/// 从前这一条是「踩线的本机落点 ⇒ 回围栏那句话，而且在转给后端之前（一条请求都不发）」。
/// 用户「文件管理器全部都可以改. 不需要任何围栏」⇒ monitor 这一侧开单时那一判删了。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_onto_a_session_file_is_forwarded_like_any_other() {
    let _g = crate::inbound_client::local_origin_test_lock();
    let mut rig = rig(&ALL);
    let session = "/home/u/.claude/projects/dash-proj/abc-123.jsonl";
    // 夹具那条路径是会话记录的形状（`projects/<proj>/<sid>.jsonl`）：那道判定今天只住后端
    //   （`agents/claudecode/paths.rs::is_session_record_file`），这里按形状现核，不借 monitor 的第二份。
    let tail: Vec<&str> = session
        .rsplit_once("/projects/")
        .map(|(_, t)| t.split('/').collect())
        .unwrap_or_default();
    assert!(
        tail.len() == 2 && tail[1].ends_with(".jsonl"),
        "夹具那条路径不是会话记录的形状 —— 本条此刻在量别的东西"
    );
    transfer_call(
        cfg("中继·会话落点"),
        TRANSFER_DOWNLOAD,
        &serde_json::json!({ "remote_path": "/srv/whatever.txt", "local_path": session }),
    )
    .await
    .expect("🔴 往会话文件那个位置上下载，开不出单");
    let sent = rig.next("transfer-download").await;
    assert_eq!(sent["args"]["local_path"], session, "落点没有原样转给后端");
}

/// 解帧：后端 `wire_tests::transfer_frames_have_exactly_these_bytes` 那四形**逐字节**的线上串（异源：后端金标准）
/// 都认得出；`end` 认不出 ⇒ 整帧 `None`（不猜一个结局）。
#[test]
fn transfer_frames_parse_exactly_as_the_backend_writes_them() {
    let cases: [(&str, Option<End>); 5] = [
        (
            r#"{"kind":"transfer","id":"xfer-7","got":262144,"total":1000000}"#,
            None,
        ),
        (
            r#"{"kind":"transfer","id":"xfer-7","got":262144,"total":1000000,"end":{"state":"done","bytes":1000000}}"#,
            Some(End::Done {
                bytes: 1_000_000,
                sha256: None,
            }),
        ),
        (
            r#"{"kind":"transfer","id":"xfer-7","got":262144,"total":1000000,"end":{"state":"failed","why":"写暂存件失败"}}"#,
            Some(End::Failed("写暂存件失败".into())),
        ),
        // 带码的那一形（后端 `wire_tests` 同一行逐字节）⇒ 单列一形，码原样带着。
        (
            r#"{"kind":"transfer","id":"xfer-7","got":262144,"total":1000000,"end":{"state":"failed","why":"w","code":"sftp_home_mismatch"}}"#,
            Some(End::FailedCoded {
                why: "w".into(),
                code: "sftp_home_mismatch".into(),
            }),
        ),
        (
            r#"{"kind":"transfer","id":"xfer-7","got":262144,"total":1000000,"end":{"state":"cancelled"}}"#,
            Some(End::Cancelled),
        ),
    ];
    for (line, want) in cases {
        match parse_frame(line) {
            Ok(InboundFrame::Transfer {
                id,
                got,
                total,
                end,
            }) => {
                assert_eq!((id.as_str(), got, total), ("xfer-7", 262_144, 1_000_000));
                assert_eq!(end, want, "{line}");
            }
            other => panic!("没认出传输帧：{line} ⇒ {other:?}"),
        }
    }
    assert!(parse_frame(
        r#"{"kind":"transfer","id":"x","got":1,"total":1,"end":{"state":"maybe"}}"#
    )
    .is_err());
    assert!(parse_frame(r#"{"kind":"transfer","id":"x","got":"1","total":1}"#).is_err());
}
