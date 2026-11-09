use super::*;
use serde_json::json;

/// 已有会话名的 Gate 1（结束 · 抓屏 · 送键共用）：会坏掉 tmux 目标的形状一律 `bad_args`，`=` 放行。
#[test]
fn shape_validation_rejects_what_would_break_the_tmux_target() {
    for (bad, why) in [
        ("", "空"),
        ("a:b", "含 `:`（tmux 目标语法）"),
        ("a\nb", "含控制字符"),
        ("a\u{202e}b", "含视觉欺骗字符（`gate_rules` 那一格）"),
    ] {
        assert_eq!(
            admit_existing_name(bad).map_err(|f| f.code),
            Err(Some("bad_args".to_string())),
            "{why} 应当被拒：{bad:?}"
        );
    }
    // `=` 放行：`=a=b:` 精确命中名叫 `a=b` 的会话（attach 那一条同样放行）。
    for good in ["proj-cc", "a=b", "=a"] {
        assert!(admit_existing_name(good).is_ok(), "{good}");
    }
}

/// `terminal-end` 的入参：多一格 · 类型不对在解入参那一下就不收（同 typed 分派那一步）。
#[test]
fn the_terminal_end_request_is_strict() {
    for bad in [
        json!({ "terminal": "t-1", "extra": 1 }),
        json!({ "terminal": 1 }),
        json!({ "name": "demo-cc" }),
    ] {
        assert!(
            serde_json::from_value::<TerminalEndReq>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

/// 没杀成那两形：句子只带原因词，原话（tmux 的 stderr · 起不来时的系统报错）进复制详情、不上句子。
#[test]
fn the_kill_failures_keep_the_raw_words_out_of_the_sentence() {
    let e = kill_refused(b"can't find pane: %9\n");
    assert_eq!(e.code.as_deref(), Some("kill_failed"));
    assert_eq!(e.said, copy_text("beKill.run.failed", &[]));
    assert_eq!(e.raw.as_deref(), Some("can't find pane: %9"));

    let dir = std::env::temp_dir().join(format!("ccm-kill-gone-{}", std::process::id()));
    let fail = crate::platform::child::Child::new(dir.join("gone"))
        .run(crate::platform::child::Deadline::secs(5))
        .expect_err("不存在的程序起得来");
    let e = tmux_unavailable(fail);
    assert_eq!(e.code.as_deref(), Some("no_tmux"));
    assert_eq!(
        e.said,
        copy_text(
            "reason.tmux.startFailed",
            &[(
                "why",
                &copy_core::spawn_reason(std::io::ErrorKind::NotFound)
            )]
        )
    );
    assert!(e.raw.is_some(), "系统原话丢了");
}

// ── `terminal-end` ───────────────────────────────────────────────────

/// 这台没有 tmux server（私有 socket 指向一个不存在的路径：名单里 tmux 那一臂是空的）：托管那一行照结束，不回 `no_tmux`。
/// 假子进程（`sh`）、沙箱数据目录，不起真 claude。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_end_ends_a_hosted_terminal_where_there_is_no_tmux() {
    use crate::host::managed::run::test_support::{
        hello, read_until_out, sandbox_own, spec, start,
    };
    use crate::host::managed::wire::{read_frame, Ctl, Frame, Want};
    let (own, data) = sandbox_own("end-cmd");
    let script = r#"trap 'exit 7' HUP; printf 'ready\r\n'; while :; do read x; done"#;
    // 用户在自己的终端里敲 `cc` 起的（`client` 同 tmux 那一形的 `@ccm_client`：`ccm` ＝ 用户的终端）。
    let mut s = spec("t-endcmd", script);
    s.client = crate::control::gate_rules::CLIENT_TERMINAL.into();
    let (sock, task) = start(&own, s).await;
    let mut viewer = hello(&sock, Want::View).await;
    read_until_out(&mut viewer, "ready").await;
    let bogus = data.join("no-tmux-here").to_string_lossy().into_owned();
    let o = own.clone();
    let b = bogus.clone();
    let got = tokio::task::spawn_blocking(move || {
        end_terminal(
            crate::host::tmux::On { socket: Some(&b) },
            Some(&o),
            &TerminalEndReq {
                terminal: Some("t-endcmd".into()),
                ..TerminalEndReq::default()
            },
        )
    })
    .await
    .expect("线没崩");
    assert_eq!(
        got.map_err(|f| f.code),
        Ok(TerminalEnded {
            terminal: "t-endcmd".into()
        })
    );
    loop {
        match read_frame(&mut viewer).await.expect("读") {
            Some(Frame::Ctl(Ctl::Ended { .. })) | None => break,
            Some(_) => {}
        }
    }
    task.await.expect("任务没崩").expect("跑完");
    assert!(
        crate::host::managed::rows_at(&own).is_empty(),
        "名单里那一行没了"
    );
    // 名单里没有的 ⇒ `not_known`。
    let o = own.clone();
    let gone = tokio::task::spawn_blocking(move || {
        end_terminal(
            crate::host::tmux::On {
                socket: Some(&bogus),
            },
            Some(&o),
            &TerminalEndReq {
                terminal: Some("t-endcmd".into()),
                ..TerminalEndReq::default()
            },
        )
    })
    .await
    .expect("线没崩");
    assert_eq!(gone.map_err(|f| f.code), Err(Some("not_known".to_string())));
}

/// 认不到：码与原话（复制详情）同别的终端命令（预览 · 跟随），照 `terminals::find_one`；给人看的那一句说「{target} 已不在」。
#[test]
fn terminal_end_says_not_known_like_the_other_terminal_commands() {
    let dir = std::env::temp_dir().join(format!("ccm-kill-nk-{}", std::process::id()));
    let bogus = dir.join("no-tmux-here").to_string_lossy().into_owned();
    let got = end_terminal(
        crate::host::tmux::On {
            socket: Some(&bogus),
        },
        None,
        &TerminalEndReq {
            terminal: Some("tmux-999".into()),
            ..TerminalEndReq::default()
        },
    )
    .expect_err("名单里没有它");
    let t = crate::control::terminals::target(Some("tmux-999"), None).expect("目标合形");
    let want = crate::control::terminals::find_one(&[], &t).expect_err("空名单认不到");
    assert_eq!((got.code, got.raw), (want.code, want.raw));
    assert_eq!(
        got.said,
        copy_text("tmuxControl.kill.noSuchSession", &[("target", "tmux-999")])
    );
}

/// 私有 socket 上的一台 tmux（`-S`，同生产 `On { socket }` 那一形）。
struct Iso {
    sock: std::path::PathBuf,
}

impl Iso {
    fn new(tag: &str) -> Iso {
        let dir = std::env::temp_dir().join(format!("ccm-kill-{}-{tag}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        Iso {
            sock: dir.join("s"),
        }
    }
    fn on(&self) -> crate::host::tmux::On<'_> {
        crate::host::tmux::On {
            socket: self.sock.to_str(),
        }
    }
    fn tmux(&self, args: &[&str]) -> std::process::Output {
        std::process::Command::new("tmux")
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .arg("-S")
            .arg(&self.sock)
            .args(args)
            .output()
            .expect("tmux 不可执行 —— 本测试要求环境有 tmux")
    }
    /// 起一个挂着 `sid` 的单窗口会话（前台 `cat`）。
    fn session(&self, name: &str, sid: &str) {
        let o = self.tmux(&["-f", "/dev/null", "new-session", "-d", "-s", name, "cat"]);
        assert!(
            o.status.success(),
            "建会话失败：{}",
            String::from_utf8_lossy(&o.stderr)
        );
        let t = crate::common::tmux_utf8::exact_target(name);
        self.tmux(&["set-option", "-t", &t, "@ccm_sid", sid]);
    }
    fn id_of(&self, name: &str) -> String {
        let t = crate::common::tmux_utf8::exact_target(name);
        String::from_utf8_lossy(
            &self
                .tmux(&["display-message", "-p", "-t", &t, "#{session_id}"])
                .stdout,
        )
        .trim()
        .to_string()
    }
}

impl Drop for Iso {
    fn drop(&mut self) {
        // 收尾不许再 panic（测试已经在 panic 时会变成 abort，把整个测试二进制带走）。
        let _ = std::process::Command::new("tmux")
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .arg("-S")
            .arg(&self.sock)
            .arg("kill-server")
            .output();
        if let Some(d) = self.sock.parent() {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

/// ★ **结束按名单那一行的句柄过门，不按名字再认一次**（`INVARIANTS §34`）：列名单之后那个会话没了、
/// 同名又起了一个（新的 `#{session_id}`）⇒ 那一行指的会话不在了（`no_such_session`），同名的新会话不被杀。
#[test]
fn terminal_end_does_not_kill_a_namesake_that_replaced_the_listed_session() {
    let iso = Iso::new("stale");
    iso.session("keep-cc", "sid-keep");
    iso.session("w-cc", "sid-old");
    let (rows, _, _) = crate::host::list_with(iso.on(), &|| Default::default()).expect("名单");
    let row = rows
        .iter()
        .find(|r| r.name == "w-cc")
        .cloned()
        .expect("名单里有 w-cc");
    let old = iso.id_of("w-cc");
    assert!(iso.tmux(&["kill-session", "-t", &old]).status.success());
    iso.session("w-cc", "sid-old");
    let new = iso.id_of("w-cc");
    assert_ne!(old, new, "同名新会话换了句柄");
    let got = end_row(iso.on(), None, &row, None);
    assert_eq!(
        got.map_err(|f| f.code),
        Err(Some("no_such_session".to_string()))
    );
    assert_eq!(iso.id_of("w-cc"), new, "同名的新会话被杀了");
}

/// 一台卡住的 tmux：socket 绑着、在听，从不应答（tmux 客户端连上之后等满期限）。
#[cfg(unix)]
fn stuck_tmux_socket(tag: &str) -> (std::path::PathBuf, std::os::unix::net::UnixListener) {
    let dir = std::env::temp_dir().join(format!("ccm-kill-stuck-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建目录");
    let sock = dir.join("s");
    let l = std::os::unix::net::UnixListener::bind(&sock).expect("绑 socket");
    (sock, l)
}

/// ★ 托管终端的结束不经 tmux：这台的 tmux 卡住（连上不应答，每一发都要等满期限）时，
/// 结束托管那一行照样成，一发 tmux 都不起（那台的 socket 上没有一个客户端连上来）。
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ending_a_hosted_terminal_does_not_wait_on_a_stuck_tmux() {
    use crate::host::managed::run::test_support::{
        hello, read_until_out, sandbox_own, spec, start,
    };
    use crate::host::managed::wire::{read_frame, Ctl, Frame, Want};
    let (own, _data) = sandbox_own("end-stuck");
    let script = r#"trap 'exit 7' HUP; printf 'ready\r\n'; while :; do read x; done"#;
    let mut s = spec("t-endstuck", script);
    s.client = crate::control::gate_rules::CLIENT_TERMINAL.into();
    let (sock, task) = start(&own, s).await;
    let mut viewer = hello(&sock, Want::View).await;
    read_until_out(&mut viewer, "ready").await;
    let (stuck, listener) = stuck_tmux_socket("hosted");
    let o = own.clone();
    let st = stuck.to_string_lossy().into_owned();
    let got = tokio::task::spawn_blocking(move || {
        end_terminal(
            crate::host::tmux::On { socket: Some(&st) },
            Some(&o),
            &TerminalEndReq {
                terminal: Some("t-endstuck".into()),
                ..TerminalEndReq::default()
            },
        )
    })
    .await
    .expect("线没崩");
    assert_eq!(
        got.map_err(|f| f.code),
        Ok(TerminalEnded {
            terminal: "t-endstuck".into()
        })
    );
    // 一发 tmux 都没起：那台卡住的 tmux 的 socket 上一个连上来的客户端都没有。
    listener.set_nonblocking(true).expect("非阻塞");
    assert!(
        matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
        "结束托管终端去问了那台卡住的 tmux"
    );
    loop {
        match read_frame(&mut viewer).await.expect("读") {
            Some(Frame::Ctl(Ctl::Ended { .. })) | None => break,
            Some(_) => {}
        }
    }
    task.await.expect("任务没崩").expect("跑完");
}

/// ★ 名单里一时没有那个终端 ⇒ 当场重读一次再判：第一次列出来没有它、第二次有 ⇒ 照结束。
#[test]
fn a_terminal_missing_from_one_listing_is_looked_up_again() {
    let iso = Iso::new("relist");
    iso.session("r-cc", "sid-relist");
    let handle = {
        let (rows, _, _) = crate::host::list_with(iso.on(), &|| Default::default()).expect("名单");
        rows.iter()
            .find(|r| r.name == "r-cc")
            .expect("名单里有 r-cc")
            .handle()
    };
    let calls = std::cell::Cell::new(0);
    let list = || {
        calls.set(calls.get() + 1);
        if calls.get() == 1 {
            Ok((vec![], vec![], false))
        } else {
            crate::host::list_with(iso.on(), &|| Default::default())
        }
    };
    let got = end_listed(
        iso.on(),
        None,
        &list,
        &TerminalEndReq {
            terminal: Some(handle.clone()),
            ..TerminalEndReq::default()
        },
    );
    assert_eq!(
        got.map_err(|f| f.code),
        Ok(TerminalEnded { terminal: handle })
    );
    assert_eq!(calls.get(), 2, "重读了一次");
    assert!(iso.id_of("r-cc").is_empty(), "那个会话还在");
}

/// ★ 身份标打不上不判死：打标那一发 tmux 失败一次（假 tmux 的 `set-option` 回 1），
/// 那个会话的名字不像我们铸的、只认 `@ccm_sid` ⇒ 结束时先补打、再判 ⇒ 照结束。
#[test]
fn ending_a_session_whose_identity_tag_failed_once_still_works() {
    let iso = Iso::new("retag");
    let o = iso.tmux(&["-f", "/dev/null", "new-session", "-d", "-s", "mine", "cat"]);
    assert!(
        o.status.success(),
        "建会话失败：{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let pid: u32 = String::from_utf8_lossy(
        &iso.tmux(&["display-message", "-p", "-t", "=mine:", "#{pane_pid}"])
            .stdout,
    )
    .trim()
    .parse()
    .expect("窗格的 pid");
    // 窗格进程 exec 之前 environ 还是 tmux server 那一份（没有 `TMUX_PANE`）：等它换成窗格自己的（有界）。
    let in_pane = || {
        std::fs::read(format!("/proc/{pid}/environ"))
            .is_ok_and(|b| String::from_utf8_lossy(&b).contains("TMUX_PANE="))
    };
    for () in guard_core::wall::rounds() {
        if in_pane() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(in_pane(), "窗格进程的环境里一直没有 TMUX_PANE");
    let dir = iso.sock.parent().expect("目录").to_path_buf();
    let once = dir.join("failed-once");
    let fake = dir.join("tmux.sh");
    std::fs::write(
        &fake,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do if [ \"$a\" = set-option ] && [ ! -e '{once}' ]; then : > '{once}'; exit 1; fi; done\nexec tmux -S '{sock}' \"$@\"\n",
            once = once.display(),
            sock = iso.sock.display()
        ),
    )
    .expect("写假 tmux");
    let _door = crate::control::identity_tag::door::isolate_with(&fake);
    let (_, wrote) = crate::control::identity_tag::tag_seen(pid, "sid-retag-1");
    assert!(!wrote && once.exists(), "打标那一发该失败一次");
    let (rows, _, _) = crate::host::list_with(iso.on(), &|| Default::default()).expect("名单");
    let row = rows
        .iter()
        .find(|r| r.name == "mine")
        .cloned()
        .expect("名单里有 mine");
    assert_eq!(
        end_row(iso.on(), None, &row, None).map_err(|f| f.code),
        Ok(())
    );
    assert!(iso.id_of("mine").is_empty(), "那个会话还在");
}
