//! 本人通道（`own-chan`）的判据：独占锁真互斥、进程放了就能再拿 · 同一个 uid 连进来是 `Ours` · 没人在听就是没人。
//! 守的要求：「我们自己两头都管的连接，一律走只有本人能连的系统通道，不要钥匙」。
//! 买不到：别的 uid 连进来那一支（`Foreign`）要第二个系统用户，单测造不出来；它只按 `peer_cred` 的 uid 比，读码可见。

use super::*;

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("own-chan-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("夹具目录");
    d
}

/// 第一个拿到，第二个（另一次打开 = 另一份打开文件描述）拿不到；第一个放了之后第二个拿得到。
#[cfg(unix)]
#[test]
fn the_dir_lock_is_exclusive_and_released_on_drop() {
    let d = scratch("lock");
    let first = try_hold(&d).expect("锁").expect("第一个该拿到");
    assert!(
        try_hold(&d).expect("锁").is_none(),
        "第二个也拿到了 —— 两个常驻后端会同时在听"
    );
    drop(first);
    assert!(
        try_hold(&d).expect("锁").is_some(),
        "第一个放了之后还拿不到 —— 陈旧锁会让常驻后端永远起不来"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 绑上之后：同一个进程（同一个 uid）连进来是 `Ours`，字节两向都通；没绑的路径上没人在听。
#[cfg(unix)]
#[tokio::test]
async fn a_peer_with_my_uid_is_ours_and_nobody_means_nobody() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let d = scratch("bind");
    let path = d.join("s.sock");
    assert!(!someone_listening(&path), "还没绑就说有人在听");
    let l = bind(&path).expect("绑");
    assert!(someone_listening(&path));
    let client = tokio::spawn({
        let path = path.clone();
        async move {
            let mut c = connect(&path).await.expect("连");
            c.write_all(b"hi\n").await.expect("写");
            let mut back = [0u8; 3];
            c.read_exact(&mut back).await.expect("读");
            back
        }
    });
    // `someone_listening` 那一下也连过一次：先把它收掉（它连上就走了）。
    let mut got_ours = None;
    for _ in 0..2 {
        match l.accept().await.expect("收") {
            Accepted::Ours(s) => {
                let mut s = s;
                let mut buf = [0u8; 3];
                if s.read_exact(&mut buf).await.is_ok() {
                    s.write_all(b"ok\n").await.expect("回");
                    got_ours = Some(buf);
                    break;
                }
            }
            Accepted::Foreign(uid) => panic!("同一个进程连进来被认成别人（{uid:?}）"),
        }
    }
    assert_eq!(got_ours, Some(*b"hi\n"));
    assert_eq!(&client.await.expect("客户端"), b"ok\n");
    let _ = std::fs::remove_dir_all(&d);
}

/// 同步那一形连上之后能交给运行时继续用（本机壳握手完了就这么接流）。
#[cfg(unix)]
#[tokio::test]
async fn a_blocking_connection_hands_over_to_the_runtime() {
    use std::io::Write as _;
    use tokio::io::AsyncReadExt;
    let d = scratch("hand");
    let path = d.join("s.sock");
    let l = bind(&path).expect("绑");
    let b = connect_blocking(&path).expect("同步连");
    let Accepted::Ours(mut server) = l.accept().await.expect("收") else {
        panic!("同一个进程被认成别人");
    };
    (&b).write_all(b"x").expect("同步写");
    let mut one = [0u8; 1];
    server.read_exact(&mut one).await.expect("读");
    assert_eq!(&one, b"x");
    let mut a = b.into_async().expect("交给运行时");
    tokio::io::AsyncWriteExt::write_all(&mut a, b"y")
        .await
        .expect("异步写");
    server.read_exact(&mut one).await.expect("读");
    assert_eq!(&one, b"y");
    let _ = std::fs::remove_dir_all(&d);
}
