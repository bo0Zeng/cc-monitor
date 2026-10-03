//! `main.rs::claim_then_log`：先抢口，抢到了才接 stderr 落盘。
//! 要求：口上已有常驻后端时，抢口失败的后起者不碰那两份日志（原先它一起来就把在跑那一个的日志滚走，再来一个就删掉）。

use super::*;

/// 口被占着 ⇒ 交回「口上已有东西」、日志一下都没接；配置不成立 ⇒ 也不接；
/// 正控：口空出来 ⇒ 抢到、接了；不走监听口的那条载体不用抢 ⇒ 接。
#[tokio::test]
async fn a_late_starter_that_cannot_claim_the_port_never_touches_the_log() {
    let held = std::net::TcpListener::bind((listen::LOOPBACK, 0)).expect("占一个口");
    let port = held.local_addr().expect("口号").port().to_string();
    // 钥匙只经文件交：夹具目录里放一把。
    let dir = std::env::temp_dir().join(format!("claim-then-log-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("夹具目录");
    let key = dir.join("listen-token");
    std::fs::write(&key, "t").expect("钥匙文件");
    let key = key.to_string_lossy().into_owned();
    let listen_env = |k: &str| match k {
        listen::ENV_PORT => Some(port.clone()),
        listen::ENV_TOKEN_FILE => Some(key.clone()),
        _ => None,
    };
    let installs = std::cell::Cell::new(0);
    let install = || {
        installs.set(installs.get() + 1);
        stderr_log::Installed::NotAsked
    };

    let got = claim_then_log(&listen_env, install).await;
    assert_eq!(got.err(), Some(listen::EXIT_ADDR_IN_USE));
    assert_eq!(
        installs.get(),
        0,
        "抢口失败的后起者接了日志（会把在跑那一个的滚走）"
    );

    let half = |k: &str| (k == listen::ENV_PORT).then(|| "51000".to_string());
    assert_eq!(
        claim_then_log(&half, install).await.err(),
        Some(listen::EXIT_BAD_LISTEN_CONFIG)
    );
    assert_eq!(installs.get(), 0, "监听口配置不成立也接了日志");

    drop(held);
    let (listening, _) = claim_then_log(&listen_env, install)
        .await
        .expect("口空出来了却没抢到");
    assert_eq!(
        listening.map(|(_, p, t)| (p.to_string(), t)),
        Some((port.clone(), "t".to_string()))
    );
    assert_eq!(installs.get(), 1);
    let (listening, _) = claim_then_log(&|_| None, install)
        .await
        .expect("stdio 那条");
    assert!(listening.is_none());
    assert_eq!(installs.get(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}
