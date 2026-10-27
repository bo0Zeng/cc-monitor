//! `bus-list` 每一行的那两样问后端那一处：这台有记录 ⇒ 标题是会话标题、在不在照「此刻谁持着它」判；没有记录 ⇒ 两格都判不了。
//! 夹具只造结构（一份两行的会话记录，正文是占位），不起进程。

use super::*;

fn sandbox(tag: &str) -> std::path::PathBuf {
    let home = std::env::temp_dir().join(format!("ccm-buslist-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("建夹具家目录");
    home
}

#[test]
fn a_known_session_gets_its_title_and_a_judged_liveness_an_unknown_one_stays_unknown() {
    let home = sandbox("seen");
    let sid = "b5000000-0000-4000-8000-000000000001";
    let dir = home.join("projects").join("-w");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{sid}.jsonl")),
        format!(
            "{}\n",
            serde_json::json!({"type":"user","uuid":"u1","parentUuid":null,"sessionId":sid,
                "timestamp":"2026-10-26T00:00:00Z","cwd":"/w",
                "message":{"role":"user","content":"占位的第一句"}})
        ),
    )
    .unwrap();
    let none = Default::default();
    let got = seen_at(&home, sid, &none);
    assert_eq!(
        got.live,
        Some(false),
        "有记录、没有进程持着它 ⇒ 判得出：不在"
    );
    let title = got.title.expect("有记录 ⇒ 有标题");
    assert!(
        !title.is_empty() && title != sid,
        "标题该是会话自己的：{title}"
    );
    assert_eq!(
        seen_at(&home, "b5000000-0000-4000-8000-0000000000ff", &none),
        Seen {
            title: None,
            live: None
        },
        "这台没有它的记录 ⇒ 两格都判不了（不是「不在」）"
    );
    // 这台注解里给它改过名 ⇒ 名单上叫改过的名（与标签页 · 历史清单同一个名）。
    let renamed: crate::history::history_annotations::Table = serde_json::from_value(
        serde_json::json!({ sid: {"starred": false, "customTitle": "修登录", "updatedAt": 0} }),
    )
    .unwrap();
    assert_eq!(seen_at(&home, sid, &renamed).title.as_deref(), Some("修登录"));
    let _ = std::fs::remove_dir_all(&home);
}
