//! 命令面板打机器名：`machine-find` 认那台（名字 · 本机那个词 · ssh 配置的 Host · 名字后跟第几条），
//! `machine-menu` 把那台的成品与这台的事实拼成几样动作（哪样灰、灰着说什么）。

use super::*;
use cmd_wire::machine::{MachineState, MachineStateKind};

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn find(q: &str) -> Option<MachineHit> {
    find_in(
        q,
        &names(&["nas", "gpu-01", "devbox", "Lab"]),
        &names(&["backup", "nas", "web*"]),
    )
}

fn hit(machine: &str, from: MachineFrom, nth: Option<u32>) -> Option<MachineHit> {
    Some(MachineHit {
        machine: cmd_wire::Machine::named(machine),
        from,
        nth,
    })
}

#[test]
fn a_name_typed_exactly_is_that_machine() {
    assert_eq!(find("nas"), hit("nas", MachineFrom::Table, None));
    assert_eq!(find("  devbox "), hit("devbox", MachineFrom::Table, None));
    // 大小写不敏感，交回的是表里那一份写法。
    assert_eq!(find("lab"), hit("Lab", MachineFrom::Table, None));
    assert_eq!(find("na"), None, "前缀不算");
    assert_eq!(find(""), None);
}

#[test]
fn the_local_word_is_this_machine() {
    let word = copy_core::local_machine();
    assert_eq!(
        find(&word),
        Some(MachineHit {
            machine: cmd_wire::Machine::local(),
            from: MachineFrom::Local,
            nth: None
        })
    );
    assert_eq!(
        find(&format!("{word}2")).map(|h| h.nth),
        Some(Some(2)),
        "本机也认第几条"
    );
}

#[test]
fn a_trailing_number_picks_the_nth_session() {
    assert_eq!(find("nas2"), hit("nas", MachineFrom::Table, Some(2)));
    assert_eq!(find("nas 2"), hit("nas", MachineFrom::Table, Some(2)));
    assert_eq!(find("nas12"), hit("nas", MachineFrom::Table, Some(12)));
    // 名字本身以数字结尾 ⇒ 整串先当名字认。
    assert_eq!(find("gpu-01"), hit("gpu-01", MachineFrom::Table, None));
    // 没有叫 gpu-012 的 ⇒ 读成 gpu-01 的第 2 条。
    assert_eq!(find("gpu-012"), hit("gpu-01", MachineFrom::Table, Some(2)));
    assert_eq!(find("nas0"), None, "第 0 条不存在");
    assert_eq!(find("foo2"), None, "切不开 ⇒ 不猜");
}

#[test]
fn an_ssh_config_host_counts_only_when_the_table_does_not_have_it() {
    assert_eq!(find("backup"), hit("backup", MachineFrom::SshConfig, None));
    // 同名以机器表为准。
    assert_eq!(find("nas"), hit("nas", MachineFrom::Table, None));
    // 通配不认；ssh 配置那一种没有会话，不切第几条。
    assert_eq!(find("web*"), None);
    assert_eq!(find("backup2"), None);
}

fn state(kind: MachineStateKind, offline: bool, word: &str) -> MachineState {
    MachineState {
        name: "nas".into(),
        state: kind,
        offline,
        reason: None,
        stage: None,
        version: None,
        version_relation: None,
        os: None,
        word: word.into(),
        said: String::new(),
        offline_said: "nas 离线".into(),
        fixes: Vec::new(),
        seen_host_key: None,
        expected_host_key: None,
        detail: None,
        unclear: None,
    }
}

fn act(m: &MachineMenu, a: MenuAct) -> Option<&MenuAction> {
    m.actions.iter().find(|x| x.act == a)
}

fn ssh() -> SshClient {
    SshClient::At("/usr/bin/ssh".into())
}

#[test]
fn a_connected_machine_offers_every_action() {
    let m = menu_in(
        "nas",
        MachineFrom::Table,
        Some(state(MachineStateKind::Up, false, "")),
        &ssh(),
        false,
    );
    let acts: Vec<MenuAct> = m.actions.iter().map(|a| a.act).collect();
    assert_eq!(
        acts,
        vec![
            MenuAct::OpenTerminal,
            MenuAct::NewSession,
            MenuAct::Files,
            MenuAct::MachineStatus
        ]
    );
    assert!(m.actions.iter().all(|a| a.blocked.is_none()));
    assert!(act(&m, MenuAct::OpenTerminal).unwrap().note.is_none());
    assert!(m.head.is_some());
}

#[test]
fn offline_keeps_the_terminal_and_grays_what_needs_the_backend() {
    let m = menu_in(
        "nas",
        MachineFrom::Table,
        Some(state(MachineStateKind::Down, true, "离线")),
        &ssh(),
        false,
    );
    let t = act(&m, MenuAct::OpenTerminal).unwrap();
    assert!(t.blocked.is_none(), "终端照开");
    assert!(t.note.is_some(), "照开时说一句连接失败");
    for a in [MenuAct::NewSession, MenuAct::Files] {
        let said = act(&m, a).unwrap().blocked.clone().expect("灰");
        assert!(said.contains("nas") && said.contains("离线"), "{said}");
    }
    assert!(act(&m, MenuAct::MachineStatus).unwrap().blocked.is_none());
}

#[test]
fn a_changed_host_key_grays_the_terminal_too() {
    let m = menu_in(
        "nas",
        MachineFrom::Table,
        Some(state(MachineStateKind::HostKeyChanged, true, "指纹变更")),
        &ssh(),
        false,
    );
    assert!(act(&m, MenuAct::OpenTerminal).unwrap().blocked.is_some());
    assert!(act(&m, MenuAct::NewSession).unwrap().blocked.is_some());
}

#[test]
fn needs_update_grays_only_new_session() {
    let m = menu_in(
        "nas",
        MachineFrom::Table,
        Some(state(MachineStateKind::NeedsUpdate, false, "要更新")),
        &ssh(),
        false,
    );
    assert!(act(&m, MenuAct::NewSession).unwrap().blocked.is_some());
    assert!(act(&m, MenuAct::Files).unwrap().blocked.is_none());
    assert!(act(&m, MenuAct::OpenTerminal).unwrap().blocked.is_none());
}

#[test]
fn no_ssh_client_grays_the_remote_terminal_but_not_the_local_one() {
    let remote = menu_in(
        "nas",
        MachineFrom::Table,
        Some(state(MachineStateKind::Up, false, "")),
        &SshClient::Missing,
        false,
    );
    assert!(act(&remote, MenuAct::OpenTerminal).unwrap().blocked.is_some());
    let local = menu_in(
        cmd_wire::machine::LOCAL,
        MachineFrom::Local,
        Some(state(MachineStateKind::Up, false, "")),
        &SshClient::Missing,
        false,
    );
    assert!(act(&local, MenuAct::OpenTerminal).unwrap().blocked.is_none());
    // 本机没有文件窗口 ⇒ 这一样不列。
    assert!(act(&local, MenuAct::Files).is_none());
}

#[test]
fn an_ssh_config_host_offers_only_terminal_and_adding_it() {
    let m = menu_in("backup", MachineFrom::SshConfig, None, &ssh(), false);
    let acts: Vec<MenuAct> = m.actions.iter().map(|a| a.act).collect();
    assert_eq!(acts, vec![MenuAct::OpenTerminal, MenuAct::AddMachine]);
    assert!(m.head.is_none());
}
