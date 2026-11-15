//! `chores-merge`（本机后端自己问到各台的「文件与数据」、件数合成一份）的判据。
//!
//! ★ 别的台经 `--resident-call` 问它的常驻 `data-report`（不带本机那几条事实，期限是发起方的）；
//! 每台那一份原样带回、件数逐格相加；那台常驻不在 ⇒ 那台只带那一句、件数不计，别台照常。

use super::*;
use crate::dial::remote_ask::test_peers::{until, Peers};
use serde_json::json;

fn report(badge: usize) -> DataReport {
    serde_json::from_value(json!({
        "changedFiles": [], "todo": [], "tmux": true,
        "counts": {"badge": badge, "open": badge, "must": badge, "decide": 0, "install": 0, "optional": 0, "done": 1},
        "rank": 0, "own": [],
    }))
    .expect("一份 data-report")
}

#[tokio::test]
async fn the_local_backend_asks_each_machine_its_data_report_and_adds_the_counts() {
    let peers = Peers::new(&[("devbox", serde_json::to_string(&report(2)).unwrap())]);
    let t = Peers::table(&["devbox", "pi"]);
    let machines: Vec<cmd_wire::Machine> = [cmd_wire::LOCAL, "devbox", "pi"]
        .iter()
        .map(|m| cmd_wire::Machine::named(*m))
        .collect();
    let got = chores_merge_with(
        &machines,
        std::future::ready(Ok(report(3))),
        &Default::default(),
        until(9_000),
        &t,
        &peers,
    )
    .await
    .expect("合得出");
    let v = serde_json::to_value(&got).unwrap();
    assert_eq!(
        (v["total"]["badge"].clone(), v["total"]["done"].clone()),
        (json!(5), json!(2))
    );
    let rows = v["machines"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "照问的先后一台一格");
    assert_eq!(rows[1]["origin"], "devbox");
    assert_eq!(
        rows[1]["report"]["counts"]["badge"], 2,
        "那台那一份原样带回"
    );
    assert_eq!(rows[2]["origin"], "pi");
    assert!(
        rows[2].get("report").is_none() && rows[2]["said"].as_str().unwrap().contains("pi"),
        "{v}"
    );
    let asked = peers.asked();
    assert_eq!(asked.len(), 2);
    for (command, line) in &asked {
        assert!(command.contains("--resident-call"), "{command}");
        assert!(Peers::within_ms(command) <= 9_000);
        assert_eq!(line["cmd"], "data-report");
        assert_eq!(
            line["args"]["client"],
            serde_json::Value::Null,
            "本机那几条事实不交给别的台"
        );
    }
}

#[tokio::test]
async fn a_machine_in_the_table_that_is_not_connected_is_listed_unasked_with_its_offline_line() {
    use crate::dial::remote_ask::test_peers::{add, host};
    let peers = Peers::new(&[("devbox", serde_json::to_string(&report(2)).unwrap())]);
    let t = Peers::table(&["devbox"]);
    let mut off = host("pi");
    off.connect = false;
    add(&t, off);
    let got = chores_merge_with(
        &t.asked(),
        std::future::ready(Ok(report(3))),
        &Default::default(),
        until(9_000),
        &t,
        &peers,
    )
    .await
    .expect("合得出");
    let v = serde_json::to_value(&got).unwrap();
    let rows = v["machines"].as_array().unwrap();
    let origins: Vec<&str> = rows.iter().map(|r| r["origin"].as_str().unwrap()).collect();
    assert_eq!(
        origins,
        [cmd_wire::LOCAL, "devbox", "pi"],
        "机器表里每一台都在：{v}"
    );
    assert!(rows[2].get("report").is_none(), "{v}");
    assert_eq!(
        rows[2]["said"],
        json!(copy_core::offline("pi")),
        "没连的那台带离线那一句"
    );
    assert_eq!(v["total"]["badge"], json!(5), "没连的那台件数不计");
    assert!(
        peers.asked().iter().all(|(c, _)| !c.contains("pi")),
        "没连的那台不去问"
    );
}
