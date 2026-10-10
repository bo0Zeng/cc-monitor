//! `--text` 通用拼字：只认顶上的 `text` 与每一处 `rows`，不按业务写。
use super::*;

#[test]
fn it_ships_the_written_cells_of_any_reply() {
    let rows = |a: &str| serde_json::json!([[{"text": a, "tone": "plain"}, {"text": "订阅", "tone": "plain"}], [{"text": "5h", "tone": "plain"}, {"text": "63%", "tone": "plain"}, {"text": "↻18:30", "tone": "plain"}], [{"text": "超额", "tone": "plain"}, {"text": "—", "tone": "plain"}]]);
    let reply = serde_json::json!({"text": "顶上一句", "items": [{"rows": rows("a"), "pct": 3}, {"rows": rows("中文")}], "now": 1});
    assert_eq!(
        ship_text(&reply),
        "顶上一句\n\na  订阅\n  5h    63%  ↻18:30\n  超额  —\n\n中文  订阅\n  5h    63%  ↻18:30\n  超额  —"
    );
    assert_eq!(
        ship_text(&serde_json::json!({"pct": 3, "blocks": [{"text": "原文是值"}]})),
        "",
        "值不出"
    );
}
