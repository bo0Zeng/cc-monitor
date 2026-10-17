//! 常驻后端的起 · 找 · 接 · 停（`control/resident.rs`）。
//! 守的要求：「远端常驻、本机远端同形」；`INVARIANTS §48.1`「门由内核给，没有钥匙」；升级那一跳先停旧版再接中转；台架不许占真家。

use super::*;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-resident-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 子进程的环境：常驻开关 ＋ 宿主层那几格（中转口要交给它；`~/` 换成家目录），没有别的。
#[test]
fn the_child_env_is_the_switch_plus_the_hosted_slots() {
    let home = PathBuf::from("/h");
    let env = child_env(
        &home,
        &[
            ("CCM_RELAY_PORT", "8788".into()),
            ("CCM_BACKEND_STDERR_LOG", format!("~/{STDERR_LOG_REL}")),
        ],
    );
    assert_eq!(
        env,
        vec![
            (
                crate::wire::listen::ENV_RESIDENT.to_string(),
                "1".to_string()
            ),
            ("CCM_RELAY_PORT".to_string(), "8788".to_string()),
            (
                "CCM_BACKEND_STDERR_LOG".to_string(),
                home.join(STDERR_LOG_REL).display().to_string()
            ),
        ]
    );
}

/// 停之前核身份：部署换过文件之后旧进程的 exe 带 ` (deleted)` 照样认；别的程序不认。
#[test]
fn the_owner_record_parses_and_the_identity_check_tolerates_a_replaced_file() {
    assert_eq!(
        parse_owner("123\n/h/.cc-monitor/bin/cc-monitor-backend\n"),
        Some((123, PathBuf::from("/h/.cc-monitor/bin/cc-monitor-backend")))
    );
    assert_eq!(parse_owner("0\n/x\n"), None);
    assert_eq!(parse_owner("123\n"), None);
    let rec = Path::new("/h/.cc-monitor/bin/cc-monitor-backend");
    assert!(exe_matches("/h/.cc-monitor/bin/cc-monitor-backend", rec));
    assert!(exe_matches(
        "/h/.cc-monitor/bin/cc-monitor-backend (deleted)",
        rec
    ));
    assert!(!exe_matches("/usr/bin/python3", rec));
}

fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |k: &str| {
        pairs
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| (*v).to_string())
    }
}

/// 〔台架防真家〕真值表：沙箱跑（标记 / HOME 挪了）× 门牌在不在真家（家 · 诊断文件）；查不到账号家目录 ⇒ 不判。
#[test]
fn the_sandbox_refusal_truth_table() {
    let acct = Path::new("/home/u");
    let real_dh = Path::new("/home/u/.cc-monitor");
    let box_dh = Path::new("/box/.cc-monitor");
    let refuses = |pairs: &[(&str, &str)], dh: &Path| {
        sandbox_refusal(&env_of(pairs), dh, Some(acct)).is_some()
    };
    let marked = (relay_route_core::SANDBOX_ENV, "1");
    // 沙箱跑 ＋ 真家 ⇒ 拒（两种「沙箱跑」各一格）。
    assert!(refuses(&[marked, ("HOME", "/home/u")], real_dh));
    assert!(refuses(&[("HOME", "/box")], real_dh));
    // 沙箱跑 ＋ 家在沙箱、诊断文件漏进真家 ⇒ 拒。
    assert!(refuses(
        &[
            ("HOME", "/box"),
            (
                crate::stderr_log::ENV,
                "/home/u/.cc-monitor/logs/backend/stderr.log"
            )
        ],
        box_dh
    ));
    // 沙箱跑 ＋ 全在沙箱 ⇒ 起。
    assert!(!refuses(&[marked, ("HOME", "/box")], box_dh));
    // 不是沙箱跑（没有标记、HOME 就是真家）⇒ 真家照起（那就是真的常驻后端）。
    assert!(!refuses(&[("HOME", "/home/u")], real_dh));
    // 标记不是 `1` ⇒ 不算标记。
    assert!(!refuses(
        &[(relay_route_core::SANDBOX_ENV, "0"), ("HOME", "/home/u")],
        real_dh
    ));
    // 查不到账号家目录 ⇒ 不判。
    assert_eq!(sandbox_refusal(&env_of(&[marked]), real_dh, None), None);
}

/// 中继断开的两种理由：没人在听（还没绑上 / 起来就退了）⇒ `absent`；别的 ⇒ `unreachable`。
#[test]
fn the_relay_says_absent_only_when_nobody_listens() {
    use std::io::ErrorKind as K;
    assert_eq!(
        relay_refusal(K::NotFound),
        crate::wire::listen::REFUSE_ABSENT
    );
    assert_eq!(
        relay_refusal(K::ConnectionRefused),
        crate::wire::listen::REFUSE_ABSENT
    );
    for k in [K::PermissionDenied, K::InvalidInput, K::Other] {
        assert_eq!(
            relay_refusal(k),
            crate::wire::listen::REFUSE_UNREACHABLE,
            "{k:?}"
        );
    }
}

/// 套接字路径太长 ⇒ 不绑、说清（内核会截断成另一个路径）。锁也不留着。
#[tokio::test]
async fn a_socket_path_over_the_cap_is_refused_out_loud() {
    let base = scratch("long");
    let deep = base.join("d".repeat(SOCKET_PATH_MAX));
    std::fs::create_dir_all(&deep).unwrap();
    match claim(&crate::platform::fs::Own::issue_data_at(&deep)) {
        Claim::Failed(why) => assert!(why.contains(&SOCKET_PATH_MAX.to_string()), "{why}"),
        Claim::Listening(..) => panic!("超长路径也绑了"),
        Claim::Held => panic!("没人攥着锁却说有"),
    }
    let _ = std::fs::remove_dir_all(&base);
}

/// ★ 远端常驻后端的诊断文件落 `~/.cc-monitor/logs/backend/stderr.log`（与本机同一层级），
/// 那几层目录由起它的那一步逐层建好（`stderr_log` 只建文件不建目录），每层只给本人。
#[test]
fn the_resident_log_lives_under_logs_backend_and_its_dirs_are_made() {
    assert_eq!(STDERR_LOG_REL, ".cc-monitor/logs/backend/stderr.log");
    let home = std::env::temp_dir().join(format!("ccm-gap1-logdir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    log_dir_chain(&crate::platform::fs::Own::issue_at(&home)).expect("逐层建");
    for rel in [
        ".cc-monitor",
        ".cc-monitor/logs",
        ".cc-monitor/logs/backend",
    ] {
        let d = home.join(rel);
        assert!(d.is_dir(), "{rel} 没建出来");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&d).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{rel} 不是只给本人");
        }
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// T1 **宽限期 > 退出排空上限**：默认值与 `--grace` 的下限两格都比 `wire::drain::DRAIN_DEADLINE` 长。
/// 要求：「常驻后端自己收 SIGTERM 排空在飞写的上限（HX1）必须 < 宽限期（判据钉两者关系）」。
/// 等得比排空短 ⇒ 后端那句「哪几条没做完」永远被 SIGKILL 截断，而且不会报错。
#[test]
fn the_grace_period_outlasts_the_backend_drain_cap() {
    let drain_ms = crate::wire::drain::DRAIN_DEADLINE.as_millis();
    assert!(
        u128::from(STOP_GRACE_MS) > drain_ms,
        "默认宽限期 {STOP_GRACE_MS}ms 不比排空上限 {drain_ms}ms 长"
    );
    let drain_secs = crate::wire::drain::DRAIN_DEADLINE.as_secs();
    let args = |v: &str| vec!["--grace".to_string(), v.to_string()];
    assert_eq!(parse_grace(&[]), Ok(STOP_GRACE_MS));
    assert!(
        parse_grace(&args(&drain_secs.to_string())).is_err(),
        "等于排空上限的宽限期被收下了"
    );
    assert_eq!(
        parse_grace(&args(&(drain_secs + 1).to_string())),
        Ok(u32::try_from((drain_secs + 1) * 1000).unwrap())
    );
    for bad in ["", "x", "-5", "3601"] {
        assert!(parse_grace(&args(bad)).is_err(), "{bad:?}");
    }
    assert!(parse_grace(&["--grace".to_string()]).is_err(), "缺值");
}

/// 起一个 `sh`，`trap` 决定它怎么对 SIGTERM；装好 trap 才往 stdout 写一行（事件，不是睡）。
#[cfg(target_os = "linux")]
fn sh_child(script: &str) -> std::process::Child {
    use std::io::BufRead as _;
    let mut c = std::process::Command::new("sh")
        .arg("-c")
        .arg(script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("起 sh");
    let mut line = String::new();
    std::io::BufReader::new(c.stdout.take().expect("stdout"))
        .read_line(&mut line)
        .expect("读就绪行");
    assert_eq!(line.trim(), "ready");
    c
}

/// 判据红了（panic）也收掉那个 `sh`（纪律 21）。
#[cfg(target_os = "linux")]
struct Reap(std::process::Child);

#[cfg(target_os = "linux")]
impl Drop for Reap {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

/// T2 **同机监督者三态，真进程 ＋ 真信号**：听话的（收 SIGTERM 就退）⇒ `graceful`；聋的（忽略 SIGTERM）⇒ 宽限期满强杀、
/// `killed`、它真没了（收尸后 `/proc` 里没有）；不在的 ⇒ `not_running`；记下的程序对不上 ⇒ 拒、不发信号。
/// 要求：「读 pid → SIGTERM → 在本机按 pidfd 等到退出或宽限期到 → 到点 SIGKILL → 回结局」。
#[test]
#[cfg(target_os = "linux")]
fn the_one_shot_supervisor_stops_politely_then_by_force() {
    let exe = |pid: u32| PathBuf::from(crate::platform::proc::exe_of(pid).expect("exe"));
    // 听话的。
    let mut c = Reap(sh_child(
        "trap 'exit 0' TERM; echo ready; while :; do sleep 0.05; done",
    ));
    let pid = c.0.id();
    assert_eq!(
        stop_pid(pid, &exe(pid), 5_000, 1_000),
        Ok(Stopped::Graceful(pid))
    );
    assert_eq!(
        c.0.wait().expect("wait").code(),
        Some(0),
        "它自己给的退出码"
    );
    // 聋的：宽限期 200ms 满 ⇒ 强杀。
    let mut c = Reap(sh_child(
        "trap '' TERM; echo ready; while :; do sleep 0.05; done",
    ));
    let pid = c.0.id();
    assert_eq!(
        stop_pid(pid, &exe(pid), 200, 2_000),
        Ok(Stopped::Killed(pid))
    );
    use std::os::unix::process::ExitStatusExt as _;
    assert_eq!(
        c.0.wait().expect("wait").signal(),
        Some(9),
        "应当是被强杀的"
    );
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "进程表里还有 {pid}"
    );
    // 已经不在的（刚收过尸的那个 pid）⇒ not_running。
    assert_eq!(
        stop_pid(pid, &exe(std::process::id()), 200, 200),
        Ok(Stopped::NotRunning)
    );
    // 记下的程序对不上 ⇒ 拒，且它照样活着（没发任何信号）。
    let mut c = Reap(sh_child(
        "trap 'exit 0' TERM; echo ready; while :; do sleep 0.05; done",
    ));
    let pid = c.0.id();
    let err = stop_pid(pid, Path::new("/nonexistent/cc-monitor-backend"), 200, 200).unwrap_err();
    assert!(err.said.contains(&pid.to_string()), "{err}");
    assert!(matches!(c.0.try_wait(), Ok(None)), "对不上身份的也被停了");
    // 线上三个词。
    let words: Vec<&str> = [
        Stopped::Graceful(1),
        Stopped::Killed(1),
        Stopped::NotRunning,
    ]
    .iter()
    .map(|s| s.word())
    .collect();
    assert_eq!(words, ["graceful", "killed", "not_running"]);
}

/// 〔升级那一跳〕旧版常驻后端占着中转口（这里用一个占着回环口的进程扮它，`<家>/listen-<口>.pid` 记着它）：
/// `retire_legacy` 停掉它、删掉旧记录与旧钥匙文件 ⇒ 那个口空出来，新的常驻后端接得上中转；没有旧记录 ⇒ 什么都不做。
/// 而 `main.rs` 里先停旧的、再接中转（同一条线程里，顺序钉住）。
#[test]
#[cfg(target_os = "linux")]
fn upgrading_retires_the_old_resident_so_the_relay_port_frees_up() {
    use std::io::BufRead as _;
    let dh = scratch("legacy");
    assert_eq!(
        retire_legacy(&crate::platform::fs::Own::issue_data_at(&dh)),
        None,
        "没有旧记录也动了手"
    );
    let mut old = std::process::Command::new("python3")
        .args([
            "-c",
            "import socket,signal; s=socket.socket(); s.bind(('127.0.0.1',0)); s.listen(); \
             print(s.getsockname()[1], flush=True); signal.pause()",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("起扮旧版的那个进程");
    let mut line = String::new();
    std::io::BufReader::new(old.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port: u16 = line.trim().parse().expect("口号");
    let pid = old.id();
    let exe = crate::platform::proc::exe_of(pid).expect("exe");
    std::fs::write(
        dh.join(format!("listen-{port}.pid")),
        format!("{pid}\n{exe}\n"),
    )
    .unwrap();
    std::fs::write(dh.join(LEGACY_LISTEN_TOKEN_NAME), "old").unwrap();
    assert!(
        std::net::TcpListener::bind(("127.0.0.1", port)).is_err(),
        "前提：旧版占着那个口"
    );
    let said =
        retire_legacy(&crate::platform::fs::Own::issue_data_at(&dh)).expect("有旧记录却说没有");
    assert!(said.contains("graceful"), "{said}");
    let _ = old.wait();
    assert!(
        std::net::TcpListener::bind(("127.0.0.1", port)).is_ok(),
        "旧版停了，口却没空出来 —— 新版接不上中转"
    );
    assert!(legacy_listen_pid_in(&dh).is_none());
    assert!(!dh.join(LEGACY_LISTEN_TOKEN_NAME).exists());
    // 顺序：同一条线程里先 `retire_legacy`，后 `host_relay`。
    let main = guard_core::production_code(include_str!("../../../src/backend/main.rs"));
    let at = main
        .find("control::resident::retire_legacy")
        .expect("main.rs 里没有升级那一跳");
    let relay_after = main[at..]
        .find("host_relay(")
        .expect("停了旧的之后没接中转");
    assert!(
        !main[at..at + relay_after].contains("thread::spawn"),
        "停旧版与接中转不在同一条线程里 —— 顺序就钉不住了"
    );
    let _ = std::fs::remove_dir_all(&dh);
}

/// 扮一个在听的常驻后端：另一个进程绑 `<家>/run/backend.sock`，每条连接先给一行 hello（带 `build`）再关；收 SIGTERM 就退。
#[cfg(target_os = "linux")]
fn fake_resident(dh: &Path, build: &str) -> Reap {
    use std::io::BufRead as _;
    let run = relay_route_core::listen_dir_for(dh);
    std::fs::create_dir_all(&run).unwrap();
    std::fs::set_permissions(&run, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    let mut c = std::process::Command::new("python3")
        .args([
            "-c",
            "import socket,sys,signal,json\n\
             signal.signal(signal.SIGTERM, lambda *a: sys.exit(0))\n\
             s=socket.socket(socket.AF_UNIX); s.bind(sys.argv[1]); s.listen()\n\
             print('ready', flush=True)\n\
             while True:\n    c,_=s.accept(); c.sendall((json.dumps({'type':'hello','build_id':sys.argv[2]})+'\\n').encode()); c.close()\n",
        ])
        .arg(relay_route_core::listen_socket_for(dh))
        .arg(build)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("起扮常驻的那个进程");
    let mut line = String::new();
    std::io::BufReader::new(c.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line.trim(), "ready");
    Reap(c)
}

/// 停按套接字认人：停的是**此刻真在听**的那一位（内核报的对端 pid），不是盘上记录指着的那个 ——
/// 陈旧的「谁在听」记录指着一个 pid 被复用、程序路径也对得上的进程，它照样活着。
#[test]
#[cfg(target_os = "linux")]
fn stopping_asks_the_socket_not_a_recorded_pid() {
    let dh = scratch("peerstop");
    let mut listening = fake_resident(&dh, "p9a-old");
    let decoy = Reap(sh_child(
        "trap 'exit 0' TERM; echo ready; while :; do sleep 0.05; done",
    ));
    let dpid = decoy.0.id();
    let exe = crate::platform::proc::exe_of(dpid).expect("exe");
    std::fs::write(
        relay_route_core::listen_dir_for(&dh).join("backend.pid"),
        format!("{dpid}\n{exe}\n"),
    )
    .unwrap();
    let own = crate::platform::fs::Own::issue_data_at(&dh);
    let lpid = listening.0.id();
    assert_eq!(stop_owner(&own, 5_000), Ok(Stopped::Graceful(lpid)));
    let mut decoy = decoy;
    assert!(
        matches!(decoy.0.try_wait(), Ok(None)),
        "停到了记录指着的那个进程"
    );
    let _ = listening.0.wait();
    // 没人在听 ⇒ 没东西要停。
    assert_eq!(stop_owner(&own, 200), Ok(Stopped::NotRunning));
    let _ = std::fs::remove_dir_all(&dh);
}

/// 「只升不降」在执行换的那一处判：在听的那一位不比这一版旧（同版 · 更新）⇒ 不停它、照实说没换；比这一版旧 ⇒ 停掉。
#[test]
#[cfg(target_os = "linux")]
fn replacing_never_downgrades_the_one_that_listens() {
    let mine = "p9b-mine";
    for (theirs, kept) in [
        ("p9c-newer", true),
        ("p9b-mine", true),
        ("p9a-older", false),
    ] {
        let dh = scratch("swap");
        let mut listening = fake_resident(&dh, theirs);
        let own = crate::platform::fs::Own::issue_data_at(&dh);
        let got = swap_out(&own, mine, 5_000).expect("换");
        let lpid = listening.0.id();
        if kept {
            assert_eq!(got, Swap::Kept(lpid), "{theirs}：不该停");
            assert!(
                matches!(listening.0.try_wait(), Ok(None)),
                "{theirs}：被停了"
            );
        } else {
            assert_eq!(got, Swap::Cleared(Stopped::Graceful(lpid)), "{theirs}");
            let _ = listening.0.wait();
        }
        drop(listening);
        let _ = std::fs::remove_dir_all(&dh);
    }
}

/// 旧版常驻后端的进程记录照名字的形状找：`listen-<口>.pid` 才算，别的同前缀文件（钥匙 · 不是口号的）不算。
#[test]
fn legacy_listen_record_is_found_by_its_name_shape() {
    let dh = scratch("legacy-shape");
    std::fs::write(dh.join(LEGACY_LISTEN_TOKEN_NAME), "old").unwrap();
    std::fs::write(dh.join("listen-x.pid"), "").unwrap();
    assert_eq!(legacy_listen_pid_in(&dh), None);
    std::fs::write(dh.join("listen-56233.pid"), "").unwrap();
    assert_eq!(legacy_listen_pid_in(&dh), Some(dh.join("listen-56233.pid")));
}

/// 扮常驻的那一侧（同进程，绑 `<家>/run/backend.sock`）：收一条连接，先给 hello，读 attach 行、回「接上了」，
/// 读一条请求交给 `answer`（它交回要写回去的几行）。交回读到的 attach 行与请求。
#[cfg(unix)]
async fn one_resident(
    dh: &Path,
    answer: impl FnOnce(&serde_json::Value) -> Vec<String> + Send + 'static,
) -> tokio::task::JoinHandle<(serde_json::Value, serde_json::Value)> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let run = relay_route_core::listen_dir_for(dh);
    std::fs::create_dir_all(&run).unwrap();
    std::fs::set_permissions(&run, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    let l = own_chan::bind(&relay_route_core::listen_socket_for(dh)).expect("绑");
    tokio::spawn(async move {
        let own_chan::Accepted::Ours(s) = l.accept().await.expect("收") else {
            panic!("不是本人")
        };
        let (rd, mut wr) = tokio::io::split(s);
        let mut rd = tokio::io::BufReader::new(rd).lines();
        wr.write_all(b"{\"kind\":\"hello\",\"build_id\":\"x\"}\n")
            .await
            .unwrap();
        let attach: serde_json::Value =
            serde_json::from_str(&rd.next_line().await.unwrap().unwrap()).unwrap();
        wr.write_all(b"{\"attach\":\"ok\"}\n").await.unwrap();
        let req: serde_json::Value =
            serde_json::from_str(&rd.next_line().await.unwrap().unwrap()).unwrap();
        for line in answer(&req) {
            wr.write_all(format!("{line}\n").as_bytes()).await.unwrap();
        }
        // 留着连接不关（期限那一条要「一直不回」）。
        let _ = rd.next_line().await;
        (attach, req)
    })
}

fn asked(cmd: &str) -> crate::wire::listen::ResidentCall {
    crate::wire::listen::ResidentCall {
        cmd: cmd.into(),
        args: serde_json::json!({"held": "h1"}),
    }
}

/// ★ `--resident-call` 问的是这台的常驻：握手时在看名单交空的（只问不看）、请求带着命令 · 入参 · 期限 · 时区，
/// 别的帧（推送 · 别人的应答）略过，只认自己那一条；答成 ⇒ 回包原样。
#[tokio::test]
#[cfg(unix)]
async fn a_resident_call_asks_the_one_that_listens() {
    let dh = scratch("call-ok");
    let peer = one_resident(&dh, |req| {
        vec![
            r#"{"kind":"changed","topic":"machines"}"#.into(),
            r#"{"kind":"reply","id":"someone-else","ok":true,"data":{"no":1}}"#.into(),
            format!(
                r#"{{"kind":"reply","id":{},"ok":true,"data":{{"got":1}}}}"#,
                req["id"]
            ),
        ]
    })
    .await;
    let tz = crate::Tz::of(&serde_json::json!("Asia/Shanghai"));
    let got = call_at(
        &dh,
        &asked("history-listing"),
        Some(9_000),
        &tz,
        Deadline::secs(5),
    )
    .await
    .expect("答成");
    assert_eq!(got, serde_json::json!({"got": 1}));
    peer.abort();
    let _ = std::fs::remove_dir_all(&dh);
}

/// ★ 请求那一行带着发起方的期限与看的那一台的时区（那台按它写「几点」、按它收紧自己的期限）；没有回包的命令 ⇒ `{}`（同 CLI 面）。
#[tokio::test]
#[cfg(unix)]
async fn a_resident_call_carries_the_askers_deadline_and_zone() {
    let dh = scratch("call-shape");
    let peer = one_resident(&dh, |req| {
        vec![format!(
            r#"{{"kind":"reply","id":{},"ok":true}}"#,
            req["id"]
        )]
    })
    .await;
    let tz = crate::Tz::of(&serde_json::json!("Asia/Shanghai"));
    let got = call_at(
        &dh,
        &asked("history-listing"),
        Some(9_000),
        &tz,
        Deadline::secs(5),
    )
    .await
    .expect("答成");
    assert_eq!(got, serde_json::json!({}));
    let (_, req) = peer.await.expect("扮常驻的那一侧");
    assert_eq!(req["within_ms"], 9_000, "{req}");
    assert_eq!(req["tz"], "Asia/Shanghai", "{req}");
    let _ = std::fs::remove_dir_all(&dh);
}

/// ★ 常驻答了失败 ⇒ 码 · 那一句 · 复制详情原样交回（不压成一个码）；没人在听 ⇒ `resident_absent`（不起冷进程顶上）；
/// 到期限常驻还没回 ⇒ `timed_out`。
#[tokio::test]
#[cfg(unix)]
async fn a_resident_call_says_what_the_resident_said_or_that_nobody_listens() {
    let tz = crate::Tz::default();
    let dh = scratch("call-fail");
    let peer = one_resident(&dh, |req| {
        assert_eq!(req["cmd"], "history-listing");
        assert_eq!(req["args"], serde_json::json!({"held": "h1"}));
        assert_eq!(req["within_ms"], 9_000);
        vec![format!(
            r#"{{"kind":"reply","id":{},"ok":false,"code":"bad_args","message":"那一句","detail":"detail-1"}}"#,
            req["id"]
        )]
    })
    .await;
    let f = call_at(
        &dh,
        &asked("history-listing"),
        Some(9_000),
        &tz,
        Deadline::secs(5),
    )
    .await
    .unwrap_err();
    assert_eq!(
        (f.code.as_str(), f.message.as_str(), f.detail.as_str()),
        ("bad_args", "那一句", "detail-1")
    );
    let (attach, _) = peer.await.expect("扮常驻的那一侧");
    assert_eq!(attach["attach"], true);
    assert_eq!(
        attach["watch"]["sids"],
        serde_json::json!([]),
        "只问不看：在看名单得是空的，不然那台照全发：{attach}"
    );
    let _ = std::fs::remove_dir_all(&dh);

    let nobody = scratch("call-absent");
    let f = call_at(
        &nobody,
        &asked("history-listing"),
        None,
        &tz,
        Deadline::secs(5),
    )
    .await
    .unwrap_err();
    assert_eq!(f.code, crate::wire::listen::CALL_ABSENT, "{}", f.message);
    assert!(
        !relay_route_core::listen_socket_for(&nobody).exists(),
        "没人在听却起了一个"
    );
    let _ = std::fs::remove_dir_all(&nobody);

    let mute = scratch("call-mute");
    let peer = one_resident(&mute, |_| Vec::new()).await;
    let f = call_at(
        &mute,
        &asked("history-listing"),
        None,
        &tz,
        Deadline::millis(300),
    )
    .await
    .unwrap_err();
    assert_eq!(f.code, crate::wire::listen::CALL_TIMED_OUT, "{}", f.message);
    peer.abort();
    let _ = std::fs::remove_dir_all(&mute);
}

/// ★ 等多久只归发起方：`--within-ms` 不给 · 给的不是正整数 ⇒ `bad_args`（说缺的是哪一格），后端不自己定一个；给了 ⇒ 照它等、原样往常驻带。
#[test]
fn a_resident_call_waits_only_as_long_as_the_asker_said() {
    let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    for bad in [
        args(&["--stdin-line"]),
        args(&["--stdin-line", "--within-ms", "0"]),
        args(&["--within-ms", "x"]),
    ] {
        let e = deadline_of(&bad).unwrap_err();
        assert_eq!(e.0, "bad_args", "{bad:?}");
        assert!(e.1.contains("--within-ms"), "没说缺的是哪一格：{e:?}");
    }
    let (ms, total) = deadline_of(&args(&["--stdin-line", "--within-ms", "45000"])).unwrap();
    assert_eq!(ms, 45_000);
    assert_eq!(total, Deadline::millis(45_000), "没照发起方给的等");
}
