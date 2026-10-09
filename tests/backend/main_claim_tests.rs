//! `main.rs::claim_then_log`：先抢门牌（`<家>/run/` 的独占锁 ＋ 套接字），抢到了才接 stderr 落盘。
//! 要求：已有常驻后端时，抢不到锁的后起者不碰那两份日志（原先它一起来就把在跑那一个的日志滚走，再来一个就删掉），
//! 也不碰在跑那一个的套接字；沙箱跑却要占本账号真家目录里的门牌 ⇒ 不起（10-08 一路台架漏清环境，改写过真家里的门牌）。

use super::*;

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("claim-then-log-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("夹具目录");
    d
}

/// 锁被攥着 ⇒ 交回「已有人在听」、日志一下都没接、套接字文件原样；配置不成立 ⇒ 也不接；
/// 正控：锁放了 ⇒ 抢到、接了、套接字在听；不走常驻的那条载体不用抢 ⇒ 接。
#[tokio::test]
async fn a_late_starter_that_cannot_claim_never_touches_the_log_or_the_socket() {
    let home = scratch("late");
    let data = home.join("data");
    let data_s = data.to_string_lossy().into_owned();
    let home_s = home.to_string_lossy().into_owned();
    let env = |k: &str| match k {
        listen::ENV_RESIDENT => Some("1".to_string()),
        creds_core::store::DATA_DIR_ENV => Some(data_s.clone()),
        "HOME" => Some(home_s.clone()),
        _ => None,
    };
    let installs = std::cell::Cell::new(0);
    let install = || {
        installs.set(installs.get() + 1);
        stderr_log::Installed::NotAsked
    };

    // 在跑的那一个：攥着锁、套接字文件在。
    let dir = relay_route_core::listen_dir_for(&data);
    std::fs::create_dir_all(&dir).expect("门牌目录");
    let sock = relay_route_core::listen_socket_for(&data);
    std::fs::write(&sock, "在跑那一个的").expect("占位");
    let held = own_chan::try_hold(&dir).expect("锁").expect("拿到");

    let got = claim_then_log(&env, None, install).await;
    assert_eq!(got.err(), Some(listen::EXIT_ADDR_IN_USE));
    assert_eq!(
        installs.get(),
        0,
        "抢不到锁的后起者接了日志（会把在跑那一个的滚走）"
    );
    assert_eq!(
        std::fs::read_to_string(&sock).ok().as_deref(),
        Some("在跑那一个的"),
        "抢不到锁的后起者动了在跑那一个的套接字"
    );

    let bad = |k: &str| (k == listen::ENV_RESIDENT).then(|| "yes".to_string());
    assert_eq!(
        claim_then_log(&bad, None, install).await.err(),
        Some(listen::EXIT_BAD_LISTEN_CONFIG)
    );
    assert_eq!(installs.get(), 0, "常驻配置不成立也接了日志");

    drop(held);
    let (listening, _) = claim_then_log(&env, None, install)
        .await
        .expect("锁放了却没抢到");
    assert!(listening.is_some(), "该在听");
    assert!(own_chan::someone_listening(&sock), "抢到了却没在听");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "门牌目录不是只给本人");
    }
    assert_eq!(installs.get(), 1);
    drop(listening);
    let (listening, _) = claim_then_log(&|_| None, None, install)
        .await
        .expect("stdio 那条");
    assert!(listening.is_none());
    assert_eq!(installs.get(), 2);
    let _ = std::fs::remove_dir_all(&home);
}

/// 〔台架防真家〕沙箱跑（`CCM_SANDBOX=1`，或 `$HOME` 不是账号真家目录）却要占真家里的门牌 ⇒ 拒绝起、日志不接、真家里一个字节不写；
/// 正控：同样的沙箱跑，家在沙箱里 ⇒ 起。真家目录这里用夹具目录扮（`account_home` 是入参）。
#[tokio::test]
async fn a_sandboxed_start_refuses_the_real_home() {
    let real = scratch("real");
    let fake_home = scratch("fake");
    let real_s = real.to_string_lossy().into_owned();
    let fake_s = fake_home.to_string_lossy().into_owned();
    let installs = std::cell::Cell::new(0);
    let install = || {
        installs.set(installs.get() + 1);
        stderr_log::Installed::NotAsked
    };
    // 10-08 那一形：HOME 隔离了，家却漏成真家里的那一份。
    let leaked_dir = real.join(".cc-monitor").to_string_lossy().into_owned();
    let leaked = |k: &str| match k {
        listen::ENV_RESIDENT => Some("1".to_string()),
        creds_core::store::DATA_DIR_ENV => Some(leaked_dir.clone()),
        "HOME" => Some(fake_s.clone()),
        _ => None,
    };
    assert_eq!(
        claim_then_log(&leaked, Some(&real), install).await.err(),
        Some(listen::EXIT_BAD_LISTEN_CONFIG)
    );
    // HOME 也没隔离、只带了标记：同样拒。
    let marked = |k: &str| match k {
        listen::ENV_RESIDENT => Some("1".to_string()),
        relay_route_core::SANDBOX_ENV => Some("1".to_string()),
        "HOME" => Some(real_s.clone()),
        _ => None,
    };
    assert_eq!(
        claim_then_log(&marked, Some(&real), install).await.err(),
        Some(listen::EXIT_BAD_LISTEN_CONFIG)
    );
    assert_eq!(installs.get(), 0, "拒绝起也接了日志");
    assert!(
        !real.join(".cc-monitor").exists(),
        "拒绝起之前已经往真家里写了东西"
    );
    // 正控：沙箱跑、家在沙箱里 ⇒ 起。
    let boxed = |k: &str| match k {
        listen::ENV_RESIDENT => Some("1".to_string()),
        relay_route_core::SANDBOX_ENV => Some("1".to_string()),
        "HOME" => Some(fake_s.clone()),
        _ => None,
    };
    let (listening, _) = claim_then_log(&boxed, Some(&real), install)
        .await
        .expect("沙箱里的家却没起来");
    assert!(listening.is_some());
    drop(listening);
    let _ = std::fs::remove_dir_all(&real);
    let _ = std::fs::remove_dir_all(&fake_home);
}
