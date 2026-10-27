use super::*;
use crate::wire::TerminalHost;

/// 一行假托管终端（形照 `TermRow`；句柄就是 id）。
fn managed_row() -> TermRow {
    TermRow {
        host: TerminalHost::Managed,
        id: "m-7f3a".into(),
        pane: None,
        shared: false,
        name: "proj".into(),
        windows: 1,
        activity: 5,
        sid: String::new(),
        agent: String::new(),
        client: String::new(),
        program: "sh".into(),
        cwd: "/w".into(),
        title: String::new(),
    }
}

/// 名单两种宿主合一份：隔离 socket 上真 tmux 那一行与注入的托管那一行都在，各带自己的 `host`（线上 `terminals-list` 那一形）。
#[test]
fn the_list_carries_both_hosts_each_with_its_own_host() {
    let sb = crate::tmux_sandbox::SandboxTmux::new("host-list");
    let sock = sb.out(&["display-message", "-p", "#{socket_path}"]);
    assert!(!sock.is_empty(), "沙箱 tmux 报不出 socket 路径");
    let (rows, clients, complete) = list_with(
        tmux::On {
            socket: Some(&sock),
        },
        &|| vec![managed_row()],
    )
    .expect("名单");
    let wire = serde_json::to_value(crate::control::terminals::list_reply(
        &rows, &clients, complete, None,
    ))
    .unwrap();
    let got: Vec<(String, String)> = wire["terminals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["host"].as_str().unwrap().to_string(),
                t["tmux_name"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("tmux".to_string(), "keep".to_string()),
            ("managed".to_string(), "proj".to_string()),
        ],
        "名单该是隔离 server 上的 `keep` 那一行 ＋ 托管那一行：{wire}"
    );
    assert_eq!(
        wire["terminals"][1]["terminal"], "m-7f3a",
        "托管那一行的句柄就是它的 id"
    );
    assert!(
        wire["terminals"][0]["terminal"]
            .as_str()
            .unwrap()
            .starts_with("tmux-"),
        "tmux 那一行的句柄照旧：{wire}"
    );
}

/// 生产的托管那一臂这一版是空名单（没有托管进程）。
#[test]
fn the_managed_arm_lists_nothing_yet() {
    assert!(managed::rows().is_empty());
}
