//! 一轮的摘要（`turns.rs`）：子运行的记录不算进主线的轮 · 从某一轮的 `at` 接着取只出那一轮起的 · 你那句与回复头的截法。
//! 夹具只造结构（占位词），不采会话正文。成品的整形由跨语言金样管（`read_face_tests.rs`）。

use super::*;

fn scan(body: &str, from: u64) -> Vec<TurnRow> {
    let mut out = Vec::new();
    let r = std::io::Cursor::new(body.as_bytes()[from as usize..].to_vec());
    scan_turns(r, from, |t| {
        out.push(TurnRow {
            conclusion: t.conclusion.clone(),
            said: t.said.clone(),
            reply: t.reply.clone(),
            start: t.start.clone(),
            end: t.end.clone(),
            uuid: t.uuid.clone(),
            ..*t
        });
        Ok(())
    })
    .unwrap();
    out
}

#[test]
fn subrun_records_do_not_count_and_from_resumes_at_a_turn() {
    let lines = [
        r#"{"type":"user","uuid":"a","timestamp":"t1","message":{"content":"q"}}"#.to_string(),
        r#"{"type":"assistant","uuid":"b","timestamp":"t2","isSidechain":true,"agentId":"ag","message":{"content":[{"type":"tool_use","id":"x","name":"Read","input":{}}]}}"#.to_string(),
        r#"{"type":"assistant","uuid":"c","timestamp":"t3","message":{"content":[{"type":"tool_use","id":"y","name":"Read","input":{}}]}}"#.to_string(),
        format!(r#"{{"type":"user","uuid":"d","timestamp":"t4","message":{{"content":"{}"}}}}"#, "字".repeat(SAID_MAX + 5)),
        r#"{"type":"assistant","uuid":"e","timestamp":"t5","message":{"content":[{"type":"text","text":"x"}]}}"#.to_string(),
    ];
    let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
    let all = scan(&body, 0);
    assert_eq!(
        all.iter()
            .map(|t| (t.uuid.as_str(), t.tools, t.done))
            .collect::<Vec<_>>(),
        [("a", 1, true), ("d", 0, false)]
    );
    assert_eq!(
        all[1].said.chars().count(),
        SAID_MAX + 1,
        "截到 50 字 ＋ 省略号"
    );
    assert_eq!(all[1].conclusion, ["e"]);
    let again = scan(&body, all[1].at);
    assert_eq!(again.len(), 1);
    assert_eq!((again[0].uuid.as_str(), again[0].at), ("d", all[1].at));
}
