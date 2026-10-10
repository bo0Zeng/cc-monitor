use super::*;
use crate::platform::console::window_of_console;

#[test]
fn hwnd_entry_roundtrip() {
    let e = HwndEntry {
        ps_pid: 9692,
        hwnd: 0x12345,
        owner_pid: 37684,
        owner_proc_start: 132456789012345678,
        ps_proc_start: "639150434950992340".to_string(),
        title_at_bind: "✳ Claude Code".to_string(),
        registered_at: 1716393600000,
    };
    let s = serde_json::to_string(&e).unwrap();
    let parsed: HwndEntry = serde_json::from_str(&s).unwrap();
    assert_eq!(parsed, e);
}

/// 本机后端那一格成品原样反序列化（`start` 那一格本进程不用）：ssh → powershell → Windows Terminal。
fn ssh_chain() -> Vec<ChainLink> {
    serde_json::from_value(serde_json::json!([
        { "pid": 700, "name": "ssh.exe", "start": 4000 },
        { "pid": 600, "name": "powershell.exe", "start": 3000 },
        { "pid": 500, "name": "WindowsTerminal.exe", "start": 2000 },
    ]))
    .unwrap()
}

/// 窗口枚举的替身：pid ⇒ 它名下的可见顶层窗口。
fn wins(table: &'static [(u32, &'static [isize])]) -> impl Fn(u32) -> Vec<isize> {
    move |pid: u32| {
        table
            .iter()
            .find(|(p, _)| *p == pid)
            .map(|(_, w)| w.to_vec())
            .unwrap_or_default()
    }
}

fn found(hwnd: isize, owner_pid: u32, owner_proc_start: u64) -> FoundWindow {
    FoundWindow {
        hwnd,
        owner_pid,
        owner_proc_start,
    }
}

/// 「这一级显示在哪个窗口」的替身：只有 `pid` 那一级认得出，是窗口 `hwnd`（属主 `owner`）。
fn known_at(pid: u32, hwnd: isize, owner: u32) -> impl Fn(u32) -> Option<FoundWindow> {
    move |p: u32| (p == pid).then(|| found(hwnd, owner, 777))
}

fn nothing_known(_: u32) -> Option<FoundWindow> {
    None
}

/// 握手表里的一条（Linux bash / zsh 接入块那一份）：shell `ps_pid` 登记了窗口 `hwnd`（属主 `owner_pid`）。
fn entry(ps_pid: u32, hwnd: isize, owner_pid: u32) -> HwndEntry {
    HwndEntry {
        ps_pid,
        hwnd,
        owner_pid,
        owner_proc_start: 777,
        ps_proc_start: "639150434950992340".to_string(),
        title_at_bind: format!("ccm-bind-{ps_pid}-abcd1234"),
        registered_at: 1716393600000,
    }
}

/// ★ 用户 10-09 那一幕（两个 Windows Terminal 窗口、从标签栏 resume 的远端会话、PowerShell 没登记）：
/// 链上开着连接的 ssh.exe 认得出它显示在哪个窗口（它的控制台挂在那个 Windows Terminal 窗口下）⇒ 就是那个窗口，
/// 不管 Windows Terminal 开着几个窗口、不要任何登记。
#[test]
fn the_window_showing_the_ssh_console_is_named_even_when_the_terminal_has_several() {
    let chain = ssh_chain();
    assert_eq!(
        pick_chain_window(
            &chain,
            known_at(700, 0x33, 500),
            wins(&[(500, &[0x22, 0x33])]),
            |_| 0
        ),
        Ok(found(0x33, 500, 777))
    );
    // 开着连接的那一级认不出、外面那个 PowerShell 认得出 ⇒ 外面那个（同一个窗口）。
    assert_eq!(
        pick_chain_window(
            &chain,
            known_at(600, 0x22, 500),
            wins(&[(500, &[0x22, 0x33])]),
            |_| 0
        ),
        Ok(found(0x22, 500, 777))
    );
}

/// 链上哪一级都认不出它显示在哪：沿进程链从下往上，第一个有可见顶层窗口的进程 —— 恰好一个 ⇒ 它；好几个 ⇒ 分不清（不挑）；
/// 整条链都没有 ⇒ 没有窗口（说开着连接的那个程序）。
#[test]
fn without_a_known_window_the_window_is_the_first_one_up_the_chain_and_only_if_it_is_alone() {
    let chain = ssh_chain();
    let start = |pid: u32| u64::from(pid) * 10;
    assert_eq!(
        pick_chain_window(&chain, nothing_known, wins(&[(500, &[0x22])]), start),
        Ok(found(0x22, 500, 5000))
    );
    let two =
        pick_chain_window(&chain, nothing_known, wins(&[(500, &[0x22, 0x33])]), start).unwrap_err();
    assert_eq!(
        two,
        FrontOutcome::Several {
            program: "WindowsTerminal.exe".into(),
            count: 2
        }
    );
    let nowin = pick_chain_window(&chain, nothing_known, wins(&[]), start).unwrap_err();
    assert_eq!(
        nowin,
        FrontOutcome::NoWindow {
            program: "ssh.exe".into()
        }
    );
}

/// ★ 终端窗口的属主以上的进程不在这个窗口里：Windows Terminal 是从某个认得出窗口的 PowerShell 里打开的，
/// 那个 PowerShell 显示在它自己那个窗口 ⇒ 不拿来用；这条连接这一侧谁都认不出、终端开着两个窗口 ⇒ 照实说分不清。
#[test]
fn a_known_window_above_the_terminal_window_is_not_this_window() {
    let chain: Vec<ChainLink> = serde_json::from_value(serde_json::json!([
        { "pid": 700, "name": "ssh.exe", "start": 4000 },
        { "pid": 650, "name": "pwsh.exe", "start": 3500 },
        { "pid": 500, "name": "WindowsTerminal.exe", "start": 2000 },
        { "pid": 400, "name": "powershell.exe", "start": 1000 },
    ]))
    .unwrap();
    let said = pick_chain_window(
        &chain,
        known_at(400, 0x99, 300),
        wins(&[(500, &[0x22, 0x33])]),
        |_| 0,
    )
    .unwrap_err();
    assert!(
        matches!(&said, FrontOutcome::Several { program, .. } if program == "WindowsTerminal.exe"),
        "{said:?}"
    );
}

/// ★ 握手表里那一条不作数就不用：登记的 shell 已经不是此刻这个进程（起始时刻对不上 / 读不到）、或那个窗口不作数了。
#[test]
fn a_registration_that_fails_its_check_is_not_used() {
    let e = entry(600, 0x33, 500);
    let ok = |_: &FoundWindow| true;
    assert!(registration_holds(&e, Some(639150434950992340), ok));
    assert!(!registration_holds(&e, Some(639150434950992341), ok));
    assert!(!registration_holds(&e, None, ok));
    assert!(!registration_holds(&e, Some(639150434950992340), |_| false));
}

/// ★ 本机会话点那一刻现走 claude 往上的进程链（同远端那一条规则）：claude 不是 shell 的直接子进程（中间隔着 ccm）⇒
/// 往上走到认得出窗口的那一级；整条链都认不出 ⇒ 「没登记」（界面再按窗口标签找一次：Linux 上会话在 tmux 里）。
#[test]
fn a_local_claude_is_found_up_its_chain_at_click_time() {
    let chain: Vec<ChainLink> = serde_json::from_value(serde_json::json!([
        { "pid": 900, "name": "claude.exe" },
        { "pid": 810, "name": "ccm.exe" },
        { "pid": 600, "name": "powershell.exe" },
        { "pid": 500, "name": "WindowsTerminal.exe" },
    ]))
    .unwrap();
    assert_eq!(
        pick_local_window(
            &chain,
            known_at(900, 0x33, 500),
            wins(&[(500, &[0x22, 0x33])]),
            |_| 0
        ),
        Ok(found(0x33, 500, 777))
    );
    let tmux: Vec<ChainLink> = serde_json::from_value(serde_json::json!([
        { "pid": 900, "name": "claude" },
        { "pid": 300, "name": "tmux: server" },
        { "pid": 1, "name": "systemd" },
    ]))
    .unwrap();
    assert_eq!(
        pick_local_window(&tmux, nothing_known, wins(&[]), |_| 0),
        Err(FrontOutcome::Unbound)
    );
}

/// ★ 那台回显的窗口标签（`<进程号>-<起始时刻>`）⇒ 那个进程显示在哪个窗口：标签里的起始时刻要与此刻那个进程的对得上（进程号被复用不认）；
/// 几个终端按交来的顺序，第一个对上的就是它；没有标签 / 形状不对 / 认不出 ⇒ 没有（接着按连接对）。
#[test]
fn a_window_label_names_the_shell_window_only_when_its_start_time_matches() {
    let start = |pid: u32| match pid {
        600 => Some(639150434950992340),
        601 => Some(5),
        _ => None,
    };
    let ts =
        |v: serde_json::Value| -> Vec<serde_json::Value> { serde_json::from_value(v).unwrap() };
    let hit = labeled_window(
        &ts(serde_json::json!([
            { "ssh": null },
            { "ssh": null, "window": "601-639150434950992340" },
            { "ssh": null, "window": "600-639150434950992340" },
        ])),
        start,
        |p| (p == 600 || p == 601).then(|| found(0x33, 500, 777)),
    );
    assert_eq!(hit, Some(found(0x33, 500, 777)));
    for bad in [
        serde_json::json!([{ "ssh": null, "window": "600-639150434950992341" }]),
        serde_json::json!([{ "ssh": null, "window": "602-639150434950992340" }]),
        serde_json::json!([{ "ssh": null, "window": "600" }]),
        serde_json::json!([{ "ssh": null, "window": 600 }]),
        serde_json::json!([{ "ssh": null }]),
    ] {
        assert_eq!(
            labeled_window(&ts(bad.clone()), start, |_| Some(found(1, 1, 1))),
            None,
            "{bad}"
        );
    }
}

/// ★ 「这个进程显示在哪个窗口」在控制台那一侧的规则（读法是参数）：控制台窗口有属主（Windows Terminal 那种伪控制台窗口，
/// 属主就是承载那个标签的终端窗口）⇒ 属主；没有属主 ⇒ 控制台窗口自己（经典控制台）；那个窗口不可见 ⇒ 不算（隐藏的控制台）；
/// 借不到控制台 ⇒ 没有。
#[test]
fn a_console_is_shown_in_its_owner_window_or_in_itself() {
    let vis = |h: isize| h != 0x404;
    assert_eq!(window_of_console(Some(0x101), |_| 0x202, vis), Some(0x202));
    assert_eq!(window_of_console(Some(0x101), |_| 0, vis), Some(0x101));
    assert_eq!(window_of_console(Some(0x404), |_| 0, vis), None);
    assert_eq!(window_of_console(Some(0x101), |_| 0x404, vis), None);
    assert_eq!(window_of_console(Some(0), |_| 0x202, vis), None);
    assert_eq!(window_of_console(None, |_| 0x202, vis), None);
}

/// ★ 每级都要先问「它显示在哪」再看它名下的窗口：一个进程（如 Windows Terminal）名下有窗口，却也认得出它自己在哪 ⇒ 用认出的那个。
#[test]
fn a_known_window_wins_over_the_owners_window_list() {
    let chain: Vec<ChainLink> = serde_json::from_value(serde_json::json!([
        { "pid": 500, "name": "WindowsTerminal.exe" },
    ]))
    .unwrap();
    assert_eq!(
        walk_chain(
            &chain,
            known_at(500, 0x33, 500),
            wins(&[(500, &[0x22, 0x33])])
        ),
        ChainHit::Known(found(0x33, 500, 777))
    );
}

/// ★ 会话由环境认：说了 Wayland 或挂着 `$WAYLAND_DISPLAY` ⇒ Wayland（哪怕也有 `$DISPLAY`：那是 Xwayland）；只有 `$DISPLAY` ⇒ X11；都没有 / 空串 ⇒ 不支持。
#[test]
fn the_display_session_is_read_from_the_environment() {
    use crate::platform::hwnd::{session_from, DisplaySession as S};
    let wl = |d: &str| S::Wayland { desktop: d.into() };
    assert_eq!(
        session_from(Some("x11"), None, Some(":0"), Some("XFCE")),
        S::X11
    );
    assert_eq!(session_from(None, None, Some(":100"), None), S::X11);
    assert_eq!(
        session_from(
            Some("wayland"),
            Some("wayland-0"),
            Some(":0"),
            Some("GNOME")
        ),
        wl("GNOME")
    );
    assert_eq!(
        session_from(None, Some("wayland-1"), Some(":1"), Some("KDE")),
        wl("KDE")
    );
    assert_eq!(session_from(Some("wayland"), None, None, None), wl(""));
    assert_eq!(
        session_from(Some("wayland"), None, None, Some("ubuntu:GNOME")),
        wl("GNOME")
    );
    assert_eq!(
        session_from(Some("tty"), Some(""), Some(""), None),
        S::Unsupported
    );
    assert_eq!(session_from(None, None, None, None), S::Unsupported);
}

/// ★ Windows · X11 走得通；Wayland 照实说切不了（带桌面名）；别的不支持。
#[test]
fn each_session_kind_says_whether_front_can_work() {
    use crate::platform::hwnd::DisplaySession as S;
    assert_eq!(refusal_of(&S::Win32), None);
    assert_eq!(refusal_of(&S::X11), None);
    assert_eq!(
        refusal_of(&S::Wayland {
            desktop: "GNOME".into()
        }),
        Some(FrontOutcome::DesktopWontSwitch {
            desktop: "GNOME".into()
        })
    );
    assert_eq!(refusal_of(&S::Unsupported), Some(FrontOutcome::Unsupported));
    assert_eq!(
        serde_json::to_value(FrontOutcome::DesktopWontSwitch {
            desktop: "GNOME".into()
        })
        .unwrap(),
        serde_json::json!({ "kind": "desktop-wont-switch", "desktop": "GNOME" })
    );
}

fn tty_rec(pid: u32, start: &str, tty: &str) -> TtyRecord {
    serde_json::from_value(serde_json::json!({ "shell_pid": pid, "proc_start": start, "tty": tty }))
        .unwrap()
}

/// ★ bash / zsh 接入块留下的那一份：那个 shell 还是它（起始时刻对得上）、还没登记 ⇒ 在它的终端上挂记号标题找窗口：
/// 找到 ⇒ 一条登记（键是 shell 的进程号 ＋ 起始时刻，窗口属主与它的起始时刻现读）；没找到（那个标签页不在前面）⇒ 留着下次再认；
/// shell 没了 / 进程号被复用 / 已经登记过 / 终端设备不像终端 ⇒ 扔掉，不去碰终端。
#[test]
fn a_shell_terminal_record_is_claimed_kept_or_dropped() {
    let hit = || MarkerHit {
        hwnd: 0x2a00004,
        owner_pid: 4100,
        title: "ccm-bind-x".into(),
    };
    let rec = tty_rec(4242, "987654", "/dev/pts/3");
    let mut asked = Vec::new();
    let got = claim_tty(
        &rec,
        Some(987654),
        false,
        |tty, marker| {
            asked.push((tty.to_string(), marker.to_string()));
            Some(hit())
        },
        |pid| if pid == 4100 { 55 } else { 0 },
    );
    let TtyClaim::Registered(e) = got else {
        panic!("找到了应当登记：{got:?}")
    };
    assert_eq!(
        (
            e.ps_pid,
            e.hwnd,
            e.owner_pid,
            e.owner_proc_start,
            e.ps_proc_start.as_str()
        ),
        (4242, 0x2a00004, 4100, 55, "987654")
    );
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].0, "/dev/pts/3");
    assert!(
        asked[0].1.starts_with("ccm-bind-4242-"),
        "记号带进程号：{:?}",
        asked[0].1
    );

    assert_eq!(
        claim_tty(&rec, Some(987654), false, |_, _| None, |_| 0),
        TtyClaim::Keep,
        "没找到 ⇒ 留着"
    );
    let never = |_: &str, _: &str| -> Option<MarkerHit> { panic!("不该去碰终端") };
    assert_eq!(
        claim_tty(&rec, Some(111), false, never, |_| 0),
        TtyClaim::Drop,
        "进程号被复用"
    );
    assert_eq!(
        claim_tty(&rec, None, false, never, |_| 0),
        TtyClaim::Drop,
        "shell 没了"
    );
    assert_eq!(
        claim_tty(&rec, Some(987654), true, never, |_| 0),
        TtyClaim::Drop,
        "已经登记过"
    );
    for bad in ["/tmp/x", "pts/3", "/dev/../etc/passwd", ""] {
        assert_eq!(
            claim_tty(
                &tty_rec(4242, "987654", bad),
                Some(987654),
                false,
                never,
                |_| 0
            ),
            TtyClaim::Drop,
            "{bad}"
        );
    }
    assert_eq!(
        claim_tty(
            &tty_rec(4242, "x", "/dev/pts/3"),
            Some(987654),
            false,
            never,
            |_| 0
        ),
        TtyClaim::Drop,
        "起始时刻认不出"
    );
}

/// ★ 壳里借别的进程的控制台只此一处：生产段里 `AttachConsole(` 的调用只在 `platform/console.rs::console_window`（恰一处，外加
/// 它那一行声明）；本机会话与远端链上的 ↗ 都经它（`bind.rs::shell_window`），别处不许自己去挂控制台（挂控制台是整个进程的状态，
/// 两处各挂各的就没有那把锁了）。
#[test]
fn borrowing_another_process_console_lives_in_one_place() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hits: Vec<(String, usize)> = Vec::new();
    for (f, raw) in guard_core::scan_tree!(&root, &["rs"]) {
        let prod = guard_core::production_code(&raw);
        let n = prod.matches("AttachConsole(").count();
        if n > 0 {
            let rel = f
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            hits.push((rel, n));
        }
    }
    hits.sort();
    assert_eq!(
        hits,
        vec![("platform/console.rs".to_string(), 2)],
        "借控制台的调用点（每份文件里 `AttachConsole(` 几处；console.rs 那两处 = 一次调用 ＋ 一行声明）"
    );
    let bind = guard_core::production_code(include_str!("../../../src/frontend/shell/src/bind.rs"));
    assert_eq!(
        bind.matches("platform::console::console_window(").count(),
        1,
        "「这一级显示在哪个窗口」只在 `shell_window` 里问一次控制台"
    );
}
