//! **一份记录一张扫描图** —— 判据。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | R1 | 一遍扫出来的 尾段 · 骨架索引 · 用户输入 · 轮次 · 事实 == 各自单扫的那一份（逐格相等，含空行 · 坏行 · 残尾） | 合成记录 |
//! | R2 | 同一份没变 ⇒ 第二次拿到的就是第一次那张（同一个 `Arc`）；追加一行 / 改了修改时刻 ⇒ 重扫、读到新内容 | 真文件 |
//! | R3 | 条数上限：超了先淘汰最久没用的；单张比字节上限还大 ⇒ 不留（每次现扫） | 真文件 |
//! | R4 | 几个线程同时冷开同一份 ⇒ 只扫一遍（都拿到同一张） | 真文件、8 线程 |

use super::*;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

fn sandbox(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "ccm-record-scan-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).expect("sandbox");
    d
}

/// 合成一份记录：两轮 · 工具调用与出错的结果 · 收尾 · 空行 · 坏行 · 分叉血缘 · 用量 ＋ 末尾一截残行。
fn fixture(turns: usize) -> String {
    let mut s = String::new();
    s.push_str(r#"{"type":"user","uuid":"f0","forkedFrom":{"sessionId":"s0","messageUuid":"m0"},"timestamp":"2026-10-01T00:00:00.000Z","message":{"role":"user","content":"开头"}}"#);
    s.push('\n');
    for t in 0..turns {
        s.push_str(&format!(r#"{{"type":"user","uuid":"u{t}","timestamp":"2026-10-01T00:{t:02}:00.000Z","message":{{"role":"user","content":"第 {t} 句\n第二行"}}}}"#));
        s.push('\n');
        s.push_str(&format!(r#"{{"type":"assistant","uuid":"a{t}","timestamp":"2026-10-01T00:{t:02}:01.000Z","message":{{"model":"m-x","content":[{{"type":"thinking","thinking":"想"}},{{"type":"text","text":"先看看"}},{{"type":"tool_use","id":"tu{t}","name":"Bash","input":{{"command":"ls"}}}}],"usage":{{"input_tokens":{},"output_tokens":5}}}}}}"#, 100 + t));
        s.push('\n');
        s.push_str(&format!(r#"{{"type":"user","uuid":"r{t}","timestamp":"2026-10-01T00:{t:02}:02.000Z","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"tu{t}","is_error":true,"content":"坏了"}}]}}}}"#));
        s.push('\n');
        s.push_str("\n   \n");
        s.push_str("{not json\n");
        s.push_str(&format!(r#"{{"type":"assistant","uuid":"e{t}","timestamp":"2026-10-01T00:{t:02}:03.000Z","message":{{"model":"m-x","stop_reason":"end_turn","content":[{{"type":"text","text":"**做完了**\n```\ncode\n```\n第二行"}}]}}}}"#));
        s.push('\n');
    }
    s.push_str(r#"{"type":"user","uuid":"torn""#);
    s
}

fn index_direct(bytes: &[u8]) -> (Vec<String>, u64) {
    let mut rows = Vec::new();
    let (_, end) = crate::observe::history_query::scan_session_index(bytes, 0, None, |r| {
        rows.push(serde_json::to_string(r).unwrap());
        Ok(())
    })
    .unwrap();
    (rows, end)
}

#[test]
fn r1_one_pass_equals_each_scan_on_its_own() {
    let text = fixture(3);
    let bytes = text.as_bytes();
    let map = ScanMap::scan_with(bytes, bytes.len() as u64, None).expect("scan");

    let (want_index, want_end) = index_direct(bytes);
    let got_index: Vec<String> = map
        .index
        .iter()
        .map(|r| serde_json::to_string(r).unwrap())
        .collect();
    assert_eq!(got_index, want_index, "骨架索引");
    assert_eq!(map.end, want_end, "end");

    let mut want_inputs = Vec::new();
    crate::observe::user_inputs::scan_user_inputs(bytes, 0, |r| {
        want_inputs.push(serde_json::to_string(r).unwrap());
        Ok(())
    })
    .unwrap();
    let got_inputs: Vec<String> = map
        .inputs
        .iter()
        .map(|r| serde_json::to_string(r).unwrap())
        .collect();
    assert_eq!(got_inputs, want_inputs, "用户输入");
    assert!(got_inputs.len() >= 3, "夹具里该有几句用户输入");

    let mut want_turns = Vec::new();
    crate::observe::turns::scan_turns(bytes, 0, |r| {
        want_turns.push(serde_json::to_string(r).unwrap());
        Ok(())
    })
    .unwrap();
    let got_turns: Vec<String> = map
        .turns
        .iter()
        .map(|r| serde_json::to_string(r).unwrap())
        .collect();
    assert_eq!(got_turns, want_turns, "轮次");

    let limits = crate::observe::facts_query::ContextLimits::default();
    let want_facts = crate::observe::facts_query::scan_facts(
        bytes,
        crate::observe::facts_query::SessionFacts::default(),
        &limits,
        None,
    )
    .unwrap();
    assert_eq!(map.facts(&limits, None), want_facts, "事实");
    assert!(
        want_facts.forked_from.is_some() && want_facts.usage.is_some(),
        "夹具该带分叉与用量"
    );

    for n in [0usize, 1, 2, 5, 1000] {
        let want = crate::observe::history_query::tail_plan_of(bytes, n).unwrap();
        assert_eq!(map.tail(n), want, "尾段 n={n}");
    }
}

#[test]
fn r2_unchanged_is_a_hit_changed_is_a_rescan() {
    let d = sandbox("r2");
    let p = d.join("s.jsonl");
    std::fs::write(&p, fixture(2)).unwrap();
    let scans = RecordScans::new(4, 64 << 20);
    let a = scans.get(&p).unwrap();
    let b = scans.get(&p).unwrap();
    assert!(Arc::ptr_eq(&a, &b), "没变也重扫了");
    let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
    f.write_all("\n{\"type\":\"user\",\"uuid\":\"late\",\"message\":{\"role\":\"user\",\"content\":\"后来的一句\"}}\n".as_bytes())
        .unwrap();
    drop(f);
    let c = scans.get(&p).unwrap();
    assert!(!Arc::ptr_eq(&a, &c), "变长了还拿旧的");
    assert!(c.end > a.end);
    assert!(c.inputs.iter().any(|r| r.uuid == "late"), "新内容没读到");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn r3_entry_cap_evicts_the_least_recent_and_oversized_is_not_kept() {
    let d = sandbox("r3");
    let paths: Vec<PathBuf> = (0..3)
        .map(|i| {
            let p = d.join(format!("s{i}.jsonl"));
            std::fs::write(&p, fixture(1 + i)).unwrap();
            p
        })
        .collect();
    let scans = RecordScans::new(2, 64 << 20);
    let a0 = scans.get(&paths[0]).unwrap();
    let a1 = scans.get(&paths[1]).unwrap();
    assert!(
        Arc::ptr_eq(&a0, &scans.get(&paths[0]).unwrap()),
        "0 还在才对"
    );
    let _a2 = scans.get(&paths[2]).unwrap();
    assert!(
        Arc::ptr_eq(&a0, &scans.get(&paths[0]).unwrap()),
        "刚用过的 0 被淘汰了"
    );
    assert!(
        !Arc::ptr_eq(&a1, &scans.get(&paths[1]).unwrap()),
        "最久没用的 1 没被淘汰"
    );

    let tiny = RecordScans::new(4, 1);
    let x = tiny.get(&paths[0]).unwrap();
    assert!(
        !Arc::ptr_eq(&x, &tiny.get(&paths[0]).unwrap()),
        "超字节上限的也留下了"
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn r4_concurrent_cold_opens_scan_once() {
    let d = sandbox("r4");
    let p = d.join("s.jsonl");
    std::fs::write(&p, fixture(4000)).unwrap();
    let scans = RecordScans::new(4, 256 << 20);
    let got: Vec<Arc<ScanMap>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..8).map(|_| s.spawn(|| scans.get(&p).unwrap())).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert!(
        got.iter().all(|m| Arc::ptr_eq(m, &got[0])),
        "同时冷开扫了不止一遍"
    );
    std::fs::remove_dir_all(&d).ok();
}

/// R5：只看不扫 —— 没留着 / 变了 ⇒ `None`（调用方走自己那条便宜的路，不等整遍扫描）；留着且没变 ⇒ 那一张。
#[test]
fn r5_peek_never_scans() {
    let d = sandbox("r5");
    let p = d.join("s.jsonl");
    std::fs::write(&p, fixture(1)).unwrap();
    let scans = RecordScans::new(4, 64 << 20);
    assert!(scans.peek(&p).is_none(), "没扫过就有了");
    let a = scans.get(&p).unwrap();
    assert!(Arc::ptr_eq(&a, &scans.peek(&p).expect("扫过却看不到")));
    std::fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(b"\n")
        .unwrap();
    assert!(scans.peek(&p).is_none(), "变了还交旧的");
    std::fs::remove_dir_all(&d).ok();
}

/// R5：回退重发 ⇒ 扫描图带主线外清单（`history-branch` 读它），「你说过的话」与轮次里不再有被回退掉的那一句。
#[test]
fn r5_retracted_lines_are_listed_and_left_out_of_inputs_and_turns() {
    let rows = [
        r#"{"type":"user","uuid":"u1","parentUuid":null,"timestamp":"2026-10-01T00:00:00.000Z","message":{"role":"user","content":"first ask"}}"#,
        r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"2026-10-01T00:00:01.000Z","message":{"model":"m-x","stop_reason":"end_turn","content":[{"type":"text","text":"ok"}]}}"#,
        r#"{"type":"user","uuid":"u2","parentUuid":"a1","timestamp":"2026-10-01T00:00:02.000Z","message":{"role":"user","content":"old wording"}}"#,
        r#"{"type":"assistant","uuid":"a2","parentUuid":"u2","timestamp":"2026-10-01T00:00:03.000Z","message":{"model":"m-x","stop_reason":"end_turn","content":[{"type":"text","text":"r2"}]}}"#,
        r#"{"type":"user","uuid":"u3","parentUuid":"a1","timestamp":"2026-10-01T00:00:04.000Z","message":{"role":"user","content":"new wording"}}"#,
        r#"{"type":"assistant","uuid":"a3","parentUuid":"u3","timestamp":"2026-10-01T00:00:05.000Z","message":{"model":"m-x","stop_reason":"end_turn","content":[{"type":"text","text":"r3"}]}}"#,
    ];
    let text = rows.join("\n") + "\n";
    let bytes = text.as_bytes();
    let chain = crate::agents::claudecode::RECORDS.chain;
    let map = ScanMap::scan_with(bytes, bytes.len() as u64, chain).expect("scan");
    assert_eq!(map.off, ["u2", "a2"]);
    let inputs: Vec<&str> = map.inputs.iter().map(|r| r.uuid.as_str()).collect();
    assert_eq!(inputs, ["u1", "u3"]);
    let turns: Vec<&str> = map.turns.iter().map(|t| t.uuid.as_str()).collect();
    assert_eq!(turns, ["u1", "u3"]);
}
