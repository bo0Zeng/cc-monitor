//! 那台的状态成品在本机后端定：本机通道一接上（本机后端可能刚重启、表是空的），每台手上最新那一轮原样重交（`machine-round`）。

use super::*;
use cmd_wire::machine::{Round, Why};

/// 一条停在某一态的循环（替身：不拨号）。
struct Still(State);

impl Looped for Still {
    fn wake(&self) {}
    fn state(&self) -> State {
        self.0.clone()
    }
    fn theirs(&self) -> Option<String> {
        None
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fresh_local_channel_gets_every_machines_latest_round() {
    let _g = crate::inbound_client::local_origin_test_lock();
    let a: Arc<dyn Looped> = Arc::new(Still(State::Connecting));
    let b: Arc<dyn Looped> = Arc::new(Still(State::Down {
        problem: Problem::new("拨号没成", "").because(Some(Why::Auth), (None, None)),
        needs_hand: false,
        blocker: None,
    }));
    {
        let mut f = feeds().lock().unwrap_or_else(|e| e.into_inner());
        f.insert("mr-a".into(), Arc::downgrade(&a));
        f.insert("mr-b".into(), Arc::downgrade(&b));
    }
    // 本机通道接上（假后端登记成本机）。
    let mut rig = crate::sftp_pool::tests::rig(&["machine-round"]);
    let mut got = std::collections::BTreeMap::new();
    while got.len() < 2 {
        let v = rig.next("machine-round").await;
        let machine = v["args"]["machine"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if machine.starts_with("mr-") {
            got.insert(machine, v["args"]["round"].clone());
        }
    }
    {
        let mut f = feeds().lock().unwrap_or_else(|e| e.into_inner());
        f.remove("mr-a");
        f.remove("mr-b");
    }
    assert_eq!(
        got,
        [
            ("mr-a".to_string(), serde_json::json!(Round::Connecting)),
            ("mr-b".to_string(), serde_json::json!(b.state().round())),
        ]
        .into_iter()
        .collect()
    );
}
