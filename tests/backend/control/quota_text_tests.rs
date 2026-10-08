//! 终端 `--text` 对金样 `tests/__fixtures__/quota-text.golden.json` 逐字：每号每行每格 ＋ 整段字。
//! 界面那一侧（悬停卡的行模型）在 `tests/frontend/ui/quota-lines.vitest.ts` 对同一份。

use super::{blocks, text};
use serde_json::Value;

fn golden() -> Vec<Value> {
    let raw = include_str!("../../__fixtures__/quota-text.golden.json");
    let v: Value = serde_json::from_str(raw).expect("golden parses");
    v["cases"].as_array().expect("cases").clone()
}

/// 去掉回包里所有 `…Text` 那几格。
fn without_texts(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, _)| !k.ends_with("Text"))
                .map(|(k, x)| (k.clone(), without_texts(x)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(without_texts).collect()),
        x => x.clone(),
    }
}

/// 金样回包里那几格时刻的字 == 出口那一遍按 `tzOffsetMin` 现写的（金样不许手写一份与出口不同的字）。
#[test]
fn golden_time_texts_are_what_the_reply_exit_writes() {
    for c in &golden() {
        let tz = c["tzOffsetMin"].as_i64().expect("tz");
        let now = c["reply"]["now"].as_i64().expect("now");
        let mut v = without_texts(&c["reply"]);
        crate::common::time::with_texts(&mut v, now, tz);
        assert_eq!(v, c["reply"], "{}", c["name"]);
    }
}

#[test]
fn every_golden_case_matches_cell_for_cell_and_as_text() {
    let cases = golden();
    assert!(cases.len() >= 10, "金样空转：只有 {} 条", cases.len());
    for c in &cases {
        let name = c["name"].as_str().unwrap_or_default();
        let machine = c["machine"].as_str().expect("machine");
        let got: Vec<Value> = blocks(&c["reply"], machine)
            .into_iter()
            .map(|(account, rows)| serde_json::json!({ "account": account, "rows": rows }))
            .collect();
        assert_eq!(Value::Array(got), c["blocks"], "{name}：行不对");
        assert_eq!(
            text(&c["reply"], machine),
            c["text"].as_str().expect("text"),
            "{name}：字不对"
        );
    }
}

/// 别的命令带 `--text` ⇒ `bad_args`（在读 stdin 之前就拒，不挂住）。
#[tokio::test]
async fn text_on_any_other_command_is_bad_args() {
    let args = vec!["--rotation-read".to_string(), "--text".to_string()];
    assert_eq!(crate::control::cli_control::run(&args).await, 2);
}
