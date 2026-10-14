//! 这台此刻活着的会话：pidfile 里的会话 id ＋ 进程还是不是同一个。

use super::*;
use std::fs;

#[cfg(unix)]
#[test]
fn live_session_ids_are_the_pidfiles_whose_process_is_still_there() {
    let home = std::env::temp_dir().join(format!("ccm-live-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    let sessions = home.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    let me = std::process::id();
    fs::write(
        sessions.join(format!("{me}.json")),
        r#"{"sessionId":"live-sid","cwd":"/w"}"#,
    )
    .unwrap();
    fs::write(
        sessions.join("4194304.json"),
        r#"{"sessionId":"dead-sid","cwd":"/w"}"#,
    )
    .unwrap();
    fs::write(
        sessions.join("notapid.json"),
        r#"{"sessionId":"ignored","cwd":"/w"}"#,
    )
    .unwrap();
    let got = live_session_ids(&home);
    assert_eq!(
        got,
        ["live-sid".to_string()].into_iter().collect(),
        "活着的那一个要在、死了的那一个不许在"
    );
    // 没有 sessionId 的活进程不算（它说不出是哪个会话）。
    fs::write(sessions.join(format!("{me}.json")), r#"{"cwd":"/w"}"#).unwrap();
    assert!(live_session_ids(&home).is_empty());
    let _ = fs::remove_dir_all(&home);
}
