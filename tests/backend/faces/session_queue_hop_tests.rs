//! 会话输入队跨台那一跳：会话在别台 ⇒ 交给那台的常驻、序号由那台发；那台够不到 ⇒ `offline`、本机一个字节不写。
//!
//! 对面是替身：照生产那一半（`history/session_queue.rs`）在它自己的临时家目录里现答。真 SSH 与真远端常驻买不到。

use super::*;
use crate::faces::session_queue_face::{SessionQueuePutCmd, SessionQueuePutErr};
use crate::history::session_queue as q;
use crate::history::session_queue_wire::{QueuePut, QueuePutOutcome};
use crate::wire::typed::Codes as _;
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

fn home(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-squeue-hop-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 那台：自己的家目录 ＋ 自己的钟；记下收到的每一问。`down` ⇒ 常驻不在。
struct Owner {
    own: crate::platform::fs::Own,
    now_ms: i64,
    down: bool,
    asked: Mutex<Vec<(String, Value)>>,
}

impl Remote for Owner {
    fn run<'a>(
        &'a self,
        _dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            let line: Value = serde_json::from_str(stdin.as_deref().unwrap_or("")).unwrap();
            self.asked.lock().unwrap().push((command, line.clone()));
            if self.down {
                return Err("down".into());
            }
            assert_eq!(line["cmd"], json!("session-queue-put"));
            let req: SessionQueuePutReq = serde_json::from_value(line["args"].clone()).unwrap();
            let got: QueuePut = q::put(&self.own, &req, self.now_ms).map_err(|f| f.message)?;
            Ok(serde_json::to_string(&got).unwrap())
        })
    }

    fn run_coded<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, Said>> + Send + 'a>> {
        Box::pin(async move {
            self.run(dial, command, stdin).await.map_err(|m| Said {
                code: Some("resident_absent".into()),
                message: m,
            })
        })
    }
}

fn put_to(machine: &str, id: &str, at_ms: i64) -> SessionQueuePutReq {
    serde_json::from_value(json!({
        "machine": machine, "sid": "s1", "id": id, "end": "desk", "atMs": at_ms, "text": "hi"
    }))
    .unwrap()
}

async fn hop(
    req: &SessionQueuePutReq,
    here_home: &std::path::Path,
    table: &Table,
    owner: &Owner,
) -> Result<QueuePut, Fail> {
    let own = crate::platform::fs::Own::issue_at(here_home);
    let r = req.clone();
    route::<SessionQueuePutCmd, _>(
        req,
        move || q::put(&own, &r, 0),
        SessionQueuePutErr::ALL,
        &crate::Tz::default(),
        None,
        table,
        owner,
    )
    .await
}

#[tokio::test]
async fn a_remote_session_is_queued_on_its_own_machine() {
    let here = home("here");
    let there = home("there");
    let owner = Owner {
        own: crate::platform::fs::Own::issue_at(&there),
        now_ms: 7,
        down: false,
        asked: Mutex::new(Vec::new()),
    };
    let table = crate::dial::remote_ask::test_peers::named(&["far"]);
    let a = hop(&put_to("far", "a", 1), &here, &table, &owner)
        .await
        .unwrap();
    let b = hop(&put_to("far", "b", 2), &here, &table, &owner)
        .await
        .unwrap();
    assert_eq!((a.outcome, a.seq), (QueuePutOutcome::Queued, Some(1)));
    assert_eq!(b.seq, Some(2), "序号由那台发");
    let asked = owner.asked.lock().unwrap();
    assert_eq!(
        asked[0].1["args"]["machine"],
        json!("<local>"),
        "交给那台的是同一条命令、machine 换成那台眼里的本机"
    );
    assert!(asked[0].0.contains("--resident-call"), "问的是那台的常驻");
    assert_eq!(q::read(&owner.own, "s1", 8).unwrap().items.len(), 2);
    assert!(!here.join(q::DIR_REL).exists(), "本机一个字节不写");
}

#[tokio::test]
async fn an_offline_owner_is_refused_on_the_spot() {
    let here = home("off-here");
    let there = home("off-there");
    let owner = Owner {
        own: crate::platform::fs::Own::issue_at(&there),
        now_ms: 7,
        down: true,
        asked: Mutex::new(Vec::new()),
    };
    let table = crate::dial::remote_ask::test_peers::named(&["far"]);
    let e = hop(&put_to("far", "a", 1), &here, &table, &owner)
        .await
        .unwrap_err();
    assert_eq!(e.code, "offline");
    // 不在机器表里的那一台：同样 `offline`，一问都不发。
    let e = hop(&put_to("nowhere", "a", 1), &here, &table, &owner)
        .await
        .unwrap_err();
    assert_eq!(e.code, "offline");
    assert_eq!(owner.asked.lock().unwrap().len(), 1);
    assert!(!here.join(q::DIR_REL).exists(), "不在发件端攒");
    assert!(!there.join(q::DIR_REL).exists());
}

#[tokio::test]
async fn the_owners_own_refusal_keeps_its_code() {
    struct Full;
    impl Remote for Full {
        fn run<'a>(
            &'a self,
            _dial: &'a Value,
            _command: String,
            _stdin: Option<String>,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
            Box::pin(async { Err("full".to_string()) })
        }
        fn run_coded<'a>(
            &'a self,
            _dial: &'a Value,
            _command: String,
            _stdin: Option<String>,
        ) -> Pin<Box<dyn Future<Output = Result<String, Said>> + Send + 'a>> {
            Box::pin(async {
                Err(Said {
                    code: Some("queue_full".into()),
                    message: "满".into(),
                })
            })
        }
    }
    let here = home("full-here");
    let own = crate::platform::fs::Own::issue_at(&here);
    let req = put_to("far", "a", 1);
    let r = req.clone();
    let e = route::<SessionQueuePutCmd, _>(
        &req,
        move || q::put(&own, &r, 0),
        SessionQueuePutErr::ALL,
        &crate::Tz::default(),
        None,
        &crate::dial::remote_ask::test_peers::named(&["far"]),
        &Full,
    )
    .await
    .unwrap_err();
    assert_eq!(
        e.code, "queue_full",
        "那台说「不行」⇒ 照那台的码，不是 offline"
    );
}
