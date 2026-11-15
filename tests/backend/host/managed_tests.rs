//! 托管名单 · 打标那一手：真起托管终端（假子进程：Unix `sh` · Windows `cmd`），沙箱数据目录，不起真 claude。

use super::home;
use super::run::test_support::{
    hello, read_until_out, sandbox_own, spec_ready, start, tags_once, within_ceiling,
};
use super::wire::{write_frame, Frame, Want};
use super::*;
use crate::wire::TerminalHost;

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
    within_ceiling(async {
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
    })
    .await;
}

/// 打标挂上会话、再打一次不写；后端重启（新后端起步重打一遍）走的也是这一下：后端对托管终端不留进程内状态。
/// 判盘上的状态、不判头一次的回音（托管进程一时答不上时它照实说没写成，卡住那一形见 `life_tests`）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tagging_the_child_pid_hangs_the_session_on_its_hosted_terminal() {
    within_ceiling(async {
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
        let _wrote_now = tags_once(&own, &data, "t-tag", pid, sid, || {});
        finish(&sock, task).await;
    })
    .await;
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
    within_ceiling(async {
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
    })
    .await;
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

/// 托管进程的退出码就是里面那个子进程的码（起它的那一方 · 收尾那一侧照它判子进程怎么退的）；参数不成形 ⇒ 退 2。
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_hosted_process_exits_with_its_childs_code() {
    within_ceiling(async {
        let (own, _data) = sandbox_own("ccmdbg-code");
        // 不经 `main`：它说「起好了」之后把本进程的 stdout 换成空设备（这里是测试进程）。
        let code = super::run::serve(
            super::run::test_support::spec("ccmdbg-c", "exit 7"),
            &own,
            |_| {},
        )
        .await
        .expect("跑完");
        assert_eq!(code, Some(7));
        assert_eq!(super::run::main(&["ccmdbg-c".to_string()], &own).await, 2);
    })
    .await;
}

/// 父进程表成了环（进程号复用时快照里会有）⇒ 走到上限就停，不挂死、不瞎认。
#[test]
fn walking_the_ancestors_stops_on_a_loop() {
    let parent = |p: u32| Some(if p == 5 { 6 } else { 5 });
    let hosts = [("a".to_string(), 20)];
    assert_eq!(host_by_ancestry(5, &hosts, parent), None);
}

/// 等到 `pid` 的 `comm` 是 `comm`、环境读得出（exec 换身那一刻 environ 是空的；墙钟只当兜底上限）。
#[cfg(target_os = "linux")]
fn settled(pid: u32, comm: &str) -> bool {
    guard_core::wall::rounds().any(|_| {
        let c = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        let env = std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
        let hit = c.trim() == comm && !env.is_empty();
        if !hit {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        hit
    })
}

/// 判据起的那几个 `sleep`：判据怎么收场（断言红了也算）都按 pid 停掉，不留到 600 秒。
#[cfg(target_os = "linux")]
struct Reap(Vec<u32>);

#[cfg(target_os = "linux")]
impl Drop for Reap {
    fn drop(&mut self) {
        for pid in &self.0 {
            let _ = std::process::Command::new("kill")
                .arg(pid.to_string())
                .status();
        }
    }
}

/// ★ 托管终端里只有前台那一个打得上标：同一个托管终端里起的 ① 没控制终端的进程（主会话 Bash 里起的 `claude -p` 那一形）
/// ② 伪终端上的后台进程组 —— 环境里都带着同一个 `CCM_HOST`，打标却不把事实里的 sid 换成自己的、容器说不清；① 报没控制终端。
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_the_hosted_terminals_foreground_process_is_tagged() {
    within_ceiling(async {
        let (own, data) = sandbox_own("fg");
        let kid_file = data.join("kid-pid");
        let bg_file = data.join("bg-pid");
        let script = format!(
            "setsid sleep 600 </dev/null >/dev/null 2>&1 & echo $! > '{}'; set -m; sleep 600 & echo $! > '{}'; printf 'ready\\r\\n'; read x",
            kid_file.display(),
            bg_file.display()
        );
        let (sock, task) = start(&own, super::run::test_support::spec("t-fg", &script)).await;
        let mut w = hello(&sock, Want::View).await;
        read_until_out(&mut w, "ready").await;
        drop(w);
        let dir = data.join("hosts").join("t-fg");
        let main = home::read_facts(&dir)
            .and_then(|f| f.child_pid)
            .expect("子进程 pid");
        let main_sid = "1f58e575-0000-4000-8000-000000000001";
        let kid_sid = "2c2cb902-0000-4000-8000-000000000002";
        let _wrote_now = tags_once(&own, &data, "t-fg", main, main_sid, || {});
        let pid_of = |f: &std::path::Path| -> u32 {
            std::fs::read_to_string(f)
                .expect("壳写下了 pid")
                .trim()
                .parse()
                .expect("pid")
        };
        let sid_now = || home::read_facts(&dir).map(|f| f.sid).unwrap_or_default();
        let (kid, bg) = (pid_of(&kid_file), pid_of(&bg_file));
        let _reap = Reap(vec![kid, bg]);

        // ① 没控制终端。
        assert!(settled(kid, "sleep"), "setsid sleep 没起来");
        assert_eq!(
            tag_pid_at(&own, kid, kid_sid),
            Some(Tagged {
                container: None,
                wrote: false,
                detached: true
            }),
            "没控制终端的进程"
        );
        assert_eq!(sid_now(), main_sid, "① 把事实里的 sid 换掉了");

        // ② 伪终端上的后台进程组。
        assert!(settled(bg, "sleep"), "后台 sleep 没起来");
        assert_eq!(
            tag_pid_at(&own, bg, kid_sid),
            Some(Tagged::seated(None, false)),
            "后台进程组里的进程"
        );
        assert_eq!(sid_now(), main_sid, "② 把事实里的 sid 换掉了");

        finish(&sock, task).await;
    })
    .await;
}

/// ★ 读不到终端那一样的平台（Windows · macOS）照「ccm 起的那一跳」认前台（事实里的 `hop_pid` · `hop_wrapped`，ccm 起的那一刻写）：
/// - 原生：会话进程就是那一跳 ⇒ 前台。
/// - 包一层（ccm 起的是 `cmd.exe /c claude.cmd` · `powershell -File claude.ps1`，那一层只跑这一条）：会话进程的父进程是那一跳 ⇒ 前台。
/// - 再往下的后代 · 没记包一层却是子进程 · 托管终端的子进程（Windows 上是 ccm 自己）· 那一跳还没记下 · 事实读不到 ⇒ 说不清。
/// 读得到终端的平台（Linux）不看那一格，照旧 [`seat_with`]。
#[test]
fn without_a_readable_terminal_the_hop_ccm_started_decides_the_foreground() {
    // 10 ccm（托管终端的子进程）→ 20 那一跳 → 30 node → 40 node 再起的。
    let parent = |p: u32| match p {
        20 => Some(10),
        30 => Some(20),
        40 => Some(30),
        _ => None,
    };
    let facts = |hop: Option<u32>, wrapped: bool| home::Facts {
        id: "t-hop".into(),
        child_pid: Some(10),
        hop_pid: hop,
        hop_wrapped: wrapped,
        ..home::Facts::default()
    };
    let at = |f: Option<&home::Facts>, pid: u32| seat_in(f, pid, false, parent);
    // 原生。
    assert_eq!(
        at(Some(&facts(Some(20), false)), 20),
        Seat::OnPane,
        "原生：会话进程就是那一跳"
    );
    assert_eq!(
        at(Some(&facts(Some(20), false)), 30),
        Seat::Unknown,
        "原生那一跳的子进程不算"
    );
    // 包一层。
    assert_eq!(
        at(Some(&facts(Some(20), true)), 30),
        Seat::OnPane,
        "包一层：父进程是那一跳"
    );
    assert_eq!(
        at(Some(&facts(Some(20), true)), 40),
        Seat::Unknown,
        "包一层底下再往下的后代"
    );
    assert_eq!(
        at(Some(&facts(Some(20), true)), 20),
        Seat::OnPane,
        "会话进程就是那一跳：包没包一层都算"
    );
    // 别的。
    assert_eq!(
        at(Some(&facts(Some(20), true)), 10),
        Seat::Unknown,
        "托管终端的子进程（ccm 自己）不是那一跳"
    );
    assert_eq!(
        at(Some(&facts(None, true)), 30),
        Seat::Unknown,
        "那一跳还没记下"
    );
    assert_eq!(at(None, 20), Seat::Unknown, "事实读不到");
    // 读得到终端：那一格不作数（首领对不上 0 ⇒ 不会是前台）。
    let me = std::process::id();
    let mut f = facts(Some(me), false);
    f.child_pid = Some(0);
    assert_ne!(
        seat_in(Some(&f), me, true, parent),
        Seat::OnPane,
        "Linux 不看那一跳"
    );
}

/// 事实落到 `want` 为止（`hop` 不等回话 ⇒ 交出去之后由托管进程随后写；兜底上限管着等多久）。
async fn facts_become(dir: &std::path::Path, want: impl Fn(&home::Facts) -> bool) -> home::Facts {
    loop {
        if let Some(f) = home::read_facts(dir).filter(|f| want(f)) {
            return f;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// 交那一跳不等托管进程回话：口那头收下连接却一个字都不回（卡住的托管进程），`hop_started` 照样当场回，不拖住起 agent。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn handing_over_the_hop_does_not_wait_for_the_hosted_process_to_answer() {
    within_ceiling(async {
        let (_own, data) = sandbox_own("hop-mute");
        let sock = data.join("sock");
        let listener = own_chan::bind(&sock).expect("绑得上");
        // 收下就攥着，不读不回。
        let held = tokio::spawn(async move {
            let mut kept = Vec::new();
            while let Ok(c) = listener.accept().await {
                kept.push(c);
            }
        });
        let s = sock.clone();
        // 要等回话的那一形读到期限就报 `TimedOut`；不等回话 ⇒ 写出去就回 `Ok`。
        let sent = tokio::task::spawn_blocking(move || client::hop_started(&s, 1, 4242, false))
            .await
            .expect("没崩");
        assert!(sent.is_ok(), "不等回话：写出去了就是交出去了：{sent:?}");
        held.abort();
    })
    .await;
}

/// 口那头没人（托管进程不在）⇒ 照实报没交上，不说交出去了。
#[test]
fn handing_over_the_hop_to_nobody_says_it_did_not_go() {
    let (_own, data) = sandbox_own("hop-nobody");
    assert!(client::hop_started(&data.join("sock"), 1, 4242, false).is_err());
}

/// ccm 起好那一跳时经托管口交来的 pid 与「包一层」落进那个托管终端的事实（不等回话，随后落盘）；子进程号不动。
/// 只认托管终端的子进程（里面那个 ccm 自己）交来的：主会话里再起的 ccm（环境里也带着 `CCM_HOST`）交的不换那一跳。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_hop_ccm_reports_lands_in_the_hosted_terminals_facts() {
    within_ceiling(async {
        let (own, data) = sandbox_own("hop");
        let (sock, task) = start(&own, spec_ready("t-hop")).await;
        let dir = data.join("hosts").join("t-hop");
        let before = home::read_facts(&dir).expect("事实");
        assert_eq!(
            (before.hop_pid, before.hop_wrapped),
            (None, false),
            "起来时还没有那一跳"
        );
        let child = before.child_pid.expect("子进程号");
        let hop = |by: u32, pid: u32, wrapped: bool| {
            let s = sock.clone();
            tokio::task::spawn_blocking(move || client::hop_started(&s, by, pid, wrapped))
        };
        hop(child, 4242, true)
            .await
            .expect("没崩")
            .expect("交出去了");
        let after = facts_become(&dir, |f| f.hop_pid.is_some()).await;
        assert_eq!((after.hop_pid, after.hop_wrapped), (Some(4242), true));
        assert_eq!(after.child_pid, before.child_pid, "子进程号不动");
        // 别人交来的：同一条连接上紧跟一个 `clients`，有了回音 ⇒ 托管进程已处理过那一帧（按收到的先后逐帧做）。
        let s = sock.clone();
        let by = child.wrapping_add(1);
        tokio::task::spawn_blocking(move || {
            let c = client::open(&s, None).expect("连得上");
            let hop = wire::Ctl::Hop {
                by,
                pid: 5353,
                wrapped: false,
            };
            client::send(&c, &Frame::Ctl(hop)).expect("写得出");
            client::send(&c, &Frame::Ctl(wire::Ctl::Clients)).expect("写得出");
            let mut buf = Vec::new();
            while !matches!(
                client::next_frame(&c, &mut buf).expect("回话"),
                Frame::Ctl(wire::Ctl::ClientsAre { .. })
            ) {}
        })
        .await
        .expect("没崩");
        let later = home::read_facts(&dir).expect("事实");
        assert_eq!(
            (later.hop_pid, later.hop_wrapped),
            (Some(4242), true),
            "别的进程交来的不换那一跳"
        );
        finish(&sock, task).await;
    })
    .await;
}
