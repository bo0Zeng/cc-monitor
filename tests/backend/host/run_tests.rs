//! 托管进程：真起一个伪终端（里面是 `sh` 跑的假程序：打印 · 回显），沙箱数据目录，不起真 claude。

use super::super::home;
use super::super::wire::{read_frame, write_frame, Ctl, Frame, Size, Want, WhoKind};
use super::test_support::{
    hello, read_until_out, sandbox_own, spec, spec_any, spec_ready, start, within_ceiling,
};
use super::*;

async fn read_until_ended(s: &mut own_chan::Stream) -> Option<i32> {
    loop {
        match read_frame(s).await.expect("读得了") {
            Some(Frame::Ctl(Ctl::Ended { code })) => return code,
            Some(_) => {}
            None => panic!("没收到 ended 就断了"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hosted_terminal_echoes_input_and_cleans_up_when_the_child_exits() {
    within_ceiling(async {
        let (own, data) = sandbox_own("echo");
        let sh = r#"printf 'ready\r\n'; read x; printf 'got:%s\r\n' "$x"; printf 'host=%s term=%s\r\n' "$CCM_HOST" "$TERM"; read y; exit 3"#;
        let cmd =
            "echo ready& set /p x=& echo got:!x!& echo host=!CCM_HOST! term=!TERM!& set /p y=& exit 3";
        let (sock, task) = start(&own, spec_any("t-echo", sh, cmd)).await;
        let dir = data.join("hosts").join("t-echo");
        // 家：0700（Unix）、事实在、锁攥着（活着）。
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&dir)
                .expect("目录在")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        let facts = home::read_facts(&dir).expect("事实在");
        assert_eq!(facts.id, "t-echo");
        assert_eq!(facts.name, "name-t-echo");
        assert!(facts.child_pid.is_some());
        assert_eq!(home::alive(&own).len(), 1, "锁攥着 ⇒ 算活着");
        assert_eq!(
            home::find(&own, "name-t-echo").map(|f| f.id),
            Some("t-echo".into())
        );

        let mut s = hello(&sock, Want::Input).await;
        read_until_out(&mut s, "ready").await;
        write_frame(&mut s, &Frame::In(b"hi\r".to_vec()))
            .await
            .expect("送字");
        // 两行可能落在同一块 `out` 里：一次读到后一行，再看前一行在不在收到的里面。
        let got = read_until_out(&mut s, "host=t-echo term=xterm-256color").await;
        let text: String = got
            .iter()
            .filter_map(|f| match f {
                Frame::Out(b) => Some(String::from_utf8_lossy(b).into_owned()),
                _ => None,
            })
            .collect();
        assert!(text.contains("got:hi"), "{text:?}");
        write_frame(&mut s, &Frame::In(b"\r".to_vec()))
            .await
            .expect("送字");
        assert_eq!(read_until_ended(&mut s).await, Some(3), "带子进程的退出码");
        task.await.expect("任务没崩").expect("跑完");
        assert!(!dir.exists(), "目录收掉了");
        assert!(home::alive(&own).is_empty());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn end_hangs_up_the_child_group() {
    within_ceiling(async {
        let (own, data) = sandbox_own("end");
        let sh = r#"trap 'exit 7' HUP; printf 'ready\r\n'; while :; do read x; done"#;
        let cmd = "echo ready& ping -n 600 127.0.0.1 >nul";
        let (sock, task) = start(&own, spec_any("t-end", sh, cmd)).await;
        let mut s = hello(&sock, Want::View).await;
        read_until_out(&mut s, "ready").await;
        write_frame(&mut s, &Frame::Ctl(Ctl::End))
            .await
            .expect("end");
        let code = read_until_ended(&mut s).await;
        assert!(code.is_some(), "收到 ended：{code:?}");
        task.await.expect("任务没崩").expect("跑完");
        assert!(!data.join("hosts").join("t-end").exists());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_id_cannot_be_hosted_twice() {
    within_ceiling(async {
        let (own, _data) = sandbox_own("twice");
        let (sock, task) = start(&own, spec_ready("t-twice")).await;
        let again = serve(spec_any("t-twice", "true", "exit 0"), &own, |_| {}).await;
        assert!(again.is_err(), "第二个拿不到锁");
        let mut s = hello(&sock, Want::Input).await;
        read_until_out(&mut s, "ready").await;
        write_frame(&mut s, &Frame::In(b"\r".to_vec()))
            .await
            .expect("送字");
        task.await.expect("任务没崩").expect("跑完");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_viewer_cannot_type_into_the_terminal() {
    within_ceiling(async {
        let (own, _data) = sandbox_own("view");
        let sh = r#"printf 'ready\r\n'; read x; printf 'got:%s\r\n' "$x""#;
        let cmd = "echo ready& set /p x=& echo got:!x!";
        let (sock, task) = start(&own, spec_any("t-view", sh, cmd)).await;
        let mut viewer = hello(&sock, Want::View).await;
        read_until_out(&mut viewer, "ready").await;
        write_frame(&mut viewer, &Frame::In(b"nope\r".to_vec()))
            .await
            .expect("送字");
        loop {
            match read_frame(&mut viewer).await.expect("读").expect("没断") {
                Frame::Ctl(Ctl::HeldElsewhere { .. }) => break,
                Frame::Out(b) => assert!(
                    !String::from_utf8_lossy(&b).contains("nope"),
                    "只看的字没进去"
                ),
                _ => {}
            }
        }
        let mut typer = hello(&sock, Want::Input).await;
        write_frame(&mut typer, &Frame::In(b"yes\r".to_vec()))
            .await
            .expect("送字");
        read_until_out(&mut viewer, "got:yes").await;
        task.await.expect("任务没崩").expect("跑完");
    })
    .await;
}

#[test]
fn args_parse_into_a_spec() {
    let a: Vec<String> = [
        "h1",
        "name=n1",
        "size=120x40",
        "cwd=/tmp",
        "account=b",
        "--",
        "claude",
        "--resume",
        "x",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let s = parse(&a).expect("解得开");
    assert_eq!(s.id, "h1");
    assert_eq!(s.name, "n1");
    assert_eq!(s.account, "b");
    assert_eq!(
        s.size,
        Size {
            rows: 40,
            cols: 120
        }
    );
    assert_eq!(s.argv, vec!["claude", "--resume", "x"]);
    assert!(parse(&["h1".to_string()]).is_err(), "没有 argv");
    assert!(
        parse(&["../x".to_string(), "--".into(), "sh".into()]).is_err(),
        "句柄带路径字符"
    );
}

/// 托管口认得连上来的那一端是哪个进程（系统给的对端进程号：Unix `SO_PEERCRED` · Windows 命名管道的客户端进程号）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_host_knows_which_process_each_end_is() {
    within_ceiling(async {
        let (own, _data) = sandbox_own("peer");
        let (sock, task) = start(&own, spec_ready("t-peer")).await;
        let mut s = hello(&sock, Want::Input).await;
        read_until_out(&mut s, "ready").await;
        let asked = sock.clone();
        let ends = tokio::task::spawn_blocking(move || super::super::client::clients(&asked))
            .await
            .expect("问的那条线没崩")
            .expect("问到了");
        let term = ends
            .iter()
            .find(|e| e.kind == WhoKind::Terminal)
            .expect("真终端那一端在名单里");
        assert_eq!(term.pid, Some(std::process::id()), "{ends:?}");
        write_frame(&mut s, &Frame::In(b"\r".to_vec()))
            .await
            .expect("送字");
        task.await.expect("任务没崩").expect("跑完");
    })
    .await;
}

/// 数据目录很长、`hosts/<id>/sock` 超过套接字上限 ⇒ 起不来，回的话带路径与长度；不偷偷换地方，盘上不留那个家。
/// 只在通道是盘上一个文件的平台上有这一条上限。
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_socket_path_over_the_limit_refuses_to_start_and_says_how_long() {
    let (own, data) = sandbox_own(&"l".repeat(home::SOCK_PATH_MAX));
    let mut bound = false;
    let got = serve(spec("t-long", "true"), &own, |_| bound = true).await;
    let said = got.expect_err("路径超过上限起不来").logged();
    let sock = data.join("hosts").join("t-long").join("sock");
    assert!(
        said.contains(&sock.as_os_str().len().to_string()),
        "说清多长：{said}"
    );
    assert!(
        said.contains(&home::SOCK_PATH_MAX.to_string()),
        "说清上限：{said}"
    );
    assert!(!bound, "没绑口");
    assert!(!data.join("hosts").join("t-long").exists(), "不留那个家");
}

/// 本进程此刻有几条线：`/proc/self/status` 的 `Threads:`（内核记着的那一个数，一次读完）。
/// 不逐条读 `/proc/self/task`：同一进程里有线在退时那张目录不是快照，读到一半退掉一条，接着读按位置往后数，跳过还活着的。
/// 数的是整个进程 ⇒ 只在只跑一条判据的子进程里数才有意义（[`alone`]）。
#[cfg(target_os = "linux")]
fn threads_here() -> usize {
    use crate::platform::fs::Ro;
    /// `status` 几十行；上限只为走有上限的那一条读法。
    const STATUS_CAP: cap_core::Cap = cap_core::Cap {
        bytes: 64 * 1024,
        over: cap_core::Over::Reject,
    };
    Ro::read_string_capped("/proc/self/status", STATUS_CAP)
        .expect("读得了本进程的状态")
        .lines()
        .find_map(|l| l.strip_prefix("Threads:"))
        .and_then(|n| n.trim().parse().ok())
        .expect("状态里有 Threads:")
}

/// 带着它重起的那一趟是子进程（[`alone`]）。
#[cfg(target_os = "linux")]
const ALONE: &str = "CCM_THREADS_ALONE";

/// 数线程的判据只在自己一个进程里数：没带 [`ALONE`] ⇒ 把测试二进制自己重起、`--exact` 只跑 `test`（本模块里那条判据的名字），
/// 照它的结果红绿；带着 ⇒ 这一趟就是子进程，跑 `body`。整片一起跑时别的判据的线时时在起在退，进程的线数里数不出这一条的。
#[cfg(target_os = "linux")]
fn alone(test: &str, body: impl FnOnce()) {
    if std::env::var_os(ALONE).is_some() {
        return body();
    }
    let module = module_path!()
        .split_once("::")
        .map_or(module_path!(), |(_, m)| m);
    let full = format!("{module}::{test}");
    let out = std::process::Command::new(std::env::current_exe().expect("测试二进制"))
        .args([full.as_str(), "--exact", "--test-threads=1"])
        .env(ALONE, "1")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("起子进程");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // 名字对不上时 libtest 一条不跑也退 0 ⇒ 还要看到恰好跑过这一条。
    assert!(
        out.status.success() && said.contains("1 passed"),
        "子进程里 {full} 没过：\n{said}"
    );
}

/// 数线程那一下不受同一进程里别的线退掉的影响：常驻线之间夹一大批线，打乱了次序一条条放它们退、同时不停地数；
/// 几条线同时数，每一次都不少于「起它们之前 ＋ 常驻线 ＋ 放线那一条 ＋ 数的那几条 ＋ 数完时还没放的」（没放的那几条整趟都活着），退光以后恰好等于起它们之前 ＋ 常驻线。
#[cfg(target_os = "linux")]
#[test]
fn counting_threads_holds_while_threads_between_them_exit() {
    alone(
        "counting_threads_holds_while_threads_between_them_exit",
        counting_while_threads_exit,
    );
}

#[cfg(target_os = "linux")]
fn counting_while_threads_exit() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    const KEPT: usize = 3;
    const BETWEEN: usize = 3000;
    const ROUNDS: usize = 20;
    /// 放的次序：每次往后跳这么多（与 [`BETWEEN`] 互素 ⇒ 每条恰好放一次）：退掉的散在线表各处。
    const STRIDE: usize = 1543;
    /// 放的节奏：放这么多条歇一小段，退线摊在许多次数当中（注入的节奏，不是期限）。
    const BATCH: usize = 10;
    const PAUSE: std::time::Duration = std::time::Duration::from_micros(200);
    /// 同时数的线：几条一起数，退线落在某一次数当中的机会多几倍。
    const COUNTERS: usize = 8;
    let stays = || {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let h = std::thread::spawn(move || {
            let _ = rx.recv();
        });
        (tx, h)
    };
    let base = threads_here();
    for _ in 0..ROUNDS {
        // 上一轮 join 过的线在内核的数里还会多留一会儿：等它们真退了再起这一轮。
        let mut settled = threads_here();
        for () in guard_core::wall::rounds() {
            if settled == base {
                break;
            }
            settled = threads_here();
        }
        assert_eq!(settled, base, "兜底上限之内上一轮的线退光");
        let mut kept = vec![stays()];
        let between: Vec<_> = (0..BETWEEN).map(|_| stays()).collect();
        kept.extend((1..KEPT).map(|_| stays()));
        let (mut go, between): (Vec<Option<_>>, Vec<_>) =
            between.into_iter().map(|(tx, h)| (Some(tx), h)).unzip();
        let floor = base + KEPT;
        // 先记一笔再放：记过的才可能已经退了。
        let released = AtomicUsize::new(0);
        let counts = AtomicUsize::new(0);
        // 几条数的线、放线那一条都起好了才开始：从那一刻起它们都算在下限里。
        // 等在兜底上限之内：哪一条起不来，别的等满上限照实红，不把子进程挂住。
        let up = AtomicUsize::new(0);
        let all_up = || {
            up.fetch_add(1, Ordering::SeqCst);
            let mut seen = 0;
            for () in guard_core::wall::rounds() {
                seen = up.load(Ordering::SeqCst);
                if seen == COUNTERS + 1 {
                    break;
                }
                std::thread::yield_now();
            }
            assert_eq!(seen, COUNTERS + 1, "兜底上限之内数线与放线都起来了");
        };
        std::thread::scope(|s| {
            for _ in 0..COUNTERS {
                s.spawn(|| {
                    all_up();
                    loop {
                        let n = threads_here();
                        let r = released.load(Ordering::SeqCst);
                        if r == BETWEEN {
                            break;
                        }
                        // 数完时还有没放的 ⇒ 放线那一条、数的这几条、没放的那几条整趟都在。
                        let least = floor + 1 + COUNTERS + (BETWEEN - r);
                        assert!(
                            n >= least,
                            "别的线在退时数漏了：数到 {n} 条，至少该有 {least} 条"
                        );
                        counts.fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
            s.spawn(|| {
                all_up();
                let mut at = 0usize;
                for i in 0..BETWEEN {
                    at = (at + STRIDE) % BETWEEN;
                    released.store(i + 1, Ordering::SeqCst);
                    drop(go[at].take());
                    if (i + 1) % BATCH == 0 {
                        std::thread::sleep(PAUSE);
                    }
                }
            });
        });
        let counts = counts.into_inner();
        assert!(counts > 0, "这一轮边退边数过：数了 {counts} 次");
        let mut n = threads_here();
        for () in guard_core::wall::rounds() {
            assert!(
                n >= floor,
                "别的线在退时数漏了常驻线：数到 {n} 条，至少该有 {floor} 条"
            );
            if n == floor {
                break;
            }
            n = threads_here();
        }
        assert_eq!(n, floor, "兜底上限之内退光：恰好是起它们之前 ＋ 常驻线");
        for h in between {
            h.join().expect("夹在中间的线没崩");
        }
        for (tx, h) in kept {
            drop(tx);
            h.join().expect("常驻线没崩");
        }
    }
}

/// 托管进程的线程按需：空闲时（只读伪终端 · 等子进程）不超过 3 条、不随核数变；来了输入才起写的那一条，闲了它自己退。
/// 照托管进程那一形跑：一条叫 [`THREAD_NAME`] 的线上起 [`runtime`]、在上面跑 [`serve`]；
/// 在只跑这一条的子进程里数「起托管之前 → 之后」多了几条（[`alone`]）。
#[cfg(target_os = "linux")]
#[test]
fn an_idle_hosted_terminal_holds_three_threads_whatever_the_core_count() {
    alone(
        "an_idle_hosted_terminal_holds_three_threads_whatever_the_core_count",
        idle_hosted_terminal,
    );
}

#[cfg(target_os = "linux")]
fn idle_hosted_terminal() {
    let (own, _data) = sandbox_own("idle");
    let base = threads_here();
    let added = || threads_here().saturating_sub(base);
    let sh = r#"printf 'ready\r\n'; read x; printf 'got:%s\r\n' "$x"; read y"#;
    // 每一步都等在兜底上限之内（`guard_core::wall::CEILING`）：等不到照实红，不挂住整片。
    let ceiling = guard_core::wall::CEILING;
    let (sock_tx, sock_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let host = std::thread::Builder::new()
        .name(THREAD_NAME.into())
        .spawn(move || {
            let ran =
                runtime()
                    .expect("起得了运行时")
                    .block_on(serve(spec("t-idle", sh), &own, |p| {
                        let _ = sock_tx.send(p.to_path_buf());
                    }));
            let _ = done_tx.send(());
            ran
        })
        .expect("起得了托管那条线");
    let sock = sock_rx.recv_timeout(ceiling).expect("托管口绑好了");
    let client = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("客户端的运行时");
    let mut s = client.block_on(async {
        tokio::time::timeout(ceiling, async {
            let mut s = hello(&sock, Want::Input).await;
            read_until_out(&mut s, "ready").await;
            s
        })
        .await
        .expect("兜底上限之内读到 ready")
    });
    let idle = added();
    assert!(
        idle <= 3,
        "空闲时托管进程开了 {idle} 条线（这台 {} 核）：只该有跑运行时的那一条 · 读伪终端 · 等子进程",
        std::thread::available_parallelism().map_or(0, usize::from)
    );
    client.block_on(async {
        tokio::time::timeout(ceiling, async {
            write_frame(&mut s, &Frame::In(b"hi\r".to_vec()))
                .await
                .expect("送字");
            read_until_out(&mut s, "got:hi").await;
        })
        .await
        .expect("兜底上限之内读到回显");
    });
    // 写伪终端那一条是来了输入才起的；闲下来它自己退（阻塞池的闲置期限），线程数回到空闲那样。
    let mut after = added();
    for () in guard_core::wall::rounds() {
        if after <= idle {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        after = added();
    }
    assert!(
        after <= idle,
        "输入写完、等到兜底上限还剩 {after} 条线（空闲时 {idle} 条）"
    );
    client.block_on(async {
        tokio::time::timeout(ceiling, write_frame(&mut s, &Frame::In(b"\r".to_vec())))
            .await
            .expect("兜底上限之内送出字")
            .expect("送字");
    });
    done_rx
        .recv_timeout(ceiling)
        .expect("兜底上限之内托管那条线跑完");
    host.join().expect("托管那条线没崩").expect("跑完");
}

/// 阻塞池的上限：照这台的线程额度（`RLIMIT_NPROC` · cgroup `pids.max`，取小的）只用一小份；
/// 不少于写伪终端那一条（池里只跑它）；读不到 ⇒ 保守值。
#[test]
fn the_blocking_pool_cap_takes_a_small_share_of_the_thread_quota() {
    assert_eq!(blocking_cap(None, None), CAP_UNKNOWN, "读不到 ⇒ 保守值");
    assert_eq!(
        blocking_cap(Some(50), None),
        CAP_FLOOR,
        "额度很小也给够写的那一条"
    );
    assert_eq!(
        blocking_cap(Some(235_100), Some(70_530)),
        CAP_CEIL,
        "额度很大也只到顶"
    );
    assert_eq!(
        blocking_cap(Some(235_100), Some(3 * CAP_SHARE)),
        3,
        "两样都读到 ⇒ 照小的那一样"
    );
    assert_eq!(blocking_cap(None, Some(3 * CAP_SHARE)), 3);
    const { assert!(CAP_FLOOR < CAP_UNKNOWN && CAP_UNKNOWN < CAP_CEIL) };
}

/// 线程额度两处的读法：`/proc/self/limits` 的 `Max processes` 软上限一格 · cgroup v2 的 `pids.max`；`unlimited` · `max` ⇒ 没限。
#[test]
fn thread_quota_texts_parse() {
    let limits = "Limit                     Soft Limit           Hard Limit           Units     \n\
                  Max cpu time              unlimited            unlimited            seconds   \n\
                  Max processes             235100               240000               processes \n";
    assert_eq!(nproc_of_limits(limits), Some(235_100));
    assert_eq!(
        nproc_of_limits(
            "Max processes             unlimited            unlimited            processes \n"
        ),
        None
    );
    assert_eq!(
        nproc_of_limits("Max cpu time   unlimited  unlimited  seconds\n"),
        None
    );
    assert_eq!(pids_max_of("70530\n"), Some(70_530));
    assert_eq!(pids_max_of("max\n"), None);
    assert_eq!(pids_max_of(""), None);
    assert_eq!(
        cgroup_path_of("0::/user.slice/user-1000.slice/x.scope\n"),
        Some("/user.slice/user-1000.slice/x.scope")
    );
    assert_eq!(
        cgroup_path_of("12:pids:/a\n"),
        None,
        "只认 cgroup v2 那一行"
    );
}

/// 托管着一个终端的运行时收得掉：子进程还在等输入（读伪终端 · 等子进程那两条都回不来）时把运行时丢掉，丢的那一下不等它们。
/// 各判据就是这一形（`#[tokio::test]` 起托管、没让子进程退就收场）：常驻那两条若挂在运行时的阻塞池上，丢运行时那一下永远等下去。
/// 期限是兜底的那个数（`guard_core::wall::CEILING`：不许无限等）；到期照实红，事后把子进程收掉。
#[cfg(unix)]
#[test]
fn a_runtime_hosting_a_terminal_drops_while_the_child_still_waits() {
    let (own, data) = sandbox_own("drop");
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let (pid_tx, pid_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("运行时");
        rt.block_on(async {
            let (sock, _task) = start(&own, spec_ready("t-drop")).await;
            let mut s = hello(&sock, Want::View).await;
            read_until_out(&mut s, "ready").await;
            let facts = home::read_facts(&data.join("hosts").join("t-drop")).expect("事实在");
            let _ = pid_tx.send(facts.child_pid);
        });
        drop(rt);
        let _ = done_tx.send(());
    });
    let child = pid_rx
        .recv_timeout(guard_core::wall::CEILING)
        .expect("托管起来了");
    let dropped = done_rx.recv_timeout(guard_core::wall::CEILING);
    if let Some(pid) = child {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
    }
    assert!(
        dropped.is_ok(),
        "子进程还在时丢运行时，丢的那一下挂住了（常驻的阻塞活挂在运行时的阻塞池上）"
    );
}

/// 合帧：窗口由判据手拨（交给写那一半的「等一个窗口」等判据拨一下才回，计帧不按墙钟）。
/// 20 个窗口、每个窗口里来 10 段、每段之后都让写那一半有机会跑（写的时候也照样来）⇒ 写出去的 `out` 帧数 ≤ 窗口数 ＋ 1，
/// 拼起来与来的 200 段一字不差。
#[tokio::test]
async fn the_writer_sends_at_most_one_out_frame_per_window() {
    use super::super::wire::{decode, Who, PROTO};
    let mut hub = Hub::new("h", Size { rows: 24, cols: 80 });
    let (id, out) = hub.connect(None, 0);
    hub.frame(
        id,
        Frame::Ctl(Ctl::Hello {
            proto: PROTO,
            build: "t".into(),
            want: Want::View,
            size: None,
            who: Who {
                kind: WhoKind::Terminal,
                client: None,
            },
        }),
        0,
    );
    // 管子窄（16 字节）：写一帧要让出好几回，写的那段时间里也有新的 out 进队（同生产里读伪终端那条线并着放）。
    let (mut near, mut far) = tokio::io::duplex(16);
    let reader = tokio::spawn(async move {
        let mut raw = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut far, &mut raw)
            .await
            .expect("读完");
        raw
    });
    let tick = std::sync::Arc::new(tokio::sync::Notify::new());
    let writer = {
        let (out, tick) = (out.clone(), std::sync::Arc::clone(&tick));
        tokio::spawn(async move {
            let window = || {
                let tick = std::sync::Arc::clone(&tick);
                async move { tick.notified().await }
            };
            write_out(&out, &mut near, window).await;
        })
    };
    let let_it_run = || async {
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
    };
    let mut sent = Vec::new();
    for w in 0..20u32 {
        for i in 0..10u32 {
            let chunk = format!("\x1b[Hframe {w}.{i}\r\n");
            sent.extend_from_slice(chunk.as_bytes());
            hub.pty_out(chunk.as_bytes());
            let_it_run().await;
        }
        tick.notify_one();
        let_it_run().await;
    }
    hub.child_exited(Some(0));
    // 收尾：队关了，写那一半取完就回（排着的若只有 out，它还在等窗口：照拨）；有期限，到了照实红。
    let deadline = std::time::Instant::now() + guard_core::wall::CEILING;
    while !writer.is_finished() {
        assert!(std::time::Instant::now() < deadline, "写那一半没收");
        tick.notify_one();
        let_it_run().await;
    }
    writer.await.expect("写那一半收了");
    let raw = reader.await.expect("读那一半收了");
    let (mut outs, mut got) = (0u64, Vec::new());
    let mut at = 0;
    while let Some((f, used)) = decode(&raw[at..]).expect("解得开") {
        at += used;
        if let Frame::Out(b) = f {
            outs += 1;
            got.extend_from_slice(&b);
        }
    }
    assert_eq!(got, sent, "拼起来一字不差");
    // 20 个窗口各一帧，收尾时拨的那几下最多再一帧。
    assert!(outs <= 21, "20 个窗口写出了 {outs} 帧 out");
}

/// 合帧：写一帧的那段时间里又来的 out，也照等下一个窗口，不接着写出去（窗口由判据手拨）。
#[tokio::test]
async fn output_arriving_while_a_frame_is_being_written_waits_for_the_next_window() {
    use super::super::wire::{decode, Who, PROTO};
    let mut hub = Hub::new("h", Size { rows: 24, cols: 80 });
    let (id, out) = hub.connect(None, 0);
    hub.frame(
        id,
        Frame::Ctl(Ctl::Hello {
            proto: PROTO,
            build: "t".into(),
            want: Want::View,
            size: None,
            who: Who {
                kind: WhoKind::Terminal,
                client: None,
            },
        }),
        0,
    );
    let (mut near, far) = tokio::io::duplex(16);
    let tick = std::sync::Arc::new(tokio::sync::Notify::new());
    let writer = {
        let (out, tick) = (out.clone(), std::sync::Arc::clone(&tick));
        tokio::spawn(async move {
            let window = || {
                let tick = std::sync::Arc::clone(&tick);
                async move { tick.notified().await }
            };
            write_out(&out, &mut near, window).await;
        })
    };
    let let_it_run = || async {
        for _ in 0..64 {
            tokio::task::yield_now().await;
        }
    };
    // 读那一头可以叫停（`paused`）：停着时管子满了，写那一半就卡在写第一帧上。
    let got = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let resume = std::sync::Arc::new(tokio::sync::Notify::new());
    let reader = {
        let (got, paused, resume) = (
            std::sync::Arc::clone(&got),
            std::sync::Arc::clone(&paused),
            std::sync::Arc::clone(&resume),
        );
        let mut far = far;
        tokio::spawn(async move {
            let mut buf = [0u8; 16];
            loop {
                if paused.load(std::sync::atomic::Ordering::Acquire) {
                    resume.notified().await;
                    continue;
                }
                match tokio::io::AsyncReadExt::read(&mut far, &mut buf).await {
                    Ok(0) | Err(_) => return,
                    Ok(n) => got.lock().unwrap().extend_from_slice(&buf[..n]),
                }
            }
        })
    };
    let outs_now = |got: &std::sync::Mutex<Vec<u8>>| {
        let raw = got.lock().unwrap().clone();
        let (mut n, mut at) = (0, 0);
        while let Ok(Some((f, used))) = decode(&raw[at..]) {
            at += used;
            n += usize::from(matches!(f, Frame::Out(_)));
        }
        n
    };
    // 先让 welcome 写完读完（控制帧不等窗口），之后队里只有 out。
    let_it_run().await;
    paused.store(true, std::sync::atomic::Ordering::Release);
    hub.pty_out("A".repeat(200).as_bytes());
    let_it_run().await;
    tick.notify_one();
    let_it_run().await;
    assert_eq!(outs_now(&got), 0, "第一帧卡在管子里");
    // 写着第一帧的时候又来一段，再放开读那一头。
    hub.pty_out(b"B");
    paused.store(false, std::sync::atomic::Ordering::Release);
    resume.notify_one();
    let_it_run().await;
    let_it_run().await;
    assert_eq!(
        outs_now(&got),
        1,
        "写着第一帧时来的 out 不该不等窗口就写出去"
    );
    tick.notify_one();
    let_it_run().await;
    assert_eq!(outs_now(&got), 2, "下一个窗口到了就写");
    hub.child_exited(Some(0));
    let deadline = std::time::Instant::now() + guard_core::wall::CEILING;
    while !writer.is_finished() {
        assert!(std::time::Instant::now() < deadline, "写那一半没收");
        tick.notify_one();
        let_it_run().await;
    }
    writer.await.expect("写那一半收了");
    reader.await.expect("读那一半收了");
}
