//! 图按引用交、要画时再取（`history-image`）：记录行上线只带引用（类型 · 字节数 · 哪一行 · 第几张），
//! 按引用取回的字节与记录里那一张逐字相同；行对不上 · 没有那一张 ⇒ `not_found`。夹具只造结构（图是占位字节）。

use super::*;
use serde_json::json;
use std::path::{Path, PathBuf};

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-img-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("projects").join("-p")).unwrap();
    d
}

fn write(home: &Path, sid: &str, text: &str) -> String {
    let p = home.join("projects").join("-p").join(format!("{sid}.jsonl"));
    std::fs::write(&p, text).unwrap();
    p.to_string_lossy().into_owned()
}

/// 600 KB 的 base64（占位字节，不是真图）。
fn shot() -> String {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    (0..600 * 1024).map(|i| alphabet[(i * 7 + i / 64) % 64] as char).collect()
}

fn with_shot(data: &str) -> String {
    json!({"type":"user","uuid":"u-shot","timestamp":"2026-10-12T00:00:01.000Z","cwd":"/w","message":{"role":"user","content":[
        {"type":"tool_result","tool_use_id":"c1","content":[
            {"type":"text","text":"t"},
            {"type":"image","source":{"type":"base64","media_type":"image/png","data":data}}
        ]}
    ]}})
    .to_string()
}

/// ★ 带一张 600 KB 截图的那一行上线 ≤ 4 KB（与流上的 `line` 帧同一份翻译）；按那一块的引用取回的字节逐字相同。
#[test]
fn a_line_with_a_shot_carries_only_a_reference_and_the_reference_fetches_the_bytes() {
    let home = scratch("ref");
    let data = shot();
    let head = json!({"type":"user","uuid":"u-0","timestamp":"2026-10-12T00:00:00.000Z","message":{"role":"user","content":"p"}}).to_string();
    let text = format!("{head}\n{}\n", with_shot(&data));
    let path = write(&home, "s-ref", &text);
    let tz = crate::Tz::default();
    let page = lines_at(&home, HistoryLinesReq { path: path.clone(), from: Some(1), until: Some(2) }, &tz).unwrap();
    let line = serde_json::to_value(&page.lines[0]).unwrap();
    let wire = serde_json::to_string(&line).unwrap();
    assert!(wire.len() <= 4096, "带截图的那一行上线 {} 字节", wire.len());
    let image = &line["record"]["blocks"][0]["content"][1];
    assert_eq!(image["type"], "image");
    assert_eq!(image["mediaType"], "image/png");
    assert_eq!(image["bytes"], data.len() as u64 * 3 / 4);
    assert_eq!(image["at"], head.len() as u64 + 1, "引用指那一行的起点");
    let got = image_at(
        &home,
        HistoryImageReq {
            path: path.clone(),
            at: image["at"].as_u64().unwrap(),
            n: image["n"].as_u64().unwrap() as u32,
            id: "u-shot".into(),
        },
    )
    .unwrap();
    assert_eq!(got.media_type, "image/png");
    assert!(got.data == data, "取回的字节与记录里那一张不同");
}

/// 行对不上（起点不是那一行 · 身份不是那一条）· 没有第 `n` 张 ⇒ `not_found`，不给别的图。
#[test]
fn a_reference_that_does_not_match_is_not_found() {
    let home = scratch("miss");
    let text = format!("{}\n", with_shot("AAAA"));
    let path = write(&home, "s-miss", &text);
    let ask = |at: u64, n: u32, id: &str| {
        image_at(&home, HistoryImageReq { path: path.clone(), at, n, id: id.into() })
    };
    assert!(ask(0, 0, "u-shot").is_ok());
    for (at, n, id) in [(0, 1, "u-shot"), (0, 0, "u-other"), (5, 0, "u-shot"), (999_999, 0, "u-shot")] {
        assert!(
            matches!(ask(at, n, id), Err(HistoryImageErr::NotFound(_))),
            "at {at} · n {n} · id {id} 该是 not_found"
        );
    }
}
