//! `sessions-needs-merge`（本机后端自己问到各台的需手动、合成一份）的判据。
//!
//! ★ 别的台经 `--resident-call` 问它的常驻 `sessions-needs`（期限是发起方的）；那台常驻不在 · 够不着 ⇒ 那台进 `down`、不计，别台照常；
//! 问到别的台却没带期限 ⇒ `bad_args`。合的先后只在 `merge_needs` 一处（`facts_tests` 判）。

use super::*;
use crate::dial::remote_ask::test_peers::{until, Peers};

fn plan(rank: u64, slice: &str) -> NeedsList {
    NeedsList {
        items: vec![cmd_wire::needs::NeedsItem::Plan {
            rank,
            workspace: "/w".into(),
            slice: slice.into(),
            k: 0,
        }],
        answered: vec![],
        manual_unreadable: None,
    }
}

#[tokio::test]
async fn the_local_backend_asks_each_machine_its_needs_and_merges() {
    let peers = Peers::new(&[("devbox", serde_json::to_string(&plan(5, "那台的")).unwrap())]);
    let t = Peers::table(&["devbox", "pi"]);
    let machines: Vec<cmd_wire::Machine> = [cmd_wire::LOCAL, "devbox", "pi"]
        .iter()
        .map(|m| cmd_wire::Machine((*m).into()))
        .collect();
    let got = needs_merge_with(
        &machines,
        std::future::ready(Ok(plan(9, "本机的"))),
        &Default::default(),
        until(9_000),
        &t,
        &peers,
    )
    .await
    .expect("合得出");
    let v = serde_json::to_value(&got).unwrap();
    assert_eq!(got.total, 2, "答了的两台加起来");
    assert_eq!(v["items"][0]["origin"], "devbox");
    assert_eq!(v["items"][0]["item"]["slice"], "那台的");
    assert_eq!(v["items"][1]["origin"], cmd_wire::LOCAL);
    assert_eq!(v["down"].as_array().map(Vec::len), Some(1));
    assert_eq!(v["down"][0]["origin"], "pi");
    assert!(v["down"][0]["said"].as_str().unwrap().contains("pi"), "{v}");
    let asked = peers.asked();
    assert_eq!(asked.len(), 2);
    for (command, line) in &asked {
        assert!(command.contains("--resident-call"), "{command}");
        assert!(Peers::within_ms(command) <= 9_000);
        assert_eq!(line["cmd"], "sessions-needs");
    }
    let no = needs_merge_with(
        &machines,
        std::future::ready(Ok(plan(9, "本机的"))),
        &Default::default(),
        None,
        &t,
        &peers,
    )
    .await;
    assert!(
        matches!(no, Err(NeedsMergeErr::BadArgs(_))),
        "没带期限却去问了别的台"
    );
}
