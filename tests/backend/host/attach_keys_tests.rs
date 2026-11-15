//! 外层的修饰回车进托管：假外层持有、真托管进程（`sh` 假程序把伪终端设成原始模式、收到的字节印成十六进制），
//! 沙箱数据目录，不起真 claude、不碰 tmux。

use super::super::run::test_support::within_ceiling;
use super::super::run::{serve, Spec};
use super::super::wire::Size;
use super::*;
use crate::platform::fs::Own;
use std::sync::{Arc, Mutex};

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
    fn text(&self) -> String {
        let mut p = vt100::Parser::new(24, 80, 0);
        p.process(&self.0.lock().unwrap());
        p.screen().contents()
    }
}

fn sandbox_own(tag: &str) -> Own {
    let data = test_home::sandbox_root().join(format!("k-{tag}"));
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

async fn until_seen(s: &Screen, needle: &str) {
    let deadline = std::time::Instant::now() + guard_core::wall::CEILING;
    while std::time::Instant::now() < deadline {
        if s.text().contains(needle) {
            return;
        }
        tokio::task::yield_now().await;
        std::thread::yield_now();
    }
    panic!("没等到 {needle:?}：{}", s.text());
}

/// ★ 外层送 kitty / CSI u 的 Shift+Enter · modifyOtherKeys 那一形 · ESC 回车 ⇒ 伪终端里逐字节收到同样的。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn modified_enters_reach_the_pty_byte_for_byte() {
    within_ceiling(async {
        let sent: &[u8] = b"\x1b[13;2u\x1b[27;2;13~\x1b\r";
        let own = sandbox_own("menter");
        let script = format!(
            "stty raw -echo; printf 'ready\\r\\n'; dd bs=1 count={} 2>/dev/null | od -An -tx1 -v | tr -d ' \\n'; printf '\\r\\nEND\\r\\n'; dd bs=1 count=1 2>/dev/null",
            sent.len()
        );
        let spec = Spec {
            id: "k-menter".into(),
            name: "k-menter".into(),
            size: Size { rows: 24, cols: 80 },
            cwd: String::new(),
            agent: "fake".into(),
            client: "terminal".into(),
            account: String::new(),
            argv: vec!["sh".into(), "-c".into(), script],
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        let o = own.clone();
        let task = tokio::spawn(async move {
            serve(spec, &o, move |p| {
                let _ = tx.send(p.to_path_buf());
            })
            .await
            .map(drop)
        });
        let sock = rx.await.expect("绑好了");
        let (ktx, krx) = tokio::sync::mpsc::unbounded_channel();
        let (_rtx, rrx) = tokio::sync::mpsc::unbounded_channel();
        let (_qtx, qrx) = tokio::sync::mpsc::unbounded_channel();
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
        let a = tokio::spawn(async move {
            session(&sock, "k", outer, &Vec::new, Default::default(), qrx).await
        });
        until_seen(&screen, "ready").await;
        ktx.send(sent.to_vec()).unwrap();
        until_seen(&screen, "END").await;
        let hex: String = sent.iter().map(|b| format!("{b:02x}")).collect();
        assert!(screen.text().contains(&hex), "伪终端收到的：{}", screen.text());
        ktx.send(b"\r".to_vec()).unwrap();
        task.await.unwrap().unwrap();
        let _ = a.await;
    })
    .await;
}
