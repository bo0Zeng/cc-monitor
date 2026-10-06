//! 一轮的摘要（`turns.rs`）：子运行的记录不算进主线的轮 · 从某一轮的 `at` 接着取只出那一轮起的 · 你那句与回复头的截法（只取正文行：代码块整块不算，只有代码 ⇒「仅代码」）。
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

#[test]
fn reply_head_is_prose_only() {
    // 截图那一形：一句 · 空行 · 整块代码 · 再一句 ⇒ 代码整块不算（围栏与里面的行），空行不算。
    let rec = |text: &str| {
        let lines = [
            r#"{"type":"user","uuid":"a","timestamp":"t1","message":{"content":"q"}}"#.to_string(),
            serde_json::json!({"type":"assistant","uuid":"b","timestamp":"t2","message":{"content":[{"type":"text","text":text}]}}).to_string(),
        ];
        lines.iter().map(|l| format!("{l}\n")).collect::<String>()
    };
    let mixed = rec("改好了。小结：\n\n```python\nclient = X(retries=3)\n```\n\n全量测试通过。\n文档也加了。\n第四行");
    assert_eq!(
        scan(&mixed, 0)[0].reply,
        "改好了。小结：\n全量测试通过。\n文档也加了。"
    );
    // 行内排版记号去掉，只留字。
    let marked = rec("## 小结\n全量测试 **213 passed**。`docs/config.md` 里加了说明。\n> 引用一句");
    assert_eq!(
        scan(&marked, 0)[0].reply,
        "小结\n全量测试 213 passed。docs/config.md 里加了说明。\n引用一句"
    );
    // 只有代码 ⇒ 一个词，不露代码原文。
    let only = rec("```sh\nrm -rf build\n```");
    assert_eq!(
        scan(&only, 0)[0].reply,
        copy_text("rsTurns.reply.codeOnly", &[])
    );
}
