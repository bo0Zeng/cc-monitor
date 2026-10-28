//! 后端替一个前端持有托管终端的输入：真起托管终端（假子进程 `sh`），沙箱数据目录，不起真 claude。

use super::super::run::tests::{hello, read_until_out, sandbox_own, spec, start};
use super::super::wire::{read_frame, write_frame, Ctl, Frame, Want};
use super::*;

/// 等读线把那一条摘掉（真终端抢走输入后，托管进程推 `holder`，读线当场摘；上限是兜底的那一个数）。
async fn until_released(id: &str) {
    for _ in guard_core::wall::rounds() {
        if holder(id).is_none() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("读线一直没摘掉 {id}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_front_takes_input_types_and_loses_it_to_a_terminal_window() {
    let (own, _data) = sandbox_own("hold");
    let script = r#"printf 'ready\r\n'; while read x; do printf 'got:%s\r\n' "$x"; done"#;
    let (sock, task) = start(&own, spec("t-hold", script)).await;
    let mut viewer = hello(&sock, Want::View).await;
    read_until_out(&mut viewer, "ready").await;

    // 桌面接管：之后经后端送的字进得去。
    let s2 = sock.clone();
    tokio::task::spawn_blocking(move || take(&s2, "t-hold", "desktop"))
        .await
        .expect("线没崩")
        .expect("接管");
    assert_eq!(holder("t-hold").as_deref(), Some("desktop"));
    assert!(send("t-hold", b"hi\r").expect("送"), "持有着 ⇒ 送了");
    read_until_out(&mut viewer, "got:hi").await;

    // 真终端接上要输入 ⇒ 输入归它，后端那一条被摘掉，再送回「不在手里」。
    let mut term = hello(&sock, Want::Input).await;
    read_until_out(&mut term, "ready").await;
    until_released("t-hold").await;
    assert!(!send("t-hold", b"nope\r").expect("送"), "不在手里 ⇒ 没送");

    // 再接管、交还：交还之后没持有。
    let s3 = sock.clone();
    tokio::task::spawn_blocking(move || take(&s3, "t-hold", "mobile"))
        .await
        .expect("线没崩")
        .expect("再接管");
    assert_eq!(holder("t-hold").as_deref(), Some("mobile"));
    release("t-hold");
    assert_eq!(holder("t-hold"), None);

    write_frame(&mut term, &Frame::Ctl(Ctl::End))
        .await
        .expect("end");
    while let Ok(Some(f)) = read_frame(&mut term).await {
        if matches!(f, Frame::Ctl(Ctl::Ended { .. })) {
            break;
        }
    }
    task.await.expect("任务没崩").expect("跑完");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_listing_says_who_holds_input_and_which_windows_are_attached() {
    let (own, _data) = sandbox_own("hold-list");
    let (sock, task) = start(&own, spec("t-who", "printf 'ready\\r\\n'; read x")).await;
    let mut term = hello(&sock, Want::Input).await;
    read_until_out(&mut term, "ready").await;
    let o = own.clone();
    let (rows, clients) = tokio::task::spawn_blocking(move || super::super::listed_at(&o))
        .await
        .expect("线没崩");
    assert_eq!(
        rows[0].input,
        crate::host::Input::Held(Some(crate::host::HeldBy::Terminal))
    );
    assert_eq!(clients.len(), 1, "连着一个真终端：{clients:?}");
    assert_eq!(clients[0].0, "t-who");
    let s2 = sock.clone();
    tokio::task::spawn_blocking(move || take(&s2, "t-who", "desktop"))
        .await
        .expect("线没崩")
        .expect("接管");
    let o = own.clone();
    let rows = tokio::task::spawn_blocking(move || super::super::rows_at(&o))
        .await
        .expect("线没崩");
    assert_eq!(
        rows[0].input,
        crate::host::Input::Held(Some(crate::host::HeldBy::Front("desktop".into())))
    );
    release("t-who");
    // 交还之后输入回到那个真终端：等它收到「现在是你」再送（交还与送字走两条连接，先后要等）。
    //   它先收到过「被桌面拿走」那一帧，之后的「现在是你」才是交还带来的。
    for want_mine in [false, true] {
        loop {
            match read_frame(&mut term).await.expect("读").expect("没断") {
                Frame::Ctl(Ctl::Holder { holder })
                    if holder.as_ref().is_some_and(|h| h.mine) == want_mine =>
                {
                    break
                }
                _ => {}
            }
        }
    }
    write_frame(&mut term, &Frame::In(b"\r".to_vec()))
        .await
        .expect("送字");
    task.await.expect("任务没崩").expect("跑完");
}
