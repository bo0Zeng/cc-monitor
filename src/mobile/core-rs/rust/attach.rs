//! 手机聊天输入框上方那一排附件的字：Kotlin 交此刻每张卡走到哪 ＋ 断着时那台离线那一句 ＋ 手机这边的错，
//! 这里写好卡上第二行 · ✕ 的读屏名 · 卡下那一条（那一句 ＋ 按钮 ＋ 是不是虚线），Kotlin 照摆。收不收 · 上限 · 那台那一句仍是那台的。
//!
//! 入：`{cards: [{name, phase, said?}], offline: "<那台离线那一句>" | null, local: "no_room" | "no_camera" | null}`，
//! `phase` ∈ `queued` · `moving` · `landed` · `refused` · `failed` · `lost`（本机副本开不了）；`offline` 只在有排队的、又断着时给。
//! 出：`{cards: [{line, remove}], note: {text, act, dashed} | null}`，`act` ∈ `retry` · `repick` · `null`。
//! 那一条的先后：手机这边的错 › 传 · 落没成的（几份 ⇒ 数 ＋ 第一份的因）› 断着。开传前那台就拒的因写在卡上，不进那一条。

use copy_core::copy_text;
use serde_json::{json, Value};

/// 一张卡上第二行：排队 · 失败 · 那台不收的那一句；传输中（只转环，不写进度数字）· 落好了 ⇒ 不写。
fn line(card: &Value) -> Option<String> {
    match card.get("phase").and_then(Value::as_str)? {
        "queued" => Some(copy_text("mobAttach.card.queued", &[])),
        "failed" | "lost" => Some(copy_text("mobAttach.card.failed", &[])),
        "refused" => card.get("said").and_then(Value::as_str).map(str::to_owned),
        _ => None,
    }
}

/// 卡下那一条；什么都没有 ⇒ `None`。
fn note(want: &Value, cards: &[Value]) -> Option<Value> {
    let local = match want.get("local").and_then(Value::as_str) {
        Some("no_room") => Some((copy_text("mobAttach.note.local", &[]), Some("repick"))),
        Some("no_camera") => Some((copy_text("mobAttach.note.noCamera", &[]), None)),
        _ => None,
    };
    if let Some((text, act)) = local {
        return Some(json!({ "text": text, "act": act, "dashed": false }));
    }
    let str_of = |c: &Value, k: &str| {
        c.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let failed: Vec<String> = cards
        .iter()
        .filter_map(|c| match c.get("phase").and_then(Value::as_str) {
            Some("failed") => Some(str_of(c, "said")),
            // 本机那一份副本开不了：照分块那一半读不动本机文件的那一句。
            Some("lost") => Some(copy_text(
                "mobFiles.local.readFailed",
                &[("name", &str_of(c, "name"))],
            )),
            _ => None,
        })
        .collect();
    if let Some(said) = failed.first().map(String::as_str) {
        let text = if failed.len() == 1 {
            copy_text("mobAttach.note.failed", &[("said", said)])
        } else {
            copy_text(
                "mobAttach.note.failedMany",
                &[("n", &failed.len().to_string()), ("said", said)],
            )
        };
        return Some(json!({ "text": text, "act": "retry", "dashed": false }));
    }
    let offline = want.get("offline").and_then(Value::as_str)?;
    Some(json!({
        "text": copy_text("mobAttach.note.offline", &[("said", offline)]),
        "act": null,
        "dashed": true,
    }))
}

/// 那一排此刻的字（见模块头）；读不动 ⇒ `None`。
pub fn face(want: &Value) -> Option<Value> {
    let cards = want.get("cards")?.as_array()?;
    let rows: Vec<Value> = cards
        .iter()
        .map(|c| {
            let name = c.get("name").and_then(Value::as_str).unwrap_or_default();
            json!({ "line": line(c), "remove": copy_text("mobAttach.card.remove", &[("name", name)]) })
        })
        .collect();
    Some(json!({ "cards": rows, "note": note(want, cards) }))
}

/// [`face`] 的 JSON 进出；读不动 ⇒ 空串。
pub fn face_json(want: &str) -> String {
    serde_json::from_str::<Value>(want)
        .ok()
        .and_then(|w| face(&w))
        .map(|v| v.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(phase: &str, said: Option<&str>) -> Value {
        json!({ "name": "a.png", "phase": phase, "said": said })
    }

    fn of(want: Value) -> Value {
        face(&want).expect("读得动")
    }

    #[test]
    fn each_card_says_where_it_is_and_moving_or_landed_ones_say_nothing() {
        let v = of(json!({ "cards": [
            card("queued", None), card("moving", None), card("landed", None),
            card("failed", Some("磁盘满")), card("refused", Some("超上限")),
        ]}));
        let lines: Vec<Value> = v["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["line"].clone())
            .collect();
        assert_eq!(
            lines,
            vec![
                json!(copy_text("mobAttach.card.queued", &[])),
                // 传输中只转环，不写进度数字。
                Value::Null,
                Value::Null,
                json!(copy_text("mobAttach.card.failed", &[])),
                json!("超上限"),
            ]
        );
        assert_eq!(
            v["cards"][0]["remove"],
            json!(copy_text("mobAttach.card.remove", &[("name", "a.png")]))
        );
    }

    #[test]
    fn the_bar_counts_alike_failures_and_says_the_first_reason() {
        let one = of(
            json!({ "cards": [card("failed", Some("磁盘满")), card("refused", Some("超上限"))] }),
        );
        assert_eq!(
            one["note"],
            json!({ "text": copy_text("mobAttach.note.failed", &[("said", "磁盘满")]), "act": "retry", "dashed": false })
        );
        let two =
            of(json!({ "cards": [card("failed", Some("磁盘满")), card("failed", Some("别的"))] }));
        assert_eq!(
            two["note"]["text"],
            json!(copy_text(
                "mobAttach.note.failedMany",
                &[("n", "2"), ("said", "磁盘满")]
            ))
        );
    }

    #[test]
    fn a_lost_local_copy_reads_like_the_chunk_loop_s_own_read_failure() {
        let v = of(json!({ "cards": [card("lost", None)] }));
        assert_eq!(
            v["note"]["text"],
            json!(copy_text(
                "mobAttach.note.failed",
                &[(
                    "said",
                    &copy_text("mobFiles.local.readFailed", &[("name", "a.png")])
                )]
            ))
        );
        assert_eq!(v["note"]["act"], json!("retry"));
    }

    #[test]
    fn local_beats_failures_and_failures_beat_offline() {
        let all = json!({ "cards": [card("failed", Some("磁盘满")), card("queued", None)], "offline": "devbox 离线", "local": "no_room" });
        assert_eq!(
            of(all.clone())["note"],
            json!({ "text": copy_text("mobAttach.note.local", &[]), "act": "repick", "dashed": false })
        );
        let mut no_local = all;
        no_local["local"] = Value::Null;
        assert_eq!(of(no_local)["note"]["act"], json!("retry"));
        let off = of(json!({ "cards": [card("queued", None)], "offline": "devbox 离线" }));
        assert_eq!(
            off["note"],
            json!({ "text": copy_text("mobAttach.note.offline", &[("said", "devbox 离线")]), "act": null, "dashed": true })
        );
        assert_eq!(
            of(json!({ "cards": [card("refused", Some("超上限"))] }))["note"],
            Value::Null
        );
        assert_eq!(
            of(json!({ "cards": [], "local": "no_camera" }))["note"]["act"],
            Value::Null
        );
    }

    #[test]
    fn unreadable_input_is_an_empty_line() {
        assert_eq!(face_json("nope"), "");
        assert_eq!(face_json("{}"), "");
    }
}
