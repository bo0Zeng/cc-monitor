//! 托管终端的一生：各种情形下谁还在（A3-3-18）。假子进程（`sh`）、沙箱数据目录，不起真 claude。
//!
//! 「后端重启」＝ 丢掉手上所有连接、只凭盘重扫：后端对托管终端不留进程内状态，名单 · 打标都是问的那一刻现做。

use super::home;
use super::run::tests::{hello, read_until_out, sandbox_own, spec, start};
use super::wire::{read_frame, write_frame, Ctl, Frame, Want};
use super::*;

const SID: &str = "0b9c2f4e-1111-4222-8333-944455556666";

/// 照一条连接收到的字节还原出来的屏（`welcome` 的整屏 ＋ 之后的 `out`）。
fn screen_of(frames: &[Frame]) -> vt100::Parser {
    let mut p = vt100::Parser::new(24, 80, 0);
    for f in frames {
        match f {
            Frame::Ctl(Ctl::Welcome { screen, .. }) => p.process(screen.as_bytes()),
            Frame::Out(b) => p.process(b),
            _ => {}
        }
    }
    p
}

async fn welcome_of(s: &mut own_chan::Stream) -> Vec<Frame> {
    loop {
        let f = read_frame(s).await.expect("读").expect("没断");
        if matches!(f, Frame::Ctl(Ctl::Welcome { .. })) {
            return vec![f];
        }
    }
}

async fn ended_of(s: &mut own_chan::Stream) -> bool {
    loop {
        match read_frame(s).await.expect("读") {
            Some(Frame::Ctl(Ctl::Ended { .. })) => return true,
            Some(_) => {}
            None => return false,
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn after_the_terminal_window_goes_a_new_backend_finds_it_with_the_same_screen() {
    let (own, _data) = sandbox_own("life-detach");
    let script = r#"printf 'one\r\ntwo\r\nready\r\n'; read x; printf 'bye\r\n'"#;
    let (sock, task) = start(&own, spec("t-life", script)).await;
    // 一个真终端接上、看到画面、走掉（没有后端连着）。
    let mut term = hello(&sock, Want::Input).await;
    let before = screen_of(&read_until_out(&mut term, "ready").await);
    write_frame(&mut term, &Frame::Ctl(Ctl::Bye))
        .await
        .expect("bye");
    drop(term);
    // 新后端：只凭盘。
    let rows = rows_at(&own);
    assert_eq!(rows.len(), 1, "托管 · 子进程照跑：{rows:?}");
    assert_eq!(rows[0].handle(), "t-life");
    // 再接上：屏幕逐格相同。
    let mut again = hello(&sock, Want::Input).await;
    let after = screen_of(&welcome_of(&mut again).await);
    assert_eq!(
        after.screen().contents_formatted(),
        before.screen().contents_formatted(),
        "屏幕与走之前逐格相同"
    );
    assert_eq!(
        after.screen().cursor_position(),
        before.screen().cursor_position()
    );
    write_frame(&mut again, &Frame::In(b"\r".to_vec()))
        .await
        .expect("送字");
    task.await.expect("任务没崩").expect("跑完");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_backend_rescans_and_retags_without_writing_twice() {
    let (own, data) = sandbox_own("life-retag");
    let (sock, task) = start(&own, spec("t-retag", "printf 'ready\\r\\n'; read x")).await;
    // 等子进程真跑起来（exec 之前它的环境还是父进程那一份）。
    let mut w = hello(&sock, Want::View).await;
    read_until_out(&mut w, "ready").await;
    drop(w);
    let pid = home::read_facts(&data.join("hosts").join("t-retag"))
        .and_then(|f| f.child_pid)
        .expect("子进程 pid");
    assert_eq!(tag_pid_at(&own, pid, SID).map(|t| t.1), Some(true));
    // 后端重启：新后端起步重打一遍标（值一样 ⇒ 不写），名单里 sid 还挂着。
    assert_eq!(tag_pid_at(&own, pid, SID).map(|t| t.1), Some(false));
    assert_eq!(
        rows_at(&own).first().map(|r| r.sid.clone()),
        Some(SID.into())
    );
    let mut s = hello(&sock, Want::Input).await;
    write_frame(&mut s, &Frame::In(b"\r".to_vec()))
        .await
        .expect("送字");
    task.await.expect("任务没崩").expect("跑完");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn when_the_agent_exits_every_end_hears_ended_and_the_row_is_gone() {
    let (own, data) = sandbox_own("life-exit");
    let (sock, task) = start(&own, spec("t-exit", "printf 'ready\\r\\n'; read x; exit 0")).await;
    let mut viewer = hello(&sock, Want::View).await;
    let mut term = hello(&sock, Want::Input).await;
    read_until_out(&mut term, "ready").await;
    write_frame(&mut term, &Frame::In(b"\r".to_vec()))
        .await
        .expect("送字");
    assert!(ended_of(&mut term).await, "持有的那一端收到 ended");
    assert!(ended_of(&mut viewer).await, "只看的那一端也收到 ended");
    task.await.expect("任务没崩").expect("跑完");
    assert!(!data.join("hosts").join("t-exit").exists(), "目录收了");
    assert!(rows_at(&own).is_empty(), "名单空");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ending_the_terminal_hangs_up_the_child_and_the_row_is_gone() {
    let (own, _data) = sandbox_own("life-end");
    let script = r#"trap 'exit 7' HUP; printf 'ready\r\n'; while :; do read x; done"#;
    let (sock, task) = start(&own, spec("t-kill", script)).await;
    let mut s = hello(&sock, Want::View).await;
    read_until_out(&mut s, "ready").await;
    write_frame(&mut s, &Frame::Ctl(Ctl::End))
        .await
        .expect("end");
    assert!(ended_of(&mut s).await);
    task.await.expect("任务没崩").expect("跑完");
    assert!(rows_at(&own).is_empty());
}
