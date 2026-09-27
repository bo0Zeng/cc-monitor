//! 〔RM1a · 第四波〕`relay/machine.rs` 的判据 —— 这台机器上的中转进程：在不在听 · 起一个脱离的。
//!
//! # 买到的
//!
//! - 「在不在听」两个方向都量：真绑一个口 ⇒ `true`；放掉 ⇒ `false`（同一把尺子，异源：口是判据自己开的）。
//! - 口上有人 ⇒ `relay-ensure` **一个进程都不起**（`started:false`）。
//! - 🔴 **真起一个子进程**走生产段那一份起法（`start`）：测试二进制自己当子进程、拿到 `CCM_RELAY_PORT`
//!   就去听那个口 ⇒ 父判据等到口上有人；并在 unix 上读 `/proc` 核两件事：
//!   ① 子进程**自成一个进程组**（pgid == pid）；② stdin/stdout/stderr 三根都接在 `/dev/null` 上。
//! - 入参闸：端口缺 / 0 / 越界 / 不是数 ⇒ `bad_args`。
//! - 起的是 `--relay` 那一臂：生产段交给 `start` 的 argv 与 `main.rs` 的分派臂同一个字面量（两处现抠对拍）。
//!
//! # 买不到的
//!
//! - 起出来的真是一个**中转**（本二进制的 `--relay`）：测试里 `current_exe` 是测试二进制，
//!   那一跳只量得到 argv；中转自己起不起得来由 `server_tests` 那一族（真子进程）管。
//! - 「听的那个是不是我们的中转」：本来就答不了（模块头注）。
//! - 🔴 真远端（SSH 断了之后它还在）：本仓不起远端；「自成一个进程组」是那条性质的前提，不是它本身。

use super::*;

/// 子进程入口：被父判据用 `start` 拉起时（环境里有端口）去听那个口，然后等着被杀。
///
/// 标 `#[ignore]`：正常那一趟里它一条断言都不跑，算成 `passed` 就是往门禁里塞一条恒绿的仪式。
#[test]
#[ignore = "子进程入口：只在被父判据经 `start` 拉起时才去听那个口"]
fn machine_child_entry() {
    let Some(port) = std::env::var(crate::relay::listen::ENV_PORT)
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
    else {
        return;
    };
    let l = std::net::TcpListener::bind(("127.0.0.1", port)).expect("子进程绑不上那个口");
    // 给父判据足够的时间看见它；父判据看完就杀，等不到这么久。
    let started = std::time::Instant::now();
    l.set_nonblocking(true).unwrap();
    while started.elapsed() < std::time::Duration::from_secs(30) {
        let _ = l.accept();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

const CHILD_TEST_NAME: &str = "relay::machine::tests::machine_child_entry";

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

/// 〔RK1〕⑧ 口上是**我们的中转**（真中转、带夹具钥匙）⇒ `relay-status` 说在听、`relay-ensure` 一个进程都不起。
#[test]
fn ensure_starts_nothing_when_our_relay_already_listens() {
    let home = super::super::door::door_tests::seed_test_home(&fixture_home("ours"));
    let listener = super::super::listen::listen(0).expect("listen");
    let port = listener.local_addr().unwrap().port();
    let relay = std::sync::Arc::new(super::super::server::Relay::new(
        std::sync::Arc::new(NoRows),
        super::super::door::Key::for_tests(),
        super::super::tee::TeeSink::new(Box::new(std::io::sink())),
        std::time::Duration::from_secs(10),
        std::time::Duration::from_secs(10),
    ));
    std::thread::spawn(move || super::super::listen::serve(listener, relay, Default::default()));
    let get = home_env(&home);
    assert_eq!(occupant(port, &get), Occupant::Ours);
    let got = answer_ensure_with(&json!({ "port": port }), &get).expect("应当成功");
    assert_eq!(got["started"], false, "口上有人还起了一个：{got}");
    assert_eq!(got["listening"], true);
    assert!(got.get("pid").is_none(), "没起进程却带着 pid：{got}");
    let st = answer_status_with(&json!({ "port": port }), &get).unwrap();
    assert_eq!(st["listening"], true);
    // 泄露判据的一面：应答里没有钥匙（正控：应答里有端口）。
    for v in [&got, &st] {
        let text = v.to_string();
        assert!(text.contains(&port.to_string()), "应答采集面是死的：{text}");
        assert_eq!(
            text.matches(super::super::door::door_tests::TEST_KEY)
                .count(),
            0,
            "钥匙进了应答：{text}"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// 〔RK1〕⑧ 口上是**别的东西**：
/// - 一个只 accept 不说话的口、而这台没有钥匙文件 ⇒ `relay-status` 说「不在」（窄窗那一格）、`relay-ensure` 说 `not_ours`；
/// - 有钥匙文件、口上是个对什么都回 404 的「旧中转」⇒ 两条都 `not_ours`；
/// - 没人 ⇒ `Nobody`。
#[test]
fn a_port_held_by_something_else_is_not_ours_and_ensure_says_so() {
    // ① 没钥匙文件、有人在听。
    let empty = fixture_home("nokey");
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    let get = home_env(&empty);
    assert_eq!(occupant(port, &get), Occupant::NoKeyFile);
    assert_eq!(
        answer_status_with(&json!({ "port": port }), &get).unwrap()["listening"],
        false
    );
    let err = answer_ensure_with(&json!({ "port": port }), &get).expect_err("该拒");
    assert_eq!(err.0, "not_ours", "{err:?}");
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
    match occupant(port, &get) {
        Occupant::NotOurs(why) => {
            assert!(
                why.contains(&port.to_string()),
                "为什么里要点名那个口：{why}"
            );
            assert_eq!(
                why.matches(super::super::door::door_tests::TEST_KEY)
                    .count(),
                0,
                "钥匙进了说法：{why}"
            );
        }
        other => panic!("对什么都回 404 的旧中转不该被认成我们的：{other:?}"),
    }
    for (name, got) in [
        (
            "relay-status",
            answer_status_with(&json!({ "port": port }), &get),
        ),
        (
            "relay-ensure",
            answer_ensure_with(&json!({ "port": port }), &get),
        ),
    ] {
        assert_eq!(got.expect_err("该拒").0, "not_ours", "{name}");
    }
    // ③ 没人。
    assert_eq!(occupant(free_port(), &get), Occupant::Nobody);
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&empty);
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

#[test]
fn bad_ports_are_refused() {
    for args in [
        json!({}),
        json!({ "port": 0 }),
        json!({ "port": 70000 }),
        json!({ "port": "8788" }),
        json!({ "port": -1 }),
    ] {
        for (name, got) in [
            ("relay-status", answer_status(&args)),
            ("relay-ensure", answer_ensure(&args)),
        ] {
            let err = got.expect_err("坏端口还成功了");
            assert_eq!(
                err.0, "bad_args",
                "`{name}` {args} 应当是 bad_args，实得 {err:?}"
            );
        }
    }
}

#[test]
fn start_really_spawns_a_detached_child_with_null_stdio() {
    let port = free_port();
    let exe = std::env::current_exe().expect("测试二进制自己");
    let pid = start(
        &exe,
        &[CHILD_TEST_NAME, "--exact", "--ignored", "--nocapture"],
        port,
    )
    .expect("起子进程失败");
    // 等子进程去听那个口（测试段可以等；生产段零定时器）。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !listening(port) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let up = listening(port);
    #[cfg(target_os = "linux")]
    let proc_facts = {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        // `pid (comm) state ppid pgrp …` —— comm 可能带空格，从最后一个 `)` 之后切。
        let after = stat.rsplit_once(')').map(|(_, r)| r).unwrap_or("");
        let pgrp: Option<u32> = after.split_whitespace().nth(2).and_then(|s| s.parse().ok());
        let fds: Vec<String> = (0..3)
            .map(|fd| {
                std::fs::read_link(format!("/proc/{pid}/fd/{fd}"))
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|e| format!("读不到：{e}"))
            })
            .collect();
        (pgrp, fds)
    };
    // 先收拾：不管断言成不成，子进程都得死。
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
    assert!(
        up,
        "子进程 {pid} 起了，但 20 秒内口 {port} 上一直没人 —— 起法或环境那一格坏了"
    );
    #[cfg(target_os = "linux")]
    {
        let (pgrp, fds) = proc_facts;
        assert_eq!(
            pgrp,
            Some(pid),
            "子进程没有自成一个进程组（pgrp {pgrp:?} ≠ pid {pid}）"
        );
        for (i, target) in fds.iter().enumerate() {
            assert_eq!(
                target, "/dev/null",
                "子进程第 {i} 根 stdio 没接空：{target}"
            );
        }
    }
}

#[test]
fn what_ensure_starts_is_the_relay_arm_of_main() {
    // 生产段交给 `start` 的 argv —— 现抠，不手抄。
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/relay/machine.rs"
    ));
    let at = prod
        .find("start(&exe, &[\"")
        .expect("生产段里找不到 `start(&exe, &[…` 那一行");
    let rest = &prod[at + "start(&exe, &[\"".len()..];
    let arg = &rest[..rest.find('"').expect("argv 字面量没收尾")];
    // `main.rs` 的分派臂 —— 同一个字面量必须恰好一条臂。
    let main = include_str!("../../../src/backend/main.rs");
    assert_eq!(
        main.matches(&format!("Some(\"{arg}\") =>")).count(),
        1,
        "`relay-ensure` 起的是 `{arg}`，而 `main.rs` 里没有恰好一条那样的分派臂"
    );
    assert_eq!(arg, "--relay", "起的不是 `--relay` 那一臂：{arg}");
}
