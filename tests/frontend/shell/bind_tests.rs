use super::*;
use crate::platform::console::{
    helper_argv, helper_request, look_line, parse_looks, refused_by, window_of_console,
    ConsoleLook, HelperRequest,
};

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

/// 「这一级显示在哪个窗口」的替身：只有 `pid` 那一级认得出，是窗口 `hwnd`（属主 `owner`）；别的级读到 `rest`。
fn shown_at(pid: u32, hwnd: isize, owner: u32, rest: Look) -> impl Fn(u32) -> Look {
    move |p: u32| {
        if p == pid {
            Look::Window(found(hwnd, owner, 777))
        } else {
            rest.clone()
        }
    }
}

fn known_at(pid: u32, hwnd: isize, owner: u32) -> impl Fn(u32) -> Look {
    shown_at(pid, hwnd, owner, Look::Denied)
}

/// 每一级都读到同一个样子。
fn every(l: Look) -> impl Fn(u32) -> Look {
    move |_| l.clone()
}

fn not_elevated(_: u32) -> bool {
    false
}

fn by_console(w: FoundWindow, pid: u32) -> Pick {
    Pick {
        window: w,
        via: Via::Shown(pid),
    }
}

fn by_owner(w: FoundWindow, elevated: bool) -> Pick {
    Pick {
        window: w,
        via: Via::Owner { elevated },
    }
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
/// 不管 Windows Terminal 开着几个窗口、不要任何登记；记下是哪一级认出的（切标签页要借它的控制台）。
#[test]
fn the_window_showing_the_ssh_console_is_named_even_when_the_terminal_has_several() {
    let chain = ssh_chain();
    assert_eq!(
        pick_chain_window(
            &chain,
            known_at(700, 0x33, 500),
            wins(&[(500, &[0x22, 0x33])]),
            |_| 0,
            not_elevated
        ),
        Ok(by_console(found(0x33, 500, 777), 700))
    );
    // 开着连接的那一级认不出、外面那个 PowerShell 认得出 ⇒ 外面那个（同一个窗口）。
    assert_eq!(
        pick_chain_window(
            &chain,
            known_at(600, 0x22, 500),
            wins(&[(500, &[0x22, 0x33])]),
            |_| 0,
            not_elevated
        ),
        Ok(by_console(found(0x22, 500, 777), 600))
    );
}

/// 链上哪一级都认不出它显示在哪：沿进程链从下往上，第一个有可见顶层窗口的进程 —— 恰好一个 ⇒ 它；好几个 ⇒ 分不清（不挑）；
/// 整条链都没有 ⇒ 没有窗口（说开着连接的那个程序）。
#[test]
fn without_a_known_window_the_window_is_the_first_one_up_the_chain_and_only_if_it_is_alone() {
    let chain = ssh_chain();
    let start = |pid: u32| u64::from(pid) * 10;
    assert_eq!(
        pick_chain_window(
            &chain,
            every(Look::Denied),
            wins(&[(500, &[0x22])]),
            start,
            not_elevated
        ),
        Ok(by_owner(found(0x22, 500, 5000), false))
    );
    assert_eq!(
        pick_chain_window(
            &chain,
            every(Look::Denied),
            wins(&[(500, &[0x22])]),
            start,
            |p| p == 500
        ),
        Ok(by_owner(found(0x22, 500, 5000), true)),
        "属主那一级只有一个窗口 ⇒ 照旧拉它，记下它是提权的（标签页切不了）"
    );
    let two = pick_chain_window(
        &chain,
        every(Look::Denied),
        wins(&[(500, &[0x22, 0x33])]),
        start,
        not_elevated,
    )
    .unwrap_err();
    assert_eq!(
        two,
        FrontOutcome::Several {
            program: "WindowsTerminal.exe".into(),
            count: 2,
            why: SeveralWhy::ConsoleUnreachable,
        }
    );
    let nowin =
        pick_chain_window(&chain, every(Look::Denied), wins(&[]), start, not_elevated).unwrap_err();
    assert_eq!(
        nowin,
        FrontOutcome::NoWindow {
            program: "ssh.exe".into()
        }
    );
}

/// ★ 分不清时说出为什么，按链上读到的样子判（读法是参数）：
/// - 有一级是「没登记」（Linux：握手表里没有它）⇒ 未登记；
/// - 终端那一级是提权的（借不到它名下的控制台）⇒ 提权；
/// - 有一级借控制台被拒 ⇒ 读不到本会话的控制台；
/// - 都借到了却不在任何可见窗口里 / 都没有控制台 ⇒ 这个程序不提供控制台（不把伪控制台挂到自己窗口下的程序，如 VS Code）。
#[test]
fn several_windows_say_why_the_console_did_not_name_one() {
    let chain = ssh_chain();
    let two = wins(&[(500, &[0x22, 0x33])]);
    let why = |look: &dyn Fn(u32) -> Look, elevated: &dyn Fn(u32) -> bool| match pick_chain_window(
        &chain,
        look,
        &two,
        |_| 0,
        elevated,
    ) {
        Err(FrontOutcome::Several { why, .. }) => why,
        other => panic!("应当分不清：{other:?}"),
    };
    assert_eq!(
        why(&every(Look::Unregistered), &not_elevated),
        SeveralWhy::Unregistered
    );
    assert_eq!(
        why(&every(Look::Denied), &|p| p == 500),
        SeveralWhy::Elevated
    );
    assert_eq!(
        why(&every(Look::Denied), &not_elevated),
        SeveralWhy::ConsoleUnreachable
    );
    assert_eq!(
        why(
            &|p| if p == 700 {
                Look::Denied
            } else {
                Look::NoConsole
            },
            &not_elevated
        ),
        SeveralWhy::ConsoleUnreachable
    );
    assert_eq!(
        why(&every(Look::Hidden), &not_elevated),
        SeveralWhy::Foreign
    );
    assert_eq!(
        why(&every(Look::NoConsole), &not_elevated),
        SeveralWhy::Foreign
    );
    // 提权只问终端那一级（链上别的级提权不算）。
    assert_eq!(
        why(&every(Look::Denied), &|p| p == 700),
        SeveralWhy::ConsoleUnreachable
    );
    assert_eq!(
        serde_json::to_value(FrontOutcome::Several {
            program: "WindowsTerminal.exe".into(),
            count: 2,
            why: SeveralWhy::ConsoleUnreachable
        })
        .unwrap(),
        serde_json::json!({ "kind": "several", "program": "WindowsTerminal.exe", "count": 2, "why": "console-unreachable" })
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
        not_elevated,
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
/// 往上走到认得出窗口的那一级；整条链都认不出 ⇒ 认不出窗口，原因照读到的说：Linux 上是没登记（界面再按窗口标签找一次：
/// 会话在 tmux 里）；Windows 上是链上没有控制台也没有窗口（后台起的）。
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
            |_| 0,
            not_elevated
        ),
        Ok(by_console(found(0x33, 500, 777), 900))
    );
    let tmux: Vec<ChainLink> = serde_json::from_value(serde_json::json!([
        { "pid": 900, "name": "claude" },
        { "pid": 300, "name": "tmux: server" },
        { "pid": 1, "name": "systemd" },
    ]))
    .unwrap();
    assert_eq!(
        pick_local_window(
            &tmux,
            every(Look::Unregistered),
            wins(&[]),
            |_| 0,
            not_elevated
        ),
        Err(FrontOutcome::Unbound {
            why: UnboundWhy::Unregistered
        })
    );
    assert_eq!(
        pick_local_window(
            &chain,
            every(Look::NoConsole),
            wins(&[]),
            |_| 0,
            not_elevated
        ),
        Err(FrontOutcome::Unbound {
            why: UnboundWhy::NoConsole
        })
    );
}

/// ★ 「切过去了」要核过前台：系统答应了、此刻前台也确实是它 ⇒ 切过去了；答应了前台却是别的窗口（真机见过）/ 读不到 ⇒
/// 系统不许抢前台（它在任务栏闪）；不答应 ⇒ 同上，不去读前台。
#[test]
fn switched_means_the_window_really_is_in_front() {
    assert_eq!(landed(true, || Some(0x33), 0x33), FrontOutcome::Switched);
    assert_eq!(landed(true, || Some(0x44), 0x33), FrontOutcome::Refused);
    assert_eq!(landed(true, || None, 0x33), FrontOutcome::Refused);
    assert_eq!(
        landed(false, || panic!("没答应就不读前台"), 0x33),
        FrontOutcome::Refused
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
/// 属主就是承载那个标签的终端窗口）⇒ 属主；没有属主 ⇒ 控制台窗口自己（经典控制台）；那个窗口不可见 ⇒ 隐藏；
/// 句柄为 0 ⇒ 没有控制台。借不到：被拒（错误码 5）⇒ 不给借；别的错误码 ⇒ 没有。
#[test]
fn a_console_is_shown_in_its_owner_window_or_in_itself() {
    let vis = |h: isize| h != 0x404;
    assert_eq!(
        window_of_console(0x101, |_| 0x202, vis),
        ConsoleLook::Shown(0x202)
    );
    assert_eq!(
        window_of_console(0x101, |_| 0, vis),
        ConsoleLook::Shown(0x101)
    );
    assert_eq!(window_of_console(0x404, |_| 0, vis), ConsoleLook::Hidden);
    assert_eq!(
        window_of_console(0x101, |_| 0x404, vis),
        ConsoleLook::Hidden
    );
    assert_eq!(window_of_console(0, |_| 0x202, vis), ConsoleLook::None);
    assert_eq!(refused_by(5), ConsoleLook::Denied);
    assert_eq!(refused_by(6), ConsoleLook::None);
    assert_eq!(refused_by(87), ConsoleLook::None);
}

/// ★ 每级都要先问「它显示在哪」再看它名下的窗口：一个进程（如 Windows Terminal）名下有窗口，却也认得出它自己在哪 ⇒ 用认出的那个。
#[test]
fn a_known_window_wins_over_the_owners_window_list() {
    let chain: Vec<ChainLink> = serde_json::from_value(serde_json::json!([
        { "pid": 500, "name": "WindowsTerminal.exe" },
    ]))
    .unwrap();
    assert_eq!(
        pick_chain_window(
            &chain,
            known_at(500, 0x33, 500),
            wins(&[(500, &[0x22, 0x33])]),
            |_| 0,
            not_elevated
        ),
        Ok(by_console(found(0x33, 500, 777), 500))
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

/// ★ 借控制台交给本二进制的一次性小助手（隐藏子命令）：起它的那一行与它认的那一行两向相等；不是小助手那一形（照常起 monitor
/// · 子命令不认 · 进程号不是数）⇒ 不认，照常起。
#[test]
fn the_console_helper_command_line_round_trips() {
    let argv = |v: Vec<String>| {
        std::iter::once("cc-monitor.exe".to_string())
            .chain(v)
            .collect::<Vec<_>>()
    };
    for req in [HelperRequest::Windows(vec![700, 600, 500])] {
        assert_eq!(
            helper_request(&argv(helper_argv(&req))),
            Some(req.clone()),
            "{req:?}"
        );
    }
    let raw = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    for bad in [
        raw(&["cc-monitor.exe"]),
        raw(&["cc-monitor.exe", "--background"]),
        raw(&["cc-monitor.exe", "--ccm-console"]),
        raw(&["cc-monitor.exe", "--ccm-console", "list", "1"]),
        raw(&["cc-monitor.exe", "--ccm-console", "windows", "x"]),
        raw(&["cc-monitor.exe", "--ccm-console", "windows"]),
        raw(&[
            "cc-monitor.exe",
            "--background",
            "--ccm-console",
            "windows",
            "1",
        ]),
    ] {
        assert_eq!(helper_request(&bad), None, "{bad:?}");
    }
}

/// ★ 小助手每借一个进程印一行，monitor 按行读回（两向相等）；认不出的行不算，小助手没答的进程号不在表里
/// （调用方当「没借成」：`look_of` 读成被拒）。
#[test]
fn the_console_helper_answers_one_line_per_process() {
    let looks = [
        (700, ConsoleLook::Shown(0x2a0b4)),
        (600, ConsoleLook::Hidden),
        (500, ConsoleLook::Denied),
        (400, ConsoleLook::None),
    ];
    let out: String = looks
        .iter()
        .map(|(p, l)| look_line(*p, *l) + "\n")
        .collect();
    let got = parse_looks(&format!("{out}垃圾一行\n700x shown 0x1\n800 shown zz\n\n"));
    for (p, l) in looks {
        assert_eq!(got.get(&p), Some(&l), "{p}");
    }
    assert_eq!(got.len(), 4, "认不出的行不算：{got:?}");
}

/// ★ 借别的进程的控制台只在小助手里：生产段里 `AttachConsole(` 只在 `platform/console.rs`（一次调用 ＋ 一行声明），那一次调用住
/// `borrow`，而 `borrow` 只被小助手那几个函数（`helper_` 开头）调；monitor 这边（`bind.rs`）一次 ↗ 只起一趟小助手问整条链
/// （`console_looks`），自己不借（挂控制台是整个进程的状态：monitor 自己挂着控制台时恒失败，开发构建就是这一形）。
#[test]
fn borrowing_another_process_console_lives_in_the_helper() {
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
    let console = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/platform/console.rs"
    ));
    let lines: Vec<&str> = console.lines().collect();
    let mut callers: Vec<String> = Vec::new();
    let mut current = String::new();
    for l in &lines {
        let t = l.trim_start();
        if let Some(rest) = t
            .strip_prefix("pub fn ")
            .or_else(|| t.strip_prefix("fn "))
            .or_else(|| t.strip_prefix("pub(crate) fn "))
        {
            current = rest.split(['(', '<']).next().unwrap_or("").to_string();
        }
        if l.contains("borrow(") && !t.starts_with("fn borrow") {
            callers.push(current.clone());
        }
    }
    assert!(!callers.is_empty(), "量具自检：应当找到借控制台的调用");
    assert!(
        callers.iter().all(|c| c.starts_with("helper_")),
        "借控制台只许在小助手里：{callers:?}"
    );
    assert!(
        console.contains("fn borrow<") && console.matches("AttachConsole(pid)").count() == 1,
        "那一次调用住 `borrow`"
    );
    let bind = guard_core::production_code(include_str!("../../../src/frontend/shell/src/bind.rs"));
    assert_eq!(
        bind.matches("platform::console::console_looks(").count(),
        1,
        "一次 ↗ 只起一趟小助手问整条链"
    );
    assert!(
        !bind.contains("console_window("),
        "monitor 这边不自己借控制台"
    );
}
