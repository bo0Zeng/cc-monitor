//! 「此刻是哪个终端在显示这个会话」只算那个窗格前台的那一个进程（判法住 `host::tmux::seat`）：
//! 带着同一个 `TMUX` · `TMUX_PANE`、却不是那个窗格前台的进程，不被算成那个窗格（连着它的 tmux 客户端）在显示。
//!
//! 私有 `-S` socket ＋ 真 pty 客户端（`script`）造现场，期望值手写；不碰用户的 tmux、不起真 claude。

use super::tests::{exec_settled, tmux, wait_until};
use super::*;
use std::process::{Child, Command, Stdio};

/// 等窗格里的壳把一个 pid 写进 `file`，读回它。
fn pid_in(file: &std::path::Path) -> u32 {
    assert!(
        wait_until(|| std::fs::read_to_string(file).is_ok_and(|s| s.trim().parse::<u32>().is_ok())),
        "窗格里那个进程的 pid 没写出来"
    );
    std::fs::read_to_string(file)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

/// `shown_by` 的答案里有没有那个客户端（认它环境里的 `SSH_CONNECTION`）。
fn names_the_client(shown: &Shown) -> bool {
    matches!(shown, Shown::By { terminals, .. } if terminals.iter().any(|t| t.ssh.as_ref().is_some_and(|s| s.client_port == 62414)))
}

/// ★ 带着同一个 `TMUX_PANE` 的 ① 没控制终端的进程（主会话里起的 `claude -p` 那一形）② 另一个窗格前台、环境改写成这个窗格的进程
/// ③ 这个窗格 tty 上的后台进程组 ④ 没控制终端、环境里有 `TMUX` 却没 `TMUX_PANE` 的进程 —— 都不被算成那个窗格在显示：`shown_by` 不报连着那个会话的客户端，本机那一问链从它自己起、不说在 tmux 里。
/// 正控：窗格前台的那一个照旧答那个客户端。
#[test]
fn only_the_panes_foreground_process_is_shown_by_its_clients() {
    let dir = test_home::scratch("seat");
    let sock = dir.join("s");
    // 窗格里只跑不读输入的程序：客户端（`script`）的 stdin 是 /dev/null，读到头往终端送 EOF，读输入的窗格进程会跟着退、会话就没了。
    let out = Command::new("tmux")
        .arg("-S")
        .arg(&sock)
        .args([
            "-f",
            "/dev/null",
            "new-session",
            "-d",
            "-s",
            "a",
            "sleep",
            "600",
        ])
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .env_remove("CLAUDE_CONFIG_DIR")
        .output()
        .expect("起不来 tmux");
    assert!(
        out.status.success(),
        "建会话失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let head = tmux(
        &sock,
        &["display-message", "-p", "-t", "a", "#{pane_id} #{pane_pid}"],
    );
    let (pane, main) = head.split_once(' ').expect("窗格 id 与 pid");
    let main: u32 = main.parse().expect("pane_pid");
    assert!(
        wait_until(|| exec_settled(main, "sleep")),
        "窗格进程没换成 sleep"
    );
    let crate::platform::proc::EnvRead::Value(tmux_env) =
        crate::platform::proc::proc_env_var(main, "TMUX")
    else {
        panic!("窗格进程的环境里没有 TMUX");
    };

    let mut client = Command::new("script")
        .args([
            "-qfc",
            &format!("exec tmux -S '{}' attach -t a", sock.display()),
            "/dev/null",
        ])
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .env("TERM", "xterm-256color")
        .env("SSH_CONNECTION", "192.0.2.5 62414 192.0.2.9 22")
        .env_remove("LC_CCM_WINDOW")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("起不来 script —— 本组要它造 pty");
    assert!(
        wait_until(|| !tmux(&sock, &["list-clients", "-F", "#{client_pid}"]).is_empty()),
        "客户端没连上"
    );

    // 正控：窗格前台的那一个 ⇒ 那个客户端。
    let shown = shown_by(main).unwrap();
    assert!(names_the_client(&shown), "窗格前台的进程：{shown:?}");

    // ① 没控制终端，环境里却是这个窗格的 `TMUX` · `TMUX_PANE`。
    let mut loose: Child = Command::new("setsid")
        .args(["sleep", "600"])
        .env("TMUX", &tmux_env)
        .env("TMUX_PANE", pane)
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("SSH_CONNECTION")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("起不来 setsid sleep");
    let lpid = loose.id();
    assert!(
        wait_until(|| exec_settled(lpid, "sleep")),
        "setsid sleep 没起来"
    );
    let detached = shown_by(lpid).unwrap();
    let detached_here = here_by(lpid).unwrap();

    // ④ 没控制终端，环境里有这台的 `TMUX`、没有 `TMUX_PANE`（窗格那一格说不出，终端那一样事实照样说得出）。
    let mut paneless: Child = Command::new("setsid")
        .args(["sleep", "600"])
        .env("TMUX", &tmux_env)
        .env_remove("TMUX_PANE")
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("SSH_CONNECTION")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("起不来 setsid sleep");
    let npid = paneless.id();
    assert!(
        wait_until(|| exec_settled(npid, "sleep")),
        "没 TMUX_PANE 的 setsid sleep 没起来"
    );
    let paneless_shown = shown_by(npid).unwrap();
    let paneless_here = here_by(npid).unwrap();
    let _ = paneless.kill();
    let _ = paneless.wait();

    // ② 另一个窗格的前台进程组里，环境改写成这个窗格（那个窗格里的壳起的子进程）。
    let other_file = dir.join("other-pid");
    tmux(
        &sock,
        &[
            "new-window",
            "-d",
            "-t",
            "a",
            &format!(
                "TMUX_PANE={pane} sleep 600 & echo $! > '{}'; wait",
                other_file.display()
            ),
        ],
    );
    let other = pid_in(&other_file);
    assert!(
        wait_until(|| exec_settled(other, "sleep")),
        "另一个窗格里的 sleep 没起来"
    );
    let elsewhere = shown_by(other).unwrap();
    let elsewhere_here = here_by(other).unwrap();

    // ③ 窗格 tty 上的后台进程组（那个窗格里的壳开了作业控制，`sleep` 在后台、前台是另一个 `sleep`；环境就是那个窗格自己的）。
    let bg_file = dir.join("bg-pid");
    tmux(
        &sock,
        &[
            "new-window",
            "-d",
            "-t",
            "a",
            &format!(
                "set -m; sleep 600 & echo $! > '{}'; exec sleep 601",
                bg_file.display()
            ),
        ],
    );
    let bg = pid_in(&bg_file);
    assert!(
        wait_until(|| exec_settled(bg, "sleep")),
        "后台 sleep 没起来"
    );
    let background = shown_by(bg).unwrap();
    let background_here = here_by(bg).unwrap();

    tmux(&sock, &["kill-server"]);
    let _ = loose.kill();
    let _ = loose.wait();
    let _ = client.kill();
    let _ = client.wait();
    for p in [other, bg] {
        let _ = Command::new("kill").arg(p.to_string()).status();
    }
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(detached, Shown::NoTerminal, "① 没控制终端的进程");
    assert_eq!(
        paneless_shown,
        Shown::NoTerminal,
        "④ 没控制终端、没 TMUX_PANE 的进程"
    );
    assert_eq!(elsewhere, Shown::Unreadable, "② 别的窗格前台的进程");
    assert_eq!(background, Shown::Unreadable, "③ 窗格 tty 上的后台进程组");
    for (what, pid, here) in [
        ("①", lpid, detached_here),
        ("②", other, elsewhere_here),
        ("③", bg, background_here),
        ("④", npid, paneless_here),
    ] {
        let Here::By { seen, tmux } = here else {
            panic!("{what} 本机那一问答成没有终端连着");
        };
        assert_eq!(
            seen.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
            vec![pid],
            "{what} 本机那一问的链不是从它自己起"
        );
        assert!(!tmux, "{what} 本机那一问说它在 tmux 里");
    }
}

/// 判据起的那几个 `sleep`：怎么收场都按 pid 停掉。
#[cfg(target_os = "linux")]
struct Reap(Vec<u32>);

#[cfg(target_os = "linux")]
impl Drop for Reap {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = Command::new("kill").arg(p.to_string()).status();
        }
    }
}

/// ★ 托管终端里只有前台那一个由附着在它上面的真终端显示（判法同 tmux 窗格那一处，`seat_with`；托管那一形认首领是事实里的子进程号）：
/// 同一个托管终端里起的 ① 没控制终端的进程（主会话 Bash 里起的 `claude -p` 那一形）② 伪终端上的后台进程组，以及 ③ 别的伪终端前台的那一个
/// —— 环境里都带着同一个 `CCM_HOST`，却不报附着端：① 没控制终端 ②③ 说不清；本机那一问链从它自己起。正控：前台那一个（托管终端起的子进程）照报附着着的那一扇。
/// 真起托管终端（假子进程 `sh`，沙箱数据目录），附着那一扇是本判据自己连上去的真终端那一端（内核报的对端进程号就是本进程）。
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_the_hosted_terminals_foreground_process_is_shown_by_its_ends() {
    use crate::host::managed::run::test_support::{
        hello, read_until_out, sandbox_own, spec, start,
    };
    use crate::host::managed::wire::{write_frame, Frame, Want};
    use crate::host::managed::{home, terminal_ends_at};
    // 整条等在兜底上限之内：哪一步等不到照实红，不挂住整片。
    let run = async {
        let (own, data) = sandbox_own("obs-fg");
        let kid_file = data.join("kid-pid");
        let bg_file = data.join("bg-pid");
        let script = format!(
            "setsid sleep 600 </dev/null >/dev/null 2>&1 & echo $! > '{}'; set -m; sleep 600 & echo $! > '{}'; printf 'ready\\r\\n'; read x",
            kid_file.display(),
            bg_file.display()
        );
        let (sock, task) = start(&own, spec("t-obs", &script)).await;
        // 附着着的那一扇：一直连着到问完。
        let mut end = hello(&sock, Want::View).await;
        read_until_out(&mut end, "ready").await;
        let main = home::read_facts(&data.join("hosts").join("t-obs"))
            .and_then(|f| f.child_pid)
            .expect("子进程 pid");
        let (kid, bg) = (pid_in(&kid_file), pid_in(&bg_file));
        // ③ 另一个伪终端前台的那一个（`script` 开的，它就是那个终端的会话首领、不是托管终端起的子进程），环境里带着同一个 `CCM_HOST`。
        let other_file = data.join("other-pid");
        let mut other_term: Child = Command::new("script")
            .args([
                "-qfc",
                &format!("echo $$ > '{}'; exec sleep 600", other_file.display()),
                "/dev/null",
            ])
            .env(crate::host::managed::run::HOST_ENV, "t-obs")
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("起不来 script —— 本组要它造 pty");
        let other = pid_in(&other_file);
        // 判据怎么收场（断言红了也算）都按 pid 停掉起的那几个 `sleep`，不留到 600 秒。
        let reap = Reap(vec![kid, bg, other]);
        let me = std::process::id();

        // 正控：前台那一个 ⇒ 附着着的那一扇。
        let front = shown_in(terminal_ends_at(&own, main), main).unwrap();
        let front_here = here_in(terminal_ends_at(&own, main), main).unwrap();

        // ① 没控制终端。
        assert!(
            wait_until(|| exec_settled(kid, "sleep")),
            "setsid sleep 没起来"
        );
        let detached = shown_in(terminal_ends_at(&own, kid), kid).unwrap();
        let detached_here = here_in(terminal_ends_at(&own, kid), kid).unwrap();

        // ② 伪终端上的后台进程组。
        assert!(
            wait_until(|| exec_settled(bg, "sleep")),
            "后台 sleep 没起来"
        );
        let background = shown_in(terminal_ends_at(&own, bg), bg).unwrap();
        let background_here = here_in(terminal_ends_at(&own, bg), bg).unwrap();

        // ③ 别的终端前台的那一个。
        assert!(
            wait_until(|| exec_settled(other, "sleep")),
            "另一个伪终端里的 sleep 没起来"
        );
        let elsewhere = shown_in(terminal_ends_at(&own, other), other).unwrap();
        let elsewhere_here = here_in(terminal_ends_at(&own, other), other).unwrap();

        drop(reap);
        let _ = other_term.kill();
        let _ = other_term.wait();
        drop(end);
        let mut input = hello(&sock, Want::Input).await;
        write_frame(&mut input, &Frame::In(b"\r".to_vec()))
            .await
            .expect("送字");
        task.await.expect("任务没崩").expect("跑完");

        assert!(
            matches!(&front, Shown::By { terminals, tmux: false } if terminals.len() == 1),
            "前台那一个：{front:?}"
        );
        let Here::By { seen, tmux } = front_here else {
            panic!("前台那一个本机那一问答成没有终端连着");
        };
        assert_eq!(
            (seen.iter().map(|(p, _)| *p).collect::<Vec<_>>(), tmux),
            (vec![me], false),
            "前台那一个本机那一问的链从附着着的那一扇起"
        );
        assert_eq!(detached, Shown::NoTerminal, "① 没控制终端的进程");
        assert_eq!(background, Shown::Unreadable, "② 后台进程组里的进程");
        assert_eq!(elsewhere, Shown::Unreadable, "③ 别的终端前台的那一个");
        for (what, pid, here) in [
            ("①", kid, detached_here),
            ("②", bg, background_here),
            ("③", other, elsewhere_here),
        ] {
            let Here::By { seen, tmux } = here else {
                panic!("{what} 本机那一问答成没有终端连着");
            };
            assert_eq!(
                seen.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
                vec![pid],
                "{what} 本机那一问的链不是从它自己起"
            );
            assert!(!tmux, "{what} 本机那一问说它在 tmux 里");
        }
    };
    tokio::time::timeout(guard_core::wall::CEILING, run)
        .await
        .expect("兜底上限之内跑完");
}
