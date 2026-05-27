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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_line_returns_none() {
        assert!(parse_line("").unwrap().is_none());
        assert!(parse_line("   ").unwrap().is_none());
        assert!(parse_line("\n").unwrap().is_none());
    }

    #[test]
    fn bom_only_line_returns_none() {
        // 纯 BOM 行：trim 后空
        assert!(parse_line("\u{feff}").unwrap().is_none());
        assert!(parse_line("\u{feff}   \n").unwrap().is_none());
    }

    #[test]
    fn bom_prefix_does_not_corrupt_type() {
        // v1.7.8 教训：PS 5.1 Out-File -Encoding utf8 写 BOM，serde 不剥就解析失败 →
        // cc 集成"装上没用"7 个版本。parse_line 必须先剥 BOM 再 from_str。
        let bom_user = "\u{feff}{\"type\":\"custom-title\",\"customTitle\":\"hi\",\"sessionId\":\"s1\"}";
        let r = parse_line(bom_user).unwrap().expect("应该解析成功");
        assert!(matches!(r, JsonlRecord::CustomTitle { .. }), "BOM 后类型识别失败：{r:?}");
    }

    #[test]
    fn malformed_json_returns_err() {
        // 半截 JSON / 非法语法 → Err，不 panic（caller 决定容错策略）
        assert!(parse_line("{ not json").is_err());
        assert!(parse_line("{\"type\":\"user\",").is_err());
    }

    #[test]
    fn unknown_type_falls_through_to_unknown_variant() {
        // 未知 `type` 字段命中 #[serde(other)] Unknown 而非 panic
        let r = parse_line(r#"{"type":"some-future-record-type","foo":42}"#)
            .unwrap()
            .expect("Unknown variant 也是 Ok(Some(_))");
        assert!(matches!(r, JsonlRecord::Unknown));
        assert!(!r.is_displayable(), "Unknown 不应 emit");
    }
}
