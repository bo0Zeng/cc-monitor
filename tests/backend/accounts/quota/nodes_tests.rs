//! 节点：每种来源各发到那一条通道；文件事件各用自己的盯，不经会话文件那一圈。

use super::*;
use std::path::{Path, PathBuf};

fn temp_home(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-nodes-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("mkdir");
    p
}

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

/// 一个号 z 的账号库：清单 ＋ z 的配置目录（带登录文件）。
fn library_home(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let home = temp_home(tag);
    let accts = home.join(relay_route_core::ACCOUNTS_DIR_REL);
    let z = home.join("acct-z");
    std::fs::create_dir_all(&accts).expect("mkdir");
    std::fs::create_dir_all(&z).expect("mkdir");
    std::fs::write(z.join(".credentials.json"), "{}").expect("creds");
    let manifest = accts.join(relay_route_core::ACCOUNTS_MANIFEST_NAME);
    write_manifest(&manifest, &accts, &z, "");
    (home, manifest, z)
}

fn write_manifest(manifest: &Path, accts: &Path, z: &Path, extra: &str) {
    std::fs::write(
        manifest,
        format!(
            r#"{{"version":1,"updatedAt":"2026-10-12T00:00:00Z{extra}","sharedStore":"/s","acctsDir":"{a}","accounts":[
               {{"name":"z","email":"","configDir":"{z}","isDefault":true,"mode":"isolated"}}]}}"#,
            a = accts.display(),
            z = z.display()
        ),
    )
    .expect("manifest");
}

/// ★ 某个号的登录文件变了（登录 · 退出 · 别处续了令牌）⇒ 一件 `Accounts`；清单变了 ⇒ 也是一件，而且照新的清单重盯。
#[test]
fn a_login_file_or_the_manifest_changing_sends_an_accounts_node() {
    let (home, manifest, z) = library_home("acct");
    let tx: Nodes = tokio::sync::broadcast::channel(64).0;
    let mut rx = tx.subscribe();
    let w = AccountsWatch::start(&home, &tx).expect("watch");
    assert!(w.armed());
    std::fs::write(z.join(".credentials.json"), "{\"x\":1}").expect("creds");
    assert!(
        wait_for(&mut rx, Source::Accounts, None).is_some(),
        "登录文件变了 ⇒ 该来一件"
    );
    write_manifest(
        &manifest,
        &manifest.parent().expect("dir").to_path_buf(),
        &z,
        "1",
    );
    assert!(
        wait_for(&mut rx, Source::Accounts, None).is_some(),
        "清单变了 ⇒ 该来一件"
    );
    // 别的文件不算。
    while rx.try_recv().is_ok() {}
    std::fs::write(z.join("other.txt"), "x").expect("w");
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(rx.try_recv().is_err(), "配置目录里别的文件不该发节点");
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ `quota.json` 被别的进程写了 ⇒ 一件 `QuotaFile`（本进程的额度账只在用的时候比戳，自己不会响）。
#[test]
fn a_foreign_write_of_the_quota_file_sends_a_quota_file_node() {
    let home = temp_home("qf");
    let own = crate::platform::fs::Own::issue_data_at(&home);
    let mut rx = channel().subscribe();
    let _w = super::super::rotation::watch(&own).expect("watch");
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
/// 判法：发节点的口在 `observe/` 下一处都没有。
#[test]
fn no_node_is_sent_from_the_session_file_loop() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("observe");
    let mut hits = Vec::new();
    let mut stack = vec![root];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).expect("observe/ 在").flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                let t = std::fs::read_to_string(&p).expect("r");
                if t.contains("nodes::emit") || t.contains("Source::") && t.contains("quota::nodes")
                {
                    hits.push(p.display().to_string());
                }
            }
        }
    }
    assert!(hits.is_empty(), "会话文件那一圈里不许发节点：{hits:?}");
}
