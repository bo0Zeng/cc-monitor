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

impl ScanMap {
    /// 从字节 0 读 `r`，至多读 `len` 字节；`chain` 是这份记录那一家的链事实面（注册表 `RecordFace.chain`）：给了就顺手算主线外清单。
    /// 判据用（产品那一路经 [`RecordScans::get`]，同一个 [`Scan`]）。
    pub(crate) fn scan_with<R: std::io::BufRead>(
        r: R,
        len: u64,
        kind: &str,
        face: Option<crate::agents::RecordFace>,
    ) -> std::io::Result<ScanMap> {
        let mut s = Scan::default();
        s.feed(r, len, kind, face)?;
        Ok(s.done(len))
    }
}

impl RecordScans {
    /// 等旁线程上的骨架都落完（判据「重启后」那一步之前用；进程里别处不等它）。
    pub(crate) fn flush(&self) {
        let pending = std::mem::take(&mut *self.writes.lock().unwrap_or_else(|e| e.into_inner()));
        for (p, h) in pending {
            if h.join().is_err() {
                tracing::debug!("skeleton write for {} panicked", p.display());
            }
        }
    }
}

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

/// 逐行各问各的（每一样只调它自己那一处口径，不经扫描图）：骨架索引行 · 用户输入 · 轮次，外加 end。
fn each_on_its_own(bytes: &[u8]) -> (Vec<String>, Vec<String>, Vec<String>, u64) {
    let kind = crate::agents::record_tree_kind().unwrap_or_default();
    let (mut index, mut inputs, mut turns) = (Vec::new(), Vec::new(), Vec::new());
    let mut scan = crate::observe::turns::TurnScan::default();
    let mut lines = crate::observe::lines::Lines::new(bytes, 0);
    while let Some(row) = lines.next().unwrap() {
        let body = row.body();
        if crate::observe::lines::counts(body) {
            let r = crate::observe::history_query::index_row_of(
                kind,
                crate::observe::lines::parse(body).as_ref(),
                row.at,
                row.whole.len() as u64,
            );
            index.push(serde_json::to_string(&r).unwrap());
        }
        let Some(v) = crate::observe::lines::parse(body) else {
            continue;
        };
        if let Some(mut r) = crate::observe::user_inputs::user_input_of(&v) {
            r.at = row.at;
            inputs.push(serde_json::to_string(&r).unwrap());
        }
        if let Some(t) = scan.line(row.at, &v) {
            turns.push(serde_json::to_string(&t).unwrap());
        }
    }
    turns.extend(scan.finish().map(|t| serde_json::to_string(&t).unwrap()));
    (index, inputs, turns, lines.end())
}

#[test]
fn r1_one_pass_equals_each_scan_on_its_own() {
    let text = fixture(3);
    let bytes = text.as_bytes();
    let map = ScanMap::scan_with(
        bytes,
        bytes.len() as u64,
        crate::agents::record_tree_kind().unwrap_or_default(),
        None,
    )
    .expect("scan");

    let (want_index, want_inputs, want_turns, want_end) = each_on_its_own(bytes);
    let got_index: Vec<String> = map
        .index
        .iter()
        .map(|r| serde_json::to_string(r).unwrap())
        .collect();
    assert_eq!(got_index, want_index, "骨架索引");
    assert_eq!(map.end, want_end, "end");

    let got_inputs: Vec<String> = map
        .inputs
        .iter()
        .map(|r| serde_json::to_string(r).unwrap())
        .collect();
    assert_eq!(got_inputs, want_inputs, "用户输入");
    assert!(got_inputs.len() >= 3, "夹具里该有几句用户输入");

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
        assert_eq!(map.tail(n, want.file.clone()), want, "尾段 n={n}");
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
    let chain = Some(crate::agents::claudecode::RECORDS);
    let map = ScanMap::scan_with(
        bytes,
        bytes.len() as u64,
        crate::agents::record_tree_kind().unwrap_or_default(),
        chain,
    )
    .expect("scan");
    assert_eq!(map.off, ["u2", "a2"]);
    let inputs: Vec<&str> = map.inputs.iter().map(|r| r.uuid.as_str()).collect();
    assert_eq!(inputs, ["u1", "u3"]);
    let turns: Vec<&str> = map.turns.iter().map(|t| t.uuid.as_str()).collect();
    assert_eq!(turns, ["u1", "u3"]);
}

/// 一张图的全部样子（线上那几样 ＋ 线上不带、续扫要用的那几格），比「接着扫的」与「整份扫的」用。
fn whole(map: &ScanMap) -> String {
    let limits = crate::observe::facts_query::ContextLimits::default();
    let rows = |v: &[TurnRow]| -> Vec<String> {
        v.iter()
            .map(|r| {
                format!(
                    "{} {:?} {} {:?}",
                    serde_json::to_string(r).unwrap(),
                    r.pending,
                    r.tail_fold,
                    r.folds
                )
            })
            .collect()
    };
    format!(
        "end {} | starts {:?} | index {} | inputs {} | turns {:?} | off {:?} | facts {:?} | tail {:?}",
        map.end,
        map.starts,
        serde_json::to_string(&map.index).unwrap(),
        serde_json::to_string(&map.inputs).unwrap(),
        rows(&map.turns),
        map.off,
        map.facts(&limits, None),
        map.tail(3, String::new()),
    )
}

/// 带分叉、带一轮没收尾的合成记录（结构性的假行）。
fn branched(turns: usize) -> String {
    let mut s = fixture(turns);
    s.truncate(s.rfind('\n').unwrap() + 1); // 去掉残尾
    s.push_str(r#"{"type":"user","uuid":"b1","parentUuid":"e0","timestamp":"2026-10-01T01:00:00.000Z","message":{"role":"user","content":"回退前那句"}}"#);
    s.push('\n');
    s.push_str(r#"{"type":"user","uuid":"b2","parentUuid":"e0","timestamp":"2026-10-01T01:00:05.000Z","message":{"role":"user","content":"回退后这句"}}"#);
    s.push('\n');
    s.push_str(r#"{"type":"assistant","uuid":"b25","parentUuid":"b2","timestamp":"2026-10-01T01:00:05.500Z","message":{"model":"m-x","content":[{"type":"thinking","thinking":"想"},{"type":"tool_use","id":"tq","name":"Bash","input":{"command":"pwd"}}]}}"#);
    s.push('\n');
    s.push_str(r#"{"type":"user","uuid":"b26","parentUuid":"b25","timestamp":"2026-10-01T01:00:05.700Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tq","content":"/w"}]}}"#);
    s.push('\n');
    s.push_str(r#"{"type":"assistant","uuid":"b3","parentUuid":"b26","timestamp":"2026-10-01T01:00:06.000Z","message":{"model":"m-x","content":[{"type":"text","text":"还在跑"},{"type":"tool_use","id":"tb","name":"Bash","input":{"command":"ls"}}]}}"#);
    s.push('\n');
    s.push_str(r#"{"type":"user","uuid":"b4","parentUuid":"b3","timestamp":"2026-10-01T01:01:00.000Z","message":{"role":"user","content":"下一句"}}"#);
    s.push('\n');
    s.push_str(r#"{"type":"assistant","uuid":"b5","parentUuid":"b4","timestamp":"2026-10-01T01:01:01.000Z","message":{"model":"m-x","content":[{"type":"tool_use","id":"tc","name":"Bash","input":{"command":"ls"}}]}}"#);
    s.push('\n');
    s
}

/// R6（骨架 ＋ 还没收尾那一轮）：在任意一行处停下、把扫到一半的样子（含还没收尾那一轮）落成骨架再读回来、接着扫完 ⇒
/// 与整份一遍扫的逐格相同（主线外清单在后半截才出现时，前半截的用户输入与轮次照新的清单重滤）。
#[test]
fn r6_resuming_from_any_line_through_the_skeleton_equals_one_pass() {
    let text = branched(3);
    let bytes = text.as_bytes();
    let kind = crate::agents::record_tree_kind().unwrap_or_default();
    let chain = Some(crate::agents::claudecode::RECORDS);
    let full = ScanMap::scan_with(bytes, bytes.len() as u64, kind, chain).unwrap();
    assert!(!full.off.is_empty(), "夹具该有主线外的");
    assert!(
        full.turns.iter().any(|t| t.done && !t.folds.is_empty()),
        "夹具该有收了尾、带过程段的一轮"
    );
    let cuts: Vec<usize> = std::iter::once(0)
        .chain(
            bytes
                .iter()
                .enumerate()
                .filter(|(_, b)| **b == b'\n')
                .map(|(i, _)| i + 1),
        )
        .collect();
    for &cut in &cuts {
        let head = ScanMap::scan_with(&bytes[..cut], cut as u64, kind, chain).unwrap();
        let mark = Mark {
            len: cut as u64,
            mtime: None,
            cursor: head.end,
            witness: head.rest.witness.clone(),
        };
        let raw = serde_json::to_vec(&SkelView {
            mark,
            scan: head.view(),
        })
        .unwrap();
        let back: Skel = serde_json::from_slice(&raw).expect("骨架读不回来");
        let start = back.scan.end as usize;
        let mut s = back.scan;
        s.feed(&bytes[start..], bytes.len() as u64, kind, chain)
            .unwrap();
        assert_eq!(
            whole(&s.done(bytes.len() as u64)),
            whole(&full),
            "从第 {cut} 字节接着扫的跟一遍扫的不一样"
        );
        // 进程里留着的那张直接接着扫（不过盘）也一样。
        let mut s = head.resume();
        s.feed(&bytes[start..], bytes.len() as u64, kind, chain)
            .unwrap();
        assert_eq!(whole(&s.done(bytes.len() as u64)), whole(&full));
    }
}

fn append(p: &std::path::Path, s: &str) {
    let mut f = std::fs::OpenOptions::new().append(true).open(p).unwrap();
    f.write_all(s.as_bytes()).unwrap();
}

/// R7（骨架落盘，数字节）：冷开扫描图照骨架 —— 重启后没变 ⇒ 读 0 字节、图与整份扫的相同；追加 k 字节 ⇒ 读 k ＋ 见证；
/// 改写过（同长）⇒ 整份。进程里留着的那张遇到追加也只读 k ＋ 见证（不再按长度作废整份重扫）。
#[test]
fn r7_cold_scan_reads_nothing_unchanged_and_only_the_appended_bytes() {
    let (d, own) = crate::observe::skeleton::tests::sandbox("scan-r7");
    let p = d.join("s.jsonl");
    let text = branched(4);
    std::fs::write(&p, &text).unwrap();
    let home = || Skeletons::at(own.clone(), 0);
    let probe = crate::observe::skeleton::WITNESS;

    let first = RecordScans::new(4, 64 << 20).with_skeletons(home());
    let a = first.get(&p).unwrap();
    assert_eq!(first.bytes_read(), text.len() as u64, "头一次读整份");
    first.flush();

    let restarted = RecordScans::new(4, 64 << 20).with_skeletons(home());
    let b = restarted.get(&p).unwrap();
    assert_eq!(restarted.bytes_read(), 0, "重启后没变，还是读了记录");
    assert_eq!(whole(&b), whole(&a), "照骨架出的图跟扫出来的不一样");

    let more = "{\"type\":\"user\",\"uuid\":\"late\",\"parentUuid\":\"b3\",\"timestamp\":\"2026-10-01T02:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"后来的一句\"}}\n";
    append(&p, more);
    let k = more.len() as u64;
    let grown = RecordScans::new(4, 64 << 20).with_skeletons(home());
    let c = grown.get(&p).unwrap();
    assert_eq!(grown.bytes_read(), k + probe, "追加 k 字节应只读 k ＋ 见证");
    grown.flush();
    let all = std::fs::read(&p).unwrap();
    let fresh = ScanMap::scan_with(
        &all[..],
        all.len() as u64,
        crate::agents::record_tree_kind().unwrap_or_default(),
        Some(crate::agents::claudecode::RECORDS),
    )
    .unwrap();
    assert_eq!(whole(&c), whole(&fresh), "接着骨架扫的跟整份扫的不一样");
    assert!(c.inputs.iter().any(|r| r.uuid == "late"));

    // 进程里留着的那张（不落盘的那一份）：再追加一行，只读新的那一截 ＋ 见证。
    let grown = RecordScans::new(4, 64 << 20);
    grown.get(&p).unwrap();
    let before = grown.bytes_read();
    let again = more
        .replace("\"late\"", "\"later\"")
        .replace("b3", "late")
        .replace("后来的一句", "再后来一句");
    append(&p, &again);
    let e = grown.get(&p).unwrap();
    assert_eq!(
        grown.bytes_read() - before,
        again.len() as u64 + probe,
        "留着的那张遇到追加又整份重扫了"
    );
    assert!(e.inputs.iter().any(|r| r.uuid == "later"));

    // 改写（同长、内容变了）⇒ 整份。先让骨架跟上此刻这一份（接着扫的那一截没过回写门槛不会重落 ⇒ 删掉旧的、整份落一份）。
    std::fs::remove_dir_all(d.join("data").join("cache").join("skeleton")).unwrap();
    let caught_up = RecordScans::new(4, 64 << 20).with_skeletons(home());
    caught_up.get(&p).unwrap();
    caught_up.flush();
    let now = std::fs::read(&p).unwrap();
    let flipped = String::from_utf8(now.clone())
        .unwrap()
        .replace("回退后这句", "回退后那句");
    assert_eq!(flipped.len(), now.len());
    std::fs::write(&p, &flipped).unwrap();
    let f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
    f.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
        .unwrap();
    drop(f);
    let rewritten = RecordScans::new(4, 64 << 20).with_skeletons(home());
    rewritten.get(&p).unwrap();
    assert_eq!(
        rewritten.bytes_read(),
        now.len() as u64,
        "改写过还照骨架读了"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 8a b 后续：接着扫完不是每次都重落骨架 —— 接着扫的那一截过了门槛（上一次落盘长度的四分之一，至少 `MIN_LEN`）才落；
/// 没过门槛 ⇒ 盘上那份不动（下一次重启多读这一截 ＋ 见证）。
#[test]
fn r8_the_scan_skeleton_is_rewritten_only_past_the_threshold() {
    let (d, own) = crate::observe::skeleton::tests::sandbox("scan-r8");
    let p = d.join("s.jsonl");
    let text = branched(40);
    std::fs::write(&p, &text).unwrap();
    let home = || Skeletons::at(own.clone(), 0);
    // 盘上那一份照到哪了（读它的 `mark.cursor`）。
    let skel = || {
        home().load::<serde_json::Value>(&p).expect("落过骨架")["mark"]["cursor"]
            .as_u64()
            .unwrap()
    };
    let scans = RecordScans::new(4, 64 << 20).with_skeletons(home());
    scans.get(&p).unwrap();
    scans.flush();
    let first = skel();
    let line = |u: &str, parent: &str| {
        format!("{{\"type\":\"user\",\"uuid\":\"{u}\",\"parentUuid\":\"{parent}\",\"timestamp\":\"2026-10-01T03:00:00.000Z\",\"message\":{{\"role\":\"user\",\"content\":\"x\"}}}}\n")
    };
    append(&p, &line("n1", "b3"));
    scans.get(&p).unwrap();
    scans.flush();
    assert!(skel() == first, "追加一行（没过门槛）就整份重落了骨架");
    // 一口气追加过门槛（上一次落盘长度的四分之一以上）⇒ 落一份新的。
    let mut big = String::new();
    let mut prev = "n1".to_string();
    for i in 0..text.len() / 100 {
        let u = format!("m{i}");
        big.push_str(&line(&u, &prev));
        prev = u;
    }
    assert!(big.len() as u64 >= text.len() as u64 / 4);
    append(&p, &big);
    scans.get(&p).unwrap();
    scans.flush();
    assert!(skel() != first, "过了门槛还没重落");
    let _ = std::fs::remove_dir_all(&d);
}

/// R9（read8b，瓶颈 #5，数字节）：续取（轮次 / 大纲 / 索引从上次的末端续）遇到留着的图而记录又长了 ⇒ 接着扫追加的那一截
/// （读 k ＋ 见证），出的轮次与整份扫的一样。
#[test]
fn r9_a_continuation_extends_the_held_map_by_the_appended_bytes_only() {
    let d = sandbox("r9");
    let p = d.join("s.jsonl");
    std::fs::write(&p, branched(4)).unwrap();
    let scans = RecordScans::new(4, 64 << 20);
    scans.get(&p).unwrap();
    let before = scans.bytes_read();
    let more = "{\"type\":\"user\",\"uuid\":\"late\",\"parentUuid\":\"b3\",\"timestamp\":\"2026-10-01T02:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"后来的一句\"}}\n";
    append(&p, more);
    let m = scans.get(&p).expect("留着的图记录又长了，续取却拿不到");
    assert_eq!(
        scans.bytes_read() - before,
        more.len() as u64 + crate::observe::skeleton::WITNESS,
        "续取把整份又扫了一遍"
    );
    let all = std::fs::read(&p).unwrap();
    let fresh = ScanMap::scan_with(
        &all[..],
        all.len() as u64,
        crate::agents::record_tree_kind().unwrap_or_default(),
        Some(crate::agents::claudecode::RECORDS),
    )
    .unwrap();
    assert_eq!(whole(&m), whole(&fresh), "接着扫的跟整份扫的不一样");
    std::fs::remove_dir_all(&d).ok();
}
