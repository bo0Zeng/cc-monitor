//! 「此刻是哪个终端在显示这个会话」：私有 tmux socket ＋ 真 pty 客户端（`script`）造现场，期望值手写。
//!
//! 不碰用户的 tmux：socket 一律是本趟临时目录里的 `-S` 路径；被问的进程的 `TMUX` 也指向它。

use super::*;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-st-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 等到 `f` 成立（最多 `secs` 秒）；夹具用，生产段零处。
fn wait_until(secs: u64, mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    f()
}

/// `pid` 已经 exec 成 `comm` 并且新映像的环境装好了。只看 `comm` 不够：exec 时内核先换进程名、后记下新映像的
/// 环境区 —— 中间那一拍读 `/proc/<pid>/environ` 回 0 字节（`proc_env_var` 照实答「这一刻读不到」）。
/// 机器忙时判据正好落进这一拍 ⇒ 间歇红（紧跟着 `comm` 变了就读，本机 3000 趟里 2985 趟读到 0 字节）。
fn exec_settled(pid: u32, comm: &str) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|c| c.trim() == comm)
        && std::fs::read(format!("/proc/{pid}/environ")).is_ok_and(|e| !e.is_empty())
}

fn tmux(sock: &std::path::Path, args: &[&str]) -> String {
    let out = Command::new("tmux")
        .arg("-S")
        .arg(sock)
        .args(args)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .output()
        .expect("起不来 tmux —— 本组要它");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// 在 pty 里起一个 tmux 客户端连上 `a` 会话，环境里带着给定的 `SSH_CONNECTION`（与窗口标签，给了的话）。
fn attach_client(sock: &std::path::Path, ssh: &str, window: Option<&str>) -> Child {
    let mut c = Command::new("script");
    c.args([
        "-qfc",
        &format!("exec tmux -S '{}' attach -t a", sock.display()),
        "/dev/null",
    ])
    .env_remove("TMUX")
    .env_remove("TMUX_PANE")
    // CI 的环境里没有 TERM，tmux 会拒绝接上（「terminal does not support clear」）。
    .env("TERM", "xterm-256color")
    .env("SSH_CONNECTION", ssh)
    .env_remove("LC_CCM_WINDOW");
    if let Some(w) = window {
        c.env("LC_CCM_WINDOW", w);
    }
    c.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    c.spawn().expect("起不来 script —— 本组要它造 pty")
}

fn clients(sock: &std::path::Path) -> usize {
    tmux(sock, &["list-clients", "-F", "#{client_pid}"])
        .lines()
        .filter(|l| !l.is_empty())
        .count()
}

/// ★ 在 tmux 里：问的是**此刻连着**那个会话的客户端，读它们的环境；没人连 ⇒ `detached`；两个 ⇒ 最近动静在前。
#[test]
fn inside_tmux_the_answer_is_the_clients_attached_right_now() {
    let dir = tmpdir("tmux");
    let sock = dir.join("s");
    let out = Command::new("tmux")
        .arg("-S")
        .arg(&sock)
        // 程序与参数分开给：tmux 直接 exec，中间不隔一层 shell（下面等 exec 做完要认得出它的名字）。
        .args(["new-session", "-d", "-s", "a", "sleep", "120"])
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .output()
        .expect("起不来 tmux");
    assert!(
        out.status.success(),
        "建会话失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let pane_pid: u32 = tmux(&sock, &["display-message", "-p", "-t", "a", "#{pane_pid}"])
        .parse()
        .expect("读不到 pane_pid");
    // 那个「claude」（pane 里的进程）自己的环境里有 TMUX / TMUX_PANE —— tmux 注的。
    //   先等 exec 成 `sleep` 做完（`exec_settled`）：exec 之前读到的环境是 fork 出来那一份（也有 TMUX），只等 TMUX 出现
    //   会在 exec 那一拍之前就放行，下面那一问落进 exec 当中 ⇒ 判「取不到」（负载下并发跑红过：Unreadable ≠ Detached）。
    assert!(wait_until(5, || exec_settled(pane_pid, "sleep")
        && matches!(
            crate::platform::proc::proc_env_var(pane_pid, "TMUX"),
            crate::platform::proc::EnvRead::Value(_)
        )));

    assert_eq!(
        shown_by(pane_pid).unwrap(),
        Shown::Detached,
        "没有终端连着却报出了终端"
    );

    let mut a = attach_client(&sock, "192.0.2.5 62414 192.0.2.9 22", None);
    assert!(wait_until(10, || clients(&sock) == 1), "客户端 A 没连上");
    // tmux 的动静是秒级的：隔开一秒多，B 才是「更近」的那个。
    std::thread::sleep(Duration::from_millis(1300));
    let mut b = attach_client(
        &sock,
        "fd00::7 50000 fd00::1 2222",
        Some("4242-133000000000000000"),
    );
    assert!(wait_until(10, || clients(&sock) == 2), "客户端 B 没连上");

    let got = shown_by(pane_pid).unwrap();
    let Shown::By(ts) = got else {
        panic!("两个终端连着却答成 {got:?}");
    };
    let view: Vec<Option<SshConnection>> = ts.iter().map(|t| t.ssh.clone()).collect();
    assert_eq!(
        view,
        vec![
            Some(SshConnection {
                client_addr: "fd00::7".into(),
                client_port: 50000,
                server_addr: "fd00::1".into(),
                server_port: 2222,
            }),
            Some(SshConnection {
                client_addr: "192.0.2.5".into(),
                client_port: 62414,
                server_addr: "192.0.2.9".into(),
                server_port: 22,
            }),
        ],
        "顺序不是「最近动静在前」，或读到的不是那两个客户端自己的环境"
    );
    assert!(ts.iter().all(|t| t.activity.is_some()));
    // 窗口标签读的也是各客户端自己的环境：B 送了、A 没送。
    let windows: Vec<Option<String>> = ts.iter().map(|t| t.window.clone()).collect();
    assert_eq!(windows, vec![Some("4242-133000000000000000".into()), None]);

    // 断开 ⇒ 又是「没有终端连着」。
    tmux(&sock, &["detach-client", "-s", "a"]);
    assert!(wait_until(10, || clients(&sock) == 0), "客户端没断开");
    assert_eq!(shown_by(pane_pid).unwrap(), Shown::Detached);

    tmux(&sock, &["kill-server"]);
    for c in [&mut a, &mut b] {
        let _ = c.kill();
        let _ = c.wait();
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// ★ 不在 tmux 里：读那个进程自己的环境；它有控制终端（pty 里起的）⇒ 那一个终端。没有控制终端 ⇒ `no-terminal`。
#[test]
fn outside_tmux_the_process_own_environment_is_the_answer() {
    let dir = tmpdir("direct");
    let pidfile = dir.join("pid");
    let mut kid = Command::new("script")
        .args([
            "-qfc",
            &format!("echo $$ > '{}'; exec sleep 120", pidfile.display()),
            "/dev/null",
        ])
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .env("SSH_CONNECTION", "203.0.113.20 51111 203.0.113.2 22")
        .env("LC_CCM_WINDOW", "4242-133000000000000000")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("起不来 script");
    let pid = {
        assert!(wait_until(10, || std::fs::read_to_string(&pidfile)
            .is_ok_and(|s| s.trim().parse::<u32>().is_ok())));
        std::fs::read_to_string(&pidfile)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap()
    };
    // 等它 exec 成 sleep、新映像的环境装好（环境定型）。
    assert!(wait_until(10, || exec_settled(pid, "sleep")));
    assert_eq!(
        shown_by(pid).unwrap(),
        Shown::By(vec![Terminal {
            ssh: Some(SshConnection {
                client_addr: "203.0.113.20".into(),
                client_port: 51111,
                server_addr: "203.0.113.2".into(),
                server_port: 22,
            }),
            activity: None,
            window: Some("4242-133000000000000000".into()),
        }])
    );
    let _ = kid.kill();
    let _ = kid.wait();
    let _ = Command::new("kill").arg(pid.to_string()).status();

    // 没有控制终端（新会话首领，`setsid`）⇒ `no-terminal`。
    let mut bare = Command::new("setsid")
        .args(["sleep", "120"])
        .env_remove("TMUX")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("起不来 setsid");
    let bpid = bare.id();
    assert!(wait_until(10, || exec_settled(bpid, "sleep")));
    assert_eq!(shown_by(bpid).unwrap(), Shown::NoTerminal);
    let _ = bare.kill();
    let _ = bare.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

/// 四段的认法：两个地址必须是 IP（带作用域的 IPv6 去掉作用域）、两个口必须是端口号；不合 ⇒ 当不是经 ssh 连的。
#[test]
fn ssh_connection_is_four_fields_or_nothing() {
    assert_eq!(
        parse_ssh_connection("fe80::1%eth0 40000 fe80::2 22"),
        Some(SshConnection {
            client_addr: "fe80::1".into(),
            client_port: 40000,
            server_addr: "fe80::2".into(),
            server_port: 22,
        })
    );
    for bad in [
        "",
        "192.0.2.1 1 192.0.2.2",
        "host 1 192.0.2.2 22",
        "192.0.2.1 70000 192.0.2.2 22",
        "192.0.2.1 1 192.0.2.2 22 x",
    ] {
        assert_eq!(parse_ssh_connection(bad), None, "{bad:?} 不该认");
    }
    // 窗口标签：`<进程号>-<起始时刻>` 才回显，别的形状一律不带。
    assert_eq!(
        window_label(" 4242-133000000000000000 "),
        Some("4242-133000000000000000".into())
    );
    for bad in [
        "",
        "4242",
        "-1",
        "4242-",
        "a-1",
        "1-2-3",
        "12345678901-1",
        "1-123456789012345678901",
    ] {
        assert_eq!(window_label(bad), None, "{bad:?} 不该认");
    }
    assert_eq!(
        parse_clients("12 1790918865\nbad\n13 x\n14 7\n"),
        vec![(12, 1790918865), (14, 7)]
    );
}

/// 找不到那个会话 ⇒ `no_such_session`；sid 形状不对 ⇒ `bad_args`（不往下读）。
#[test]
fn an_unknown_or_malformed_sid_is_said_not_guessed() {
    let home = tmpdir("home");
    assert_eq!(
        answer_at(&home, &serde_json::json!({ "sid": "s-nope" }))
            .unwrap_err()
            .0,
        "no_such_session"
    );
    assert_eq!(
        answer_at(&home, &serde_json::json!({ "sid": "-x" }))
            .unwrap_err()
            .0,
        "bad_args"
    );
    assert_eq!(
        serde_json::to_value(product(&Shown::Detached)).expect("json"),
        serde_json::json!({ "terminals": [], "why": "detached" })
    );
    let _ = std::fs::remove_dir_all(&home);
}

impl crate::guard_support::Shaped for Showing {
    fn samples() -> Vec<Self> {
        vec![Showing {
            terminals: vec![Terminal {
                ssh: Some(SshConnection {
                    client_addr: "203.0.113.2".into(),
                    client_port: 50000,
                    server_addr: "203.0.113.1".into(),
                    server_port: 22,
                }),
                activity: Some(1),
                window: Some("1-2".into()),
            }],
            why: Some("detached"),
        }]
    }
}
