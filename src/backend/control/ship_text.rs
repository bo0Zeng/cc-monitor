//! **装运成字**（CLI `--text`，所有命令通用）：把一份回包里核心写好的那几格拼成给人看的字 —— 不按业务写。
//!
//! 只认两种格：顶上那一句 `text`（核心写好的一句，如「读取失败 · …」「无采样」）与成品里每一处 `rows`（`[[{text, tone}]]`，
//! 一段几行几格：额度每号一段 …）。别的格（值）不出。排法：顶上那句先出；每一段首行各格隔两个空格，其余行缩进两格、
//! 非末格按本段同列最宽补齐（汉字与全角记 2 宽）、格间两个空格；段间空一行。回包里一格都没有 ⇒ 空串。

use serde_json::Value;

/// 终端里一格占几列（汉字与全角记 2）。
fn width(cell: &str) -> usize {
    cell.chars()
        .map(|c| {
            let u = c as u32;
            let wide = matches!(u,
                0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF
                | 0xA000..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6);
            if wide {
                2
            } else {
                1
            }
        })
        .sum()
}

/// 一段排成字。
fn block_text(rows: &[Vec<String>]) -> String {
    let Some((first, body)) = rows.split_first() else {
        return String::new();
    };
    let mut widths: Vec<usize> = Vec::new();
    for r in body {
        for (i, c) in r.iter().enumerate().take(r.len().saturating_sub(1)) {
            if widths.len() <= i {
                widths.resize(i + 1, 0);
            }
            widths[i] = widths[i].max(width(c));
        }
    }
    let mut lines = vec![first.join("  ")];
    for r in body {
        let cells: Vec<String> = r
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if i + 1 < r.len() {
                    format!("{c}{}", " ".repeat(widths[i] - width(c)))
                } else {
                    c.clone()
                }
            })
            .collect();
        lines.push(format!("  {}", cells.join("  ")));
    }
    lines.join("\n")
}

/// `rows` 那一格读成字（形状不对的格跳过）。
fn rows_of(v: &Value) -> Option<Vec<Vec<String>>> {
    let rows: Vec<Vec<String>> = v
        .as_array()?
        .iter()
        .filter_map(Value::as_array)
        .map(|r| {
            r.iter()
                .filter_map(|c| c.get("text").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .collect();
    (!rows.is_empty()).then_some(rows)
}

/// 回包里每一处 `rows`，按回包的次序。
fn blocks(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                match (k.as_str(), rows_of(x)) {
                    ("rows", Some(rows)) => out.push(block_text(&rows)),
                    _ => blocks(x, out),
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| blocks(x, out)),
        _ => {}
    }
}

/// ★ 一份回包 ⇒ 给人看的整段字（不带末尾换行）。
pub(crate) fn ship_text(reply: &Value) -> String {
    let mut out: Vec<String> = reply
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_string)
        .into_iter()
        .collect();
    blocks(reply, &mut out);
    out.join("\n\n")
}

#[cfg(test)]
#[path = "../../../tests/backend/control/ship_text_tests.rs"]
mod tests;
