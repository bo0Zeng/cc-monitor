//! 终端 `--text` 对金样 `tests/__fixtures__/quota-text.golden.json` 逐字：每号每行每格 ＋ 整段字。
//! 界面那一侧（悬停卡的行模型）在 `tests/frontend/ui/quota-lines.vitest.ts` 对同一份。

use super::{blocks, text};
use serde_json::Value;

fn golden() -> Vec<Value> {
    let raw = include_str!("../../__fixtures__/quota-text.golden.json");
    let v: Value = serde_json::from_str(raw).expect("golden parses");
    v["cases"].as_array().expect("cases").clone()
}

#[test]
fn every_golden_case_matches_cell_for_cell_and_as_text() {
    let cases = golden();
    assert!(cases.len() >= 10, "金样空转：只有 {} 条", cases.len());
    for c in &cases {
        let name = c["name"].as_str().unwrap_or_default();
        let tz = c["tzOffsetMin"].as_i64().expect("tz");
        let machine = c["machine"].as_str().expect("machine");
        let got: Vec<Value> = blocks(&c["reply"], tz, machine)
            .into_iter()
            .map(|(account, rows)| serde_json::json!({ "account": account, "rows": rows }))
            .collect();
        assert_eq!(Value::Array(got), c["blocks"], "{name}：行不对");
        assert_eq!(
            text(&c["reply"], tz, machine),
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
