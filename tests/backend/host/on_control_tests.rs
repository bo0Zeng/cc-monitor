//! 宿主口判据里要问控制面的那几条（名单那一形 · 持有者那一句 · 终端页的回滚上限）：宿主口是独立的包（`backend-host`）、
//! 够不到上层 ⇒ 挂在 `control` 底下（`control/mod.rs`）。其余宿主口的判据住本目录各份、挂在包里。

use crate::host::managed::attach::held_line;
use crate::host::managed::wire::{Holder, WhoKind};
use crate::host::{list_with, tmux, HeldBy, Input, TermRow};
use crate::wire::TerminalHost;

/// 一行假托管终端（形照 `TermRow`；句柄就是 id）；桌面经后端持有输入。
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
        input: Input::Held(Some(HeldBy::Front("desktop".into()))),
    }
}

/// 名单两种宿主合一份：隔离 socket 上真 tmux 那一行与注入的托管那一行都在，各带自己的 `host` · `name` · `input`（线上 `terminals-list` 那一形）。
#[test]
fn the_list_carries_both_hosts_each_with_its_own_host() {
    let sb = crate::platform::tmux_sandbox::SandboxTmux::new("host-list");
    let sock = sb.out(&["display-message", "-p", "#{socket_path}"]);
    assert!(!sock.is_empty(), "沙箱 tmux 报不出 socket 路径");
    let (rows, clients, complete) = list_with(
        tmux::On {
            socket: Some(&sock),
        },
        &|| (vec![managed_row()], vec![("m-7f3a".to_string(), 10, 20)]),
    )
    .expect("名单");
    let wire = serde_json::to_value(crate::control::terminals::list_reply(
        &rows,
        &clients,
        complete,
        Some("desktop"),
    ))
    .unwrap();
    let got: Vec<(String, String)> = wire["terminals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["host"].as_str().unwrap().to_string(),
                t["name"].as_str().unwrap().to_string(),
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
    let m = &wire["terminals"][1];
    assert_eq!(m["terminal"], "m-7f3a", "托管那一行的句柄就是它的 id");
    assert_eq!(
        m["input"],
        serde_json::json!({"kind": "held", "holder": {"who": "desktop", "mine": true, "said": copy_core::copy_text("beTerminal.holder.here", &[])}}),
        "托管那一行带此刻的持有者与给人看的那一句（这一端持有 ⇒ 输入在此处）：{wire}"
    );
    assert_eq!(
        m["clients"].as_array().map(Vec::len),
        Some(1),
        "连着的真终端进名单：{wire}"
    );
    assert_eq!(m["can"]["input"], true, "托管的全是我们起的：不跑名字规则");
    assert_eq!(
        wire["terminals"][0]["input"],
        serde_json::json!({"kind": "shared"})
    );
    assert!(
        wire["terminals"][0]["terminal"]
            .as_str()
            .unwrap()
            .starts_with("tmux-"),
        "tmux 那一行的句柄照旧：{wire}"
    );
}

/// 瘦客户端底行与名单那一格同一句：无人持有 ⇒ 底行前半句与 `terminals-list` 的 `holder.said` 逐字相同；
/// 终端窗口持有（看的这一端不是它）⇒ 同。
#[test]
fn the_bottom_line_starts_with_the_same_holder_sentence_as_the_list() {
    use crate::control::terminals::input_of_row;
    use cmd_wire::session::TermInput;
    let listed = |input: Input| match input_of_row(&input, Some("desktop")) {
        TermInput::Held { holder } => holder.said,
        TermInput::Shared => panic!("托管那一行是 held"),
    };
    let none = held_line(None);
    assert!(none.starts_with(&listed(Input::Held(None))), "{none:?}");
    let other = Holder {
        client: 3,
        kind: WhoKind::Terminal,
        front: None,
        pid: None,
        mine: false,
    };
    let terminal = listed(Input::Held(Some(HeldBy::Terminal)));
    assert!(held_line(Some(&other)).starts_with(&terminal));
}

#[test]
fn the_scrollback_constant_matches_the_terminal_page_limit() {
    assert_eq!(
        crate::host::managed::screen::SCROLLBACK,
        crate::control::terminals::MAX_SCROLLBACK as usize
    );
}
