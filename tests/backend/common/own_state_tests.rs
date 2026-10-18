//! **后端自有状态文件的读三态** —— 判据（原子写那一半的判据随本体住 `platform/fs/own.rs` 的测试）。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | S1 | 不在 ⇒ `Absent`；是目录 / 超上限 ⇒ `Unreadable`；在且读得懂 ⇒ `Present` | 真文件 |
//! | S6 | 状态读不懂（坏 JSON · 超上限 · 是目录 · 解码拒）⇒ `Unreadable`；缓存同样几样 ⇒ `Absent`；三种读法都一样 | 真文件 |
//! | S7 | 流式读：读得懂 ⇒ `Present`；不在 ⇒ `Absent` | 真文件 |

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

const STATE: Doc<serde_json::Value> = Doc::state(".cc-monitor/x.json", 16);
const CACHE: Doc<serde_json::Value> = Doc::cache(".cc-monitor/x.json", 16);

fn stream_json(r: &mut dyn std::io::Read) -> Result<serde_json::Value, Said> {
    serde_json::from_reader(r).map_err(|e| Said::with_raw("bad".to_string(), e))
}

fn bytes_json(b: Vec<u8>) -> Result<serde_json::Value, Said> {
    serde_json::from_slice(&b).map_err(|e| Said::with_raw("bad".to_string(), e))
}

/// 三种读法各读一遍：（整份 JSON, 自己解, 流式）。
fn read_three(d: &Doc<serde_json::Value>, p: &Path) -> [Read<serde_json::Value>; 3] {
    [
        d.read(p),
        d.read_with(p, bytes_json),
        d.read_stream(p, stream_json),
    ]
}

#[test]
fn s6_state_unreadable_stays_unreadable_and_cache_unreadable_is_absent() {
    let base = sandbox("s6");
    let p = base.join("x.json");
    let seeds: [(&str, Box<dyn Fn()>); 3] = [
        (
            "坏 JSON",
            Box::new(|| std::fs::write(&p, b"{").expect("seed")),
        ),
        (
            "超上限",
            Box::new(|| std::fs::write(&p, vec![b' '; 17]).expect("seed")),
        ),
        (
            "落点是个目录",
            Box::new(|| std::fs::create_dir(&p).expect("dir")),
        ),
    ];
    for (what, seed) in &seeds {
        std::fs::remove_dir(&p).ok();
        std::fs::remove_file(&p).ok();
        seed();
        for r in read_three(&STATE, &p) {
            assert!(matches!(r, Read::Unreadable(_)), "状态 · {what}：{r:?}");
        }
        for r in read_three(&CACHE, &p) {
            assert!(matches!(r, Read::Absent), "缓存 · {what}：{r:?}");
        }
    }
    std::fs::remove_dir(&p).ok();
    std::fs::write(&p, b"1").expect("seed");
    let refuse = |_: Vec<u8>| -> Result<serde_json::Value, Said> { Err("no".to_string().into()) };
    assert!(matches!(STATE.read_with(&p, refuse), Read::Unreadable(_)));
    assert!(matches!(CACHE.read_with(&p, refuse), Read::Absent));
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn s7_streamed_read_reads_and_absent_is_absent() {
    let base = sandbox("s7");
    let p = base.join("x.json");
    for d in [&STATE, &CACHE] {
        assert!(matches!(d.read_stream(&p, stream_json), Read::Absent));
    }
    std::fs::write(&p, b"{\"a\":1}").expect("seed");
    for d in [&STATE, &CACHE] {
        for r in read_three(d, &p) {
            match r {
                Read::Present(v) => assert_eq!(v["a"], 1),
                other => panic!("读得懂的没读出来：{other:?}"),
            }
        }
    }
    std::fs::remove_dir_all(&base).ok();
}
