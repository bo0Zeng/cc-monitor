//! 托管进程：真起一个伪终端（里面是 `sh` 跑的假程序：打印 · 回显），沙箱数据目录，不起真 claude。

use super::super::home;
use super::super::wire::{read_frame, write_frame, Ctl, Frame, Size, Want, Who, WhoKind, PROTO};
use super::*;

/// 每条判据一个自己的数据目录（沙箱根底下，路径短：套接字要压在 108 字节里）。
pub(crate) fn sandbox_own(tag: &str) -> (Own, std::path::PathBuf) {
    let data = test_home::sandbox_root().join(format!("h-{tag}"));
    let _ = std::fs::remove_dir_all(&data);
    std::fs::create_dir_all(&data).expect("建沙箱数据目录");
    let d = data.to_string_lossy().into_owned();
    let home = test_home::sandbox_root()
        .join("home")
        .to_string_lossy()
        .into_owned();
    let own = Own::issue_with(&move |k| match k {
        "CCM_DATA_DIR" => Some(d.clone()),
        "HOME" => Some(home.clone()),
        _ => None,
    });
    (own, data)
}

/// 假子进程是 `sh -c <script>`（只在 Unix 上跑的那几条判据用）。
pub(crate) fn spec(id: &str, script: &str) -> Spec {
    spec_argv(id, vec!["sh".into(), "-c".into(), script.into()])
}

/// 两个平台都跑的那几条判据：Unix 交 `sh -c <sh>`，Windows 交 `cmd /d /v:on /c <cmd>`（`!x!` 是读进来那一刻的值）。
pub(crate) fn spec_any(id: &str, sh: &str, cmd: &str) -> Spec {
    let argv = if cfg!(windows) {
        ["cmd", "/d", "/v:on", "/c", cmd]
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        vec!["sh".into(), "-c".into(), sh.into()]
    };
    spec_argv(id, argv)
}

/// 最常用的那一个假子进程：打一行 `ready`，读一行就退。
pub(crate) fn spec_ready(id: &str) -> Spec {
    spec_any(id, "printf 'ready\\r\\n'; read x", "echo ready& set /p x=")
}

fn spec_argv(id: &str, argv: Vec<String>) -> Spec {
    Spec {
        id: id.into(),
        name: format!("name-{id}"),
        size: Size { rows: 24, cols: 80 },
        cwd: String::new(),
        agent: "fake".into(),
        client: "terminal".into(),
        argv,
    }
}

/// 起托管进程（同一进程里的一个任务），等它绑好口；交回套接字路径与那个任务。
pub(crate) async fn start(
    own: &Own,
    s: Spec,
) -> (
    std::path::PathBuf,
    tokio::task::JoinHandle<Result<(), Said>>,
) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let own = own.clone();
    let task = tokio::spawn(async move {
        run(s, &own, move |p| {
            let _ = tx.send(p.to_path_buf());
        })
        .await
    });
    let sock = rx.await.expect("托管口绑好了");
    (sock, task)
}

pub(crate) async fn hello(sock: &std::path::Path, want: Want) -> own_chan::Stream {
    let mut s = own_chan::connect(sock).await.expect("连得上");
    write_frame(
        &mut s,
        &Frame::Ctl(Ctl::Hello {
            proto: PROTO,
            build: "t".into(),
            want,
            size: Some(Size { rows: 24, cols: 80 }),
            who: Who {
                kind: WhoKind::Terminal,
                client: None,
            },
        }),
    )
    .await
    .expect("说 hello");
    s
}

/// 一直读，直到收到的 `out` 拼起来含 `needle`（中途的控制帧一并交回）。
pub(crate) async fn read_until_out(s: &mut own_chan::Stream, needle: &str) -> Vec<Frame> {
    let mut seen = String::new();
    let mut frames = Vec::new();
    loop {
        let f = read_frame(s).await.expect("读得了").expect("没断");
        match &f {
            Frame::Out(b) => seen.push_str(&String::from_utf8_lossy(b)),
            Frame::Ctl(Ctl::Welcome { screen, .. }) => seen.push_str(screen),
            _ => {}
        }
        frames.push(f);
        if seen.contains(needle) {
            return frames;
        }
    }
}

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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn end_hangs_up_the_child_group() {
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_id_cannot_be_hosted_twice() {
    let (own, _data) = sandbox_own("twice");
    let (sock, task) = start(&own, spec_ready("t-twice")).await;
    let again = run(spec_any("t-twice", "true", "exit 0"), &own, |_| {}).await;
    assert!(again.is_err(), "第二个拿不到锁");
    let mut s = hello(&sock, Want::Input).await;
    read_until_out(&mut s, "ready").await;
    write_frame(&mut s, &Frame::In(b"\r".to_vec()))
        .await
        .expect("送字");
    task.await.expect("任务没崩").expect("跑完");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_viewer_cannot_type_into_the_terminal() {
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
}

#[test]
fn args_parse_into_a_spec() {
    let a: Vec<String> = [
        "h1",
        "name=n1",
        "size=120x40",
        "cwd=/tmp",
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
}

/// 数据目录很长、`hosts/<id>/sock` 超过套接字上限 ⇒ 起不来，回的话带路径与长度；不偷偷换地方，盘上不留那个家。
/// 只在通道是盘上一个文件的平台上有这一条上限。
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_socket_path_over_the_limit_refuses_to_start_and_says_how_long() {
    let (own, data) = sandbox_own(&"l".repeat(home::SOCK_PATH_MAX));
    let mut bound = false;
    let got = run(spec("t-long", "true"), &own, |_| bound = true).await;
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
