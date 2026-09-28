//! 〔RM1a · RK1 · DEL〕`relay/machine.rs` 的判据 —— 这台机器上**我们的**中转在不在听（只读）。
//! 守的要求：`设计/20 §3.3`「「在不在」认得出「是不是我们的」…… 两条都对上才算我们的中转在听」（`INVARIANTS §48.1a`）。
//!
//! # 买到的
//!
//! - 「有没有人在听」两个方向都量：真绑一个口 ⇒ `true`；放掉 ⇒ `false`（异源：口是判据自己开的）。
//! - 差分探针三格：真中转（带夹具钥匙）⇒ 是我们的；没钥匙文件 / 对什么都回 404 的旧中转 ⇒ 不是；没人 ⇒ 不是。
//!
//! # 买不到的
//!
//! - 一个**有意模仿**我们门的程序（模块头注）。

use super::*;

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("拿不到空闲口");
    l.local_addr().unwrap().port()
}

#[test]
fn listening_sees_a_bound_port_and_stops_seeing_it_once_released() {
    // 负向那一半在并行跑的判据旁边可能撞上「放掉的口被别人立刻拿走」：
    // 那时说「有人在听」是**对的** —— 用「我自己绑不绑得上」核一次，分得开就算一次有结论的量。
    let mut conclusive = 0;
    for _ in 0..5 {
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        assert!(listening(port), "口上明明有人在听，却说没有");
        drop(l);
        if !listening(port) {
            conclusive += 1;
            continue;
        }
        // 说有人在听：真有人（我绑不上）⇒ 这一趟没结论；没人（我绑得上）⇒ 尺子说谎。
        assert!(
            std::net::TcpListener::bind(("127.0.0.1", port)).is_err(),
            "口已经放掉、也没人拿走（我自己绑得上），却说有人在听"
        );
    }
    assert!(
        conclusive >= 1,
        "五趟都撞上别人拿走那个口 —— 负向那一半一次都没量成"
    );
}

/// 夹具家目录（钥匙文件在它底下）＋ 喂给 `*_with` 的取值器。
fn fixture_home(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "ccm-rk1-machine-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&d).expect("夹具家目录");
    d
}

fn home_env(home: &std::path::Path) -> impl Fn(&str) -> Option<String> {
    let h = home.display().to_string();
    move |k: &str| (k == "HOME").then(|| h.clone())
}

/// 〔RK1〕口上是**我们的中转**（真中转、带夹具钥匙）⇒ 在听。
#[test]
fn our_real_relay_with_the_fixture_key_is_ours() {
    let home = super::super::door::door_tests::seed_test_home(&fixture_home("ours"));
    let listener = super::super::listen::listen(0).expect("listen");
    let port = listener.local_addr().unwrap().port();
    let relay = std::sync::Arc::new(super::super::server::Relay::new(
        std::sync::Arc::new(NoRows),
        super::super::door::Key::for_tests(),
        super::super::tee::TeeSink::to_port(std::sync::Arc::new(NoTap)),
        std::time::Duration::from_secs(10),
        std::time::Duration::from_secs(10),
    ));
    std::thread::spawn(move || super::super::listen::serve(listener, relay, Default::default()));
    let get = home_env(&home);
    assert!(ours(port, &get), "带对钥匙的真中转没被认成我们的");
    let _ = std::fs::remove_dir_all(&home);
}

/// 〔RK1〕口上是**别的东西** ⇒ 不是我们的：
/// - 一个只 accept 不说话的口、而这台没有钥匙文件；
/// - 有钥匙文件、口上是个对什么都回 404 的「旧中转」（没有门）；
/// - 没人。
#[test]
fn a_port_held_by_something_else_is_not_ours() {
    // ① 没钥匙文件、有人在听。
    let empty = fixture_home("nokey");
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    assert!(!ours(port, &home_env(&empty)));
    drop(l);
    // ② 有钥匙文件，口上是一个对什么都回 404 的旧中转（没有门）。
    let home = super::super::door::door_tests::seed_test_home(&fixture_home("old"));
    let old = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = old.local_addr().unwrap().port();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for s in old.incoming() {
            let Ok(mut s) = s else { continue };
            let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(5)));
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
    });
    let get = home_env(&home);
    assert!(listening(port), "夹具那个旧中转没在听 —— 下一条是空真");
    assert!(!ours(port, &get), "对什么都回 404 的旧中转被认成了我们的");
    // ③ 没人。
    assert!(!ours(free_port(), &get));
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&empty);
}

/// 什么都不收的 tap 口（本族判据量的不是 tee）。
struct NoTap;
impl super::super::TapPort for NoTap {
    fn offer(&self, _ev: super::super::TapEvent) -> bool {
        false
    }
}

/// 一行都没有的上游选择（探针只打「不是路由」那一形，不需要表）。
struct NoRows;
impl super::super::Destinations for NoRows {
    fn resolve(
        &self,
        _mode: super::super::Mode,
        _key: &super::super::RouteKey,
        act: &mut dyn FnMut(super::super::Destination<'_>),
    ) {
        act(super::super::Destination::Refuse {
            status: "404 Not Found",
            why: "夹具：一行都没有",
        });
    }
    fn stream_label_headers(&self) -> Vec<&'static str> {
        Vec::new()
    }
}
