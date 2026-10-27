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
    let home = test_home::sandbox_root().join("home").to_string_lossy().into_owned();
    let own = Own::issue_with(&move |k| match k {
        "CCM_DATA_DIR" => Some(d.clone()),
        "HOME" => Some(home.clone()),
        _ => None,
    });
    (own, data)
}

pub(crate) fn spec(id: &str, script: &str) -> Spec {
    Spec {
        id: id.into(),
        name: format!("name-{id}"),
        size: Size { rows: 24, cols: 80 },
        cwd: String::new(),
        agent: "fake".into(),
        client: "terminal".into(),
        argv: vec!["sh".into(), "-c".into(), script.into()],
    }
}

/// 起托管进程（同一进程里的一个任务），等它绑好口；交回套接字路径与那个任务。
pub(crate) async fn start(
    own: &Own,
    s: Spec,
) -> (std::path::PathBuf, tokio::task::JoinHandle<Result<(), Said>>) {
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
            who: Who { kind: WhoKind::Terminal, client: None },
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
    let script = r#"printf 'ready\r\n'; read x; printf 'got:%s\r\n' "$x"; printf 'host=%s term=%s\r\n' "$CCM_HOST" "$TERM"; read y; exit 3"#;
    let (sock, task) = start(&own, spec("t-echo", script)).await;
    let dir = data.join("hosts").join("t-echo");
    // 家：0700、事实在、锁攥着（活着）。
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&dir).expect("目录在").permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
    let facts = home::read_facts(&dir).expect("事实在");
    assert_eq!(facts.id, "t-echo");
    assert_eq!(facts.name, "name-t-echo");
    assert!(facts.child_pid.is_some());
    assert_eq!(home::alive(&own).len(), 1, "锁攥着 ⇒ 算活着");
    assert_eq!(home::find(&own, "name-t-echo").map(|f| f.id), Some("t-echo".into()));

    let mut s = hello(&sock, Want::Input).await;
    read_until_out(&mut s, "ready").await;
    write_frame(&mut s, &Frame::In(b"hi\r".to_vec())).await.expect("送字");
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
    write_frame(&mut s, &Frame::In(b"\r".to_vec())).await.expect("送字");
    assert_eq!(read_until_ended(&mut s).await, Some(3), "带子进程的退出码");
    task.await.expect("任务没崩").expect("跑完");
    assert!(!dir.exists(), "目录收掉了");
    assert!(home::alive(&own).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn end_hangs_up_the_child_group() {
    let (own, data) = sandbox_own("end");
    let script = r#"trap 'exit 7' HUP; printf 'ready\r\n'; while :; do read x; done"#;
    let (sock, task) = start(&own, spec("t-end", script)).await;
    let mut s = hello(&sock, Want::View).await;
    read_until_out(&mut s, "ready").await;
    write_frame(&mut s, &Frame::Ctl(Ctl::End)).await.expect("end");
    let code = read_until_ended(&mut s).await;
    assert!(code.is_some(), "收到 ended：{code:?}");
    task.await.expect("任务没崩").expect("跑完");
    assert!(!data.join("hosts").join("t-end").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_id_cannot_be_hosted_twice() {
    let (own, _data) = sandbox_own("twice");
    let (sock, task) = start(&own, spec("t-twice", "printf 'ready\\r\\n'; read x")).await;
    let again = run(spec("t-twice", "true"), &own, |_| {}).await;
    assert!(again.is_err(), "第二个拿不到锁");
    let mut s = hello(&sock, Want::Input).await;
    read_until_out(&mut s, "ready").await;
    write_frame(&mut s, &Frame::In(b"\r".to_vec())).await.expect("送字");
    task.await.expect("任务没崩").expect("跑完");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_viewer_cannot_type_into_the_terminal() {
    let (own, _data) = sandbox_own("view");
    let script = r#"printf 'ready\r\n'; read x; printf 'got:%s\r\n' "$x""#;
    let (sock, task) = start(&own, spec("t-view", script)).await;
    let mut viewer = hello(&sock, Want::View).await;
    read_until_out(&mut viewer, "ready").await;
    write_frame(&mut viewer, &Frame::In(b"nope\r".to_vec())).await.expect("送字");
    loop {
        match read_frame(&mut viewer).await.expect("读").expect("没断") {
            Frame::Ctl(Ctl::HeldElsewhere { .. }) => break,
            Frame::Out(b) => assert!(!String::from_utf8_lossy(&b).contains("nope"), "只看的字没进去"),
            _ => {}
        }
    }
    let mut typer = hello(&sock, Want::Input).await;
    write_frame(&mut typer, &Frame::In(b"yes\r".to_vec())).await.expect("送字");
    read_until_out(&mut viewer, "got:yes").await;
    task.await.expect("任务没崩").expect("跑完");
}

#[test]
fn args_parse_into_a_spec() {
    let a: Vec<String> = ["h1", "--name", "n1", "--size", "120x40", "--cwd", "/tmp", "--", "claude", "--resume", "x"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let s = parse(&a).expect("解得开");
    assert_eq!(s.id, "h1");
    assert_eq!(s.name, "n1");
    assert_eq!(s.size, Size { rows: 40, cols: 120 });
    assert_eq!(s.argv, vec!["claude", "--resume", "x"]);
    assert!(parse(&["h1".to_string()]).is_err(), "没有 argv");
    assert!(parse(&["../x".to_string(), "--".into(), "sh".into()]).is_err(), "句柄带路径字符");
}
