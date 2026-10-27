//! 托管名单 · 打标那一手：真起托管终端（假子进程 `sh`），沙箱数据目录，不起真 claude。

use super::home;
use super::run::tests::{hello, read_until_out, sandbox_own, spec, start};
use super::wire::{write_frame, Frame, Want};
use super::*;
use crate::wire::{SessionContainer, TerminalHost};

/// 收尾：让假子进程读到一个回车退出，等托管任务跑完。
async fn finish(
    sock: &std::path::Path,
    task: tokio::task::JoinHandle<Result<(), copy_core::said::Said>>,
) {
    let mut s = hello(sock, Want::Input).await;
    write_frame(&mut s, &Frame::In(b"\r".to_vec()))
        .await
        .expect("送字");
    task.await.expect("任务没崩").expect("跑完");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hosted_terminal_is_listed_as_managed_from_disk_alone() {
    let (own, _data) = sandbox_own("rows");
    let mut s = spec("t-rows", "printf 'ready\\r\\n'; read x");
    s.cwd = "/".into();
    let (sock, task) = start(&own, s).await;
    let rows = rows_at(&own);
    assert_eq!(rows.len(), 1, "{rows:?}");
    let r = &rows[0];
    assert_eq!(r.host, TerminalHost::Managed);
    assert_eq!(r.handle(), "t-rows");
    assert_eq!(r.name, "name-t-rows");
    assert_eq!(r.cwd, "/");
    assert_eq!(r.agent, "fake");
    assert_eq!(r.client, "terminal");
    assert_eq!(r.sid, "");
    finish(&sock, task).await;
    assert!(rows_at(&own).is_empty(), "子进程退了 ⇒ 名单空");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tagging_the_child_pid_hangs_the_session_on_its_hosted_terminal() {
    let (own, data) = sandbox_own("tag");
    let (sock, task) = start(&own, spec("t-tag", "printf 'ready\\r\\n'; read x")).await;
    // 等子进程真跑起来（exec 之前 `/proc/<pid>/environ` 还是父进程那一份，没有 `CCM_HOST`）——同生产：claude 跑起来才写 pidfile。
    let mut w = hello(&sock, Want::View).await;
    read_until_out(&mut w, "ready").await;
    drop(w);
    let pid = home::read_facts(&data.join("hosts").join("t-tag"))
        .and_then(|f| f.child_pid)
        .expect("子进程 pid");
    let sid = "0b9c2f4e-1111-4222-8333-944455556666";
    let got = tag_pid_at(&own, pid, sid);
    assert_eq!(
        got,
        Some((
            Some(SessionContainer::Managed {
                terminal: Some("t-tag".into())
            }),
            true
        ))
    );
    // 打标那一下等到托管进程回了话才回（同一条连接上 `tag` 之后的 `clients` 有了回音 ⇒ 事实已写）。
    assert_eq!(
        rows_at(&own).first().map(|r| r.sid.as_str()),
        Some(sid),
        "那一行挂上了这个 sid"
    );
    // 再打一次：值一样就不写。
    assert_eq!(
        tag_pid_at(&own, pid, sid),
        Some((
            Some(SessionContainer::Managed {
                terminal: Some("t-tag".into())
            }),
            false
        ))
    );
    finish(&sock, task).await;
}

#[test]
fn a_process_outside_any_hosted_terminal_is_left_to_tmux() {
    let (own, _data) = sandbox_own("none");
    assert_eq!(
        tag_pid_at(
            &own,
            std::process::id(),
            "0b9c2f4e-1111-4222-8333-944455556666"
        ),
        None
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stale_home_is_not_listed_and_the_next_host_sweeps_it() {
    let (own, data) = sandbox_own("stale");
    // 机器重启过：目录与事实还在、没人攥锁。
    let stale = data.join("hosts").join("t-old");
    std::fs::create_dir_all(&stale).expect("建陈旧目录");
    let facts = home::Facts {
        id: "t-old".into(),
        name: "t-old".into(),
        ..home::Facts::default()
    };
    std::fs::write(
        stale.join(home::FACTS),
        serde_json::to_vec(&facts).expect("编"),
    )
    .expect("写事实");
    assert!(rows_at(&own).is_empty(), "锁空 ⇒ 不列");
    let (sock, task) = start(&own, spec("t-new", "printf 'ready\\r\\n'; read x")).await;
    assert!(!stale.exists(), "起托管的那一方顺手删了陈旧的");
    let rows = rows_at(&own);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].handle(), "t-new", "活着的那个不动");
    let mut s = hello(&sock, Want::View).await;
    read_until_out(&mut s, "ready").await;
    drop(s);
    finish(&sock, task).await;
}
