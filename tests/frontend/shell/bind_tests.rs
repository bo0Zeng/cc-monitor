use super::*;

#[test]
fn parse_await_request() {
    let raw =
        r#"{"ps_pid":9692,"marker":"ccm-bind-9692-abc12345","proc_start":"639150434950992340"}"#;
    let req: AwaitRequest = serde_json::from_str(raw).unwrap();
    assert_eq!(req.ps_pid, 9692);
    assert_eq!(req.marker, "ccm-bind-9692-abc12345");
    assert_eq!(req.proc_start, "639150434950992340");
}

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
    assert_eq!(parsed.ps_pid, e.ps_pid);
    assert_eq!(parsed.hwnd, e.hwnd);
    assert_eq!(parsed.title_at_bind, e.title_at_bind);
}

// ==== K-W1C：三条主动失效边的端到端判据 ====
//
// 「后端算出会话没了 / 被顶替」到「那条缓存真的被忘了」这一段线：下面两条钉的是**事实 → 缓存状态**，不是「调用了 forget」。形状照 `K-R16` 那条
// 先例（`tests/frontend/ui/gray-light-wiring.vitest.ts` 是「帧 → 前端状态」的直测），同形不同料。
//
// ⚠ 每条都先断言「本来在」再断言「已经没了」——少了入场自检，一份没建起来的夹具
// 会让下半截**空转地绿**（brief 第 9 条那族「空真」）。

/// 造一条形状完整的绑定。字段值只要求**互不相同、且不含 sid**：
/// 落盘那一半按**解析后的键**判，不按子串判，免得 sid 从别的字段（如标题）漏进来。
fn sample_binding(hwnd: isize) -> SidHwndBinding {
    SidHwndBinding {
        hwnd,
        owner_pid: 4321,
        owner_proc_start: 132456789012345678,
        ps_pid: 8765,
        ps_proc_start: "639150434950992340".to_string(),
        title_at_bind: "Windows PowerShell".to_string(),
        registered_at: 1716393600000,
    }
}

/// 造一份**盘上已经有一条绑定**的本机缓存，返回（缓存, 落盘文件）。
/// `tag` 只为让并行跑的各条判据各用一个目录（同进程同 pid），**不进任何断言**。
fn seeded_local_cache(tag: &str, sid: &str) -> (Arc<SidHwndCache>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("ccm-bindcache-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("sid-hwnd-cache.json");
    let mut on_disk: HashMap<String, SidHwndBinding> = HashMap::new();
    on_disk.insert(sid.to_string(), sample_binding(0x1111));
    std::fs::write(&file, serde_json::to_string(&on_disk).unwrap()).unwrap();
    (SidHwndCache::load(file.clone()), file)
}

/// 读回落盘那份，按**键**判（不按子串）。
fn on_disk_sids(file: &Path) -> HashMap<String, SidHwndBinding> {
    let raw = std::fs::read_to_string(file).expect("落盘文件应该在");
    serde_json::from_str(&raw).expect("落盘文件应该是一份 sid → 绑定的 map")
}

/// K-W1C D3 第一句：**本机会话真死（`Gone`）⇒ 那条 sid 查不到了，且落盘里也没了。**
#[test]
fn a_local_session_that_is_gone_gets_forgotten_in_memory_and_on_disk() {
    let sid = "s-alpha";
    let (cache, file) = seeded_local_cache("l1", sid);
    assert!(
        cache.lookup(sid).is_some(),
        "入场自检：绑定本来就不在内存里 —— 夹具没建起来，下面那句是空转"
    );
    assert!(
        on_disk_sids(&file).contains_key(sid),
        "入场自检：绑定本来就不在盘上 —— 落盘那一半此刻是空转"
    );

    cache.apply_local_removal(sid);

    assert!(
        cache.lookup(sid).is_none(),
        "本机会话真死之后那条绑定还查得到 —— 「主动推送」这条边断了：\n\
             ↗ 会继续拉一个已经不属于那个会话的窗口，而这正是用户逐字要防的\n\
             「防止搞到错误的」。"
    );
    assert!(
        !on_disk_sids(&file).contains_key(sid),
        "内存忘了、**落盘文件里还留着** —— monitor 一重启这条死绑定就复活。\n\
             本机这份缓存是持久化的（`sid-hwnd-cache.json`），忘掉必须落到盘上才算忘。"
    );
}

/// K-W1C D3 第二句：**本机会话被顶替（`Superseded`）⇒ 同上。**
///
/// ★ 这一格今天是**顺带**买到的（`lib.rs` 那句头注逐字「cause 在这里无分支意义，
/// 取 sid 即可」）⇒ 属「碰巧对」。本条把它变成「设计如此」：谁在
/// `apply_local_removal` 里加一个 `match cause`，这一条就红。
///
/// ⚠ 它**不**覆盖上游那一步（「同 pid + 同 procStart 换 sid 该判 `Superseded`」）——
/// 那一格由本机后端说（`session_removed.cause` ⇒ 后端会话账本裁成 `session_state`），是另一条边。
#[test]
fn a_local_session_that_was_superseded_gets_forgotten_too() {
    let sid = "s-beta";
    let (cache, file) = seeded_local_cache("l2", sid);
    assert!(
        cache.lookup(sid).is_some(),
        "入场自检：绑定本来就不在内存里 —— 夹具没建起来，下面那句是空转"
    );
    assert!(
        on_disk_sids(&file).contains_key(sid),
        "入场自检：绑定本来就不在盘上 —— 落盘那一半此刻是空转"
    );

    // 去向由后端裁好（`session_state`），本机这一格不看去向：同一个入口、同一个结果。
    cache.apply_local_removal(sid);

    assert!(
        cache.lookup(sid).is_none(),
        "会话被顶替（`/branch` `/clear` 同一个 pidfile 原地换 sid）之后，\n\
             旧 sid 那条绑定还查得到 —— 旧 sid 连 attach 都 attach 不上，\n\
             那条绑定只会把用户拉到一个**现在挂着别的会话**的窗口上。"
    );
    assert!(
        !on_disk_sids(&file).contains_key(sid),
        "被顶替的那条绑定还留在落盘文件里 —— 重启即复活。"
    );
}

/// K-W1C D4 乙：**「窗口还在，但里面已经换人了」这一格今天靠 `Superseded` 兜住，
/// `verify_binding` 自己判不了。**
///
/// # 它钉的是一句**负向**的话，为什么值得钉
///
/// `verify_binding` 只看三样：窗口还在（`IsWindow`，读法住 `platform::hwnd::exists`）· 属主 PID · 属主 procStart。
/// 窗口还在、属主进程没换 ⇒ **恒绿**，哪怕那个终端里现在跑的是另一个会话。
/// 链上唯一能分辨这件事的证据是 `title_at_bind` —— 它**写四处、读作判据零处**
/// （唯一的非写读点是一句 `tracing::info!`）。
///
/// 今天真正挡住这一幕的是**另一条路**：同一个 pidfile 换 sid 会被判 `Superseded`
/// → 推成 removed → `apply_local_removal` 把旧绑定忘掉。
/// ⇒ 这是一条**隐式前提**：`verify_binding` 的安全性**借给了上游那条边**。
/// 本条把它从注释变成判据 —— 谁哪天给 `verify_binding` 加上标题比对（那是件好事），
/// 这一条会红，红的那句话要求同轮补一条**真机正向**判据。
///
/// # 它买不到什么（逐条写明）
///
/// - 买不到「这一幕今天真的到不了」——那一问要 Windows 真机造一次
///   「A 的 ↗ 拉到 B 的窗口」，本机做不到（拉前整族 Windows-only）。件计划 D4 甲。
/// - 买不到「加了标题比对就对了」——标题会被 claude 自己改写，正向判据必须在真窗口上验（要在 session 1 跑）。
/// - 它按**源文本**判，不按行为判 ⇒ 只对 `verify_window`（`verify_binding` 的本体）这一个函数体的**写法**说话；
///   哪天有人把标题比对写进一个被调用的 helper 里，本条**看不见**。
#[test]
fn verify_binding_cannot_tell_that_the_window_changed_hands() {
    let prod = guard_core::production_code(include_str!("../../../src/frontend/shell/src/bind.rs"));
    let at = prod
        .find("fn verify_window(")
        .expect("生产段里没有 `fn verify_window` —— 抽取器坏了，本条此刻无效");
    let body: Vec<&str> = prod[at..]
        .lines()
        .take_while(|l| l.is_empty() || l.starts_with(char::is_whitespace) || l.starts_with("fn "))
        .collect();
    // 抽取器自检：拿到的必须是那个真比对的函数体，
    // 而且它**确实**在看那三样。少了这三句，下面那两句会零命中地绿。
    for needle in ["hwnd::exists", "owner_pid", "owner_proc_start"] {
        assert!(
            body.iter().any(|l| l.contains(needle)),
            "抽出来的 `verify_window` 里没有 `{needle}` —— \
                 要么抽到了非 Windows 那一支，要么它的判据换了，本条此刻无效"
        );
    }
    for forbidden in ["title_at_bind", "GetWindowText"] {
        let hits: Vec<&&str> = body.iter().filter(|l| l.contains(forbidden)).collect();
        assert!(
            hits.is_empty(),
            "`verify_binding` 开始看窗口标题了（命中 `{forbidden}`）：{hits:?}\n\
                 ★ 这大概率是**一件好事** —— 它正是件计划 D4 要的那一格。\n\
                 但本条守的是「这一格今天靠 `Superseded` 兜住、verify 自己判不了」\n\
                 这个**隐式前提**：前提一旦不成立，同轮必须\n\
                 ① 给「窗口还在但换人了」补一条**真机正向**判据（不许与「窗口没了」\n\
                    压回同一个读数 —— 那是又造一个「一个值装两件事」）；\n\
                 ② 把本条改写成新的事实。**别只把这句话删掉。**"
        );
    }
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

/// 记号标题探针的替身：借不到控制台。
fn unavail(_: u32) -> TitleProbe {
    TitleProbe::Unavailable
}

fn found(hwnd: isize, owner_pid: u32, owner_proc_start: u64) -> FoundWindow {
    FoundWindow {
        hwnd,
        owner_pid,
        owner_proc_start,
    }
}

/// 握手表里的一条：PowerShell `ps_pid` 登记了窗口 `hwnd`（属主 `owner_pid`）。
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

/// 握手表一条也没有时：沿进程链从下往上，第一个有可见顶层窗口的进程 —— 恰好一个 ⇒ 它；好几个 ⇒ 分不清（不挑）；
/// 整条链都没有 ⇒ 没有窗口。
#[test]
fn without_a_registration_the_window_is_the_first_one_up_the_chain_and_only_if_it_is_alone() {
    let chain = ssh_chain();
    let none = |_: u32| None;
    let start = |pid: u32| u64::from(pid) * 10;
    // 经典控制台：窗口属主是 shell。
    assert_eq!(
        pick_chain_window(
            &chain,
            none,
            wins(&[(600, &[0x11]), (500, &[0x22])]),
            start,
            unavail
        ),
        Ok(found(0x11, 600, 6000))
    );
    // Windows Terminal 只开一个窗口。
    assert_eq!(
        pick_chain_window(&chain, none, wins(&[(500, &[0x22])]), start, unavail),
        Ok(found(0x22, 500, 5000))
    );
    // 开着两个窗口 ⇒ 分不清：说出是哪个程序与候选个数（不挑）。
    let two =
        pick_chain_window(&chain, none, wins(&[(500, &[0x22, 0x33])]), start, unavail).unwrap_err();
    assert_eq!(
        two,
        FrontOutcome::Several {
            program: "WindowsTerminal.exe".into(),
            count: 2
        }
    );
    // 链上有 PowerShell 却一个窗口都没有（Windows 默认终端把它交给了 Windows Terminal，窗口属主不在链上）
    // ⇒ 窗口归别的程序托管，不说「在后台跑」。
    let handed = pick_chain_window(&chain, none, wins(&[]), start, unavail).unwrap_err();
    assert_eq!(
        handed,
        FrontOutcome::HostedByWt {
            program: "ssh.exe".into()
        }
    );
    // 同一条断链，点击那一刻在那个 shell 的控制台上挂记号标题：带着记号的窗口 ⇒ 就是它；
    // 挂上了却没有窗口带着它（Windows Terminal 只给前台标签页的标题）⇒ 照实说在后台标签页里。
    let marked = |pid: u32| {
        assert_eq!(pid, 600, "记号挂在链上那个控制台 shell 上");
        TitleProbe::Found(found(0x55, 900, 9000))
    };
    assert_eq!(
        pick_chain_window(&chain, none, wins(&[]), start, marked),
        Ok(found(0x55, 900, 9000))
    );
    let behind =
        pick_chain_window(&chain, none, wins(&[]), start, |_| TitleProbe::NotShown).unwrap_err();
    assert_eq!(
        behind,
        FrontOutcome::BackgroundTab {
            program: "ssh.exe".into()
        }
    );
    // 整条链连个 shell 都没有 ⇒ 真在后台：说开着连接的那个程序没有窗口。
    let bg: Vec<ChainLink> = serde_json::from_value(serde_json::json!([
        { "pid": 700, "name": "ssh.exe", "start": 4000 },
        { "pid": 650, "name": "svchost.exe", "start": 3000 },
    ]))
    .unwrap();
    let nowin = pick_chain_window(&bg, none, wins(&[]), start, unavail).unwrap_err();
    assert_eq!(
        nowin,
        FrontOutcome::NoWindow {
            program: "ssh.exe".into()
        }
    );
}

/// ★ 链上某个 PowerShell 在握手表里登记过、且作数 ⇒ 用它登记的那个窗口，哪怕 Windows Terminal 开着好几个窗口
/// （精确到窗口，不靠枚举）；嵌套时开着连接的那个没登记、它外面那个登记了 ⇒ 外面那个（同一个窗口）。
#[test]
fn a_registered_shell_on_the_chain_names_its_window_even_when_the_terminal_has_several() {
    let chain = ssh_chain();
    let start = |pid: u32| u64::from(pid) * 10;
    let reg = |pid: u32| (pid == 600).then(|| entry(600, 0x33, 500));
    assert_eq!(
        pick_chain_window(
            &chain,
            reg,
            wins(&[(500, &[0x22, 0x33, 0x44])]),
            start,
            unavail
        ),
        Ok(found(0x33, 500, 777))
    );
    let nested: Vec<ChainLink> = serde_json::from_value(serde_json::json!([
        { "pid": 700, "name": "ssh.exe", "start": 4000 },
        { "pid": 650, "name": "pwsh.exe", "start": 3500 },
        { "pid": 600, "name": "powershell.exe", "start": 3000 },
        { "pid": 500, "name": "WindowsTerminal.exe", "start": 2000 },
    ]))
    .unwrap();
    assert_eq!(
        pick_chain_window(&nested, reg, wins(&[(500, &[0x22, 0x33])]), start, unavail),
        Ok(found(0x33, 500, 777))
    );
}

/// ★ 终端窗口的属主以上的进程不在这个窗口里：Windows Terminal 是从某个登记过的 PowerShell 里打开的，
/// 那个 PowerShell 登记的是它自己那个窗口 ⇒ 不拿来用；这条连接所在的 PowerShell 没登记、终端开着两个窗口 ⇒ 照实说分不清。
#[test]
fn a_registration_above_the_terminal_window_is_not_this_window() {
    let chain: Vec<ChainLink> = serde_json::from_value(serde_json::json!([
        { "pid": 700, "name": "ssh.exe", "start": 4000 },
        { "pid": 650, "name": "pwsh.exe", "start": 3500 },
        { "pid": 500, "name": "WindowsTerminal.exe", "start": 2000 },
        { "pid": 400, "name": "powershell.exe", "start": 1000 },
    ]))
    .unwrap();
    let reg = |pid: u32| (pid == 400).then(|| entry(400, 0x99, 300));
    let said =
        pick_chain_window(&chain, reg, wins(&[(500, &[0x22, 0x33])]), |_| 0, unavail).unwrap_err();
    assert!(
        matches!(&said, FrontOutcome::Several { program, .. } if program == "WindowsTerminal.exe"),
        "{said:?}"
    );
}

/// ★ 握手表里那一条不作数就不用：登记的 PowerShell 已经不是此刻这个进程（起始时刻对不上 / 读不到）、
/// 或那个窗口不作数了 ⇒ 当没登记；仍好几个窗口 ⇒ 照实说分不清，并说怎么办，不挑。
#[test]
fn a_registration_that_fails_its_check_is_not_used_and_several_windows_are_never_guessed() {
    let e = entry(600, 0x33, 500);
    let ok = |_: &FoundWindow| true;
    assert!(registration_holds(&e, Some(639150434950992340), ok));
    // 进程号被复用：此刻 600 是另一个进程。
    assert!(!registration_holds(&e, Some(639150434950992341), ok));
    // 读不到此刻的起始时刻 ⇒ 校验不了 ⇒ 不用。
    assert!(!registration_holds(&e, None, ok));
    // 窗口没了 / 换了属主。
    assert!(!registration_holds(&e, Some(639150434950992340), |_| false));

    // 生产那一侧只交作数的那一条：不作数 ⇒ 当没登记 ⇒ 退回终端窗口那一级；两个窗口 ⇒ 分不清，不挑其中任何一个。
    let chain = ssh_chain();
    let stale = |pid: u32| {
        (pid == 600)
            .then(|| entry(600, 0x33, 500))
            .filter(|e| registration_holds(e, Some(1), |_| true))
    };
    let said = pick_chain_window(&chain, stale, wins(&[(500, &[0x22, 0x33])]), |_| 0, unavail)
        .unwrap_err();
    assert_eq!(
        said,
        FrontOutcome::Several {
            program: "WindowsTerminal.exe".into(),
            count: 2
        },
        "分不清时照实说拉不了、带候选个数，不挑其中任何一个"
    );
}

/// ★ 本机会话也走同一条规则：claude 不是 PowerShell 的直接子进程（敲 cc 时中间隔着 ccm）⇒ 往上走到登记过的那个 PowerShell。
#[test]
fn a_local_claude_finds_its_shell_through_ccm() {
    let chain: Vec<ChainLink> = serde_json::from_value(serde_json::json!([
        { "pid": 810, "name": "ccm.exe" },
        { "pid": 600, "name": "powershell.exe" },
        { "pid": 500, "name": "WindowsTerminal.exe" },
    ]))
    .unwrap();
    let reg = |pid: u32| (pid == 600).then(|| entry(600, 0x33, 500));
    assert_eq!(
        walk_chain(&chain, reg, wins(&[(500, &[0x22, 0x33])])),
        ChainHit::Registered(entry(600, 0x33, 500))
    );
}

/// ★ 那台回显的窗口标签 ⇒ 握手表里那个 PowerShell 登记的窗口：标签里的起始时刻要与登记时那个对得上（进程号被复用不认）；
/// 几个终端按交来的顺序，第一个对上的就是它；没有标签 / 形状不对 / 没登记 ⇒ 没有（接着按连接对）。
#[test]
fn a_window_label_names_the_registered_shell_only_when_its_start_time_matches() {
    let reg = |pid: u32| (pid == 600).then(|| entry(600, 0x33, 500));
    let ts =
        |v: serde_json::Value| -> Vec<serde_json::Value> { serde_json::from_value(v).unwrap() };
    let hit = labeled_registration(
        &ts(serde_json::json!([
            { "ssh": null },
            { "ssh": null, "window": "601-639150434950992340" },
            { "ssh": null, "window": "600-639150434950992340" },
        ])),
        reg,
    );
    assert_eq!(hit, Some(entry(600, 0x33, 500)));
    for bad in [
        serde_json::json!([{ "ssh": null, "window": "600-639150434950992341" }]),
        serde_json::json!([{ "ssh": null, "window": "600" }]),
        serde_json::json!([{ "ssh": null, "window": 600 }]),
        serde_json::json!([{ "ssh": null }]),
    ] {
        assert_eq!(labeled_registration(&ts(bad.clone()), reg), None, "{bad}");
    }
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
