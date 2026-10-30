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
            cell: None,
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
        .map(|m| cmd_wire::Machine::named(*m))
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

/// 界面测试的需手动替身交的那几份合并的金样（`tests/test-support/needs-fake.ts` 只照它回）。
const REPLAYED: &str = "tests/__fixtures__/needs-merged.golden.json";
/// 替身遇到金样里没有的输入记在这里（本树、不进仓）。
const MISSING: &str = ".build/golden-missing/needs-merged.jsonl";

/// 一组输入跑真的扇出与合并：`asked` 问了哪几台，`answers` 答了的那几台各自的整份；问了没答的那台 ⇒ 常驻不在。
async fn merged_for(asked: &[String], answers: &serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    let list = |m: &str, v: &serde_json::Value| -> NeedsList {
        serde_json::from_value(v.clone())
            .unwrap_or_else(|e| panic!("{REPLAYED}：{m} 那一份不是线上的 NeedsList（{e}）"))
    };
    for m in answers.keys() {
        assert!(asked.contains(m), "{REPLAYED}：{m} 答了却没问");
    }
    let local = answers
        .get(cmd_wire::LOCAL)
        .map(|v| list(cmd_wire::LOCAL, v))
        .unwrap_or_else(|| panic!("{REPLAYED}：本机那一台没答（本机常驻不在那一形不经这本账）"));
    let remote: Vec<(&str, String)> = answers
        .iter()
        .filter(|(m, _)| m.as_str() != cmd_wire::LOCAL)
        .map(|(m, v)| (m.as_str(), serde_json::to_string(&list(m, v)).unwrap()))
        .collect();
    let peers = Peers::new(&remote);
    let names: Vec<&str> = asked
        .iter()
        .map(String::as_str)
        .filter(|m| *m != cmd_wire::LOCAL)
        .collect();
    let machines: Vec<cmd_wire::Machine> =
        asked.iter().map(|m| cmd_wire::Machine::named(m)).collect();
    let got = needs_merge_with(
        &machines,
        std::future::ready(Ok(local)),
        &Default::default(),
        until(9_000),
        &Peers::table(&names),
        &peers,
    )
    .await
    .expect("合得出");
    serde_json::to_value(&got).unwrap()
}

/// ★ 需手动替身回的每一份合并都是这里现打的：金样每条的输入原样留着，`merged` 由 [`needs_merge_with`] 现打
/// （先后 · 总数 · 没问到的那一句都出自后端）。替身记下的没录的输入（[`MISSING`]）一并收进现打的那份 ⇒ 红，对过后 bless 拷回。
#[tokio::test]
async fn the_needs_fake_replays_merges_made_here() {
    let root = guard_core::addr::repo_root();
    let text = std::fs::read_to_string(root.join(REPLAYED)).unwrap_or_else(|e| panic!("读不到 {REPLAYED}：{e}"));
    let golden: serde_json::Value = serde_json::from_str(&text).expect("金样不是 JSON");
    let mut inputs: Vec<serde_json::Value> = golden["cases"].as_array().expect("金样没有 cases").clone();
    if let Ok(missing) = std::fs::read_to_string(root.join(MISSING)) {
        for line in missing.lines().filter(|l| !l.trim().is_empty()) {
            let c: serde_json::Value = serde_json::from_str(line).expect("没录的那一行不是 JSON");
            let same = |x: &serde_json::Value| x["asked"] == c["asked"] && x["answers"] == c["answers"];
            if !inputs.iter().any(same) {
                inputs.push(c);
            }
        }
    }
    assert!(!inputs.is_empty(), "{REPLAYED} 一条都没有（空集上的回放恒真）");
    let mut cases = Vec::new();
    for c in &inputs {
        let asked: Vec<String> = serde_json::from_value(c["asked"].clone()).expect("asked");
        let answers = c["answers"].as_object().expect("answers").clone();
        let merged = merged_for(&asked, &answers).await;
        cases.push(serde_json::json!({ "asked": asked, "answers": answers, "merged": merged }));
    }
    let want = serde_json::json!({
        "读法": [
            "界面测试的需手动替身（tests/test-support/needs-fake.ts）只照这份回：按输入（asked 问了哪几台 · answers 答了的那几台各自的整份）找到那一条、交它的 merged。",
            "merged 由后端判据 tests/backend/faces/needs_merge_tests.rs 跑真的 needs_merge_with 现打（问了没答的那台 ⇒ 常驻不在）；只比不写，对过后 bless 拷回。",
            "替身遇到没录的输入记进本树 .build/golden-missing/needs-merged.jsonl，那条判据一并收进现打的那份。"
        ],
        "cases": cases,
    });
    crate::golden::assert_golden(
        REPLAYED,
        &(serde_json::to_string_pretty(&want).unwrap() + "\n"),
        "需手动替身的回放金样变了：界面测试照它画（needs-panel · main-window-needs · session-hidden）",
    );
}
