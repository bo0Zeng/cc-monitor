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
        rbind_token: None,
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
/// 那一格〔LOC1b〕今天由本机后端说（`session_removed.cause`），monitor 这边钉的是「原样交出去」
/// （`ssh_source_f032_idle_tests` 的 ③ 与 `local_lines_tests` 的 L1），
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

    cache.apply_local_removal(&crate::session_map::RemovedSid {
        sid: sid.to_string(),
        cause: crate::session_map::RemovalCause::Superseded,
    });

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
        CreateWindowExW, DestroyWindow, ShowWindow, CW_USEDEFAULT, HMENU, SW_SHOW, WINDOW_EX_STYLE,
        WS_OVERLAPPEDWINDOW, WS_VISIBLE,
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

// ══════════ `设计/80 §8.7` 步 3：本地半 —— `令牌 → 窗口句柄` ══════════
//
// 这一组守的是**方案 E 的本地那一半**：`↗ 拉前终端` 需要的全部东西是一个映射
// `(sid) → (本地 HWND)`，而方案 E 把那个映射的键从「跨五跳广播过来的 sid」
// 换成「monitor 自己铸的启动期令牌」。本模块要多记的只有 `令牌 → HWND`。
//
// 🔴 **下面每一条都是平台无关的那一段**。Win32 那一跳（`EnumWindows` 找窗口）
//    在非 Windows 上是个恒 `None` 的桩 ⇒ 「↗ 真的把窗口拉起来了」这一维
//    **本机一格都买不到**，它今天仍只有 `remote_bind_finds_real_ccm_rbind_window`
//    那条要在 Windows session 1 手动跑的 smoke。别把这一组读成端到端。

/// 形状合法的令牌：恰好 32 个小写十六进制字符。值本身无意义。
const T3_TOK: &str = "0f1e2d3c4b5a69788796a5b4c3d2e1f0";

fn t3_marker(tok: &str) -> String {
    format!("ccm-rbind-token-{tok}")
}

/// ★ 正题：**这一种 marker 解得出令牌**，而 Era 2 那一种解不出（行为一字未改）。
///
/// ⚠ 判据里那个前缀 **`"ccm-rbind-token-"` 是手写的字面量**，不是从
/// `super::RBIND_TOKEN_MARKER_PREFIX` 取的 —— 两侧同源的话，改常量时判据跟着改，
/// 恒真。要的正是「改了那个常量，本条会红」。
#[test]
fn a_launch_token_marker_yields_the_token_and_the_era2_one_does_not() {
    assert_eq!(
        rbind_token_from_marker(&t3_marker(T3_TOK)),
        Some(T3_TOK),
        "带令牌的 marker 解不出令牌 —— 本地那张 `token → HWND` 表就永远是空的"
    );
    // Era 2 的 marker（PowerShell profile 的 `__ccm_bind` 产的）—— 不该被认成令牌。
    assert_eq!(rbind_token_from_marker("ccm-bind-9692-abc12345"), None);
    // Era 3 远端标题路的 marker（`ccm-rbind-<sid>`，sid 是 uuid）—— 同上。
    assert_eq!(
        rbind_token_from_marker("ccm-rbind-9d66c46d-bf88-4f99-877e-455555555555"),
        None,
        "`ccm-rbind-token-` 与 `ccm-rbind-<sid>` 互相误命中了 —— 两条路会互相拉错窗口"
    );
    assert_eq!(rbind_token_from_marker("plain-shell"), None);
}

/// ★ fail closed：形状不对**一律当没有**，不许当「大概是它」。
///
/// 失效方向是**极安静**的：把一个形状可疑的串记进表里，会让「拉错窗口」
/// （有键、键错了）伪装成「拉不到窗口」（没键）—— `§6.2` 记的正是这个病。
#[test]
fn a_malformed_launch_token_marker_is_treated_as_no_token_at_all() {
    let bad = [
        ("少一位", "0f1e2d3c4b5a69788796a5b4c3d2e1f"),
        ("多一位", "0f1e2d3c4b5a69788796a5b4c3d2e1f00"),
        ("有大写", "0F1E2D3C4B5A69788796A5B4C3D2E1F0"),
        ("非十六进制", "0f1e2d3c4b5a69788796a5b4c3d2e1fg"),
        ("空", ""),
    ];
    for (why, tok) in bad {
        assert_eq!(
            rbind_token_from_marker(&t3_marker(tok)),
            None,
            "「{why}」这一形被当成了合法令牌"
        );
    }
    // 尾巴上挂东西也不行（子串匹配的世界里这一条是必须的）。
    assert_eq!(
        rbind_token_from_marker(&format!("{}-extra", t3_marker(T3_TOK))),
        None,
        "marker 后面还挂着东西却照样解出了令牌 —— 那会把两个不同的窗口记成同一个键"
    );
    // 前缀差一个字节 ⇒ 不是这一种 marker。
    assert_eq!(
        rbind_token_from_marker(&format!("x{}", t3_marker(T3_TOK))),
        None
    );
}

fn t3_entry(ps_pid: u32, marker: &str) -> HwndEntry {
    let req = AwaitRequest {
        ps_pid,
        marker: marker.to_string(),
        proc_start: "639150434950992340".to_string(),
    };
    let hit = MarkerHit {
        hwnd: 0x4321 + ps_pid as isize,
        owner_pid: 37684,
        title: format!("{marker} - Windows PowerShell"),
    };
    entry_from_marker_hit(&req, hit, 132456789012345678)
}

fn t3_registry(dir: std::path::PathBuf, entries: Vec<HwndEntry>) -> BindRegistry {
    let map: HashMap<u32, HwndEntry> = entries.into_iter().map(|e| (e.ps_pid, e)).collect();
    BindRegistry {
        monitor_data_dir: dir,
        by_ps_pid: Arc::new(parking_lot::RwLock::new(map)),
    }
}

/// ★★ 本件的正题：**`令牌 → 窗口句柄` 查得到**，而且查的是 Era 2 那张表。
///
/// 三件一起验，缺一件读数就不可信：
/// ① 带令牌那条查得到，拿回来的 `hwnd` 是**那一条**（不是碰巧有一条）；
/// ② 不带令牌那条（Era 2 的 `ccm-bind-…`）**不因此消失** —— 它照样按 `ps_pid` 查得到；
/// ③ 查一个没铸过的令牌 ⇒ `None`（不是「随便给一条」）。
#[test]
fn the_launch_token_finds_its_window_handle_in_the_same_era2_table() {
    let dir = std::env::temp_dir().join("ccm-t3-lookup");
    let era2 = t3_entry(4242, "ccm-bind-4242-abc12345");
    let tokd = t3_entry(9692, &t3_marker(T3_TOK));
    let want_hwnd = tokd.hwnd;
    let reg = t3_registry(dir, vec![era2, tokd]);

    // ① 令牌查得到，而且是那一条
    let got = reg
        .lookup_hwnd_for_token(T3_TOK)
        .expect("按令牌查不到窗口 —— 方案 E 的本地半整条不通");
    assert_eq!(got.hwnd, want_hwnd);
    assert_eq!(got.ps_pid, 9692);
    // ② Era 2 那条一点没受影响
    assert_eq!(reg.lookup_hwnd_for_ps(4242).map(|e| e.ps_pid), Some(4242));
    assert_eq!(
        reg.lookup_hwnd_for_ps(4242).and_then(|e| e.rbind_token),
        None,
        "Era 2 的 marker 被记上了令牌 —— 那是凭空造了一个键"
    );
    // ③ 没铸过的令牌查不到
    assert_eq!(
        reg.lookup_hwnd_for_token("ffffffffffffffffffffffffffffffff")
            .map(|e| e.hwnd),
        None
    );
    // ④ 形状不对的查询串命不中（靠「表里的键入表时就过了形状闸」＋ 逐字节相等）
    assert!(reg
        .lookup_hwnd_for_token("0F1E2D3C4B5A69788796A5B4C3D2E1F0")
        .is_none());
    assert!(reg.lookup_hwnd_for_token("").is_none());
}

/// ★ **持久化是白拿的** —— 因为根本没有第二张表。
///
/// 写一份 `ps-registry/<PID>.json`（= monitor 上一次跑的时候留下的），
/// 走生产那条 `scan_registry_dir` 重载，令牌照样查得到。
/// 这一条同时钉住 `HwndEntry.rbind_token` 的 **additive 兼容**：
/// 同一个目录里那份**没有**该字段的老文件必须照样读得进来（`serde(default)`）。
#[test]
fn the_token_survives_a_monitor_restart_and_old_files_still_load() {
    let dir = std::env::temp_dir().join(format!("ccm-t3-restart-{}", std::process::id()));
    let reg_dir = dir.join("ps-registry");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&reg_dir).unwrap();

    // 新文件：带令牌（用生产的写法产 JSON）
    let tokd = t3_entry(9692, &t3_marker(T3_TOK));
    std::fs::write(
        reg_dir.join("9692.json"),
        serde_json::to_string(&tokd).unwrap(),
    )
    .unwrap();
    // 老文件：**手写**一份**没有** `rbind_token` 字段的（= 升级前留在盘上的那种）。
    //  ⚠ 刻意不是 `serde_json::to_string(HwndEntry{rbind_token:None})` —— 那两侧同源，
    //    `skip_serializing_if` 一旦被删掉，同源那种写法照样绿。
    std::fs::write(
        reg_dir.join("4242.json"),
        r#"{"ps_pid":4242,"hwnd":123,"owner_pid":7,"owner_proc_start":0,"ps_proc_start":"1","title_at_bind":"old","registered_at":1}"#,
    )
    .unwrap();

    let loaded = scan_registry_dir(&reg_dir);
    assert_eq!(
        loaded.len(),
        2,
        "老文件没读进来 —— additive 兼容破了：{loaded:?}"
    );
    let reg = t3_registry(dir.clone(), loaded.into_values().collect());
    assert_eq!(
        reg.lookup_hwnd_for_token(T3_TOK).map(|e| e.ps_pid),
        Some(9692),
        "monitor 重启之后按令牌查不到了 —— 持久化那一维断了"
    );
    assert_eq!(reg.lookup_hwnd_for_ps(4242).map(|e| e.hwnd), Some(123));

    // 顺带钉住「新写出去的字节对老 monitor 也无害」：不带令牌的条目**不序列化**该字段。
    let plain = t3_entry(4243, "ccm-bind-4243-abc12345");
    let json = serde_json::to_string(&plain).unwrap();
    assert!(
        !json.contains("rbind_token"),
        "不带令牌的条目也把字段写出去了 —— 老 monitor 读到的字节不再逐字节等于从前：{json}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// ★ 心跳清理那一维也是白拿的：PS 进程死了 ⇒ 条目走 ⇒ 令牌跟着查不到。
///
/// ⚠ **诚实边界**：`is_pid_alive` 在非 Windows 上是恒 `false` 的桩
/// ⇒ 本机上「谁该被清掉」这一问它答不了，本条真正在买的是
/// **「清掉之后令牌确实查不到了」**（= 没有第二张表漏清）。
/// 清掉之前那一半（`lookup_hwnd_for_token` 命中）才是本条的正控。
#[test]
fn a_dead_shell_takes_its_token_out_of_the_table_too() {
    let dir = std::env::temp_dir().join(format!("ccm-t3-heartbeat-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("ps-registry")).unwrap();
    let reg = t3_registry(dir.clone(), vec![t3_entry(9692, &t3_marker(T3_TOK))]);
    assert!(
        reg.lookup_hwnd_for_token(T3_TOK).is_some(),
        "清理之前就查不到 —— 下面那句什么也证明不了（空真）"
    );
    cleanup_dead(&reg);
    assert!(
        reg.lookup_hwnd_for_token(T3_TOK).is_none(),
        "主表清了、按令牌还查得到 —— 那说明有第二张没人清的表"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 🔴 ★ **令牌一个字节都不许进日志**（`设计/80 §8.6 ③`）。
///
/// 后端读侧有一条同名同形的判据（`identity_tag::tests::
/// the_token_value_never_reaches_a_log_macro`），**本地这一侧此前没有** ——
/// 因为此前本地 marker 里没有敏感值。步 3 把令牌**变成了** marker，
/// 于是 `req.marker` 与 `entry.title_at_bind` 两处都成了泄露点。
///
/// 两半一起验：① 脱敏函数真的抹掉了值；② 生产段里那两个 `tracing!` 调用点
/// **确实过了脱敏**（只验 ① 的话，把 `redact_marker(...)` 从调用点删掉本条照样绿）。
#[test]
fn the_launch_token_value_never_reaches_a_log_macro() {
    // ① 行为：抹值、留形状
    let red = redact_marker(&t3_marker(T3_TOK));
    assert!(!red.contains(T3_TOK), "脱敏之后令牌还在：{red}");
    assert!(
        red.contains("ccm-rbind-token-"),
        "形状也被抹掉了，排障看不出这是哪一类：{red}"
    );
    // 窗口标题那一档：令牌夹在中间（WT 会往标题里塞别的东西）
    let titled = redact_marker(&format!("{} - Windows PowerShell", t3_marker(T3_TOK)));
    assert!(!titled.contains(T3_TOK), "标题里的令牌没被抹掉：{titled}");
    // 不含令牌的文本原样过（不许把 Era 2 的排障信息也一起吃掉）
    assert_eq!(
        redact_marker("ccm-bind-9692-abc12345"),
        "ccm-bind-9692-abc12345"
    );

    // ② 接线：生产段里凡是把 marker / title_at_bind 交给 tracing 的，必须过脱敏。
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/bind.rs"));
    assert!(
        prod.len() > 5000,
        "抽出来的生产段只有 {} 字节 —— 抽取器坏了，本条此刻在空转",
        prod.len()
    );
    //    切法：从每个 `tracing::` 起，收到那一句的 `);` 为止 —— 只看**日志宏的实参**。
    //    （整文件裸扫会把 `find_window_by_marker_substr(&req.marker)` 这种正常用法
    //     也算成泄露，那是刀没落在靶子上。）
    let mut calls: Vec<String> = Vec::new();
    let mut cur: Option<String> = None;
    for line in prod.lines() {
        let t = line.trim();
        if t.starts_with("//") {
            continue;
        }
        if cur.is_none() && t.contains("tracing::") {
            cur = Some(String::new());
        }
        if let Some(buf) = cur.as_mut() {
            buf.push_str(line);
            buf.push('\n');
            if t.ends_with(");") || t.ends_with(";") && t.contains(')') {
                calls.push(cur.take().unwrap());
            }
        }
    }
    assert!(
        calls.len() >= 8,
        "只切出 {} 处 `tracing::` 调用 —— 切法坏了，本条此刻在空转",
        calls.len()
    );
    let offenders: Vec<&String> = calls
        .iter()
        .filter(|c| {
            (c.contains("req.marker") || c.contains("title_at_bind"))
                && !c.contains("redact_marker")
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "有 `tracing!` 把可能含令牌的串裸着打出去了（漏了 `redact_marker`）：{offenders:#?}"
    );
    // 反向自检：那两处**确实**在日志里出现过（否则上面那条是零命中的绿）。
    assert_eq!(
        calls.iter().filter(|c| c.contains("redact_marker")).count(),
        2,
        "过了脱敏的 `tracing!` 不是 2 处 —— 要么调用点搬家了、要么本条抽错了"
    );
}

// ═══════ `设计/80 §8.7` 步 4 / 步 5（第二波 T4）：↗ 远端那一格的分派 ═══════════════════
//
// 🔴 **下面每一条都是平台无关的那一段**：`verify` 与 `rescan` 注入成假的，判的是分派本身
//    （哪条路先、什么时候退、失败说哪句话）。「窗口真的到了前台」本机一格都买不到。

const T4_TOKEN_HWND: isize = 0x4321 + 9692; // = t3_entry(9692, …) 的 hwnd
const T4_TITLE_CACHED: isize = 0x9001;
const T4_TITLE_RESCANNED: isize = 0x9002;

fn t4_title_binding(hwnd: isize) -> SidHwndBinding {
    SidHwndBinding {
        hwnd,
        owner_pid: 1,
        owner_proc_start: 0,
        ps_pid: 0,
        ps_proc_start: String::new(),
        title_at_bind: "ccm-rbind-s".into(),
        registered_at: 0,
    }
}

/// 一格真值表的入参。`table_hit`：本地表里有没有这个令牌那一条（没令牌时它也在表里 —— 正好验「没令牌就不去查」）。
#[derive(Clone, Copy, Debug)]
struct T4Case {
    has_token: bool,
    table_hit: bool,
    token_ok: bool,
    title_cached: bool,
    rescan_finds: bool,
    title_ok: bool,
}

/// 跑一格：返回（结果, 现扫次数）。
fn t4_run(c: T4Case) -> (Result<SidHwndBinding, String>, usize) {
    let dir = std::env::temp_dir().join("ccm-t4-dispatch");
    let entries = if c.table_hit {
        vec![t3_entry(9692, &t3_marker(T3_TOK))]
    } else {
        vec![]
    };
    let reg = t3_registry(dir, entries);
    let cache = RemoteHwndCache::new();
    if c.title_cached {
        cache
            .by_sid
            .write()
            .insert("s".into(), t4_title_binding(T4_TITLE_CACHED));
    }
    let rescans = std::cell::Cell::new(0usize);
    let verify = |b: &SidHwndBinding| -> Result<(), String> {
        let ok = if b.hwnd == T4_TOKEN_HWND {
            c.token_ok
        } else {
            c.title_ok
        };
        if ok {
            Ok(())
        } else {
            Err(format!("假校验：{:#x} 不在了", b.hwnd))
        }
    };
    let rescan = |sid: &str| -> bool {
        rescans.set(rescans.get() + 1);
        if c.rescan_finds {
            cache
                .by_sid
                .write()
                .insert(sid.to_string(), t4_title_binding(T4_TITLE_RESCANNED));
        }
        c.rescan_finds
    };
    let tok = c.has_token.then_some(T3_TOK);
    let r = resolve_remote_front("s", tok, &reg, &cache, &verify, &rescan);
    (r, rescans.get())
}

/// 规格 —— **独立于生产代码手写的**判定表：期望拿到哪个 hwnd（`Err` = 拉不到）、现扫几次。
fn t4_spec(c: T4Case) -> (Option<isize>, usize) {
    if c.has_token && c.table_hit && c.token_ok {
        return (Some(T4_TOKEN_HWND), 0); // 令牌路命中：一次现扫都不许有（那是 4 秒的白等）
    }
    // 标题退路，与改之前 `lib.rs` 那一段逐步相同
    if c.title_cached {
        if c.title_ok {
            return (Some(T4_TITLE_CACHED), 0);
        }
        // 缓存那个校验不过 ⇒ 忘掉、现扫一次、再校验
        return if c.rescan_finds && c.title_ok {
            (Some(T4_TITLE_RESCANNED), 1)
        } else {
            (None, 1)
        };
    }
    if !c.rescan_finds {
        return (None, 1);
    }
    if c.title_ok {
        (Some(T4_TITLE_RESCANNED), 1)
    } else {
        (None, 2) // 扫到了、校验不过 ⇒ 忘掉再扫一次、仍不过
    }
}

/// ★★ 步 4 ＋ 步 5 的正题：**2⁶ = 64 格真穷举**，每一格对上独立手写的规格；
/// 失败那几格说的话**只由 `has_token` 一个布尔决定**（`§8.5 ②`）。
#[test]
fn the_front_dispatch_goes_token_first_then_title_and_blames_only_the_token_bit() {
    let mut n = 0usize;
    let mut errs_with = 0usize;
    let mut errs_without = 0usize;
    for bits in 0u8..64 {
        let b = |i: u8| bits & (1 << i) != 0;
        let c = T4Case {
            has_token: b(0),
            table_hit: b(1),
            token_ok: b(2),
            title_cached: b(3),
            rescan_finds: b(4),
            title_ok: b(5),
        };
        let (got, rescans) = t4_run(c);
        let (want_hwnd, want_rescans) = t4_spec(c);
        assert_eq!(
            got.as_ref().ok().map(|b| b.hwnd),
            want_hwnd,
            "{c:?} 拿到的窗口不对：{got:?}"
        );
        assert_eq!(rescans, want_rescans, "{c:?} 现扫次数不对");
        if let Err(msg) = got {
            // ③ 归因只看一个布尔：开头二选一，而且**只**由 has_token 决定。
            let with = msg.starts_with(FRONT_FAIL_WITH_TOKEN.as_str());
            let without = msg.starts_with(FRONT_FAIL_WITHOUT_TOKEN.as_str());
            assert!(
                with != without,
                "{c:?} 的失败说法两种开头都不是 / 都是：{msg}"
            );
            assert_eq!(
                with, c.has_token,
                "{c:?} 的失败归因与「有没有令牌」对不上：{msg}"
            );
            // 令牌那个窗口「在表里、但校验不过」⇒ 把校验说的原因带出来（不是只说「关了」）
            let gone = c.has_token && c.table_hit && !c.token_ok;
            assert_eq!(
                msg.contains(&format!("{:#x}", T4_TOKEN_HWND)),
                gone,
                "{c:?}：令牌窗口校验不过的原因该带出来、且只在那一格带：{msg}"
            );
            if c.has_token {
                errs_with += 1
            } else {
                errs_without += 1
            }
        }
        n += 1;
    }
    assert_eq!(n, 64);
    // 反空真：两类失败都真的出现过（否则上面那条「只由一个布尔决定」是零命中的绿）。
    assert!(
        errs_with > 0 && errs_without > 0,
        "失败格只出现了一类：带令牌 {errs_with} · 不带 {errs_without}"
    );
}

/// ★ 账本三件事：记 · 以 `None` 重新宣告 ⇒ 删（旧令牌不许粘着）· 忘。
#[test]
fn the_token_book_forgets_on_a_tokenless_reannounce() {
    let book = RbindTokenBook::new();
    book.note("s1", Some(T3_TOK));
    assert_eq!(book.token_of("s1").as_deref(), Some(T3_TOK));
    book.note("s1", None);
    assert_eq!(
        book.token_of("s1"),
        None,
        "重新宣告成「没令牌」之后旧令牌还粘着 —— ↗ 会拉到不属于它的窗口"
    );
    book.note("s2", Some(T3_TOK));
    book.forget("s2");
    assert_eq!(book.token_of("s2"), None);
}

/// ★ 远端会话归档（`Archive`）⇒ 令牌跟着忘；只是闲了（`Idle`）⇒ 不忘。口径与那条标题绑定**同一条**。
///
/// ⚠ 用的是进程里那本全局账本 ⇒ sid 取本条独有的名字，不与别的判据串味。
#[test]
fn archiving_a_remote_session_takes_its_token_out_of_the_book_but_idling_does_not() {
    let cache = RemoteHwndCache::new();
    let (a, i) = ("t4-archive-only-sid", "t4-idle-only-sid");
    remote_rbind_tokens().note(a, Some(T3_TOK));
    remote_rbind_tokens().note(i, Some(T3_TOK));
    cache.apply_remote_disposition(a, &crate::ssh_source::RemovedDisposition::Archive);
    cache.apply_remote_disposition(
        i,
        &crate::ssh_source::RemovedDisposition::Idle {
            origin: "devbox".into(),
        },
    );
    assert_eq!(
        remote_rbind_tokens().token_of(a),
        None,
        "会话归档了、令牌还在账本里"
    );
    assert_eq!(
        remote_rbind_tokens().token_of(i).as_deref(),
        Some(T3_TOK),
        "会话只是闲了，令牌却被忘了"
    );
    remote_rbind_tokens().forget(i);
}

/// ★★ LF1「远端绑定落盘」：**monitor 重启之后 ↗ 照样按令牌拉得到** —— 不靠新的落盘文件。
///
/// `令牌 → HWND` 那一半从 `ps-registry/*.json` 重载（生产那条 `scan_registry_dir`）；
/// `sid → 令牌` 那一半是重连后 `SessionAdded` 重新喂进来的（这里用 `note` 模拟那一下）。
/// ⚠ 本条不证「重连后后端一定会再宣告」—— 那是 `ssh_source` 重连那一段的事，另有判据。
#[test]
fn after_a_monitor_restart_the_token_path_still_resolves_from_disk() {
    let dir = std::env::temp_dir().join(format!("ccm-t4-restart-{}", std::process::id()));
    let reg_dir = dir.join("ps-registry");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&reg_dir).unwrap();
    std::fs::write(
        reg_dir.join("9692.json"),
        serde_json::to_string(&t3_entry(9692, &t3_marker(T3_TOK))).unwrap(),
    )
    .unwrap();
    // —— 「重启」：一张全新的表，只从盘上读 ——
    let reg = t3_registry(
        dir.clone(),
        scan_registry_dir(&reg_dir).into_values().collect(),
    );
    let book = RbindTokenBook::new();
    book.note("s-after-restart", Some(T3_TOK));
    let rescans = std::cell::Cell::new(0usize);
    let got = resolve_remote_front(
        "s-after-restart",
        book.token_of("s-after-restart").as_deref(),
        &reg,
        &RemoteHwndCache::new(),
        &|_| Ok(()),
        &|_| {
            rescans.set(rescans.get() + 1);
            false
        },
    );
    assert_eq!(
        got.map(|b| b.hwnd).ok(),
        Some(T4_TOKEN_HWND),
        "重启之后按令牌拉不到了 —— 持久化那一维断了"
    );
    assert_eq!(rescans.get(), 0, "令牌路走得通却还去现扫了标题");
    let _ = std::fs::remove_dir_all(&dir);
}

/// ★ 接线（**文本**这一层）：写账本的那一处、读账本的那一处，各恰好一处。
///
/// - 写：`ssh_source` 收 `SessionAdded` 那一臂把 `rbind_token` 记进账本；
/// - 读：`lib.rs` 的 ↗ 命令只剩一句 `bind::bring_remote_front(`，**不许**再自己内联一段标题路
///   （那会长出第二个分派点：一个先令牌、一个只看标题）。
#[test]
fn the_book_has_one_writer_in_ssh_source_and_the_front_command_one_dispatcher() {
    let ssh = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    assert!(
        ssh.len() > 50_000,
        "ssh_source 生产段只抽到 {} 字节 —— 抽取器坏了",
        ssh.len()
    );
    guard_core::find_pinned(
        &ssh,
        "remote_rbind_tokens().note(&sid, rbind_token.as_deref())",
    )
    .expect("ssh_source 里「把 wire 上的令牌记进账本」那一句不是恰好一处");
    let lib = guard_core::production_code(include_str!("../../src/bridge/src/lib.rs"));
    let start = lib
        .find("async fn bring_remote_terminal_to_front(")
        .expect("lib.rs 里找不到 ↗ 远端命令");
    let body = &lib[start..];
    let body = &body[..body.find("\n}\n").expect("找不到函数体收尾")];
    assert_eq!(
        body.matches("bind::bring_remote_front(").count(),
        1,
        "↗ 命令没走唯一分派点：\n{body}"
    );
    for inlined in ["try_bind_with_retry", "verify_binding", ".lookup("] {
        assert!(
            !body.contains(inlined),
            "↗ 命令里又内联了一段 `{inlined}` —— 第二个分派点：\n{body}"
        );
    }
}

/// ★ LF1「死绑定周期清」接到 ↗ 上：**心跳一清，↗ 就不再按令牌拉那个死窗口**，
/// 而是退到标题路、归因照「有令牌」那一句说。
///
/// 节拍归 `run_heartbeat`（10s，已登记在 `rust_timer_registry`）；本条直接调它每一拍做的那件事
/// （`cleanup_dead`），不等 10 秒。⚠ `is_pid_alive` 在非 Windows 上恒 `false`
/// ⇒ 「谁该被清」这一问本机答不了，本条买的是「清掉之后 ↗ 那一侧真的跟着变了」。
#[test]
fn the_heartbeat_sweep_takes_a_dead_window_out_of_the_front_dispatch() {
    let dir = std::env::temp_dir().join(format!("ccm-t4-sweep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("ps-registry")).unwrap();
    let reg = t3_registry(dir.clone(), vec![t3_entry(9692, &t3_marker(T3_TOK))]);
    let cache = RemoteHwndCache::new();
    let rescans = std::cell::Cell::new(0usize);
    let run = || {
        resolve_remote_front("s-sweep", Some(T3_TOK), &reg, &cache, &|_| Ok(()), &|_| {
            rescans.set(rescans.get() + 1);
            false
        })
    };
    assert_eq!(
        run().map(|b| b.hwnd).ok(),
        Some(T4_TOKEN_HWND),
        "清之前就拉不到 —— 下面那句是空转"
    );
    assert_eq!(rescans.get(), 0);
    cleanup_dead(&reg);
    let after = run();
    let msg = after.expect_err("心跳清掉了那条，↗ 却还按令牌拉到了一个窗口");
    assert!(
        msg.starts_with(FRONT_FAIL_WITH_TOKEN.as_str()),
        "清掉之后的归因不是「有令牌」那一句：{msg}"
    );
    assert_eq!(rescans.get(), 1, "清掉之后没有退到标题路去现扫一次");
    let _ = std::fs::remove_dir_all(&dir);
}
