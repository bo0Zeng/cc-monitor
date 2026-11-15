//! 托管终端的一生：各种情形下谁还在（A3-3-18）。假子进程（Unix `sh` · Windows `cmd`）、沙箱数据目录，不起真 claude。
//! 同一份在 Linux 门禁里跑，也交叉编到 Windows 上真跑（Win11 虚拟机）。
//!
//! 「后端重启」＝ 丢掉手上所有连接、只凭盘重扫：后端对托管终端不留进程内状态，名单 · 打标都是问的那一刻现做。

use super::home;
use super::run::test_support::{
    hello, read_until_out, sandbox_own, spec_any, spec_ready, start, tags_once, within_ceiling,
};
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
    within_ceiling(async {
        let (own, _data) = sandbox_own("life-detach");
        let sh = r#"printf 'one\r\ntwo\r\nready\r\n'; read x; printf 'bye\r\n'"#;
        let cmd = "echo one& echo two& echo ready& set /p x=& echo bye";
        let (sock, task) = start(&own, spec_any("t-life", sh, cmd)).await;
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
    })
    .await;
}

/// 托管进程一时答不上（整片一起跑、机器忙）：卡住托管进程跑的那一条线，让打标那一下等回音等过它的期限。
#[test]
fn a_host_slow_to_answer_the_tag_still_ends_with_the_sid_written_once() {
    let ceiling = guard_core::wall::CEILING;
    let (own, data) = sandbox_own("life-slow");
    let (sock_tx, sock_rx) = std::sync::mpsc::channel();
    let (rt_tx, rt_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let host_own = own.clone();
    let host = std::thread::spawn(move || {
        // 自己起一个单线程运行时（同托管进程那一形），不用 `run::runtime`：它的线叫 `ccm-host`，
        // 同一进程里数这个名字的那条判据（`an_idle_hosted_terminal_holds_three_threads…`）会把这里的线数进去。
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("起得了运行时");
        let _ = rt_tx.send(rt.handle().clone());
        let ran = rt.block_on(super::run::serve(spec_ready("t-slow"), &host_own, |p| {
            let _ = sock_tx.send(p.to_path_buf());
        }));
        let _ = done_tx.send(());
        ran
    });
    let rt = rt_rx.recv_timeout(ceiling).expect("托管的运行时起来了");
    let sock = sock_rx.recv_timeout(ceiling).expect("托管口绑好了");
    let client = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("客户端的运行时");
    client.block_on(async {
        tokio::time::timeout(ceiling, async {
            let mut w = hello(&sock, Want::View).await;
            read_until_out(&mut w, "ready").await;
        })
        .await
        .expect("兜底上限之内读到 ready");
    });
    let pid = home::read_facts(&data.join("hosts").join("t-slow"))
        .and_then(|f| f.child_pid)
        .expect("子进程 pid");
    // 卡住：托管进程那一条线上放一个任务，它一跑就攥着这条线，直到判据放开。
    let (stuck_tx, stuck_rx) = std::sync::mpsc::channel();
    let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
    rt.spawn(async move {
        let _ = stuck_tx.send(());
        let _ = gate_rx.recv();
    });
    stuck_rx.recv_timeout(ceiling).expect("托管那条线卡住了");
    assert!(
        !tags_once(&own, &data, "t-slow", pid, SID, || drop(gate_tx)),
        "卡住时头一次真等过了回话的期限、照实说没写成（不然这条没复现那个时机）"
    );
    client.block_on(async {
        tokio::time::timeout(ceiling, async {
            let mut s = hello(&sock, Want::Input).await;
            write_frame(&mut s, &Frame::In(b"\r".to_vec()))
                .await
                .expect("送字");
        })
        .await
        .expect("兜底上限之内送得进字");
    });
    done_rx
        .recv_timeout(ceiling)
        .expect("兜底上限之内托管那条线跑完");
    host.join().expect("托管那条线没崩").expect("跑完");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn when_the_agent_exits_every_end_hears_ended_and_the_row_is_gone() {
    within_ceiling(async {
        let (own, data) = sandbox_own("life-exit");
        let (sock, task) = start(
            &own,
            spec_any(
                "t-exit",
                "printf 'ready\\r\\n'; read x; exit 0",
                "echo ready& set /p x=& exit 0",
            ),
        )
        .await;
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
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ending_the_terminal_hangs_up_the_child_and_the_row_is_gone() {
    within_ceiling(async {
        let (own, _data) = sandbox_own("life-end");
        let sh = r#"trap 'exit 7' HUP; printf 'ready\r\n'; while :; do read x; done"#;
        let cmd = "echo ready& ping -n 600 127.0.0.1 >nul";
        let (sock, task) = start(&own, spec_any("t-kill", sh, cmd)).await;
        let mut s = hello(&sock, Want::View).await;
        read_until_out(&mut s, "ready").await;
        write_frame(&mut s, &Frame::Ctl(Ctl::End))
            .await
            .expect("end");
        assert!(ended_of(&mut s).await);
        task.await.expect("任务没崩").expect("跑完");
        assert!(rows_at(&own).is_empty());
    })
    .await;
}
