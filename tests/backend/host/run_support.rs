//! 起托管终端的共用夹具：沙箱数据目录 · 假子进程（`sh` / `cmd` 跑的假程序）· 起托管进程 · 说 hello · 读到哪一句。
//! 本包判据与上层判据（经 `test-support`）共用；不起真 claude。

use super::super::wire::{read_frame, write_frame, Ctl, Frame, Size, Want, Who, WhoKind, PROTO};
use super::{serve, Spec};
use crate::platform::fs::Own;
use copy_core::said::Said;

/// 每条判据一个自己的数据目录（沙箱根底下，路径短：套接字要压在 108 字节里）。
pub fn sandbox_own(tag: &str) -> (Own, std::path::PathBuf) {
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
pub fn spec(id: &str, script: &str) -> Spec {
    spec_argv(id, vec!["sh".into(), "-c".into(), script.into()])
}

/// 两个平台都跑的那几条判据：Unix 交 `sh -c <sh>`，Windows 交 `cmd /d /v:on /c <cmd>`（`!x!` 是读进来那一刻的值）。
pub fn spec_any(id: &str, sh: &str, cmd: &str) -> Spec {
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
pub fn spec_ready(id: &str) -> Spec {
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
pub async fn start(
    own: &Own,
    s: Spec,
) -> (
    std::path::PathBuf,
    tokio::task::JoinHandle<Result<(), Said>>,
) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let own = own.clone();
    let task = tokio::spawn(async move {
        serve(s, &own, move |p| {
            let _ = tx.send(p.to_path_buf());
        })
        .await
        .map(drop)
    });
    let sock = rx.await.expect("托管口绑好了");
    (sock, task)
}

/// 以真终端的身份连上托管口、说 hello（24×80）。
pub async fn hello(sock: &std::path::Path, want: Want) -> own_chan::Stream {
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
pub async fn read_until_out(s: &mut own_chan::Stream, needle: &str) -> Vec<Frame> {
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
