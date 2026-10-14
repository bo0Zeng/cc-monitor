//! 机器 · 链路 · 总线 · 别名那几张命令表换成 typed 之后：壳（Rust）读的那几条回包线上字节不变，入参按类型收。
//!
//! 字节那几条：换形之前用旧代码跑出来，钉成字面量；换形之后逐字节比（经 `serde_json::Value` 那一跳，同帧面）。

use serde_json::json;

/// 帧面那一跳的写法：先成 `Value`、再成串（键按字典序，同 `spec::wire`）。
fn wire_of(v: &impl serde::Serialize) -> String {
    serde_json::to_string(&serde_json::to_value(v).unwrap()).unwrap()
}

/// 开终端那一行里认 cc-monitor 那份 known_hosts 的那一段（测试进程的家目录下，POSIX 那一形）。
fn known_hosts_posix() -> String {
    crate::dial::terminal::known_hosts_arg(crate::dial::known_hosts::path().as_deref(), false)
}

/// `terminal-ssh`（壳的「在此打开终端」与主界面都读 `command` 那一格）：带跳板 · 上次赢的那条 · 钥匙。
#[test]
fn the_terminal_line_the_shell_reads_keeps_its_wire_bytes() {
    let args = json!({
        "machine": { "host": "192.0.2.5", "user": "dev", "port": 2222, "keyPath": "/k/id", "label": "box",
                     "addresses": ["192.0.2.6:2200"], "jump": "hop", "resumeCommand": "", "connect": true },
        "saved": null,
        "jump": { "host": "hop.example", "user": "j", "port": 22, "label": "hop" },
        "prefer": { "host": "192.0.2.6", "port": 2200 },
        "command": "claude --resume abc",
    });
    let req = crate::wire::typed::take(&args).unwrap();
    let got = crate::dial::terminal::answer_in(
        &req,
        false,
        &crate::platform::ssh_client::SshClient::At("/usr/bin/ssh".into()),
    )
    .unwrap();
    assert_eq!(
        wire_of(&got),
        format!(
            r#"{{"command":"'/usr/bin/ssh' -t -J j@hop.example -p 2200 -i '/k/id'{} dev@192.0.2.6 -- 'bash -lic '\\''claude --resume abc'\\'''"}}"#,
            known_hosts_posix()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        )
    );
}

/// `remote-reach`（壳在每台远端握手那一刻登记）：回 `{origin, reach}`，可达表每行 `{origin, machine}`。
#[test]
fn the_reach_table_the_shell_reads_keeps_its_wire_bytes() {
    let table = crate::dial::remote_ask::Table::default();
    let dial = json!({ "machine": { "host": "h", "user": "u", "label": "m1" }, "saved": null, "jump": null,
                       "prefer": null, "use": "capture", "agent_sock": null });
    let got = crate::dial::remote_ask::answer_reach_with(
        &crate::wire::typed::take(&json!({ "origin": "m1", "dial": dial })).unwrap(),
        &table,
    )
    .unwrap();
    assert_eq!(
        wire_of(&got),
        r#"{"origin":"m1","reach":[{"machine":null,"origin":"m1"}]}"#
    );
    let got = crate::dial::remote_ask::answer_reach_with(
        &crate::wire::typed::take(&json!({ "origin": "m0", "dial": dial })).unwrap(),
        &table,
    )
    .unwrap();
    assert_eq!(
        wire_of(&got),
        r#"{"origin":"m0","reach":[{"machine":null,"origin":"m0"},{"machine":null,"origin":"m1"}]}"#
    );
}

/// 走登记表跑一条（同分派那一层）：阻塞档直接跑，异步档在一个当场起的运行时上等它。
fn run_any(cmd: &str, args: serde_json::Value) -> super::spec::Outcome {
    use super::spec::{prepare, Prepared};
    let spec = super::lookup(cmd).unwrap_or_else(|| panic!("`{cmd}` 不在登记表里"));
    let req = crate::wire::Request {
        id: "t".to_string(),
        cmd: cmd.to_string(),
        args,
        within_ms: None,
        view: serde_json::Value::Null,
        tz: Default::default(),
        until: None,
    };
    match prepare(spec.run, Some(&crate::platform::fs::Own::for_tests())) {
        Some(Prepared::Blocking(f)) => f(req),
        Some(Prepared::Async(f)) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(f(req)),
        None => panic!("`{cmd}` 是内建档"),
    }
}

/// 机器那一族的入参按类型收：多一格 · 一格类型不对 ⇒ `bad_args`，在解入参那一步就回（碰盘 · 拨号 · 起进程之前）。
/// 那台的配置那一格（`machine` · `saved` · `jump`）是配置文件里的一条，多出来的键照收（界面那份多带 `resumeCommand` 等），外层照拒。
#[test]
fn the_machine_family_refuses_what_its_request_types_do_not_have() {
    let m = json!({ "host": "192.0.2.9", "user": "u", "resumeCommand": "", "connect": true });
    for (cmd, args) in [
        (
            "terminal-ssh",
            json!({ "machine": m, "command": "x", "extra": 1 }),
        ),
        ("terminal-ssh", json!({ "machine": m, "command": 7 })),
        (
            "terminal-ssh",
            json!({ "machine": m, "command": "x", "prefer": { "host": "h", "port": 22, "x": 1 } }),
        ),
        ("pubkey-push", json!({ "machine": m, "extra": 1 })),
        ("pubkey-push", json!({ "machine": m, "pubKeyPath": 3 })),
        (
            "authorized-keys-add",
            json!({ "key": "ssh-ed25519 AAAA", "extra": 1 }),
        ),
        (
            "forward-start",
            json!({ "origin": "dev", "localPort": 1, "remoteHost": "h", "remotePort": 2, "extra": 1 }),
        ),
        (
            "forward-start",
            json!({ "origin": "dev", "localPort": 70000, "remoteHost": "h", "remotePort": 2 }),
        ),
        ("forward-stop", json!({ "id": "fwd-1", "extra": 1 })),
        ("forward-list", json!({ "extra": 1 })),
        (
            "remote-reach",
            json!({ "origin": "dev", "dial": { "machine": m }, "extra": 1 }),
        ),
        (
            "remote-reach",
            json!({ "origin": "dev", "dial": { "machine": m, "backendPath": "/x" } }),
        ),
        ("ssh-config-import", json!({ "known": [], "extra": 1 })),
        (
            "ssh-config-import",
            json!({ "known": [{ "host": "h", "user": "u", "port": 22, "label": "x" }] }),
        ),
        ("bus-list", json!({ "extra": 1 })),
        ("bus-state", json!({ "extra": 1 })),
        ("bus-send", json!({ "to": "a_cc", "text": "x", "extra": 1 })),
        ("bus-send", json!({ "to": "a_cc", "text": 3 })),
        ("bus-broadcast", json!({ "text": "x", "extra": 1 })),
        ("bus-kill", json!({ "id": "a_cc", "extra": 1 })),
        ("bus-inbox", json!({ "id": "a_cc", "lines": "200" })),
        (
            "bus-spawn",
            json!({ "dir": "/w", "base": true, "extra": 1 }),
        ),
        ("bus-spawn", json!({ "dir": "/w", "base": "yes" })),
        (
            "aliases-read",
            json!({ "shell": "posix", "rcPath": null, "extra": 1 }),
        ),
        ("aliases-read", json!({ "shell": "fish" })),
        (
            "aliases-block-render",
            json!({ "rcPath": "~/.bashrc", "extra": 1 }),
        ),
        (
            "aliases-block-install",
            json!({ "rcPath": "~/.bashrc", "extra": 1 }),
        ),
        ("aliases-block-remove", json!({ "rcPath": 7 })),
        ("powershell-policy-set", json!({ "host": "cmd" })),
        ("profiles-read", json!({ "extra": 1 })),
        ("profiles-resolve", json!({ "name": "cc", "extra": 1 })),
        ("profiles-resolve", json!({ "name": 3 })),
        (
            "profiles-resolve",
            json!({ "name": "cc", "edit": { "name": "cc", "tmux-x": 1 } }),
        ),
        ("profiles-impact", json!({ "changes": [], "extra": 1 })),
        ("profiles-impact", json!({ "changes": [{ "op": "drop" }] })),
        ("profiles-bases", json!({ "name": "cc", "extra": 1 })),
        // 指纹必给（字符串或 `null`）：缺了不当 `null`。
        ("profiles-write", json!({ "changes": [] })),
        ("profiles-write", json!({ "changes": [], "fingerprint": 3 })),
    ] {
        let got = run_any(cmd, args.clone());
        assert_eq!(
            got.as_ref().err().map(|f| f.code.as_str()),
            Some("bad_args"),
            "`{cmd}` 入参 {args} 该回 bad_args：{got:?}"
        );
    }
    // 那台的配置那一格多带的键照收：入参解得出、走到组请求那一步（跳板查无 ⇒ `bad_jump`，不是 `bad_args`）。
    let got = run_any(
        "terminal-ssh",
        json!({ "machine": { "host": "h", "user": "u", "jump": "hop", "resumeCommand": "x" }, "command": "x" }),
    );
    assert_eq!(got.err().map(|f| f.code), Some("bad_jump".to_string()));
}

/// 壳读的部署那几条（`deploy-plan` · `place-verdict` · `exit-policy-read` · `ccm-probe`）：回包字节照旧（换形前用旧代码钉的字面量）。
#[test]
fn the_deploy_replies_the_shell_reads_keep_their_wire_bytes() {
    use crate::control::deploy_plan as dp;
    let plan = |action| dp::Plan {
        key: deploy_contract::key_of("Linux", "x86_64").unwrap(),
        expected: "p9a-mine".into(),
        action,
        leftovers: vec![".cc-monitor/bin/ccm.1-2-3.tmp".into()],
        ack: json!({ "ok": true, "fingerprints": { "192.0.2.2:22": "SHA256:t" }, "jump_fingerprints": {}, "strict": false, "jump_strict": false }),
    };
    assert_eq!(
        wire_of(&dp::plan_json(&plan(deploy_contract::DeployAction::Keep {
            theirs: "p9b".into(),
            why: "w".into()
        }))),
        r#"{"ack":{"fingerprints":{"192.0.2.2:22":"SHA256:t"},"jump_fingerprints":{},"jump_strict":false,"ok":true,"strict":false},"action":"keep","arch":"x86_64","expected":"p9a-mine","label":"Linux / x86_64","leftovers":[".cc-monitor/bin/ccm.1-2-3.tmp"],"os":"Linux","theirs":"p9b","why":"w"}"#
    );
    assert_eq!(
        wire_of(&dp::plan_json(&plan(deploy_contract::DeployAction::Skip))),
        r#"{"ack":{"fingerprints":{"192.0.2.2:22":"SHA256:t"},"jump_fingerprints":{},"jump_strict":false,"ok":true,"strict":false},"action":"skip","arch":"x86_64","expected":"p9a-mine","label":"Linux / x86_64","leftovers":[".cc-monitor/bin/ccm.1-2-3.tmp"],"os":"Linux","theirs":null,"why":""}"#
    );
    // 本机那一份放不放：落点不在 ⇒ 放。
    let got = run_any(
        "place-verdict",
        json!({ "dest": "/nonexistent-ccm-dest/ccm", "machine": copy_core::local_machine() }),
    )
    .unwrap();
    assert_eq!(wire_of(&got), r#"{"action":"place","why":"该机无后端"}"#);
    // 退出行为：沙箱家目录里没那份文件 ⇒ `absent`。
    let got = run_any("exit-policy-read", json!({})).unwrap();
    let path = crate::control::exit_policy::policy_path()
        .unwrap()
        .display()
        .to_string();
    assert_eq!(
        wire_of(&got).replace(&path, "<PATH>"),
        r#"{"detail":null,"killOnExit":false,"path":"<PATH>","reason":null,"said":"关 = 不主动停止 · 掉线后自行退出","state":"absent"}"#
    );
    // 这台的 `ccm` 会哪些。
    let got = run_any("ccm-probe", json!({})).unwrap();
    assert_eq!(
        wire_of(&got).replace(crate::BUILD_ID, "<BUILD>"),
        r#"{"agents":["claude","codex"],"build":"<BUILD>","capabilities":["new","resume","attach","tmux","account","model","cwd","agent","launcher","ccm-sid","print","detach","tmux-size","tmux-base","bus-register","backend-discover","account-via-backend","base-url-across-tmux"],"version":"7"}"#
    );
}
