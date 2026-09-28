fn probe_cfg() -> RemoteConfig {
    RemoteConfig {
        host: "这个主机一定不存在-audit0805".into(),
        label: "probe".into(),
        port: 1,
        user: "nobody".into(),
        key_path: None,
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    }
}

/// ★★ **部署入口真的过了目录围栏吗**〔audit-0805 08-08，Phase G 第 54 件〕。
///
/// `is_safe_remote_acct_iso_dir` 有直接的行为判据，但主语是**围栏本身**。
/// 08-08 实测：把 `deploy_remote_acct_iso` 里那句 `if !is_safe_…` 短路，
/// **全仓 986 条判据一条不红** —— 而那条路会往用户远端机器上的**任意目录**部署。
/// 与 F47/F48/F53 同一族（围栏有判据、接线没人钉）。
///
/// 围栏在任何 I/O 之前 ⇒ 本条跑真路：喂一个非法目录，要求**零网络**就被拒。
#[tokio::test]
async fn the_deploy_entry_point_actually_checks_the_destination() {
    let err = deploy_remote_acct_iso(probe_cfg(), "/etc".into())
        .await
        .expect_err("非法部署目录竟然没被拒 —— 围栏没接上");
    assert!(
        err.contains("部署目录不安全"),
        "拒绝了，但不是目录围栏拒的（错误：{err}）—— \
             说明它已经越过围栏去连主机了，而下一步是往那个目录里写东西。"
    );
}

use super::*;

#[test]
fn safe_dir_accepts_conventional_paths() {
    assert!(is_safe_remote_acct_iso_dir(
        "/home/z/.cc-monitor/cc-acct-iso"
    ));
    assert!(is_safe_remote_acct_iso_dir("/opt/cc-acct-iso"));
    assert!(is_safe_remote_acct_iso_dir("/home/z/.cc-monitor/x")); // 含 .cc-monitor
}

/// 〔IV1 · V121〕要求住址：`INVARIANTS §47`（外部值拼进 shell / 交给对端之前本侧先过放行判定）；②形（远端落点路径）。
#[test]
fn safe_dir_rejects_dangerous() {
    assert!(!is_safe_remote_acct_iso_dir("")); // 空
    assert!(!is_safe_remote_acct_iso_dir("/")); // 根
    assert!(!is_safe_remote_acct_iso_dir("relative/cc-acct-iso")); // 相对
    assert!(!is_safe_remote_acct_iso_dir("/home/../cc-acct-iso")); // ..
    assert!(!is_safe_remote_acct_iso_dir("/home/z/projects")); // 无标记词
    assert!(!is_safe_remote_acct_iso_dir("/tmp/evil")); // 无标记词
}

#[test]
fn vendor_id_is_nonempty_trimmed() {
    let v = vendor_id();
    assert!(!v.is_empty());
    assert_eq!(v, v.trim());
    assert!(!v.contains('\n'));
}

// 〔MIG-3a〕远端 `acct-iso.*` 两问那一节〔散文墓碑〕随两条 Tauri 命令退役：判读进了后端（`tests/backend/accounts/iso_tests.rs`），
//   请求形状与失败的话由界面那一侧钉（`tests/acct-iso-reads.vitest.ts`）。
