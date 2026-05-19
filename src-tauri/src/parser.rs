use crate::messages::JsonlRecord;

/// 解析单行 JSONL。
/// 处理 UTF-8 BOM、空行；其余错误向上抛。
/// M1 落地：增量按行解析、记录 unknown type 的告警。
pub fn parse_line(raw: &str) -> Result<Option<JsonlRecord>, serde_json::Error> {
    let trimmed = raw.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let record: JsonlRecord = serde_json::from_str(trimmed)?;
    Ok(Some(record))
}
