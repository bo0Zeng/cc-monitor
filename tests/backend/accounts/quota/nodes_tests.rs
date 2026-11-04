//! 节点：每种来源各发到那一条通道；文件事件各用自己的盯，不经会话文件那一圈。
//! 登录盯防接上帧面宿主那一份号目录读法的那一条在 `on_upper_tests.rs`（挂主 crate）。

use super::*;
use std::path::{Path, PathBuf};

/// 等一件这个来源的节点（别的来源的跳过）；到期没来 ⇒ `None`。
fn wait_for(
    rx: &mut tokio::sync::broadcast::Receiver<Node>,
    want: Source,
    sid: Option<&str>,
) -> Option<Node> {
    let deadline = std::time::Instant::now() + guard_core::wall::CEILING;
    while std::time::Instant::now() < deadline {
        match rx.try_recv() {
            Ok(n) if n.source == want && n.sid.as_deref() == sid => return Some(n),
            Ok(_) | Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {}
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
    None
}

/// ★ `quota.json` 被别的进程写了 ⇒ 一件 `QuotaFile`（本进程的额度账只在用的时候比戳，自己不会响）。
#[test]
fn a_foreign_write_of_the_quota_file_sends_a_quota_file_node() {
    let home = test_home::scratch("qf");
    let own = crate::platform::fs::Own::issue_data_at(&home);
    let mut rx = bus().subscribe();
    let _w = super::super::setup::uses::watch(&own).expect("watch");
    let side = home.join("quota.json.tmp");
    std::fs::write(&side, "{\"version\":1,\"accounts\":[]}").expect("w");
    std::fs::rename(&side, home.join(super::super::ledger::FILE_NAME)).expect("mv");
    assert!(
        wait_for(&mut rx, Source::QuotaFile, None).is_some(),
        "别处写了 quota.json ⇒ 该来一件"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ 节点不经会话文件那一圈（`observe/` 下的 jsonl 盯）：那一圈卡住时（4.1.9 hot1 那一次）节点照到。
/// 判法：`observe/` 下没有一处点名节点那一层（`quota::nodes`）或额度账的发节点口（`.notify(`）。
#[test]
fn no_node_is_sent_from_the_session_file_loop() {
    let root = guard_core::addr::backend_src_root().join("observe");
    let hits: Vec<String> = guard_core::scan_tree!(&root, &["rs"])
        .into_iter()
        .filter(|(_, t)| {
            guard_core::contains_word(t, "quota::nodes")
                || guard_core::contains_word(t, "nodes::emit")
        })
        .map(|(p, _)| p.display().to_string())
        .collect();
    assert!(hits.is_empty(), "会话文件那一圈里不许发节点：{hits:?}");
}

/// 账号库那一层在家目录底下的路径（盯不上那一句里给人看的就是它）：分隔符照家目录自己的写法，Windows 上不写成 `…\.cc-monitor/accounts`。
#[test]
fn the_account_library_path_uses_the_homes_own_separator() {
    let none: AccountDirs = std::sync::Arc::new(|_| Vec::new());
    let (win, _) = logins_dirs(Path::new(r"C:\Users\user"), &none);
    assert_eq!(win, PathBuf::from(r"C:\Users\user\.cc-monitor\accounts"));
    let (unix, _) = logins_dirs(Path::new("/home/user"), &none);
    assert_eq!(unix, PathBuf::from("/home/user/.cc-monitor/accounts"));
}
