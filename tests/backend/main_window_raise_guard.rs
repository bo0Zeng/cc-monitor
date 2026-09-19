/// 拉窗那一族构件（Win32）。**只认名字，不认它从哪个 crate 来** ——
/// `windows` 与 `winapi` 两条路都盖得住，换 crate 不会让它掉出人群。
const WINDOW_RAISE: &[&str] = &[
    "SetForegroundWindow",
    "AllowSetForegroundWindow",
    "AttachThreadInput",
    "BringWindowToTop",
    "SwitchToThisWindow",
    "ShowWindow",
    "SetWindowPos",
    "EnumWindows",
    "GetForegroundWindow",
    "keybd_event",
    "WindowsAndMessaging",
    "winapi::um::winuser",
];

/// 「带 `#[cfg(windows)]`」只认这两种写法。
///
/// ⚠ 刻意**不认** `#[cfg(all(windows, …))]` 之类：那种写法要么是加了第二个条件
///（于是这段代码在某些 Windows 上会不存在，那是另一件事、要另外说清楚），
/// 要么是把平台条件藏进了一个更长的表达式里。**要放宽就来改这里，别在别处绕。**
const WINDOWS_CFGS: &[&str] = &["#[cfg(windows)]", "#[cfg(target_os = \"windows\")]"];

/// 一份**生产段**文本里，拉窗构件的落法合不合规。`Ok(n)` = 命中 n 处且每处都在门后。
///
/// # 判法：往上找**最近的一条** `#[cfg(`
///
/// 它必须是 `WINDOWS_CFGS` 里那两种之一。
/// **守得住**：完全不带 cfg 就写了一段拉窗 · 带的是别的平台 cfg（`unix`/`linux`）·
/// 带的是更长的 cfg 表达式。
/// **守不住**（如实写）：属性归属是**文本近似**，不是语法树 ——
/// 一个 `#[cfg(windows)]` 挂在 A 上、拉窗写在它下面**另一个**没有 cfg 的 item 里，
/// 本条会误判为合规。要堵这一格得解析语法树，本拍不假装能做。
fn raise_sites_are_gated(prod: &str) -> Result<usize, String> {
    // 🔴 注释里提到名字**不算落点** —— 一段注释调不动 Win32，而且这一族名字
    //    正是要在注释里被讨论的。剥法用共享那份（块注释也剥、行号不变，`K-R9`），
    //    别在这里内联第二份。
    let text = guard_core::strip_comment_lines(prod);
    let lines: Vec<&str> = text.lines().collect();
    let mut sites = 0usize;
    for (i, line) in lines.iter().enumerate() {
        let Some(hit) = WINDOW_RAISE.iter().find(|n| line.contains(**n)) else {
            continue;
        };
        sites += 1;
        let gate = lines[..i]
            .iter()
            .rev()
            .find(|l| l.trim_start().starts_with("#[cfg("));
        match gate {
            Some(g) if WINDOWS_CFGS.contains(&g.trim()) => {}
            Some(g) => {
                return Err(format!(
                    "第 {} 行的 `{hit}` 落在 `{}` 之下，而不是 `#[cfg(windows)]` —— \
                         拉窗构件只许在 Windows 那一支里存在。",
                    i + 1,
                    g.trim()
                ))
            }
            None => {
                return Err(format!(
                    "第 {} 行的 `{hit}` **一条 `#[cfg]` 门都没有** —— 它会被编进每个平台，\
                         而拉窗这件事只在 Windows 上成立。",
                    i + 1
                ))
            }
        }
    }
    Ok(sites)
}

/// ★ 正题：**拉窗构件只许出现在一处，且只许在 `#[cfg(windows)]` 之下。**
///
/// # 「一处」为什么按**文件**算
///
/// 这一件真正怕的是「Windows 那条路在后端里长出第二份」：一份在 `control/focus.rs`、
/// 一份在别人顺手写的地方，两份各自演化 ⇒ 就是本工作区最贵的那类病（第二份真相）。
/// 文件是今天唯一能机检的「一处」；更细的粒度要语法树。
#[test]
fn window_raising_lives_in_one_file_and_only_behind_cfg_windows() {
    let root = crate::guard_support::src_root();
    // ⚠ 走共享原语而不是自己 `read_dir`：`scanning_guard_registry` 那条棘轮要求如此。
    //
    // 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §6.2` A 类〕
    //    **从 `scan_tree!` 换成 `scan_tree_excluding(.., &[])`，并删掉「把 `main.rs` 补回来」那两段。**
    //    上一版靠 `scan_tree!` 的 `file!()` 自摘掉 `main.rs`（本判据当年住在 `main.rs` 里），
    //    再手工补回来。剖分之后本判据住 `tests/backend/main_window_raise_guard.rs`
    //    ⇒ `file!()` 是折返路径 ⇒ 自摘恒不命中 ⇒ `main.rs` 本来就在人群里，
    //    补回来那一份变成**第二份**。它自己那条自检（「自摘那一刀没落下」）**当场红** ——
    //    红得对，而且方向是好的那一半（A 类的另一半是静默多算，不会红）。
    let files: Vec<(String, String)> = guard_core::scan_tree_excluding(&root, &["rs"], &[])
        .into_iter()
        .map(|(p, src)| {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, crate::guard_support::production_code(&src))
        })
        .collect();
    // 反向自检（换了方向）：`main.rs` **必须在**人群里 ——
    // 「拉窗写进 main.rs」这一格逃掉之后看起来和守住一模一样，所以这一格要明着断。
    assert!(
        files.iter().any(|(rel, _)| rel == "main.rs"),
        "`main.rs` 不在人群里 —— 「拉窗写进 main.rs」这一格从此逃得掉，\
         而逃掉之后的输出与守住一模一样。多半是排除名单又开始摘它了。"
    );
    // 反空真：扫描面塌了的话，下面两条会在空集上恒绿。地板同 `guard_support` 那条（37）。
    assert!(
        files.len() >= 37,
        "只扫到 {} 个 `.rs`（地板 37）—— 扫描面塌了，本护栏正在空转",
        files.len()
    );

    let mut hosts: Vec<(String, usize)> = Vec::new();
    for (rel, prod) in &files {
        match raise_sites_are_gated(prod) {
            Ok(0) => {}
            Ok(n) => hosts.push((rel.clone(), n)),
            Err(why) => panic!("{rel}：{why}"),
        }
    }
    assert!(
        hosts.len() <= 1,
        "拉窗构件散在 {} 个文件里（只许一处）：{hosts:?}\n\
             ⇒ 第二处就是第二份真相；要挪就整段挪，别两边各留一半。",
        hosts.len()
    );

    // ── 空转自检：今天真实命中是 0 ⇒ 上面两条都在空转，判定本体的读数只能从这儿来 ──
    assert_eq!(
        raise_sites_are_gated("#[cfg(windows)]\nfn f() { unsafe { SetForegroundWindow(h) } }")
            .expect("带门的合规样本被判违规了"),
        1,
        "带 `#[cfg(windows)]` 的拉窗必须放行，否则这条判据会逼人把它藏起来"
    );
    assert!(
        raise_sites_are_gated("fn f() { unsafe { SetForegroundWindow(h) } }").is_err(),
        "🔴 不带任何 cfg 的拉窗**没被逮住** —— 本护栏此刻是个摆设"
    );
    assert!(
        raise_sites_are_gated("#[cfg(unix)]\nfn f() { unsafe { AttachThreadInput(a, b) } }")
            .is_err(),
        "带着**别的平台** cfg 的拉窗没被逮住 —— 那比不带 cfg 更坏（它看起来是合规的）"
    );
    assert_eq!(
        raise_sites_are_gated("fn f() { /* 这里提一句 SetForegroundWindow */ }")
            .expect("注释里提一句不该算违规"),
        0,
        "注释里提到名字被算成了落点 —— 那会让人不敢在注释里讨论它"
    );
}
