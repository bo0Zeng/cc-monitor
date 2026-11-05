//! 本机壳 ↔ 本机常驻后端那条本人通道（`own-chan`），在**哪一个系统上都**通：Linux 是 Unix 套接字，Windows 是命名管道。
//! 壳这一侧用到的每一问都过一遍（独占锁 · 绑 · 同步连 ＋ 期限读 · 交给运行时 · 对端是谁 · 没人在听）。
//! 同一份在 Linux 门禁里跑，也交叉编到 Windows 上真跑（`cargo test --target x86_64-pc-windows-gnu --no-run` 之后在那台跑）。
//!
//! 末尾一条 `#[ignore]` 的真进程判据：monitor 退了，本机常驻后端照跑（要一份真后端，`CCM_E2E_BACKEND=<路径>`；只在 Win11 虚拟机上跑过）。

use std::io::{BufRead as _, Write as _};

/// 第一个拿到，第二个拿不到；第一个放了之后又拿得到（进程没了也一样：放是系统替它放的）。
#[test]
fn the_listen_lock_is_exclusive_and_released_on_drop() {
    let d = test_home::scratch("lock");
    let first = own_chan::try_hold(&d).expect("锁").expect("第一个该拿到");
    assert!(
        own_chan::try_hold(&d).expect("锁").is_none(),
        "第二个也拿到了 —— 两个常驻后端会同时在听"
    );
    drop(first);
    assert!(
        own_chan::try_hold(&d).expect("锁").is_some(),
        "第一个放了之后还拿不到 —— 陈旧锁会让常驻后端永远起不来"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 没人在听 ⇒ 同步连回 `NotFound`（壳据此判「没有常驻那一位」，起一个新的）。
#[test]
fn nobody_listening_reads_as_not_found() {
    let d = test_home::scratch("absent");
    let e = own_chan::connect_blocking(&d.join("listen.sock"))
        .err()
        .expect("没人在听却连上了");
    assert_eq!(e.kind(), std::io::ErrorKind::NotFound, "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

/// 绑上之后：同步连上的是本人（`Ours`），对端 pid 就是在听的这个进程；期限读到点就回、不挂死；
/// 握手那几行同步读写完了交给运行时，两向照通；第二条异步连接也收得下（收一条补一个实例）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bound_channel_serves_the_host_side_end_to_end() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let d = test_home::scratch("serve");
    let path = d.join("listen.sock");
    let l = own_chan::bind(&path).expect("绑");
    let p = path.clone();
    let client = tokio::task::spawn_blocking(move || {
        let b = own_chan::connect_blocking(&p).expect("同步连");
        let pid = b.peer_pid().expect("对端是谁");
        b.set_read_timeout(Some(std::time::Duration::from_millis(300)))
            .expect("期限");
        let mut one = [0u8; 1];
        let timed = std::io::Read::read(&mut &b, &mut one);
        (&b).write_all(b"hello\n").expect("同步写");
        let mut line = String::new();
        std::io::BufReader::new(&b)
            .read_line(&mut line)
            .expect("同步读一行");
        (b, pid, timed.is_err(), line)
    });
    let own_chan::Accepted::Ours(mut server) = l.accept().await.expect("收") else {
        panic!("同一个进程连进来被认成别人");
    };
    let mut got = [0u8; 6];
    server.read_exact(&mut got).await.expect("读握手");
    assert_eq!(&got, b"hello\n");
    server.write_all(b"ok\n").await.expect("回握手");
    let (b, pid, timed_out, line) = client.await.expect("客户端那一侧");
    assert_eq!(pid, std::process::id(), "对端 pid 不是在听的这个进程");
    // 期限到了、对端一个字节没写：那一次读回错（没挂死在那里：挂死的话下面的握手根本走不到）。
    assert!(timed_out, "期限到了没字节，读却没报错");
    assert_eq!(line, "ok\n");
    let mut a = b.into_async().expect("交给运行时");
    a.write_all(b"y").await.expect("异步写");
    let mut one = [0u8; 1];
    server.read_exact(&mut one).await.expect("读");
    assert_eq!(&one, b"y");
    server.write_all(b"z").await.expect("回");
    a.read_exact(&mut one).await.expect("异步读");
    assert_eq!(&one, b"z");
    // 第二条（异步连）：收下一条之后补的那个实例接得住。
    let p = path.clone();
    let second = tokio::spawn(async move {
        let mut c = own_chan::connect(&p).await.expect("第二条");
        c.write_all(b"2").await.expect("写");
    });
    let own_chan::Accepted::Ours(mut s2) = l.accept().await.expect("收第二条") else {
        panic!("第二条被认成别人");
    };
    s2.read_exact(&mut one).await.expect("读第二条");
    assert_eq!(&one, b"2");
    second.await.expect("第二条客户端");
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ monitor 退了，本机常驻后端照跑（R5：本机后端与远端常驻同形）。
///
/// 一个子进程当 monitor：照生产那一条起本机常驻后端（[`super::start_detached`]，脱离着起、连上握手），然后整个进程退出。
/// 父进程（判据本体）随后连这台家里的通道：还连得上、对端还是那个 pid ⇒ 后端没跟着 monitor 走；最后经 `--resident-stop` 停掉它。
/// 沙箱：数据目录换成临时目录（`CCM_DATA_DIR`），不碰真家；不起 claude。
#[test]
#[ignore = "要一份真后端：CCM_E2E_BACKEND=<路径>；在 Win11 虚拟机上跑"]
fn e2e_the_local_backend_outlives_the_monitor() {
    const ROLE: &str = "CCM_E2E_HOST_ROLE";
    let bin =
        std::path::PathBuf::from(std::env::var("CCM_E2E_BACKEND").expect("没给 CCM_E2E_BACKEND"));
    if std::env::var(ROLE).is_ok() {
        // ── 子进程：当 monitor ──
        let resolve = || Ok(bin.clone());
        let out = super::start_detached(&resolve, &bin, &super::backend_env());
        let ok = matches!(
            out,
            super::DetachOutcome::Done(super::StartOutcome::Started(_))
        );
        let why = match &out {
            super::DetachOutcome::Done(super::StartOutcome::Failed { reason, .. }) => {
                reason.logged()
            }
            super::DetachOutcome::NotTaken => "没走脱离那条路".into(),
            _ => String::new(),
        };
        println!("HOST-STARTED {ok} {} {why}", super::is_detached());
        let _ = std::io::stdout().flush();
        std::process::exit(if ok { 0 } else { 3 });
    }
    let home = test_home::scratch("e2e-home");
    let me = std::env::current_exe().expect("本判据二进制");
    // 那个子进程的输出落文件、不接管道：它起的后端会继承它的标准句柄（Windows 起进程时可继承的句柄都带过去），
    // 接管道的话后端活着一天，这一头就等不到管道关。
    let log = home.join("host.out");
    let file = std::fs::File::create(&log).expect("子进程输出那份文件");
    let status = std::process::Command::new(&me)
        .args([
            "--exact",
            "local_backend_host::own_chan_any_os_tests::e2e_the_local_backend_outlives_the_monitor",
            "--ignored",
            "--nocapture",
        ])
        .env(ROLE, "1")
        .env("CCM_DATA_DIR", &home)
        .env("CCM_E2E_BACKEND", &bin)
        .stdin(std::process::Stdio::null())
        .stdout(file.try_clone().expect("复制句柄"))
        .stderr(file)
        .status()
        .expect("起当 monitor 的那个子进程");
    let said = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        status.success()
            && said
                .lines()
                .any(|l| l.trim_end() == "HOST-STARTED true true"),
        "当 monitor 的那个进程没起成本机常驻后端：{said}"
    );
    // monitor 那个进程已经退了：这台家里的通道还连得上 ⇒ 后端还在跑。
    let sock = relay_route_core::listen_socket_for(&home);
    let b = own_chan::connect_blocking(&sock).expect("monitor 退了之后本机后端不在了");
    let pid = b.peer_pid().expect("对端是谁");
    assert_ne!(pid, 0);
    drop(b);
    // 中转住在这个后端进程里：中转口照旧有人接 ⇒ 经中转的会话不断。
    let relay = std::net::SocketAddr::from(([127, 0, 0, 1], relay_route_core::PORT));
    assert!(
        std::net::TcpStream::connect_timeout(&relay, std::time::Duration::from_secs(3)).is_ok(),
        "monitor 退了之后中转口没人接了"
    );
    // 收尾：照生产那一条停（`--resident-stop`，同一个家）。
    let stop = std::process::Command::new(&bin)
        .args([crate::local_backend::BACKEND_SEP, "--resident-stop"])
        .env("CCM_DATA_DIR", &home)
        .envs(super::backend_env())
        .output()
        .expect("起 --resident-stop");
    // 结局照生产那一个读法读（`remote_resident::read_stop`）。
    let answer = crate::remote_resident::read_stop(
        &crate::local_backend_host::local_captured(stop.status.code(), &stop.stdout, &stop.stderr),
        &copy_core::local_machine(),
    )
    .unwrap_or_else(|e| panic!("停那一下没答成：{}", e.said));
    assert_eq!(answer.pid, Some(pid), "停的不是那一位");
    assert!(
        own_chan::connect_blocking(&sock).is_err(),
        "停了之后通道还连得上"
    );
    println!("E2E-OK 本机常驻后端 pid={pid} 活过了 monitor，又照常停掉");
    let _ = std::fs::remove_dir_all(&home);
}
