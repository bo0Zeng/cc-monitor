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
