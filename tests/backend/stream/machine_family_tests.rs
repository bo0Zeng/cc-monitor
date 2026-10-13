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
    let got = crate::dial::terminal::answer_in(
        &args,
        false,
        &crate::platform::ssh_client::SshClient::At("/usr/bin/ssh".into()),
    )
    .unwrap();
    assert_eq!(
        wire_of(&got),
        format!(
            r#"{{"command":"'/usr/bin/ssh' -t -J j@hop.example -p 2200 -i '/k/id'{} dev@192.0.2.6 -- 'bash -lic '\\''claude --resume abc'\\'''"}}"#,
            known_hosts_posix().replace('\\', "\\\\").replace('"', "\\\"")
        )
    );
}

/// `remote-reach`（壳在每台远端握手那一刻登记）：回 `{origin, reach}`，可达表每行 `{origin, machine}`。
#[test]
fn the_reach_table_the_shell_reads_keeps_its_wire_bytes() {
    let table = crate::dial::remote_ask::Table::default();
    let dial = json!({ "machine": { "host": "h", "user": "u", "label": "m1" }, "saved": null, "jump": null,
                       "prefer": null, "use": "capture", "agent_sock": null });
    let got = crate::dial::remote_ask::answer_reach_with(&json!({ "origin": "m1", "dial": dial }), &table)
        .unwrap();
    assert_eq!(wire_of(&got), r#"{"origin":"m1","reach":[{"machine":null,"origin":"m1"}]}"#);
    let got = crate::dial::remote_ask::answer_reach_with(&json!({ "origin": "m0", "dial": dial }), &table)
        .unwrap();
    assert_eq!(
        wire_of(&got),
        r#"{"origin":"m0","reach":[{"machine":null,"origin":"m0"},{"machine":null,"origin":"m1"}]}"#
    );
}
