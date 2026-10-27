//! 瘦客户端：假外层终端（键从一条通道进、画进一块内存），真托管进程（`sh` 假程序），沙箱数据目录。

use super::super::run::{run, Spec};
use super::super::wire::Size;
use super::*;
use crate::platform::fs::Own;
use std::sync::{Arc, Mutex};

fn sandbox_own(tag: &str) -> Own {
    let data = test_home::sandbox_root().join(format!("a-{tag}"));
    let _ = std::fs::remove_dir_all(&data);
    std::fs::create_dir_all(&data).expect("建沙箱数据目录");
    let d = data.to_string_lossy().into_owned();
    let home = test_home::sandbox_root()
        .join("home")
        .to_string_lossy()
        .into_owned();
    Own::issue_with(&move |k| match k {
        "CCM_DATA_DIR" => Some(d.clone()),
        "HOME" => Some(home.clone()),
        _ => None,
    })
}

async fn host(
    own: &Own,
    id: &str,
    script: &str,
) -> (
    std::path::PathBuf,
    tokio::task::JoinHandle<Result<(), Said>>,
) {
    let spec = Spec {
        id: id.into(),
        name: id.into(),
        size: Size { rows: 24, cols: 80 },
        cwd: String::new(),
        agent: "fake".into(),
        client: "terminal".into(),
        argv: vec!["sh".into(), "-c".into(), script.into()],
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let own = own.clone();
    let task = tokio::spawn(async move {
        run(spec, &own, move |p| {
            let _ = tx.send(p.to_path_buf());
        })
        .await
    });
    (rx.await.expect("绑好了"), task)
}

#[derive(Clone, Default)]
struct Screen(Arc<Mutex<Vec<u8>>>);

impl Write for Screen {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Screen {
    /// 外层终端看到的样子：画出去的字节喂进一个 80×24 的终端。
    fn seen(&self) -> vt100::Parser {
        let mut p = vt100::Parser::new(24, 80, 0);
        p.process(&self.0.lock().unwrap());
        p
    }
    fn text(&self) -> String {
        self.seen().screen().contents()
    }
    fn raw(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

struct Fake {
    keys: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    _resized: tokio::sync::mpsc::UnboundedSender<()>,
    screen: Screen,
    task: tokio::task::JoinHandle<Result<Left, Said>>,
}

fn attach_fake(sock: &std::path::Path) -> Fake {
    let (ktx, krx) = tokio::sync::mpsc::unbounded_channel();
    let (rtx, rrx) = tokio::sync::mpsc::unbounded_channel();
    let screen = Screen::default();
    let outer = Outer {
        keys: krx,
        resized: rrx,
        size: Box::new(|| Some(Size { rows: 24, cols: 80 })),
        out: Box::new(screen.clone()),
        prefix: keys::DEFAULT_PREFIX,
        vi: false,
        in_tmux: false,
    };
    let sock = sock.to_path_buf();
    let task = tokio::spawn(async move { session(&sock, outer, &Vec::new).await });
    Fake {
        keys: ktx,
        _resized: rtx,
        screen,
        task,
    }
}

/// 等外层终端上出现那段字（拍子跟着托管口走，判据这边只能轮询；每轮让出一下）。
async fn until_seen(s: &Screen, needle: &str) {
    for _ in 0..20_000 {
        if s.text().contains(needle) || s.raw().contains(needle) {
            return;
        }
        tokio::task::yield_now().await;
        std::thread::yield_now();
    }
    panic!("没等到 {needle:?}：{}", s.text());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn detaching_leaves_the_terminal_running_and_reattaching_shows_the_same_screen() {
    let own = sandbox_own("detach");
    let (sock, task) = host(
        &own,
        "a-detach",
        r#"printf 'ready\r\n'; while read x; do printf 'echo:%s\r\n' "$x"; done"#,
    )
    .await;
    let a = attach_fake(&sock);
    until_seen(&a.screen, "ready").await;
    a.keys.send(b"hello\r".to_vec()).unwrap();
    until_seen(&a.screen, "echo:hello").await;
    let before = a.screen.seen();
    a.keys.send(b"\x02d".to_vec()).unwrap();
    assert_eq!(a.task.await.unwrap().unwrap(), Left::Detached);
    // 托管与子进程照跑。
    assert_eq!(super::super::home::alive(&own).len(), 1);
    assert!(!a.screen.raw().ends_with("\x02"), "前缀没送进去");

    let b = attach_fake(&sock);
    until_seen(&b.screen, "echo:hello").await;
    let after = b.screen.seen();
    assert_eq!(
        before.screen().contents(),
        after.screen().contents(),
        "再接上屏幕逐格相同"
    );
    assert_eq!(
        before.screen().cursor_position(),
        after.screen().cursor_position()
    );
    b.keys.send(b"\x02x".to_vec()).unwrap();
    until_seen(&b.screen, &copy_text("beHost.attach.confirmEnd", &[])).await;
    b.keys.send(b"y".to_vec()).unwrap();
    assert!(matches!(b.task.await.unwrap().unwrap(), Left::Ended(_)));
    task.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_viewer_types_nothing_until_enter_takes_over() {
    let own = sandbox_own("take");
    let (sock, task) = host(
        &own,
        "a-take",
        r#"printf 'ready\r\n'; while read x; do printf 'echo:%s\r\n' "$x"; done"#,
    )
    .await;
    let a = attach_fake(&sock);
    until_seen(&a.screen, "ready").await;
    // B 接上 ⇒ B 持有，A 只看。
    let b = attach_fake(&sock);
    until_seen(&b.screen, "ready").await;
    until_seen(&a.screen, &copy_text("beHost.attach.viewOnly", &[])).await;
    a.keys.send(b"zz".to_vec()).unwrap();
    until_seen(
        &a.screen,
        &copy_text(
            "beHost.attach.heldBy",
            &[("who", &copy_text("beHost.attach.whoTerminal", &[]))],
        ),
    )
    .await;
    // 回车接管（这个回车不送进去），之后送得进。
    a.keys.send(b"\r".to_vec()).unwrap();
    for _ in 0..2000 {
        tokio::task::yield_now().await;
    }
    a.keys.send(b"mine\r".to_vec()).unwrap();
    until_seen(&b.screen, "echo:mine").await;
    assert!(!b.screen.text().contains("echo:zz"), "只看时敲的键没送进去");
    assert!(
        !b.screen.text().contains("echo:\r"),
        "接管的那个回车没送进去"
    );
    b.keys.send(b"\x02x".to_vec()).unwrap();
    for _ in 0..2000 {
        tokio::task::yield_now().await;
    }
    b.keys.send(b"y".to_vec()).unwrap();
    task.await.unwrap().unwrap();
    let _ = a.task.await;
    let _ = b.task.await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_session_ending_returns_the_childs_code() {
    let own = sandbox_own("code");
    let (sock, task) = host(&own, "a-code", r#"printf 'ready\r\n'; read x; exit 5"#).await;
    let a = attach_fake(&sock);
    until_seen(&a.screen, "ready").await;
    a.keys.send(b"\r".to_vec()).unwrap();
    assert_eq!(a.task.await.unwrap().unwrap(), Left::Ended(Some(5)));
    assert!(
        a.screen.raw().ends_with("\x1b[?1049l"),
        "出来时还原外层终端"
    );
    task.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn copy_mode_shows_history_from_the_host() {
    let own = sandbox_own("copy");
    let (sock, task) = host(
        &own,
        "a-copy",
        r#"i=1; while [ $i -le 60 ]; do printf 'row %s\r\n' $i; i=$((i+1)); done; read x"#,
    )
    .await;
    let a = attach_fake(&sock);
    until_seen(&a.screen, "row 60").await;
    a.keys.send(b"\x02[".to_vec()).unwrap();
    until_seen(&a.screen, "[0/").await;
    a.keys.send(b"\x1b[5~\x1b[5~".to_vec()).unwrap();
    until_seen(&a.screen, "row 5\n").await;
    a.keys.send(b"q".to_vec()).unwrap();
    until_seen(&a.screen, "row 60").await;
    a.keys.send(b"\r".to_vec()).unwrap();
    let _ = a.task.await;
    task.await.unwrap().unwrap();
}

#[test]
fn the_host_run_word_routes_to_the_hosted_terminal_not_the_query_face() {
    let a: Vec<String> = ["--", "--host-run", "h1", "--", "sh"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    // 不进后端的一次性查询面（那一面只认 `SUBCOMMANDS`），落在 ccm 那一侧、由它在起会话那一套之前接走。
    assert_eq!(
        crate::control::ccm::route("ccm", &a),
        crate::control::ccm::Entry::Ccm(a.clone())
    );
    assert_eq!(crate::control::ccm::host_run_args(&a), Some(&a[2..]));
    let not: Vec<String> = ["--host-run", "h1", "--", "sh"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        crate::control::ccm::host_run_args(&not),
        None,
        "不紧跟打头的 `--` 不算"
    );
}
