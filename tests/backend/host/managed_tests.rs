//! 托管名单 · 打标那一手：真起托管终端（假子进程：Unix `sh` · Windows `cmd`），沙箱数据目录，不起真 claude。

use super::home;
use super::run::test_support::{hello, read_until_out, sandbox_own, spec_ready, start};
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
    let mut s = spec_ready("t-rows");
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
    let (sock, task) = start(&own, spec_ready("t-tag")).await;
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
    let (sock, task) = start(&own, spec_ready("t-new")).await;
    assert!(!stale.exists(), "起托管的那一方顺手删了陈旧的");
    let rows = rows_at(&own);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].handle(), "t-new", "活着的那个不动");
    let mut s = hello(&sock, Want::View).await;
    read_until_out(&mut s, "ready").await;
    drop(s);
    finish(&sock, task).await;
}

/// 读不到那个进程的环境（Windows 上读不到别的进程的环境）⇒ 按祖先认：往上找父进程，碰到某个托管终端记下的子进程号就是它（含它自己）。
#[test]
fn a_process_is_found_in_its_hosted_terminal_by_its_ancestors() {
    // 10 → 20（托管终端 a 的子进程）· 30 → 31 → 40（托管终端 b 的子进程）→ 1；99 不在任何托管终端里。
    let parent = |p: u32| match p {
        10 => Some(20),
        20 => Some(1),
        30 => Some(31),
        31 => Some(40),
        40 => Some(1),
        99 => Some(1),
        _ => None,
    };
    let hosts = [("a".to_string(), 20), ("b".to_string(), 40)];
    assert_eq!(host_by_ancestry(10, &hosts, parent).as_deref(), Some("a"));
    assert_eq!(
        host_by_ancestry(20, &hosts, parent).as_deref(),
        Some("a"),
        "它自己就是那个子进程"
    );
    assert_eq!(host_by_ancestry(30, &hosts, parent).as_deref(), Some("b"));
    assert_eq!(host_by_ancestry(99, &hosts, parent), None);
    assert_eq!(host_by_ancestry(7, &hosts, parent), None, "父进程问不到");
}

/// 附着在托管终端上的那几端里，显示会话的只算真终端（后端那一端 —— 终端页 · 抓屏 · 打标 —— 不算）：
/// 带上进程号（报不出 ⇒ `None`）与最近动静（秒），最近动静在前。
#[test]
fn only_the_real_terminals_attached_to_a_hosted_terminal_show_it() {
    use super::wire::{ClientRow, WhoKind};
    let row = |client: u64, kind: WhoKind, pid: Option<u32>, last_ms: i64| ClientRow {
        client,
        kind,
        front: None,
        pid,
        since_ms: 1_000,
        last_ms,
        holder: false,
        mine: false,
    };
    let ends = terminal_ends(vec![
        row(1, WhoKind::Terminal, Some(11), 5_000),
        row(2, WhoKind::Backend, Some(22), 9_000),
        row(3, WhoKind::Terminal, None, 7_000),
        row(4, WhoKind::Terminal, Some(44), 8_500),
    ]);
    assert_eq!(ends, vec![(Some(44), 8), (None, 7), (Some(11), 5)]);
    assert_eq!(
        terminal_ends(vec![row(2, WhoKind::Backend, Some(22), 9_000)]),
        vec![],
        "只有后端那一端连着 ⇒ 没有终端"
    );
}

/// 父进程表成了环（进程号复用时快照里会有）⇒ 走到上限就停，不挂死、不瞎认。
#[test]
fn walking_the_ancestors_stops_on_a_loop() {
    let parent = |p: u32| Some(if p == 5 { 6 } else { 5 });
    let hosts = [("a".to_string(), 20)];
    assert_eq!(host_by_ancestry(5, &hosts, parent), None);
}
