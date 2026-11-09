//! 阻塞档命令的总期限归发起方：信封里带的期限减余量装进那条命令；不带 / 带坏了 ⇒ 按后端登记的上限。

use super::*;
use crate::wire::Frame;

/// 信封里那一格宽读：正整数才算带了；别的形状当没带，整条请求照收（不回 `bad_request`）。
#[test]
fn the_envelope_deadline_is_read_leniently_and_never_rejects_the_request() {
    let parse = |extra: &str| -> Option<u64> {
        let line = format!(r#"{{"id":"1","cmd":"terminal-end","args":{{}}{extra}}}"#);
        serde_json::from_str::<Request>(&line)
            .unwrap_or_else(|e| panic!("带 {extra} 的信封被拒了：{e}"))
            .within_ms
    };
    assert_eq!(parse(r#","within_ms":3000"#), Some(3000));
    assert_eq!(parse(""), None);
    for bad in [
        r#","within_ms":"3000""#,
        r#","within_ms":-1"#,
        r#","within_ms":1.5"#,
        r#","within_ms":0"#,
        r#","within_ms":null"#,
        r#","within_ms":1e30"#,
        r#","within_ms":{}"#,
    ] {
        assert_eq!(parse(bad), None, "{bad} 该当没带");
    }
}

/// 登记行上写了总期限上限（`.capped(..)`）的那几条命令。
fn capped() -> Vec<&'static str> {
    REGISTRY
        .iter()
        .filter(|s| s.cap.is_some())
        .map(|s| s.name)
        .collect()
}

/// 装了上限的每一条都在阻塞档（异步档那一路不装：分派那一层装不到它的线程上）。
#[test]
fn every_capped_command_is_a_registered_blocking_command() {
    assert!(
        capped().len() >= 10,
        "装了上限的只有 {} 条 —— 本条在空转",
        capped().len()
    );
    for spec in REGISTRY.iter().filter(|s| s.cap.is_some()) {
        assert!(
            spec.run.blocking(),
            "{} 不在阻塞档：分派那一层装不到它的线程上",
            spec.name
        );
    }
}

/// 装了总期限的命令，到点回的码只有一种（`child_timed_out`），并且登记在它的码表里：发起方（手机 · CLI · 界面）
/// 按这一个码认「后端在我放手之前答了：超时」，不按命令各认一套。
/// 例外只有这五条，各自的超时不整条失败：`aliases-read` / `powershell-policy-set` 落在成品那一格 `policy.error`（逐份说）、
/// `ssh-config-import` 到点交已解析的那几个（尽力而为，见 `dial::ssh_config::SSH_IMPORT_CAP`）、
/// `plan-list` · `plan-read` 照 pb 读不成那一路说（有上一份好的 ⇒ 给它 ＋ 过期那一句；`plan-list` 那个工作区不列；
/// `plan-read` 没有 ⇒ `failed` 带「读计划超时」那一句，界面正文照写 ＋［重试］）。
#[test]
fn every_capped_command_answers_its_timeout_with_the_one_registered_code() {
    const IN_PRODUCT: &[&str] = &[
        "aliases-read",
        "powershell-policy-set",
        "ssh-config-import",
        "plan-list",
        "plan-read",
    ];
    let mut missing = Vec::new();
    for name in &capped() {
        if IN_PRODUCT.contains(name) {
            continue;
        }
        let spec = REGISTRY.iter().find(|s| s.name == *name).expect("登记了");
        if !spec.codes.contains(&crate::platform::child::TIMED_OUT) {
            missing.push(*name);
        }
        for c in spec.codes {
            assert!(
                *c == crate::platform::child::TIMED_OUT || !c.contains("timed_out"),
                "{name} 的码表里有第二种超时码 {c}"
            );
        }
    }
    assert!(
        missing.is_empty(),
        "这几条装了总期限、码表里却没有 child_timed_out：{missing:?}"
    );
    for name in IN_PRODUCT {
        assert!(
            capped().contains(name),
            "例外表里的 {name} 登记行上没写上限了，删掉这一项"
        );
    }
}

/// ★ 开计划页那两问装总期限：pb 期限取 20 秒与发起方剩下的里小的那个（客户端放弃了，后端不再空跑 pb）。
#[test]
fn the_plan_reads_carry_the_pb_deadline_as_their_cap() {
    let pb = crate::platform::child::Deadline::secs(20);
    for name in ["plan-list", "plan-read"] {
        assert_eq!(caps::cap_of(name), Some(pb), "{name} 没装 20 秒的总期限");
    }
}

/// 子进程那一半：假 tmux 放在 PATH 最前（只在这个子进程里），`display-message` 照答、别的每发都卡。
const CAPS_CHILD_MARK: &str = "CCM_CAPS_CHILD";

#[cfg(unix)]
#[test]
#[ignore = "只由 a_stuck_command_answers_by_the_carried_deadline_or_else_by_the_cap 在隔离的子进程里起"]
fn caps_child() {
    if std::env::var(CAPS_CHILD_MARK).is_err() {
        return;
    }
    // 同一批三条一起进读循环：带期限 3 s · 不带 · 带了个坏的。
    let input = concat!(
        r#"{"id":"carried","cmd":"terminal-end","args":{"terminal":"t-stuck"},"within_ms":3000}"#,
        "\n",
        r#"{"id":"absent","cmd":"terminal-end","args":{"terminal":"t-stuck"}}"#,
        "\n",
        r#"{"id":"broken","cmd":"terminal-end","args":{"terminal":"t-stuck"},"within_ms":"3000"}"#,
        "\n",
    );
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let (tx, mut rx) = mpsc::channel(REPLY_CHANNEL_CAPACITY);
        let t0 = std::time::Instant::now();
        let h = spawn(
            std::io::Cursor::new(input.as_bytes().to_vec()),
            tx,
            crate::stream::inbound::WatchDesk::new(|_| {
                tokio::sync::oneshot::channel::<Vec<crate::wire::WatchFrom>>().1
            }),
            crate::platform::fs::Own::for_tests(),
            crate::wire::HelloFlushed::for_tests(),
        );
        h.await.expect("读循环");
        while let Some(f) = rx.recv().await {
            if let Frame::Reply(crate::wire::Reply { id, code, .. }) = f {
                let code = code.unwrap_or_else(|| "ok".into());
                println!("CAPS {id} {code} {}", t0.elapsed().as_millis());
            }
        }
    });
}

/// 每发 tmux 都卡。结束一个终端先列一次名单（`list-panes`，一发 5 s），总期限上限 18 s 这时够不着：
/// 带 3 s 的那条在 3 s − 余量 2 s ＝ 1 s 上回 `child_timed_out`；不带的、带坏了的按那一发自己的 5 s 回。
#[cfg(unix)]
#[test]
fn a_stuck_command_answers_by_the_carried_deadline_or_else_by_the_cap() {
    let dir = std::env::temp_dir().join(format!("ccm-caps-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).expect("建目录");
    let fake = bin.join("tmux");
    std::fs::write(
        &fake,
        "#!/bin/sh\nfor a in \"$@\"; do\n  if [ \"$a\" = display-message ]; then printf '$1\\ts1\\t1\\t\\n'; exit 0; fi\ndone\nexec sleep 30\n",
    )
    .expect("写假 tmux");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let out = std::process::Command::new(std::env::current_exe().expect("测试二进制"))
        .args([
            "stream::inbound::caps_tests::caps_child",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env_clear()
        .env(CAPS_CHILD_MARK, "1")
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("HOME", &dir)
        .output()
        .expect("起子进程");
    let text = String::from_utf8_lossy(&out.stdout);
    let got = |id: &str| -> (String, u64) {
        let line = text
            .lines()
            .find_map(|l| {
                l.split_once(&format!("CAPS {id} "))
                    .map(|(_, r)| r.to_string())
            })
            .unwrap_or_else(|| {
                panic!(
                    "{id} 没回：{text}\n{}",
                    String::from_utf8_lossy(&out.stderr)
                )
            });
        let (code, ms) = line.split_once(' ').expect("码 用时");
        (code.to_string(), ms.trim().parse().expect("毫秒"))
    };
    let _ = std::fs::remove_dir_all(&dir);
    let (code, ms) = got("carried");
    assert_eq!(code, "child_timed_out");
    assert!(
        (900..2_500).contains(&ms),
        "带 3 s 的那条该在 1 s 上回，用了 {ms} ms"
    );
    for id in ["absent", "broken"] {
        let (code, ms) = got(id);
        assert_eq!(code, "child_timed_out", "{id}");
        assert!(
            (4_900..6_500).contains(&ms),
            "{id} 该按那一发 tmux 的 5 s 回（上限 8 s 够不着），用了 {ms} ms"
        );
    }
}
