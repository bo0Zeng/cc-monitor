//! 机器 · 链路 · 总线 · 别名那几张命令表换成 typed 之后：壳（Rust）读的那几条回包线上字节不变，入参按类型收。
//!
//! 字节那几条：换形之前用旧代码跑出来，钉成字面量；换形之后逐字节比（经 `serde_json::Value` 那一跳，同帧面）。

use serde_json::json;

/// 帧面那一跳的写法：先成 `Value`、再成串（键按字典序，同 `spec::wire`）。
fn wire_of(v: &impl serde::Serialize) -> String {
    serde_json::to_string(&serde_json::to_value(v).unwrap()).unwrap()
}

/// 开终端那一行里 `-F` 那一段（测试档没装写句柄 ⇒ 不带，POSIX 那一形）。
fn known_hosts_posix() -> String {
    crate::dial::terminal::config_arg(
        crate::dial::known_hosts::terminal_config().as_deref(),
        false,
    )
}

/// `terminal-ssh`（壳的「在此打开终端」与主界面都读 `prelude` · `line` 两格）：只交名字；跳板 · 地址次序 · 钥匙都照这一端的机器表。
#[test]
fn the_terminal_line_the_shell_reads_keeps_its_wire_bytes() {
    let hosts: Vec<crate::dial::Host> = [
        json!({ "host": "192.0.2.5", "user": "dev", "port": 2222, "keyPath": "/k/id", "label": "box-term",
                "addresses": ["192.0.2.6:2200"], "jump": "hop", "connect": true }),
        json!({ "host": "hop.example", "user": "j", "port": 22, "label": "hop" }),
    ]
    .iter()
    .map(|v| crate::dial::machine::loose(v).unwrap())
    .collect();
    let args = json!({ "machine": "box-term", "command": "claude --resume abc" });
    let req = crate::wire::typed::take(&args).unwrap();
    let got = crate::dial::terminal::answer_with(
        &req,
        &hosts,
        &[],
        false,
        &crate::platform::ssh_client::SshClient::At("/usr/bin/ssh".into()),
        None,
    )
    .unwrap();
    assert_eq!(
        wire_of(&got),
        format!(
            r#"{{"line":"'/usr/bin/ssh' -t -J j@hop.example -p 2222 -i '/k/id'{} dev@192.0.2.5 -- 'bash -lic '\\''claude --resume abc'\\'''","prelude":""}}"#,
            known_hosts_posix()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        )
    );
}

/// 走登记表跑一条（同分派那一层）：阻塞档直接跑，异步档在一个当场起的运行时上等它。
fn run_any(cmd: &str, args: serde_json::Value) -> super::spec::Outcome {
    use super::spec::{invoke, Prepared};
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
    match invoke(
        spec.run,
        &crate::stream::inbound::hands(Some(&crate::platform::fs::Own::for_tests())),
    ) {
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
            json!({ "machine": "dev", "command": "x", "extra": 1 }),
        ),
        ("terminal-ssh", json!({ "machine": "dev", "command": 7 })),
        // 出口只交机器名：整台配置 · 线上自带「上次赢的那条」⇒ 拒。
        ("terminal-ssh", json!({ "machine": m, "command": "x" })),
        (
            "terminal-ssh",
            json!({ "machine": "dev", "command": "x", "prefer": { "host": "h", "port": 22 } }),
        ),
        (
            "assets-sync",
            json!({ "origin": "dev", "dial": { "machine": m } }),
        ),
        ("pubkey-push", json!({ "machine": m, "extra": 1 })),
        ("pubkey-push", json!({ "machine": m, "jump": m })),
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
        (
            "forward-start",
            json!({ "origin": "dev", "localPort": 1, "remoteHost": "h", "remotePort": 2, "machine": m }),
        ),
        ("forward-stop", json!({ "id": "fwd-1", "extra": 1 })),
        ("forward-list", json!({ "extra": 1 })),
        ("ssh-config-import", json!({ "known": [], "extra": 1 })),
        (
            "ssh-config-import",
            json!({ "known": [{ "host": "h", "user": "u", "port": 22, "label": "x" }] }),
        ),
        ("bus-list", json!({ "extra": 1 })),
        (
            "bus-send",
            json!({ "from": "a", "to": "b", "act": "ask", "title": "t", "extra": 1 }),
        ),
        (
            "bus-send",
            json!({ "from": "a", "to": "b", "act": "ask", "title": 3 }),
        ),
        ("bus-inbox", json!({ "id": "a_cc", "lines": "200" })),
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
        // 版本 `ifRev` 必给（字符串或 `null`）：缺了不当 `null`。
        ("profiles-write", json!({ "changes": [] })),
    ] {
        let got = run_any(cmd, args.clone());
        assert_eq!(
            got.as_ref().err().map(|f| f.code.as_str()),
            Some("bad_args"),
            "`{cmd}` 入参 {args} 该回 bad_args：{got:?}"
        );
    }
    // 版本给错了类型 ⇒ `bad_args`，拒的原因是那一格的类型（不是「多了个不认识的键」：键名对得上才测得到类型）。
    let got = run_any("profiles-write", json!({ "changes": [], "ifRev": 3 }));
    let f = got.err().expect("版本给成数该拒");
    assert_eq!(f.code, "bad_args");
    assert!(
        f.raw.as_deref() == Some("`ifRev`: expected a string"),
        "该拒在版本的类型上：{:?}",
        f.raw
    );
    // 还没存的那一台（推公钥测的那一台）多带的键照收：入参解得出、走到组请求那一步（跳板照表查无 ⇒ `bad_jump`，不是 `bad_args`）。
    let got = run_any(
        "pubkey-push",
        json!({ "machine": { "host": "h", "user": "u", "jump": "hop", "resumeCommand": "x" } }),
    );
    assert_eq!(got.err().map(|f| f.code), Some("bad_jump".to_string()));
}

/// 壳读的部署那几条（`deploy-install` · `place-verdict` · `exit-policy-read` · `ccm-probe`）：回包字节照钉的字面量。
#[test]
fn the_deploy_replies_the_shell_reads_keep_their_wire_bytes() {
    use crate::control::deploy_plan as dp;
    let plan = |action| dp::Plan {
        key: deploy_contract::key_of("Linux", "x86_64").unwrap(),
        expected: "p9a-mine".into(),
        action,
        leftovers: vec![],
    };
    assert_eq!(
        wire_of(&dp::installed_of(&plan(
            deploy_contract::DeployAction::Keep {
                theirs: "p9b".into(),
                why: "w".into()
            }
        ))),
        r#"{"action":"keep","build":"p9b","os":"Linux","said":"w"}"#
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
        r#"{"detail":null,"killOnExit":false,"path":"<PATH>","reason":null,"said":"关 = 不主动停止 · 连接断开后自行退出","state":"absent"}"#
    );
    // 这台的 `ccm` 会哪些（终端里敲它走到哪那一格随这台的 shell 变，只核形状）。
    let mut got = run_any("ccm-probe", json!({})).unwrap().expect("回包");
    let here = got
        .as_object_mut()
        .unwrap()
        .remove("onPath")
        .expect("onPath");
    assert_eq!(
        here.as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>()),
        Some(vec!["said".to_string(), "verdict".to_string()])
    );
    assert_eq!(
        wire_of(&got).replace(crate::BUILD_ID, "<BUILD>"),
        r#"{"agents":["claude","codex"],"build":"<BUILD>","capabilities":["new","resume","attach","tmux","account","model","cwd","agent","launcher","ccm-sid","print","detach","tmux-size","tmux-base","backend-discover","account-via-backend","base-url-across-tmux"],"version":"7"}"#
    );
}
