//! 瘦客户端底行「排队 N 条」那一句：盯托管终端的事实（打标那一下）与这台的队目录，照会话那台的名单写、变了才说。
//! 沙箱数据目录与家目录，真 `~/.cc-monitor/` 不碰。

use super::*;
use crate::history::session_queue as q;
use crate::host::managed::home::{Facts, Home};
use cmd_wire::queue::SessionQueuePutReq;
use cmd_wire::Machine;
use copy_core::copy_text;
use std::sync::mpsc::Receiver;

fn sandbox_own(tag: &str) -> Own {
    let root = test_home::sandbox_root().join(format!("ql-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data")).expect("建沙箱数据目录");
    std::fs::create_dir_all(root.join("home")).expect("建沙箱家目录");
    let d = root.join("data").to_string_lossy().into_owned();
    let h = root.join("home").to_string_lossy().into_owned();
    Own::issue_with(&move |k| match k {
        "CCM_DATA_DIR" => Some(d.clone()),
        "HOME" => Some(h.clone()),
        _ => None,
    })
}

const SID: &str = "0b8f2c1e-6a3d-4c55-9e1f-2a7b3c4d5e6f";

fn put(own: &Own, id: &str) {
    let req = SessionQueuePutReq {
        machine: Machine::local(),
        sid: SID.parse().unwrap(),
        id: id.to_string(),
        end: "mobile".to_string(),
        n: None,
        at_ms: 1,
        text: format!("text of {id}"),
        attachments: Vec::new(),
    };
    q::put(own, &req, crate::common::time::now_ms()).unwrap();
}

fn waiting(n: usize) -> Option<String> {
    Some(copy_text("beQueue.said.waiting", &[("n", &n.to_string())]))
}

/// 等下一句（盯盘的线程那一侧说；判据只能限时等）。
fn next(rx: &Receiver<Option<String>>) -> Option<String> {
    rx.recv_timeout(guard_core::wall::CEILING)
        .expect("等到下一句")
}

/// ★ 打标前投的两条，打标那一下现读 ⇒「排队 2 条」；撤一条 ⇒「排队 1 条」；撤完 ⇒ 没有。
#[test]
fn the_line_follows_the_owners_list_from_tag_to_empty() {
    let own = sandbox_own("follow");
    let id = "ql-follow";
    let home = Home::claim(&own, id).expect("占家");
    let mut facts = Facts {
        id: id.into(),
        name: id.into(),
        ..Facts::default()
    };
    home.write_facts(&facts).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let _ears = ears(&own, id, move |l| {
        let _ = tx.lock().unwrap().send(l);
    })
    .expect("挂上");
    assert_eq!(next(&rx), None, "还没打标 ⇒ 没有");
    put(&own, "a");
    put(&own, "b");
    facts.sid = SID.into();
    home.write_facts(&facts).unwrap();
    assert_eq!(next(&rx), waiting(2), "打标那一下现读");
    q::cancel(&own, SID, "a", crate::common::time::now_ms()).unwrap();
    assert_eq!(next(&rx), waiting(1));
    q::cancel(&own, SID, "b", crate::common::time::now_ms()).unwrap();
    assert_eq!(next(&rx), None, "撤完 ⇒ 没有");
    home.clear();
}
