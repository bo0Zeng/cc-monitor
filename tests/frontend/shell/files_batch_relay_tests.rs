//! `files_batch_relay.rs` 的判据：按票交、只留最新那一格、没人订就丢、停订只摘自己那一份。

use super::*;
use futures::StreamExt as _;

#[tokio::test]
async fn a_watcher_gets_the_latest_cell_of_its_own_ticket_only() {
    let mut s = watch("fbr-a");
    deliver("fbr-other", r#"{"done":9,"total":9}"#.into());
    deliver("fbr-a", r#"{"done":1,"total":3}"#.into());
    deliver("fbr-a", r#"{"done":2,"total":3}"#.into());
    let got = tokio::time::timeout(guard_core::wall::CEILING, s.next())
        .await
        .expect("等到一格")
        .expect("流没关");
    assert_eq!(got, r#"{"done":2,"total":3}"#, "只留最新那一格");
}

#[tokio::test]
async fn dropping_a_watch_unregisters_only_that_one() {
    let a = watch("fbr-b");
    let mut b = watch("fbr-b");
    drop(a);
    assert_eq!(table().get("fbr-b").map(Vec::len), Some(1));
    deliver("fbr-b", r#"{"done":1,"total":1}"#.into());
    let got = tokio::time::timeout(guard_core::wall::CEILING, b.next())
        .await
        .expect("等到一格");
    assert!(got.is_some(), "剩下那一位照样收得到");
    drop(b);
    assert!(table().get("fbr-b").is_none(), "没人订了 ⇒ 那张票整行摘掉");
    // 没人订 ⇒ 丢，不留。
    deliver("fbr-b", r#"{"done":1,"total":1}"#.into());
    assert!(table().get("fbr-b").is_none());
}

/// 本机那条流上来的 `files_batch` 帧（两台之间的复制在本机后端里做）⇒ 经分派交到订了那张票的那一位，字节那两格原样带着。
#[tokio::test]
async fn a_frame_on_the_local_stream_reaches_the_watcher_with_its_bytes() {
    let mut s = watch("fbr-local");
    let f = crate::stream_source::decode(
        r#"{"kind":"files_batch","ticket":"fbr-local","done":1,"total":3,"bytes_done":512,"bytes_total":2048}"#,
    )
    .expect("认得这一帧");
    crate::stream_source::route(
        crate::stream_source::Via::Local,
        &crate::origin::Origin::local(),
        None,
        f,
    );
    let got = tokio::time::timeout(guard_core::wall::CEILING, s.next())
        .await
        .expect("等到一格")
        .expect("流没关");
    let v: serde_json::Value = serde_json::from_str(&got).unwrap();
    assert_eq!(
        v,
        serde_json::json!({"done": 1, "total": 3, "bytes_done": 512, "bytes_total": 2048})
    );
}
