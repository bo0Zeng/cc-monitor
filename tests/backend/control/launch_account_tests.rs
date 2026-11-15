//! `control/launch_account.rs` 的那份记录（便条 · 账本：临时目录上真写真读、并发写不丢）。挑号的规则表要观测面的账号清单夹具，住 `launch_account_pick_tests.rs`（挂主 crate）。

use super::*;
use crate::history::launch_book::{last_of, FILE_NAME};

// ───────── 记录 ─────────

/// 写句柄：数据目录那一根就是这个临时家（便条目录与记录都落它底下）。
fn own_at(h: &std::path::Path) -> crate::platform::fs::Own {
    crate::platform::fs::Own::issue_data_at(h)
}

fn none_alive(_: u32) -> Option<Option<u64>> {
    None
}

#[test]
fn a_note_is_adopted_into_the_record_and_kept_while_the_process_lives() {
    let h = test_home::scratch("adopt");
    leave_note(&own_at(&h), 4242, Some("x"), false, None, 1_000).unwrap();
    assert!(adopt(&own_at(&h), 4242, "s1", Some(990), &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s1").as_deref(), Some("x"));
    // 同一个进程原地换了 sid（`/clear`）⇒ 新 sid 也记得到。
    assert!(adopt(&own_at(&h), 4242, "s2", Some(990), &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s2").as_deref(), Some("x"));
    // 已经记着同一个号 ⇒ 不再写。
    assert!(!adopt(&own_at(&h), 4242, "s2", Some(990), &none_alive).unwrap());
    assert!(h.join(NOTES_DIR).join("4242.json").exists());
    let _ = std::fs::remove_dir_all(&h);
}

/// 台账与便条都带声明的版本落盘；没有版本的旧台账照第 1 版读。
#[test]
fn the_ledger_and_the_note_carry_their_version() {
    let h = test_home::scratch("ver");
    std::fs::write(h.join(FILE_NAME), r#"{"sessions":{"s0":"old"}}"#).unwrap();
    leave_note(&own_at(&h), 4343, Some("x"), false, None, 1_000).unwrap();
    let note: serde_json::Value =
        serde_json::from_slice(&std::fs::read(h.join(NOTES_DIR).join("4343.json")).unwrap())
            .unwrap();
    assert_eq!(
        note["version"],
        serde_json::json!(1),
        "便条没带版本：{note}"
    );
    assert!(adopt(&own_at(&h), 4343, "s1", Some(990), &none_alive).unwrap());
    let book: serde_json::Value =
        serde_json::from_slice(&std::fs::read(h.join(FILE_NAME)).unwrap()).unwrap();
    assert_eq!(
        book["version"],
        serde_json::json!(1),
        "台账没带版本：{book}"
    );
    assert_eq!(last_of(Some(&h), "s0").as_deref(), Some("old"));
    assert_eq!(last_of(Some(&h), "s1").as_deref(), Some("x"));
    let _ = std::fs::remove_dir_all(&h);
}

/// 起会话时给了 `--tab`（别名里写的带别名名）⇒ 便条带着它，认便条时记进启动台账的 `tabs`；没有号也照记（只为这一格留的便条）。
#[test]
fn a_note_carries_the_tab_into_the_launch_ledger() {
    let h = test_home::scratch("tab");
    let tab = LaunchTab {
        tab: cmd_wire::session::Tab::Hide,
        alias: Some("cct".into()),
    };
    leave_note(&own_at(&h), 77, None, false, Some(tab.clone()), 1_000).unwrap();
    assert!(adopt(&own_at(&h), 77, "s-tab", Some(990), &none_alive).unwrap());
    let book = read_book(&h).unwrap();
    assert_eq!(book.tabs.get("s-tab"), Some(&tab));
    assert_eq!(book.sessions.get("s-tab"), None, "没有号的便条记了一个号");
    // 已经记着同一格 ⇒ 不再写。
    assert!(!adopt(&own_at(&h), 77, "s-tab", Some(990), &none_alive).unwrap());
    // 没写 `--tab` 的那一份台账，盘上不多一格。
    leave_note(&own_at(&h), 78, Some("x"), false, None, 1_000).unwrap();
    assert!(adopt(&own_at(&h), 78, "s-plain", Some(990), &none_alive).unwrap());
    assert_eq!(read_book(&h).unwrap().tabs.get("s-plain"), None);
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn no_note_no_record() {
    let h = test_home::scratch("nonote");
    assert!(!adopt(&own_at(&h), 7, "s1", None, &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s1"), None);
    assert!(!h.join(FILE_NAME).exists());
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn a_note_older_than_the_process_is_a_reused_pid_and_does_not_count() {
    let h = test_home::scratch("reused");
    leave_note(&own_at(&h), 9, Some("x"), false, None, 1_000).unwrap();
    assert!(!adopt(&own_at(&h), 9, "s1", Some(1_001), &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s1"), None);
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn stale_notes_of_other_processes_are_swept() {
    let h = test_home::scratch("sweep");
    leave_note(&own_at(&h), 11, Some("x"), false, None, 1_000).unwrap(); // 进程不在了
    leave_note(&own_at(&h), 12, Some("x"), false, None, 1_000).unwrap(); // 在，但比便条晚起（pid 被复用）
    leave_note(&own_at(&h), 13, Some("x"), false, None, 1_000).unwrap(); // 在、对得上
    leave_note(&own_at(&h), 14, Some("x"), false, None, 1_000).unwrap(); // 在、起始时刻说不出
    let alive = |p: u32| match p {
        12 => Some(Some(1_005)),
        13 => Some(Some(999)),
        14 => Some(None),
        _ => None,
    };
    adopt(&own_at(&h), 99, "s", None, &alive).unwrap();
    let left: Vec<u32> = [11, 12, 13, 14]
        .into_iter()
        .filter(|p| h.join(NOTES_DIR).join(format!("{p}.json")).exists())
        .collect();
    assert_eq!(left, vec![13, 14]);
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn an_unreadable_record_is_never_overwritten() {
    let h = test_home::scratch("unreadable");
    std::fs::write(h.join(FILE_NAME), "{坏").unwrap();
    leave_note(&own_at(&h), 5, Some("x"), false, None, 1_000).unwrap();
    assert!(adopt(&own_at(&h), 5, "s1", None, &none_alive).is_err());
    assert_eq!(std::fs::read_to_string(h.join(FILE_NAME)).unwrap(), "{坏");
    let _ = std::fs::remove_dir_all(&h);
}

/// 两个进程同时记（这里用线程，锁是跨进程那一把、线程间同样互斥）⇒ 一条都不丢。
#[test]
fn concurrent_records_all_land() {
    let h = test_home::scratch("concurrent");
    const N: u32 = 24;
    std::thread::scope(|s| {
        for i in 0..N {
            let h = h.clone();
            s.spawn(move || {
                leave_note(&own_at(&h), 50_000 + i, Some("x"), false, None, 1_000).unwrap();
                adopt(&own_at(&h), 50_000 + i, &format!("s{i}"), None, &|_| {
                    Some(None)
                })
                .unwrap();
            });
        }
    });
    let book = read_book(&h).unwrap();
    assert_eq!(
        book.sessions.len(),
        N as usize,
        "并发写丢了几条：{:?}",
        book.sessions.keys()
    );
    let _ = std::fs::remove_dir_all(&h);
}

// ───────── 选不了号那一句 ─────────

/// 选不了号那一句由这里写全：跟随 / 点名 × 有替代 / 没替代 × 清单读不出，再加别家的号 × 有替代 / 没替代，七种各一句、互不相同；
/// 都带要的号；有替代 ⇒ 带替代的名字；清单读不出 ⇒ 不提替代（那台说不出谁能用）；别家的号 ⇒ 说出是哪一家的（那一家的叫法，不露 kind）。
#[test]
fn each_unavailable_shape_has_its_own_sentence() {
    let reg = crate::agents::fake::two_families();
    let whose = crate::agents::fake::said_of("fake2");
    let u = |pinned: bool, list_known: bool, alt: Option<&str>| AccountUnavailable {
        requested: "acct-gone".into(),
        pinned,
        list_known,
        alternative: alt.map(Into::into),
        login: false,
        other_agent: None,
    };
    let other = |alt: Option<&str>| AccountUnavailable {
        other_agent: Some(cmd_wire::AgentKind::new("fake2")),
        ..u(false, true, alt)
    };
    let shapes = [
        u(true, true, Some("acct-alt")),
        u(true, true, None),
        u(false, true, Some("acct-alt")),
        u(false, true, None),
        u(false, false, None),
        other(Some("acct-alt")),
        other(None),
    ];
    let said: Vec<String> = shapes
        .iter()
        .map(|u| unavailable_said_among(reg, u))
        .collect();
    let distinct: std::collections::BTreeSet<&String> = said.iter().collect();
    assert_eq!(distinct.len(), shapes.len(), "七种说成了同一句：{said:#?}");
    for (s, line) in shapes.iter().zip(&said) {
        assert_eq!(
            line.contains(whose),
            s.other_agent.is_some(),
            "是哪一家的号那一半说得不对（{s:?}）：{line}"
        );
        assert!(line.contains("acct-gone"), "没说要的是哪个号：{line}");
        let alt_named = line.contains("acct-alt");
        assert_eq!(
            alt_named,
            s.list_known && s.alternative.is_some(),
            "替代那一半说得不对（{s:?}）：{line}"
        );
    }
    // 清单读不出时有替代也不提（那台的默认号未必还能用）。
    assert!(!unavailable_said(&u(true, false, Some("acct-alt"))).contains("acct-alt"));
}

/// 经别名起的那一发：便条带别名名字，认便条时记进启动台账的 `aliases`（没有号、没写 `--tab` 也照记）。
#[test]
fn a_note_carries_the_alias_into_the_launch_ledger() {
    let h = test_home::scratch("alias");
    leave(
        &own_at(&h),
        81,
        &Note {
            account: String::new(),
            at: 1_000,
            plugin: false,
            tab: None,
            alias: Some("dev".into()),
            host: None,
        },
    )
    .unwrap();
    assert!(adopt(&own_at(&h), 81, "s-alias", Some(990), &none_alive).unwrap());
    let book = read_book(&h).unwrap();
    assert_eq!(book.aliases.get("s-alias").map(String::as_str), Some("dev"));
    assert_eq!(book.sessions.get("s-alias"), None);
    // 已经记着 ⇒ 不再写。
    assert!(!adopt(&own_at(&h), 81, "s-alias", Some(990), &none_alive).unwrap());
    let _ = std::fs::remove_dir_all(&h);
}

fn host_note(account: &str, host: Option<cmd_wire::frame::TerminalHost>) -> Note {
    Note {
        account: account.to_string(),
        at: 1_000,
        plugin: false,
        tab: None,
        alias: None,
        host,
    }
}

/// 起在哪种宿主里：便条带宿主（不带号也认）⇒ 记进启动台账的 `hosts`；同一条后来经直路起（便条没宿主）⇒ 摘掉那一条。
#[test]
fn a_note_carries_the_host_into_the_launch_ledger_and_a_direct_launch_drops_it() {
    use cmd_wire::frame::TerminalHost;
    let h = test_home::scratch("host");
    leave(&own_at(&h), 91, &host_note("", Some(TerminalHost::Tmux))).unwrap();
    assert!(adopt(&own_at(&h), 91, "s-host", Some(990), &none_alive).unwrap());
    assert_eq!(
        crate::history::launch_book::host_of(Some(&h), "s-host"),
        Some(TerminalHost::Tmux)
    );
    assert!(
        !adopt(&own_at(&h), 91, "s-host", Some(990), &none_alive).unwrap(),
        "记着同一个还写"
    );
    leave(
        &own_at(&h),
        92,
        &host_note("x", Some(TerminalHost::Managed)),
    )
    .unwrap();
    assert!(adopt(&own_at(&h), 92, "s-host", Some(990), &none_alive).unwrap());
    assert_eq!(
        crate::history::launch_book::host_of(Some(&h), "s-host"),
        Some(TerminalHost::Managed)
    );
    leave(&own_at(&h), 93, &host_note("x", None)).unwrap();
    assert!(adopt(&own_at(&h), 93, "s-host", Some(990), &none_alive).unwrap());
    assert_eq!(
        crate::history::launch_book::host_of(Some(&h), "s-host"),
        None
    );
    let _ = std::fs::remove_dir_all(&h);
}

// ───────── 便条认哪个进程：同「会话进程是不是 ccm 起的那一跳」那一判 ─────────

/// 托管事实里记的那一跳：`pid` ＋ 是不是包一层（Windows 上 `cmd.exe /c claude.cmd` · `powershell -File claude.ps1` 才是）。
fn hop(pid: u32, wrapped: bool) -> Option<crate::host::managed::Hop> {
    Some(crate::host::managed::Hop { pid, wrapped })
}

/// 父进程表：501 ← 500（那一跳）、502 ← 501（会话里又起的进程）。
fn parent(p: u32) -> Option<u32> {
    match p {
        501 => Some(500),
        502 => Some(501),
        _ => None,
    }
}

/// 原生那一形（POSIX 就地 exec · Windows 起的就是 `claude.exe`）：会话进程就是那一跳 ⇒ 认那一跳上的便条。
#[test]
fn a_native_hop_is_the_session_process_and_its_note_is_adopted() {
    assert_eq!(noted_pid(500, hop(500, false), parent), 500);
    let h = test_home::scratch("hop-native");
    leave_note(&own_at(&h), 500, Some("x"), false, None, 1_000).unwrap();
    let at = noted_pid(500, hop(500, false), parent);
    assert!(adopt(&own_at(&h), at, "s1", Some(990), &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s1").as_deref(), Some("x"));
    let _ = std::fs::remove_dir_all(&h);
}

/// 包一层那一形：会话进程是那一跳的直接子进程 ⇒ 认那一跳上的便条（号记得上）；没记「包一层」的那一跳，它的子进程不认。
#[test]
fn under_a_wrapped_hop_the_direct_child_adopts_the_hops_note() {
    assert_eq!(noted_pid(501, hop(500, true), parent), 500);
    assert_eq!(
        noted_pid(501, hop(500, false), parent),
        501,
        "没记包一层的那一跳，子进程不该认它的便条"
    );
    let h = test_home::scratch("hop-wrapped");
    leave_note(&own_at(&h), 500, Some("x"), false, None, 1_000).unwrap();
    let at = noted_pid(501, hop(500, true), parent);
    assert!(adopt(&own_at(&h), at, "s1", Some(990), &none_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s1").as_deref(), Some("x"));
    let _ = std::fs::remove_dir_all(&h);
}

/// 包一层底下再往下的后代（会话里又起的 `claude -p`）：不认那一跳的便条；不在托管终端里（没有那一跳）⇒ 只认自己的 pid。
#[test]
fn a_deeper_descendant_of_a_wrapped_hop_does_not_adopt_its_note() {
    assert_eq!(noted_pid(502, hop(500, true), parent), 502);
    assert_eq!(noted_pid(501, None, parent), 501);
    let h = test_home::scratch("hop-deeper");
    leave_note(&own_at(&h), 500, Some("x"), false, None, 1_000).unwrap();
    let at = noted_pid(502, hop(500, true), parent);
    let hop_alive = |p: u32| (p == 500).then_some(Some(990));
    assert!(!adopt(&own_at(&h), at, "s1", Some(990), &hop_alive).unwrap());
    assert_eq!(last_of(Some(&h), "s1"), None);
    assert!(
        h.join(NOTES_DIR).join("500.json").exists(),
        "那一跳还在，它的便条不该被清"
    );
    let _ = std::fs::remove_dir_all(&h);
}

// ───────── 照会话找便条只一口：hook 认「ccm 起的那一个」也经它 ─────────

/// 包一层那一形（便条在那一跳上、发问的会话进程是它的直接子进程）：hook 认得出是 ccm 起的那一个；
/// 没记那一跳（tmux · 直路）⇒ 只认发问那个进程自己的 pid，认不出。
#[cfg(unix)]
#[test]
fn the_hook_admits_a_session_under_a_wrapped_hop_by_the_hops_note() {
    let h = test_home::scratch("hook-wrapped");
    let w = WrappedHop::spawn(|_| {});
    let now = crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    leave_note(&own_at(&h), w.hop, Some("x"), true, None, now + 1).unwrap();
    let hop = w.as_hop();
    let in_table = |hop_of: &dyn Fn(u32) -> Option<crate::host::managed::Hop>| {
        crate::control::asks::hook::in_table_among(&h, w.session, "s-none", hop_of)
    };
    assert!(
        in_table(&|p| (p == w.session).then_some(hop)),
        "包一层：认那一跳的便条"
    );
    assert!(!in_table(&|_| None), "没有那一跳 ⇒ 只认自己的 pid");
    drop(w);
    let _ = std::fs::remove_dir_all(&h);
}

/// 判据夹具：「包一层」那一形 —— 一个 `sh`（那一跳）起一个子进程 `sh`（会话进程），会话进程报出自己的 pid 后读标准输入等着
/// （不按墙钟定寿命）；丢掉时关上标准输入，两个都退。`tune` 调那一跳的环境（会话进程原样继承）。
#[cfg(unix)]
struct WrappedHop {
    child: std::process::Child,
    /// 那一跳（便条留在它上面）。
    hop: u32,
    /// 会话进程：那一跳的直接子进程。
    session: u32,
}

#[cfg(unix)]
impl WrappedHop {
    fn spawn(tune: impl FnOnce(&mut std::process::Command)) -> Self {
        use std::io::BufRead;
        let mut cmd = std::process::Command::new("sh");
        // 末尾那个 `:` 让那一跳不就地 exec 成里面那一个：它留着，里面那一个是它的直接子进程。
        cmd.args(["-c", "sh -c 'echo $$; read _'; :"]);
        tune(&mut cmd);
        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("起得了 sh");
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.as_mut().expect("接了标准输出"))
            .read_line(&mut line)
            .expect("会话进程报了 pid");
        let session = line.trim().parse().expect("会话进程报的是 pid");
        Self {
            hop: child.id(),
            session,
            child,
        }
    }

    /// 托管事实里记的那一跳：包一层。
    fn as_hop(&self) -> crate::host::managed::Hop {
        crate::host::managed::Hop {
            pid: self.hop,
            wrapped: true,
        }
    }
}

#[cfg(unix)]
impl Drop for WrappedHop {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        let _ = self.child.wait();
    }
}
