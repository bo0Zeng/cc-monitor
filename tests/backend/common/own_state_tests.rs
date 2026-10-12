//! **后端自有状态文件的读三态** —— 判据（原子写那一半的判据随本体住 `platform/fs/own.rs` 的测试）。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | S1 | 不在 ⇒ `Absent`；是目录 / 超上限 ⇒ `Unreadable`；在且读得懂 ⇒ `Present` | 真文件 |

use super::*;
use std::path::PathBuf;

fn sandbox(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "ccm-own-state-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).expect("sandbox");
    base
}

/// 目录底下留着的旁名（`.tmp` 结尾）。
#[test]
fn s1_three_states() {
    let base = sandbox("s1");
    let p = base.join("x.json");
    assert!(matches!(read_bytes(&p, 64), Read::Absent));
    std::fs::create_dir(&p).expect("dir in place");
    assert!(matches!(read_bytes(&p, 64), Read::Unreadable(_)));
    std::fs::remove_dir(&p).expect("rm");
    std::fs::write(&p, vec![b'a'; 65]).expect("seed");
    assert!(
        matches!(read_bytes(&p, 64), Read::Unreadable(_)),
        "超上限也读进来了"
    );
    std::fs::write(&p, b"{\"a\":1}").expect("seed");
    match read_json::<serde_json::Value>(&p, 64) {
        Read::Present(v) => assert_eq!(v["a"], 1),
        _ => panic!("读得懂的没读出来"),
    }
    std::fs::write(&p, b"{").expect("seed");
    assert!(matches!(
        read_json::<serde_json::Value>(&p, 64),
        Read::Unreadable(_)
    ));
    std::fs::remove_dir_all(&base).ok();
}

/// S5：读不出的那一句只带路径与原因词，下层原话（解析器 · 系统）进 `raw`（复制详情）。
#[test]
fn s5_the_sentence_carries_the_reason_word_and_the_raw_words_go_to_the_detail() {
    let base = sandbox("s5");
    let p = base.join("x.json");
    std::fs::write(&p, b"{").expect("seed");
    let Read::Unreadable(s) = read_json::<serde_json::Value>(&p, 64) else {
        panic!("坏 JSON 没读成读不出");
    };
    let shown = p.display().to_string();
    assert_eq!(
        s.said,
        copy_text("beOwnState.read.notJson", &[("path", &shown)])
    );
    assert!(
        s.raw.as_deref().is_some_and(|r| r.contains("EOF")),
        "解析器原话没进详情：{s:?}"
    );
    std::fs::remove_dir_all(&base).ok();
}
