use super::*;

#[test]
fn parse_await_request() {
    let raw = r#"{"ps_pid":9692,"marker":"ccm-bind-9692-abc12345","proc_start":"639150434950992340"}"#;
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

/// RemoteHwndCache 是纯内存映射：直接插入 → lookup 命中 → forget 清除。
/// 不依赖 Win32（try_bind 需要真实窗口，单测里只验 map 语义）。
#[test]
fn remote_hwnd_cache_insert_lookup_forget() {
    let cache = RemoteHwndCache::new();
    let binding = SidHwndBinding {
        hwnd: 0xABCDEF,
        owner_pid: 1234,
        owner_proc_start: 132456789012345678,
        ps_pid: 0,
        ps_proc_start: String::new(),
        title_at_bind: "ccm-rbind-sess-42".to_string(),
        registered_at: 1716393600000,
    };
    // 通过内部 map 直接插入（远端绑定正常由 try_bind 写，单测绕过 Win32）。
    cache
        .by_sid
        .write()
        .insert("sess-42".to_string(), binding.clone());

    let got = cache.lookup("sess-42").expect("lookup should hit");
    assert_eq!(got.hwnd, binding.hwnd);
    assert_eq!(got.owner_pid, binding.owner_pid);
    assert_eq!(got.ps_pid, 0, "远端绑定 ps_pid 应为 0");

    cache.forget("sess-42");
    assert!(cache.lookup("sess-42").is_none(), "forget 后应查不到");
}

// ==== K-W1C：三条主动失效边的端到端判据 ====
//
// 病灶逐字（件计划 §0b 甲）：`bind.rs` 原有 4 条判据，碰 `forget` 的只有
// `remote_hwnd_cache_insert_lookup_forget`，而它是**直接调原语**
// （insert → lookup → forget），**一条推送边都不经过** ⇒ 「后端算出会话没了/被顶替」
// 到「那条缓存真的被忘了」这一段线，本仓没人验。
//
// 下面四条钉的是**事实 → 缓存状态**，不是「调用了 forget」。形状照 `K-R16` 那条
// 先例（`tests/gray-light-wiring.vitest.ts` 是「帧 → 前端状态」的直测），同形不同料。
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

    cache.apply_local_removal(&crate::session_map::RemovedSid::gone(sid));

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
/// 那一格由 `diff_detects_superseded_only_with_positive_identity_evidence` 守着，
/// 是另一条边。两条缺一不可，别把其中一条读成两条。
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

    cache.apply_local_removal(&crate::session_map::RemovedSid::superseded(sid));

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

/// K-W1C D3 第三句（前半）：**远端判 `Archive` ⇒ 那条 sid 在远端缓存里查不到了。**
#[test]
fn a_remote_session_classified_as_archive_gets_forgotten() {
    let sid = "s-gamma";
    let cache = RemoteHwndCache::new();
    cache
        .by_sid
        .write()
        .insert(sid.to_string(), sample_binding(0x2222));
    assert!(
        cache.lookup(sid).is_some(),
        "入场自检：绑定本来就不在 —— 夹具没建起来，下面那句是空转"
    );

    cache.apply_remote_disposition(sid, &crate::ssh_source::RemovedDisposition::Archive);

    assert!(
        cache.lookup(sid).is_none(),
        "远端会话归档之后那条绑定还查得到 —— 远端这条推送边断了。"
    );
}

/// K-W1C D3 第三句（后半）：**远端判 `Idle` ⇒ 不忘。**
///
/// 这是**今天的行为**，本条只钉住它：`Idle` = claude 退了、tmux 会话还在
/// （灰灯那一格），此时本地那个 ssh 窗口可能还开着 ⇒ 绑定仍然拉得前。
///
/// ⚠ **它对不对，本条不答**（件计划 D5 在问：那条绑定此刻指的窗口还是不是
/// 那个会话的窗口）。⇒ 谁哪天裁「Idle 也该忘」，这一条会红，**红就是提醒去改判据，
/// 不是提醒去改回代码**。
///
/// ⚠ **诚实边界**：它是一条**负向**性质 ⇒ 把 `apply_remote_disposition` 的函数体
/// 整个掏空，本条**仍绿**。它的牙在反方向那一刀上（改成无条件 `forget`）。
#[test]
fn a_remote_session_that_only_went_idle_keeps_its_binding() {
    let sid = "s-delta";
    let cache = RemoteHwndCache::new();
    cache
        .by_sid
        .write()
        .insert(sid.to_string(), sample_binding(0x3333));
    assert!(
        cache.lookup(sid).is_some(),
        "入场自检：绑定本来就不在 —— 夹具没建起来，下面那句是空转"
    );

    cache.apply_remote_disposition(
        sid,
        &crate::ssh_source::RemovedDisposition::Idle {
            origin: "one-host".to_string(),
        },
    );

    assert!(
        cache.lookup(sid).is_some(),
        "远端只是进了 idle-tmux（claude 退了、tmux 会话还在），绑定却被忘了 ——\n\
             那个 ssh 窗口可能还开着，忘掉它 ↗ 当场失灵。\n\
             ⚠ 若这是**有意**改的（件计划 D5 裁「Idle 也该忘」），\n\
             要改的是这一条判据、并同轮说清理由，不是把这句话删掉。"
    );
}

/// K-W1C D4 乙：**「窗口还在，但里面已经换人了」这一格今天靠 `Superseded` 兜住，
/// `verify_binding` 自己判不了。**
///
/// # 它钉的是一句**负向**的话，为什么值得钉
///
/// `verify_binding` 只看三样：`IsWindow` · 属主 PID · 属主 procStart。
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
/// - 买不到「加了标题比对就对了」——标题会被 claude 自己改写，
///   正向判据必须在真窗口上验（先例：`remote_bind_finds_real_ccm_rbind_window`
///   要在 session 1 跑）。
/// - 它按**源文本**判，不按行为判 ⇒ 只对 `#[cfg(windows)]` 那一支的**写法**说话；
///   哪天有人把标题比对写进一个被调用的 helper 里，本条**看不见**。
#[test]
fn verify_binding_cannot_tell_that_the_window_changed_hands() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/bind.rs"));
    let at = prod
        .find("pub fn verify_binding")
        .expect("生产段里没有 `pub fn verify_binding` —— 抽取器坏了，本条此刻无效");
    let body: Vec<&str> = prod[at..]
        .lines()
        .take_while(|l| {
            l.is_empty() || l.starts_with(char::is_whitespace) || l.starts_with("pub fn")
        })
        .collect();
    // 抽取器自检：拿到的必须是 Windows 那一支（非 Windows 那支只有一句 Err），
    // 而且它**确实**在看那三样。少了这三句，下面那两句会零命中地绿。
    for needle in ["IsWindow", "owner_pid", "owner_proc_start"] {
        assert!(
            body.iter().any(|l| l.contains(needle)),
            "抽出来的 `verify_binding` 里没有 `{needle}` —— \
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

/// F-Vwin：真 Windows 验证 #74/#41（远端 ↗ HWND 绑定层）。
///
/// 建一个标题含 `ccm-rbind-<sid>` 的**真可见顶层窗口**（用预注册系统类 `Static`，
/// 免 RegisterClass/WNDPROC 样板），走完整绑定链断言：
/// ① `find_window_by_marker_substr` 扫到该窗口（hwnd/owner_pid 对得上）；
/// ② `RemoteHwndCache::try_bind`（内部拼 `ccm-rbind-<sid>` marker）绑上；
/// ③ 窗口存活时 `verify_binding` 通过；④ `DestroyWindow` 后 `verify_binding`
/// 失效（`IsWindow` 假）。这是 #74/#41「窗口按 ccm-rbind 标题可找到+可绑+可验」
/// 的直接自动证据——填补此前 `find_window_by_marker_substr`/`try_bind`/`verify_binding`
/// 无 native 测试的空白。**不覆盖** `SetForegroundWindow` 是否真把窗口拉到前台
/// （前台锁 + 无头会话下不可靠，属半自动 smoke）。
///
/// ⚠️ **必须在 session 1（交互窗口站）跑**：session 0（services）无法让窗口
/// `IsWindowVisible=true`（即便置 WS_VISIBLE 也不翻），而 `find_window_by_marker_substr`
/// 在 `bind.rs:319` 过滤掉不可见窗口 → session 0 里本测试会找不到窗口而失败。
/// 跑法：经 `schtasks /it` 把 `cargo test` 投进已登录的 session 1。
#[cfg(windows)]
#[test]
fn remote_bind_finds_real_ccm_rbind_window() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HINSTANCE, HWND};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, ShowWindow, CW_USEDEFAULT, HMENU, SW_SHOW,
        WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };

    let pid = std::process::id();
    let sid = format!("vwin-{pid}");
    let title = format!("ccm-rbind-{sid}");
    let title_w: Vec<u16> = OsStr::new(&title)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        // 顶层可见窗口（父=null → EnumWindows 可枚举；WS_VISIBLE + SW_SHOW → session 1 里 IsWindowVisible 真）
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("Static"),
            PCWSTR(title_w.as_ptr()),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            220,
            140,
            HWND::default(),
            HMENU::default(),
            HINSTANCE::default(),
            None,
        );
        assert!(hwnd.0 != 0, "CreateWindowExW 应返回非空 HWND");
        let _ = ShowWindow(hwnd, SW_SHOW);

        // ① leaf primitive 扫到
        let hit = find_window_by_marker_substr(&title)
            .expect("find_window_by_marker_substr 应扫到 ccm-rbind 窗口（须在 session 1 跑）");
        assert_eq!(hit.hwnd, hwnd.0, "命中的 hwnd 应为我们建的窗口");
        assert_eq!(hit.owner_pid, pid, "owner_pid 应为本测试进程");
        assert!(hit.title.contains(&title), "title 应含 marker");

        // ② RemoteHwndCache.try_bind 绑上（内部拼 ccm-rbind-<sid>）
        let cache = RemoteHwndCache::new();
        assert!(cache.try_bind(&sid), "try_bind 应成功");
        let binding = cache.lookup(&sid).expect("绑定后 lookup 应命中");
        assert_eq!(binding.hwnd, hwnd.0);
        assert_eq!(binding.owner_pid, pid);

        // ③ 窗口存活 → verify_binding 通过
        verify_binding(&binding).expect("窗口存活时 verify_binding 应通过");

        // ④ 关窗 → verify_binding 失效（IsWindow 假）
        let _ = DestroyWindow(hwnd);
        assert!(
            verify_binding(&binding).is_err(),
            "窗口销毁后 verify_binding 应报错（IsWindow=false）"
        );
    }
}
