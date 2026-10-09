//! Linux 单实例：第一个在会话总线上占住名字；第二个见名字被占，把自己的参数与**激活令牌**交给第一个、然后退出。
//! 总线是一条私有的 `dbus-daemon --session`（判据不碰真会话总线），用完杀掉。

use super::linux::{claim, launch_token, Claim};
use super::Second;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::channel;
use std::time::Duration;

/// 一条私有会话总线（`dbus-daemon --nofork --print-address`），随值收掉。
struct PrivateBus {
    child: Child,
    addr: String,
}

impl PrivateBus {
    fn start() -> Self {
        use std::io::BufRead;
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon 起得来");
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        PrivateBus {
            child,
            addr: line.trim().to_string(),
        }
    }

    fn builder(&self) -> zbus::blocking::connection::Builder<'static> {
        zbus::blocking::connection::Builder::address(self.addr.as_str()).unwrap()
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// ★ 第一个占住；第二个交来参数与令牌（第一个那边收到的正是这两样）后自认「交过了」；没有令牌的那一次照样交，令牌是 `None`。
#[test]
fn the_second_launch_hands_its_args_and_activation_token_to_the_first() {
    let bus = PrivateBus::start();
    let (tx, rx) = channel::<Second>();
    let open = || Ok(bus.builder());
    let first = claim(
        &open,
        Box::new(move |s| {
            let _ = tx.send(s);
        }),
        &[],
        None,
    )
    .expect("第一个连得上");
    assert!(matches!(first, Claim::First(_)), "第一个应当占住名字");

    let second = claim(
        &open,
        Box::new(|_| panic!("第二个不该自己接到调用")),
        &["cc-monitor".to_string(), "--background".to_string()],
        Some("tok-123".to_string()),
    )
    .expect("第二个连得上");
    assert!(matches!(second, Claim::Handed), "名字被占 ⇒ 交给第一个");
    let got = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("第一个收到了");
    assert_eq!(
        got,
        Second {
            args: vec!["cc-monitor".into(), "--background".into()],
            token: Some("tok-123".into()),
        }
    );

    let third = claim(&open, Box::new(|_| {}), &[], None).unwrap();
    assert!(matches!(third, Claim::Handed));
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap().token, None);
    drop(first);
}

/// ★ 令牌从哪来：Wayland 的 `XDG_ACTIVATION_TOKEN` 优先，X11 的 `DESKTOP_STARTUP_ID` 其次；空串当没有。
#[test]
fn the_launch_token_is_read_from_the_two_launch_variables() {
    let s = |v: &str| Some(v.to_string());
    assert_eq!(launch_token(s("wl"), s("x11")), s("wl"));
    assert_eq!(launch_token(None, s("x11")), s("x11"));
    assert_eq!(launch_token(s(""), s("x11")), s("x11"));
    assert_eq!(launch_token(s(""), s("")), None);
    assert_eq!(launch_token(None, None), None);
}
