//! 〔U4b · 第四波〕`session_facts` 的判据：线上字面量 · 账本 · 出口。
//!
//! 出口是进程级 `OnceLock`，本文件是全 crate 里**唯一**装它的测试（生产里只有 `lib.rs` 的 setup 装）
//! ⇒ 下面那条装一次、按本测试专用的 sid 前缀过滤读数，与并行跑的别的测试互不干扰。

use super::*;
use std::sync::Mutex as StdMutex;

static SEEN: StdMutex<Vec<Fact>> = StdMutex::new(Vec::new());

fn ensure_sink() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| install_sink(|f| SEEN.lock().unwrap().push(f)));
}

fn seen_with(prefix: &str) -> Vec<Fact> {
    SEEN.lock()
        .unwrap()
        .iter()
        .filter(|f| match f {
            Fact::Container { sid, .. } | Fact::LocalIdleGone { sid } => sid.starts_with(prefix),
        })
        .cloned()
        .collect()
}

/// 线上两个字面量 ＋ 未知取值当不知道（M1 的 monitor 半；后端那半钉在 `wire_tests`）。
#[test]
fn container_literals_round_trip_and_unknown_is_not_none() {
    assert_eq!(Container::from_wire("tmux"), Some(Container::Tmux));
    assert_eq!(Container::from_wire("none"), Some(Container::None));
    for bad in ["", "TMUX", "screen", " none"] {
        assert_eq!(Container::from_wire(bad), None, "{bad:?} 不许被认成容器");
    }
    assert_eq!(Container::Tmux.as_wire(), "tmux");
    assert_eq!(Container::None.as_wire(), "none");
}

/// 账本：有值 ⇒ 记账 ＋ 发一件事；`None` ⇒ 忘掉、**不发**；`forget` ⇒ 不再出现在重发快照里。
#[test]
fn the_ledger_records_emits_and_forgets() {
    ensure_sink();
    note_container("u4bT-a", Some(Container::Tmux));
    note_container("u4bT-b", Some(Container::None));
    note_container("u4bT-c", Some(Container::Tmux));
    note_container("u4bT-c", None); // 重新宣告成「判不了」⇒ 旧值不许粘着
    forget("u4bT-b");
    let snap: Vec<(String, Container)> = containers_snapshot()
        .into_iter()
        .filter(|(s, _)| s.starts_with("u4bT-"))
        .collect();
    assert_eq!(snap, vec![("u4bT-a".to_string(), Container::Tmux)]);
    assert_eq!(
        seen_with("u4bT-"),
        vec![
            Fact::Container {
                sid: "u4bT-a".into(),
                container: Container::Tmux
            },
            Fact::Container {
                sid: "u4bT-b".into(),
                container: Container::None
            },
            Fact::Container {
                sid: "u4bT-c".into(),
                container: Container::Tmux
            },
        ],
        "`None` 那一次不许发事件"
    );
}

/// 本机收割的结论原样交出口（逐条、同序）。
#[test]
fn local_retirements_reach_the_sink_in_order() {
    ensure_sink();
    retire_local_idle(vec!["u4bR-1".into(), "u4bR-2".into()]);
    assert_eq!(
        seen_with("u4bR-"),
        vec![
            Fact::LocalIdleGone {
                sid: "u4bR-1".into()
            },
            Fact::LocalIdleGone {
                sid: "u4bR-2".into()
            },
        ]
    );
}
