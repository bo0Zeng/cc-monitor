use super::*;
use crate::structural_scan::ScanReport;

fn home() -> PathBuf {
    PathBuf::from("/h")
}
fn no_dir(_: &Path) -> bool {
    false
}
fn yes_dir(_: &Path) -> bool {
    true
}

/// 一个什么都没有的文件系统。
fn empty_probe<'a>() -> FsProbe<'a> {
    FsProbe {
        meta: &|_| None,
        list: &|_| None,
    }
}

/// 建表用的一套基准。**`path_env` 默认给一个非空值** —— 给 `None` 的话
/// `EnvProbe::OnPath` 那一族一律「查不动」，那是**另一个盘面**，
/// 要它就显式写出来（`the_prompt_tier_really_looks_before_it_speaks` 两边都跑）。
fn env_with<'a>(
    home: &'a Path,
    fs: &'a FsProbe<'a>,
    path_env: Option<&'a str>,
) -> SurfaceEnv<'a> {
    SurfaceEnv {
        home,
        cfg_dir_env: None,
        is_dir: &no_dir,
        fs,
        path_env,
    }
}

// ===== 解析：五种结果各一条 =====

#[test]
fn resolves_local_home_paths() {
    let r = resolve_touched_path(
        "~/.local/bin/ccm",
        &ToolDestination::LocalHomeRelative("x"),
        HostScope::Client,
        &home(),
        None,
        &no_dir,
    )
    .unwrap();
    assert_eq!(r, PathResolution::Local(PathBuf::from("/h/.local/bin/ccm")));
}

/// `~/.claude/…` **必须**走 `CLAUDE_CONFIG_DIR` 那条规则，而且只解释一次。
#[test]
fn claude_paths_honor_config_dir_and_fall_back() {
    let acct = PathBuf::from("/h/.claude-accts/z");
    let with = resolve_touched_path(
        "~/.claude/settings.json",
        &ToolDestination::LocalHomeRelative("x"),
        HostScope::Client,
        &home(),
        Some(&acct),
        &yes_dir,
    )
    .unwrap();
    assert_eq!(
        with,
        PathResolution::Local(PathBuf::from("/h/.claude-accts/z/settings.json"))
    );
    // 环境变量指向的不是目录 → 回落 `~/.claude`（与 hooks_diag 同一条规则）
    let without = resolve_touched_path(
        "~/.claude/settings.json",
        &ToolDestination::LocalHomeRelative("x"),
        HostScope::Client,
        &home(),
        Some(&acct),
        &no_dir,
    )
    .unwrap();
    assert_eq!(
        without,
        PathResolution::Local(PathBuf::from("/h/.claude/settings.json"))
    );
}

#[test]
fn resolves_one_level_glob() {
    let r = resolve_touched_path(
        "~/.local/bin/cc-*",
        &ToolDestination::LocalHomeRelative("x"),
        HostScope::Client,
        &home(),
        None,
        &no_dir,
    )
    .unwrap();
    assert_eq!(
        r,
        PathResolution::LocalGlob {
            dir: PathBuf::from("/h/.local/bin"),
            prefix: "cc-".into(),
            suffix: String::new(),
        }
    );
}

#[test]
fn remote_project_and_profile_are_not_local() {
    assert_eq!(
        resolve_touched_path(
            "~/.local/bin/ccm-daemon",
            &ToolDestination::RemoteHomeRelative("x"),
            HostScope::Remote,
            &home(),
            None,
            &no_dir
        )
        .unwrap(),
        PathResolution::Remote("~/.local/bin/ccm-daemon".into())
    );
    assert_eq!(
        resolve_touched_path(
            ".mcp.json",
            &ToolDestination::ProjectRelative("x"),
            HostScope::ProjectDir,
            &home(),
            None,
            &no_dir
        )
        .unwrap(),
        PathResolution::NeedsProjectDir(".mcp.json".into())
    );
    assert_eq!(
        resolve_touched_path(
            "$PROFILE",
            &ToolDestination::UserShellProfile,
            HostScope::Client,
            &home(),
            None,
            &no_dir
        )
        .unwrap(),
        PathResolution::WindowsProfile
    );
}

/// 申报路径与落点**不自洽**时要报错，而不是猜一个。
#[test]
fn declaration_inconsistency_is_an_error_not_a_guess() {
    for (declared, dest) in [
        (
            "/etc/passwd",
            ToolDestination::LocalHomeRelative("x"), // 不以 ~/ 开头
        ),
        ("~/somewhere", ToolDestination::ProjectRelative("x")),
        ("~/.bashrc", ToolDestination::UserShellProfile),
        (
            "~/.local/*/bin",
            ToolDestination::LocalHomeRelative("x"), // glob 不在最后一段
        ),
        (
            "~/.local/bin/*-*",
            ToolDestination::LocalHomeRelative("x"), // 两个 *
        ),
    ] {
        assert!(
            resolve_touched_path(declared, &dest, HostScope::Client, &home(), None, &no_dir)
                .is_err(),
            "{declared:?} 配 {dest:?} 应判不自洽"
        );
    }
}

// ===== 观测：不确定必须带理由，且绝不冒充"缺失" =====

#[test]
fn undetermined_always_carries_a_reason() {
    for res in [
        PathResolution::Remote("~/x".into()),
        PathResolution::NeedsProjectDir(".mcp.json".into()),
        PathResolution::WindowsProfile,
    ] {
        match observe(&res, &empty_probe()) {
            SurfaceState::Undetermined { why } => {
                assert!(!why.trim().is_empty(), "{res:?} 的理由不能是空的");
            }
            other => panic!("{res:?} 不该被判成 {other:?}——那是对能用的安装报假警报"),
        }
    }
}

#[test]
fn present_absent_and_dir_listing() {
    let f = FsProbe {
        meta: &|p| match p.to_string_lossy().as_ref() {
            "/h/f" => Some((false, 42)),
            "/h/d" => Some((true, 0)),
            _ => None,
        },
        list: &|p| {
            if p == Path::new("/h/d") {
                Some(vec!["a".into(), "b".into()])
            } else {
                None
            }
        },
    };
    assert_eq!(
        observe(&PathResolution::Local("/h/f".into()), &f),
        SurfaceState::Present {
            detail: "文件，42 字节".into()
        }
    );
    assert_eq!(
        observe(&PathResolution::Local("/h/d".into()), &f),
        SurfaceState::Present {
            detail: "目录，2 项".into()
        }
    );
    assert_eq!(
        observe(&PathResolution::Local("/h/nope".into()), &f),
        SurfaceState::Absent
    );
}

/// 目录在但**列不出来**（权限）≠ 空目录。混掉的话用户会以为东西被删了。
#[test]
fn unlistable_dir_is_undetermined_not_empty() {
    let f = FsProbe {
        meta: &|_| Some((true, 0)),
        list: &|_| None,
    };
    match observe(&PathResolution::Local("/h/d".into()), &f) {
        SurfaceState::Undetermined { why } => assert!(why.contains("列不出")),
        other => panic!("实得 {other:?}"),
    }
}

#[test]
fn glob_counts_only_real_matches() {
    let f = FsProbe {
        meta: &|_| None,
        list: &|_| {
            Some(vec![
                "cc-send".into(),
                "cc-recv".into(),
                "ccm".into(), // 不匹配 cc-*（缺连字符）
                "other".into(),
            ])
        },
    };
    let g = PathResolution::LocalGlob {
        dir: "/h/.local/bin".into(),
        prefix: "cc-".into(),
        suffix: String::new(),
    };
    assert_eq!(
        observe(&g, &f),
        SurfaceState::Present {
            detail: "2 项匹配".into()
        }
    );
    // 一个都不匹配 → 是真的没有，可以说 Absent
    let none = FsProbe {
        meta: &|_| None,
        list: &|_| Some(vec!["x".into()]),
    };
    assert_eq!(observe(&g, &none), SurfaceState::Absent);
    // 目录列不出来 → **不能说 Absent**
    assert!(matches!(
        observe(&g, &empty_probe()),
        SurfaceState::Undetermined { .. }
    ));
}

// ===== 结构性守卫：注册表里**每一条**申报路径都必须可解析 =====

/// 要件 1+2+3：枚举 `TOOLS` 里每一条 `touches[].path`，逐条要求它能被解析成五种之一，
/// 落进 `Err` 就是违规；`require` 拿计数自检。
///
/// 这条守的是一个具体的退化：有人图省事把散文写回 `path`
/// （第一版 6 个工具里就有 4 条是散文），于是审计页那一行永远显示"申报路径与落点不自洽"。
fn every_declared_path_resolves() -> ScanReport {
    let mut r = ScanReport {
        checked: 0,
        violations: Vec::new(),
    };
    for t in TOOLS {
        for (c, f) in t.carrier_touches() {
            r.checked += 1;
            if let Err(e) =
                resolve_touched_path(f.path, &c.destination, f.host, &home(), None, &no_dir)
            {
                r.violations.push(format!("{}/{:?}：{e}", t.id, f.path));
            }
        }
    }
    r
}

#[test]
fn all_registry_paths_are_machine_resolvable() {
    every_declared_path_resolves()
        .require(10, "TOOLS 的申报路径")
        .unwrap();
}

/// **反向自检**：守卫真的会抓散文。把 T01 第一版那四条散文路径各喂一次，必须全红。
/// （不是构造的边角——它们是这个文件昨天的真实内容。）
#[test]
fn prose_paths_are_rejected() {
    for declared in [
        "~/.bashrc（或所选 profile）",
        "~/.claude/settings.json 的 hooks 段",
        "~/.local/bin/cc-*（12 条软链）",
        "<项目目录>/.mcp.json",
    ] {
        let dest = if declared.starts_with('<') {
            ToolDestination::ProjectRelative("x")
        } else {
            ToolDestination::LocalHomeRelative("x")
        };
        let r =
            resolve_touched_path(declared, &dest, HostScope::Client, &home(), None, &no_dir);
        // **必须直接 Err。** 第一版这里写的是"Err 或者解析出带括号的假路径都算抓到"，
        // 于是 `~/.local/bin/cc-*（12 条软链）` 溜了过去——它成功解析成
        // `LocalGlob { prefix: "cc-", suffix: "（12 条软链）" }`，`dir` 干干净净，
        // 我的谓词只看了 `dir`。测试当场红，才补上 `resolve_touched_path` 开头那道
        // ASCII-graphic 白名单。**断言要打在"解析必须失败"上，不是打在结果长相上。**
        assert!(
            r.is_err(),
            "散文路径 {declared:?} 必须被判不自洽，实得 {r:?}"
        );
    }
    // 而现在真文件里一条散文都没有
    assert!(every_declared_path_resolves().violations.is_empty());
}

// 原先这里有一条 `locality_is_derivable_from_destination_today`，**删了**（T02 审计重要 1）。
// 审计实测它是**同义反复**：`PathResolution::Remote` 只由 `RemoteHomeRelative` 臂产生、
// 且必然产生，所以 `matches!(res, Remote(_)) == remote` 对任意输入恒真——把 `ccm` 的
// `destination` 翻成 `LocalHomeRelative`（会让两行从"远端未确定"变成去 stat 本机 `~/.bashrc`）
// **492 项照样全绿**。而它自称守的那件事（"本机落点却申报远端文件"）在类型上根本
// 表达不出来（`TouchedFile` 没有 host 字段），永远不会红。
//
// **如实登记：远端性没有门禁。** 它现在只是 `resolve_touched_path` 的一条实现约定 +
// 文档。真要门禁得给 `TouchedFile` 加 `host`，而那件事有个真实的第二消费者在等着：
// `~/.cc-bus/` 被本页解析成**本机**，可 `cc_bus.rs` 是按 `origin` 在**可能是远端**的
// 主机上读它——一个 `const destination` 表达不了"按运行期 origin 跨主机"。
// 这条留给 T04（五套机制收编）时连着 origin 模型一起做，不在 T02 硬塞。
// 替代的有牙测试放在 `tool_registry.rs`：`installable_tools_declare_where_they_land`
// 与 `owned_file_implies_installable`（两条都是跨字段一致性，改任一边就红）。

// ===== T04：`host` 维度 =====

/// **这一条是 T04 存在的理由。** `cc-bus` 三条 touches 原先被当纯本机路径，
/// 于是在**生产平台 Windows** 上审计页会说"不存在"，而同一个 app 的驾驶舱
/// 正从远端把 inbox 读得好好的——T02 专门要防的假警报，出现在那一页上格外讽刺。
#[test]
fn either_host_never_says_absent() {
    // 审计指出：原先只用 `empty_probe`（meta/list 恒 None），而 bug 的输出恰好就是
    // 它想要的 `Undetermined` —— 断言与 bug 撞了同一个答案。现在两种探针都跑：
    // ① 全空（本机什么都没有）② 目录列得出但一条都不匹配。两种都不许说 Absent。
    for f in [
        empty_probe(),
        FsProbe {
            meta: &|_| None,
            list: &|_| Some(vec!["unrelated".to_string()]),
        },
    ] {
        either_never_absent_with(&f);
    }
}

fn either_never_absent_with(probe: &FsProbe) {
    let nothing = probe;
    for f in TOOLS
        .iter()
        .flat_map(|t| t.carrier_touches())
        .filter(|(_, f)| f.host == HostScope::Either)
        .map(|(c, f)| {
            resolve_touched_path(f.path, &c.destination, f.host, &home(), None, &no_dir)
                .unwrap()
        })
    {
        assert!(
            matches!(f, PathResolution::EitherHost { .. }),
            "Either 的路径必须解析成 EitherHost，实得 {f:?}"
        );
        match observe(&f, &nothing) {
            SurfaceState::Undetermined { why } => {
                assert!(
                    why.contains("Claude Code 跑的那台"),
                    "理由要说清为什么：{why}"
                );
                assert!(why.contains("不连 SSH"), "要指路：{why}");
            }
            other => panic!("本机没找到不等于不存在，不许判 {other:?}"),
        }
    }
}

/// 但本机**真找到了**就该确定地说存在——`Either` 不是"永远说不知道"。
#[test]
fn either_host_reports_present_when_found_locally() {
    let f = FsProbe {
        meta: &|_| Some((false, 7)),
        list: &|_| None,
    };
    let r = PathResolution::EitherHost(Box::new(PathResolution::Local("/h/.cc-bus".into())));
    match observe(&r, &f) {
        SurfaceState::Present { detail } => {
            assert!(detail.contains("本机存在"), "实得 {detail}");
            assert!(detail.contains("7 字节"), "内层细节要保住：{detail}");
        }
        other => panic!("实得 {other:?}"),
    }
}

/// **`Either` + glob 必须保住计数**（T04 审计阻塞 1）。
///
/// 第一版 `EitherHost { local: PathBuf }` 把 glob 拍成 `dir.join("cc-*")`，
/// 于是 `observe` 去 stat 一个**字面含 `*` 的文件名**、计数分支永远走不到，
/// 那一行从 T04 之前正确的「12 项匹配」退化成「未确定 —— 本机 …/cc-* 不存在」
/// ——`why` 里陈述了一个**假事实**（实测 `ls -d ~/.local/bin/'cc-*'` 报不存在，
/// 而那个目录下真有 12 条 `cc-*`）。
#[test]
fn either_host_keeps_the_glob_count() {
    // 真实盘面：`~/.local/bin` 下 12 条 cc-*（其中一条属于 cc-acct-iso）
    let names: Vec<String> = (0..11)
        .map(|i| format!("cc-{i}"))
        .chain(["cc-acct-iso".to_string()])
        .collect();
    let f = FsProbe {
        meta: &|_| None, // 目录本身 stat 不到也不影响 glob 走 list
        list: &|_| Some(names.clone()),
    };
    let ccbus = TOOLS.iter().find(|t| t.id == "cc-bus").unwrap();
    let (carrier, glob) = ccbus
        .carrier_touches()
        .find(|(_, f)| f.path.contains('*'))
        .expect("cc-bus 应有一条 glob touch");
    assert_eq!(glob.host, HostScope::Either, "前提：这条是 Either");
    let r = resolve_touched_path(
        glob.path,
        &carrier.destination,
        glob.host,
        &home(),
        None,
        &no_dir,
    )
    .unwrap();
    match observe(&r, &f) {
        SurfaceState::Present { detail } => {
            assert!(detail.contains("12 项匹配"), "计数丢了：{detail}");
            assert!(detail.contains("本机存在"), "要标明是本机：{detail}");
        }
        other => panic!("有 12 条匹配却报 {other:?}——这正是阻塞 1 的形态"),
    }
}

/// **跨字段一致性**：`host == Remote` ⇔ 解析结果不含任何本机路径。
///
/// 这条**不是**同义反复（T02 那条被删的"钉子"是）：`host` 与 `destination` 是
/// **两个独立字段**，解析要先按 destination 全量校验、再用 host 投影。
/// 改任一边都会红——变异验证见 §5。
#[test]
fn remote_host_never_resolves_to_a_local_path() {
    let mut checked = 0;
    for t in TOOLS {
        for (c, f) in t.carrier_touches() {
            let r =
                resolve_touched_path(f.path, &c.destination, f.host, &home(), None, &no_dir)
                    .unwrap();
            let local = matches!(
                r,
                PathResolution::Local(_)
                    | PathResolution::LocalGlob { .. }
                    | PathResolution::EitherHost { .. }
            );
            if f.host == HostScope::Remote {
                checked += 1;
                assert!(
                    !local,
                    "{}/{:?} 声明在远端，却解析出本机路径 {r:?}——本机不许替远端回答\
                         「这个路径在不在」（T03 阻塞 3 的根因）",
                    t.id, f.path
                );
            }
        }
    }
    // 计数自检：一条 Remote 都没扫到 = 守卫空转
    // **等号而不是 `>=`**（T04 审计重要 5）：真实是 5 条，写 `>= 4` 恰好容忍一次
    // 静默降级——审计实测单独改一条 host 就是全绿。改 TOOLS 时要来改这个数。
    assert_eq!(
        checked, 4,
        "Remote 条目数变了（真实应为 4）——改 TOOLS 就要来确认这个数。\
             ★ P4c（08-12）5→4：`~/.cc-bus/` 转 Either（`P4a` 把读面做成本机可用）"
    );
}

/// **逐条钉死 `(tool_id, path) → host`**（T04 审计阻塞 3）。
///
/// 审计实测：把 `~/.claude-accts/` 的 host 改回**我上一版刚更正的那个错值**
/// （`Remote → Client`），`cargo test` **512 项全绿**。也就是说 commit message 里
/// 「跨字段守卫…改任一边都红」**是假的**——那条守卫只守 `destination` 那一边
/// （变异 B 改的是 `project_onto_host` 的代码，证明的是代码承重，不是两个字段相互约束）。
///
/// T04 的中心论据是「host 把我一直在犯的错变成了必须逐条声明的东西」。声明是有了，
/// **门禁没有**——我上一次犯的那个错今天改回去仍然全绿。这张表就是那道门禁：
/// 改任何一条 host 都会红，且报错直接指出改了哪一条。
///
/// 改 `TOOLS` 时**必须来改这张表**——这是有意的摩擦：host 判错过三次
/// （T02 的 `~/.cc-bus/`、T03 的 basename 猜远端、T04 的 `~/.claude-accts/` 连错两版），
/// 让它必须被显式确认一次。
#[test]
fn every_host_declaration_is_pinned() {
    use HostScope::*;
    let want: &[(&str, &str, HostScope)] = &[
        ("ccm", "~/.local/bin/ccm", Remote),
        // 🔴 〔`K-R69` 09-12〕**本机那条** —— `Client` 是刻意的、也是本件的正题：
        //    在它之前，闭集里落点是 `…/ccm` 的只有上面那一条（远端）⇒ 本机 0 条，
        //    而用户 `K34` 逐字要的「旧的干净退役」就此没有承接方。
        //    ⚠ 标 `Either` 会**说假话**：这一份是 monitor 自己在**它跑着的那台**上
        //    放下去的（`local_backend::install_local_ccm_entry`），远端那台上没有它。
        ("ccm", "~/.cc-monitor/bin/ccm*", Client),
        ("ccm", "~/.bashrc", Remote),
        // 〔`K-R60` 09-11〕cc-bus 的 `installable` 翻成 true 之后，
        // 「装得了就必须申报装到哪」当场要它 —— 部署真正写的就是这个目录。
        // `Either`：装的口只有本机一个，但 cc-bus 本身跟着 Claude Code 走
        // （与下面三条同一条理由，别只因为「装口在本机」就标 Client）。
        ("cc-bus", "~/.claude/skills/cc-bus", Either),
        // 钩子诊断真有本机+远端两条路径（`diagnose_local_/remote_cc_bus_hooks`）
        ("cc-bus", "~/.claude/settings.json", Either),
        ("cc-bus", "~/.local/bin/cc-*", Either),
        // ★ P4c 订正（08-12）：原写「5 个 IPC 全走 origin+ssh，**零本机读取路径** → Remote」
        //   —— `P4a` 把读面三条做成了本机可用（同一条串、不包 ssh），下拉也加了「本机」档
        //   ⇒ `Either`。理由的长版住 `tool_registry.rs` 那条 `TouchedFile`。
        ("cc-bus", "~/.cc-bus/", Either),
        ("cc-acct-iso", "$ACCT_ISO_DEST", Remote),
        // 列举走远端 ssh，但本机 CLAUDE_CONFIG_DIR 会指进来 → 两端皆可
        ("cc-acct-iso", "~/.claude-accts/", Either),
        // 🔴 〔`K-R81` 09-12〕`remote-daemon` → `backend`，而它今天有**三行**：
        //    同一份后端的三种载体（`K-R68` 现打）。三行的 `host` 逐条不同源：
        //    ① 安装包旁边那份与 ② 自释放那份都落在 monitor 跑着的**这台**（`Client`）；
        //    ③ 推给远端那台的那份是 `Remote` —— 而「远端」说的是「相对这台 monitor」，
        //    **不是它的身份**：在那台机器上它就是那台机器的本地后端（`K36`）。
        //    ⚠ 标 `Either` 会说假话：①② 那两份远端那台上没有。
        ("backend", "$APP_DIR", Client),
        ("backend", "~/.cc-monitor/bin/cc-monitor-local-*", Client),
        ("backend", "$DAEMON_PATH", Remote),
        ("project-mcp", ".mcp.json", ProjectDir),
        ("powershell-profile", "$PROFILE", Client),
        // 〔`K-R62` 09-11〕本机 POSIX 那一格补上之后升进 `TOOLS` 的那一条。
        // `Client`：它写的是 **cc-monitor 跑着的这台**的 rc（远端那份 rc 归 `ccm` 那两行）。
        // 路径是占位符而不是 `~/.bashrc`：那份 rc 由界面上的人从盘上真实存在的几份里选，
        // 申报一个我们其实没在用的常量，这一页会拿它去查一个没人写的路径再报「缺失」。
        ("posix-rc-aliases", "$POSIX_RC", Client),
        // 〔`K-R60` 09-11〕装不了、只读的那一档。`Either` 的理由与 cc-bus 那几条同源：
        // Claude Code 跑在哪台，这份记录就在哪台（`remote_history.rs` 真的从远端读它），
        // 标 `Client` 会让远端会话的用户在这一页上看到一句假话。
        ("claude-code", "~/.claude/projects/", Either),
    ];
    let mut actual: Vec<(&str, &str, HostScope)> = TOOLS
        .iter()
        .flat_map(|t| t.touches().map(move |f| (t.id, f.path, f.host)))
        .collect();
    let mut expect = want.to_vec();
    actual.sort_by_key(|(a, b, _)| (*a, *b));
    expect.sort_by_key(|(a, b, _)| (*a, *b));
    assert_eq!(
        actual, expect,
        "host 声明与钉死的表不一致——改 TOOLS 就要来改这张表，并说清为什么"
    );
}

/// **`host` 必须携带 `destination` 之外的信息**（T04 审计重要 1 的机器化）。
///
/// 审计核实：T04 第一版的 `destination → host` 是一张 **1:1 表**
/// （`RemoteHomeRelative→Remote`、`LocalHomeRelative→Either`、
///  `UserConfiguredPath→Remote`、`ProjectRelative→ProjectDir`、`UserShellProfile→Client`），
/// 于是那条"跨字段"断言在真实 `TOOLS` 上**可以完全由 destination 推出**
/// ——与 T02 被删的那颗钉子同一类。
///
/// 把两条标错的改对之后它才真正独立：`LocalHomeRelative` 同时映到
/// `Either`（settings.json / cc-*）与 `Remote`（~/.cc-bus/），
/// `UserConfiguredPath` 同时映到 `Remote`（两个占位符）与 `Either`（~/.claude-accts/）。
/// **这条测试就是"host 不是 destination 的函数"这句话的门禁**——
/// 哪天它变回 1:1，说明 host 退化成了冗余标签，那时该删掉这个字段而不是留着装样子。
#[test]
fn host_is_not_a_function_of_destination() {
    use std::collections::HashMap;
    let mut by_dest: HashMap<String, std::collections::HashSet<HostScope>> = HashMap::new();
    for t in TOOLS {
        // 🔴 〔`K-R81`〕`destination` 现在住在**载体**上 ⇒ 这条性质也按载体走。
        //    那不是顺手改写：它买到的东西比先前**多一格** —— 先前一个工具只有一个
        //    destination，`ccm` 那两条落点被同一个 key 盖住；今天两个载体各自入表。
        for c in t.carriers {
            let key = match &c.destination {
                ToolDestination::RemoteHomeRelative(_) => "RemoteHomeRelative",
                ToolDestination::LocalHomeRelative(_) => "LocalHomeRelative",
                ToolDestination::UserShellProfile => "UserShellProfile",
                ToolDestination::ProjectRelative(_) => "ProjectRelative",
                ToolDestination::UserConfiguredPath { .. } => "UserConfiguredPath",
                ToolDestination::NotInstalledByUs { .. } => "NotInstalledByUs",
            };
            for f in c.touches {
                by_dest.entry(key.to_string()).or_default().insert(f.host);
            }
        }
    }
    let multi: Vec<_> = by_dest
        .iter()
        .filter(|(_, hosts)| hosts.len() >= 2)
        .map(|(d, hosts)| (d.clone(), hosts.len()))
        .collect();
    // ★★ **门槛从 2 降到 1（P4c 08-12），账写在这里** —— 这不是为了让测试变绿。
    //
    // 本条命名的性质是「**`host` 不是 `destination` 的函数**」，
    // 而那句话被证伪当且仅当**每个** destination 都只映到一个 host。
    // 「≥2 个变体各多 host」是当年按**当时那张表**选的门槛，比性质本身更严。
    //
    // 今天真实变了一格：`P4a` 把 cc-bus 读面做成本机可用 ⇒ `~/.cc-bus/` 从
    // `Remote` 改成 `Either`（理由的长版住 `tool_registry.rs`）。
    // 于是 `LocalHomeRelative` 只剩 `{Either}`，多 host 的变体从 2 个降到 1 个
    // （`UserConfiguredPath` 仍同时映 `Remote` 与 `Either`）。
    //
    // ⇒ **字段没有退化成冗余标签**（头注那条「变回 1:1 就该删字段」的准则未触发），
    // 只是余量少了一格。降门槛的同时把这句话留下：**再少一格就真的是 1:1**，
    // 那时按头注办 —— 删字段，别留着装样子。
    assert!(
        !multi.is_empty(),
        "没有任何 destination 变体映到 ≥2 个 host ⇒ `host` 就是 `destination` 的函数、\
             这条跨字段守卫等于同义反复（T02 删掉的那颗钉子就是这个病）。\
             **此时该删掉 `host` 字段，而不是留着装样子**（见本条头注）。实得 {multi:?}，\
             全表 {by_dest:?}"
    );
}

/// **Phase G：保留 `Client`/`ProjectDir` 的理由是"合并会说假话"，这条把它钉住。**
///
/// 它们各只有 1 个使用者、零行为影响（都落 `project_onto_host` 的 `(_, other)`）
/// ——按 ≥2 尺子本该砍掉。保留的唯一理由是 `host_label` 是**用户可见事实**：
/// `$PROFILE` 确定在客户端、`.mcp.json` 确定在项目目录，合进 `Either` 后标签变成
/// 「本机或远端」，对这两条都是**假的**。
#[test]
fn host_labels_are_distinct_and_truthful() {
    use HostScope::*;
    let all = [Client, Remote, Either, ProjectDir];
    let labels: Vec<&str> = all.iter().map(|h| host_label(*h)).collect();
    // 四个标签互不相同——相同就说明该合并了
    let uniq: std::collections::HashSet<_> = labels.iter().collect();
    assert_eq!(uniq.len(), 4, "四个 host 标签必须互不相同，实得 {labels:?}");
    // **确定在客户端 / 确定在项目目录的，标签里不许出现"远端"**
    assert!(
        !host_label(Client).contains("远端"),
        "$PROFILE 确定在客户端，标签不许含「远端」：{}",
        host_label(Client)
    );
    assert!(
        !host_label(ProjectDir).contains("远端"),
        ".mcp.json 确定在项目目录，标签不许含「远端」：{}",
        host_label(ProjectDir)
    );
    // 而 Either 必须**明说**两端皆可（那是它存在的理由）
    assert!(host_label(Either).contains("本机") && host_label(Either).contains("远端"));
}

/// `host` 的四个变体都得有真实使用者。
///
/// **如实登记一处尺子不一致**（T04 审计重要 4）：这条用 **≥1**，而同文件
/// `user_configured_destinations_declare_a_placeholder_not_a_guess` 用 **≥2**。
/// 变体真实用户数：`Client` 1 条/1 工具、`ProjectDir` 1/1、`Either` 4 条/2 工具、`Remote` 4/3。
///
/// 为什么**不**统一到 ≥2：同文件的 `ToolSource` 5 个变体里 4 个是单用户（T01 保留了它），
/// `TouchEffect` 的门禁也只要求 `>= 3` 种被用到。**描述数据的 enum 允许变体各自单用户**
/// ——这一点 T01 论证过（变体差异是数据的本性，不是过度设计）。
/// 而 `UserConfiguredPath` 那条 ≥2 守的是**别的东西**：那是"这个变体值不值得存在"的判据，
/// 因为它是我为了不写死一个假常量而**新造**的。两把尺子各有其位，但**同一文件里并存
/// 就该写明**，不能让人以为是疏忽。
#[test]
fn all_host_scopes_are_really_used() {
    let used: std::collections::HashSet<_> = TOOLS
        .iter()
        .flat_map(|t| t.touches().map(|f| f.host))
        .collect();
    for want in [
        HostScope::Client,
        HostScope::Remote,
        HostScope::Either,
        HostScope::ProjectDir,
    ] {
        assert!(
            used.contains(&want),
            "{want:?} 没有任何使用者，那它不该存在"
        );
    }
}

/// **`host` 投影不许吞掉 `NeedsUserConfig`**：那个结果比 `Remote` 信息更多
/// （它告诉用户去哪儿看那个值），覆盖掉是降级。
#[test]
fn host_projection_preserves_the_richer_resolution() {
    // 🔴 〔`K-R81` 09-12〕`remote-daemon` 改名成 `backend`；而它今天有**三个载体**
    //    ⇒ 这里不许再拿 `touches[0]` 碰运气，要**点名那一份**（推给远端的那份）。
    let daemon = TOOLS.iter().find(|t| t.id == "backend").unwrap();
    let (c, f) = daemon
        .carrier_touches()
        .find(|(_, f)| f.host == HostScope::Remote)
        .expect("后端必须有一份是推给远端那台机器的");
    let r =
        resolve_touched_path(f.path, &c.destination, f.host, &home(), None, &no_dir).unwrap();
    match r {
        PathResolution::NeedsUserConfig { what } => {
            assert!(what.contains("daemon"), "实得 {what}");
        }
        other => panic!("远端投影把 NeedsUserConfig 吞成了 {other:?}"),
    }
}

/// **destination 的校验不许被 host 短路。**
///
/// 第一版 host 优先短路，于是 `LocalHomeRelative` 的"必须 `~/` 开头"与
/// "glob 只许在最后一段 / 只许一个 `*`"对所有 `Remote`/`Either` 的 touches
/// **完全不再执行**——而 10 条 touches 里 7 条是这两种 host。
#[test]
fn destination_checks_still_run_under_every_host() {
    for host in [
        HostScope::Client,
        HostScope::Remote,
        HostScope::Either,
        HostScope::ProjectDir,
    ] {
        for bad in ["/etc/passwd", "~/.local/*/bin", "~/.local/bin/*-*"] {
            let e = resolve_touched_path(
                bad,
                &ToolDestination::LocalHomeRelative("x"),
                host,
                &home(),
                None,
                &no_dir,
            );
            assert!(
                e.is_err(),
                "host={host:?} 时 {bad:?} 仍该判不自洽，实得 {e:?}"
            );
        }
    }
}

// ===== 注册表 ↔ 真写入方对齐（T02 审计重要 6：此前零耦合） =====

/// **申报的落点必须与真正执行写入的那段代码逐字一致。**
///
/// 审计说得对：此前没有一条测试把 `TOOLS` 的申报与 `sftp.rs` / `mcp.rs` /
/// `acct_iso_deploy` 对齐，所以这张告知页可以自信地说错而门禁不会红。
/// 追查下去比审计报的更严重——**六条声明里三条没有任何代码支撑**：
/// `remote-daemon` 的 `.local/bin/ccm-daemon` 全仓只出现在注册表自己里
/// （真实是 `RemoteConfig.daemon_path`）、`cc-acct-iso` 声明成本机而实际是远端 +
/// 前端传的 `dest_dir`、`cc-bus` 的落点是未实现的愿景。
/// 前两条已改成 `ToolDestination::UserConfiguredPath`（承认"这是配置项"），
/// 剩下**真有常量**的两条在这里用 `pin_definition` 钉死。
///
/// 🔴 **〔`K-R63` 09-11 登记，本轮没治它〕本条与那一族是同一个病，而它今天仍是专名的。**
///
/// 「申报 ↔ 现实」这条性质在 `tool_registry` 的两个 `bool`（`installable` / `uninstallable`）
/// 上已经收成**一条覆盖全表**的判据了
/// （`tool_registry.rs::every_tool_declares_install_and_uninstall_as_the_implementations_really_are`
/// ：对拍表与 `TOOLS` 的 id 集合逐字相等，多一条少一条都红）。
/// **本条守的是同一族的第三格 `destination`，而它逐个工具手写、只钉了 `TOOLS` 里的两条**
/// （`ccm` 与 `project-mcp`；这个「两条」是下面那段代码自己数得出来的，不写死在这里）。
/// ⇒ 别的工具的 `destination` 申报错了，**这一格今天不会红** ——
/// 与 `K-R63` 之前 `uninstallable` 的处境逐字同形。
///
/// 为什么 `K-R63` 没顺手收它：那一件的两条 dod 逐字只说 `installable` / `uninstallable`
/// 两格，多做一格是「比该做的宽了一格」（`brief` 第 17 条）。⇒ **登记在这里，等 PM 裁**，
/// 不靠人记得。
#[test]
fn declared_destinations_are_pinned_to_the_real_writers() {
    use crate::structural_scan::pin_definition;

    // ① ccm：`sftp.rs` 里那个常量就是真落点
    let sftp = include_str!("../../src/bridge/src/sftp.rs");
    pin_definition(
        sftp,
        r#"const CCM_CLI_REMOTE_PATH: &str = ".local/bin/ccm";"#,
        "const CCM_CLI_REMOTE_PATH",
        "ccm 远端落点",
    )
    .unwrap();
    let ccm = TOOLS.iter().find(|t| t.id == "ccm").unwrap();
    // 🔴 〔`K-R69` 09-12〕`ccm` 现在**两台机器上各一个落点** ⇒ 这一格从
    //    「与 `CCM_CLI_REMOTE_PATH` 相等」变成「**远端那一半**与它相等」。
    //    本机那一半钉在别处（`tool_registry` 的
    //    `the_declared_local_ccm_path_really_matches_the_name_we_install`：
    //    申报的那个串要盖得住 `local_backend::local_ccm_entry_name()` 真放下去的名字）——
    //    两处钉的是两个真落点，别在这里再抄一份本机那个名字（`13b`：闭集只许一个住址）。
    // 🔴 〔`K-R81` 09-12〕`ccm` 现在是**两个载体**（远端 shim / 本机那份改名副本）
    //    ⇒ 这一格从「那个双值变体逐字相等」改成「**远端那个载体**的落点相等」。
    //    本机那一半仍钉在别处（`tool_registry` 的
    //    `the_declared_local_ccm_path_really_matches_the_name_we_install`）——
    //    两处钉的是两个真落点，别在这里再抄一份本机那个名字（`13b`：闭集只许一个住址）。
    let remote_dests: Vec<&ToolDestination> = ccm
        .carriers
        .iter()
        .map(|c| &c.destination)
        .filter(|d| matches!(d, ToolDestination::RemoteHomeRelative(_)))
        .collect();
    assert_eq!(
        remote_dests,
        vec![&ToolDestination::RemoteHomeRelative(".local/bin/ccm")],
        "注册表声明的 ccm 远端落点与 sftp.rs 的 CCM_CLI_REMOTE_PATH 不一致"
    );

    // ② 项目 MCP：`mcp.rs` 真正 join 的就是这个文件名
    let mcp = include_str!("../../src/bridge/src/mcp.rs");
    let joins = mcp.matches(r#"join(".mcp.json")"#).count();
    assert!(
        joins >= 1,
        "mcp.rs 里找不到 join(\".mcp.json\")——落点变了还是扫描器失效了？"
    );
    let pm = TOOLS.iter().find(|t| t.id == "project-mcp").unwrap();
    assert_eq!(
        pm.carriers
            .iter()
            .map(|c| &c.destination)
            .collect::<Vec<_>>(),
        vec![&ToolDestination::ProjectRelative(".mcp.json")]
    );

    // ③ 反向自检：确认上面读到的是真源码，不是空串
    assert!(
        sftp.len() > 10_000 && mcp.len() > 5_000,
        "include_str! 读空了"
    );
}

/// **配置项型落点不许再冒充常量**：`UserConfiguredPath` 的申报路径必须是占位符
/// （`$` 开头），否则就是又一次"凭印象写个常量"。
#[test]
fn user_configured_destinations_declare_a_placeholder_not_a_guess() {
    let mut n = 0;
    for t in TOOLS {
        // 🔴 〔`K-R81`〕按**载体**判：占位符要出现在**它自己那个载体**的 touches 里，
        //    不是「这个工具的某一条 touch 里」—— 后者在多载体下会**静默配错对**。
        for c in t.carriers {
            if let ToolDestination::UserConfiguredPath { token, what } = &c.destination {
                n += 1;
                assert!(token.starts_with('$'), "{}: {token:?} 不像占位符", t.id);
                assert!(
                    !what.trim().is_empty(),
                    "{}: 得告诉用户去哪儿看这个值",
                    t.id
                );
                // 这个占位符必须真出现在 touches 里，否则表格上那一行会显示别的东西
                assert!(
                    c.touches.iter().any(|f| f.path == *token),
                    "{}: 载体「{}」的 touches 里没有 {token:?}",
                    t.id,
                    c.what
                );
            }
        }
    }
    // 计数自检：≥2 个使用者才配有这个变体（本工作区的 ≥2 判据）
    assert!(
        n >= 2,
        "UserConfiguredPath 只有 {n} 个使用者——不够 2 个就不该是一个变体"
    );
}

/// 🔴 `KR60D2`：**这个视图的人群 = 环境清单的闭集**，一项不多、一项不少。
///
/// 〔`K-R57` 摸底：它今天只看 `TOOLS` 的 `touches` —— **10 条路径 / 6 个工具**，
/// 而 app 真正要的环境项现打 **17** 项。于是「齐了没有」这个问题它答不了，
/// 而用户读到的是一张看起来很干净的表（模块头注自己写的是
/// 「cc-monitor 到底动过你哪些文件」—— 拿它当「环境齐了没有」用是**分母对不上**）。〕
///
/// **死值验**：往闭集里加一项而不动这个视图（或把建表退回 `TOOLS.iter()`）⇒ 本条红。
#[test]
fn the_view_population_is_exactly_the_closed_set() {
    use crate::tool_registry::environment;
    use std::collections::BTreeSet;
    let h = home();
    let fs = empty_probe();
    let rows = build_rows(&env_with(&h, &fs, Some("/usr/bin")));
    let shown: BTreeSet<&str> = rows.iter().map(|r| r.tool_id).collect();
    let want: BTreeSet<&str> = environment().iter().map(|e| e.id).collect();
    assert!(
        !want.is_empty(),
        "闭集是空的 —— 先查 environment()，别改断言"
    );
    assert_eq!(
        shown, want,
        "这一页的人群与环境清单的闭集对不上 —— 少掉的那几项，\n\
             用户在这一页上**看不见**，而这一页正是他问「齐了没有」时唯一能看的地方。\n\
             （闭集住 `tool_registry::environment`，它是唯一一份；这里不许再抄一张名单。）"
    );
}

// ===== `K-R65`：「提示用户装」那一档**真的会出声** =====

/// 一台**假机器**：`PATH` 上只有 `/usr/bin`，那里只放着 `present` 里列的那几个名字，
/// 家目录下只放着 `home_files` 里那几条绝对路径。
///
/// ⚠ 名字刻意取**中性**的（`present` / `home_files`），不含被断言的任何子串
/// 〔`6g`：断言用的子串别取自夹具的名字〕。
fn machine_with<'a>(
    present: &'a [&'a str],
    home_files: &'a [&'a str],
) -> impl Fn(&Path) -> Option<(bool, u64)> + 'a {
    move |p: &Path| {
        let s = p.to_string_lossy().into_owned();
        let on_path = present.iter().any(|n| s == format!("/usr/bin/{n}"));
        if on_path || home_files.contains(&s.as_str()) {
            Some((false, 42))
        } else {
            None
        }
    }
}

/// 🔴 `KR65D1` 的正题：**这一档真去查，而且「缺了」与「查不动」是两回事。**
///
/// 上一版这一档的行为逐字是「`state` 一律 `Undetermined`、`path_resolved` **故意不解析**」
/// ⇒ 一台缺了 `tmux` 的机器和一台查不动的机器，在这一页上**长得一模一样**。
///
/// 本条三格一起断（缺一格都能装样子）：
///   ① 装着的 ⇒ `Present`，而且 `path_resolved` **真解析出来了**（不再是 `None`）；
///   ② 没装的 ⇒ `Absent` —— **查了、确认没有**，前端据此劝人去装；
///   ③ 读不到 `PATH` ⇒ `Undetermined { why }` —— **查不动**，绝不说成「不存在」。
///
/// **失效方向**（件计划逐字）：「把 9 项的 `tier` 改个名就收工」⇒ ①②③ 全红。
/// **死值验**：把 `observe_unmanaged` 的 `OnPath` 那一支退回 `(None, Undetermined{..})`
/// ⇒ ①② 红；把 `Some(false)` 那一臂也答成 `Undetermined` ⇒ ② 红（而 ③ 仍绿，
/// 那正说明 ② 与 ③ 是分开的两格）。
#[test]
fn the_prompt_tier_really_looks_before_it_speaks() {
    use crate::tool_registry::{environment, EnvTier};
    let h = home();
    // `git` 装着、`tmux` 没装 —— 两条都在「你自己装」那一档里。
    let meta = machine_with(&["git"], &[]);
    let fs = FsProbe {
        meta: &meta,
        list: &|_| None,
    };

    let rows = build_rows(&env_with(&h, &fs, Some("/usr/bin")));
    let pick = |id: &str| {
        rows.iter()
            .find(|r| r.tool_id == id)
            .unwrap_or_else(|| panic!("这一页上没有 `{id}` 这一行"))
    };

    // 反向自检：这两条**确实在那一档**（不然下面断的是别人）。
    for id in ["git", "tmux"] {
        let e = environment().into_iter().find(|e| e.id == id).unwrap();
        assert_eq!(
            e.tier,
            EnvTier::UserInstallsWePrompt,
            "`{id}` 不在「{}」那一档 —— 先查闭集，别改断言",
            EnvTier::UserInstallsWePrompt.label()
        );
    }

    // ① 装着的：真解析出来了 + Present
    let ok = pick("git");
    assert!(
        matches!(ok.state, SurfaceState::Present { .. }),
        "PATH 上有它，这一行却不是 Present —— 实得 {:?}",
        ok.state
    );
    assert!(
        ok.path_resolved.is_some(),
        "这一档上一版**故意不解析**（「解析了就等于查了」）—— \
             今天它必须解析，否则「真去查」这句话是假的"
    );

    // ② 没装的：**Absent**，不是 Undetermined —— 这一格就是本件买到的东西
    let gone = pick("tmux");
    assert_eq!(
        gone.state,
        SurfaceState::Absent,
        "PATH 上查过、确认没有，这一行却没说「缺」——\n\
             那正是 `K-R60` 那一版的行为（一片 Undetermined），用户读不出自己缺了什么。"
    );

    // ③ 读不到 PATH：查不动 —— 与 ② **必须是两回事**
    let blind_rows = build_rows(&env_with(&h, &fs, None));
    let blind = blind_rows.iter().find(|r| r.tool_id == "tmux").unwrap();
    match &blind.state {
        SurfaceState::Undetermined { why } => assert!(
            !why.is_empty(),
            "「查不动」必须带理由，否则它和「缺失」在观感上没区别"
        ),
        other => panic!("读不到 PATH 时必须是「查不动」，实得 {other:?}"),
    }
    assert_ne!(
        gone.state, blind.state,
        "「查了、确认没有」与「查不动」显示成了同一格 —— \
             件计划 `KR65D1` 的死值验逐字要求它们是两回事"
    );
}

/// `KR65D1` 的死值验落点：**同一个名字，换一种查法，出来的是不同的一格。**
///
/// 「把某一项的探测掐掉、让它变成『查不动』」这个动作在这里可以直接做出来 ——
/// 三种 [`EnvProbe`] 各喂一次，三格互不相同。
///
/// ⚠ 断言用的是 `SurfaceState` 的**变体**，不是措辞里的子串
/// 〔`6g`：断言用的子串别取自夹具的名字，也别靠一句话恒真〕。
#[test]
fn the_same_name_under_three_probes_gives_three_different_cells() {
    let h = home();
    let meta = machine_with(&[], &[]);
    let fs = FsProbe {
        meta: &meta,
        list: &|_| None,
    };
    let env = env_with(&h, &fs, Some("/usr/bin"));

    // 查得动、确认没有 ⇒ 缺
    let (_, missing) = observe_unmanaged("tmux", EnvProbe::OnPath, &env);
    assert_eq!(missing, SurfaceState::Absent);

    // 探测掐掉 ⇒ 查不动（**同一个名字、同一台机器**，只换了查法）
    let (_, blind) = observe_unmanaged(
        "tmux",
        EnvProbe::CannotProbe {
            why: "这一支是死值验用的：把探测掐掉，看它会不会被显示成「缺」",
        },
        &env,
    );
    assert!(matches!(blind, SurfaceState::Undetermined { .. }));
    assert_ne!(
        missing, blind,
        "掐掉探测之后这一行仍然说「缺」—— 那是替用户下了一个他没做过的结论"
    );

    // 第三种查法：`~/` 路径。同一台空机器上它也该是「缺」，而不是「查不动」——
    // 否则「查不动」就成了万能挡箭牌。
    let (_, home_missing) = observe_unmanaged("~/.local/bin/x-probe", EnvProbe::HomePath, &env);
    assert_eq!(home_missing, SurfaceState::Absent);
    // 而申报了一个根本不是路径的名字 ⇒ 如实说查不动，不静默显示成空
    let (shown, bad) = observe_unmanaged("$SOMETHING", EnvProbe::HomePath, &env);
    assert!(shown.is_none());
    assert!(matches!(bad, SurfaceState::Undetermined { .. }));
}

/// 🔴 `KR65D2` 的上屏那一半：**「档」进了线上形状，而且欠装口那一档不许被读成
/// 「不该我们装」。**
///
/// **死值验**：把 `SurfaceRow::tier` 摘掉 ⇒ 编译不过；
/// 把 `unmanaged_row` 里 `AppShipsNoInstallerYet` 那一支的措辞换成
/// 「不由 cc-monitor 提供」（也就是与「你自己装」那一支同文）⇒ 本条红。
#[test]
fn every_row_carries_its_tier_and_the_owed_one_never_reads_as_not_ours() {
    use crate::tool_registry::{environment, EnvTier};
    use std::collections::BTreeSet;
    let h = home();
    let fs = empty_probe();
    let rows = build_rows(&env_with(&h, &fs, Some("/usr/bin")));

    // ① 每一行的档 = 闭集里那一项的档（不是这一页自己算的第二份）
    let want: std::collections::HashMap<&str, EnvTier> =
        environment().iter().map(|e| (e.id, e.tier)).collect();
    for r in &rows {
        assert_eq!(
            Some(&r.tier),
            want.get(r.tool_id),
            "`{}` 这一行的档与闭集对不上",
            r.tool_id
        );
    }
    // ② 档在这一页上真有区分力（一档一色的表等于没有档）
    let seen: BTreeSet<EnvTier> = rows.iter().map(|r| r.tier).collect();
    assert!(
        seen.len() >= 3,
        "这一页上只出现了 {} 档（{:?}）—— 分母是 {} 档",
        seen.len(),
        seen.iter().map(|t| t.label()).collect::<Vec<_>>(),
        EnvTier::ALL.len()
    );

    // ③ 欠装口那一档：两半都要说到，且**不许**说成「不由 cc-monitor 提供」
    let owed: Vec<&SurfaceRow> = rows
        .iter()
        .filter(|r| r.tier == EnvTier::AppShipsNoInstallerYet)
        .collect();
    assert!(!owed.is_empty(), "这一页上一行「欠装口」都没有 —— 先查闭集");
    for r in &owed {
        assert!(
            r.source_label.contains("该由 cc-monitor 自带"),
            "`{}` 的「从哪来」没说清这是我们该自带的东西，实得 {:?}",
            r.tool_id,
            r.source_label
        );
        assert!(
            !r.source_label.contains("不由 cc-monitor 提供"),
            "`{}` 的措辞把「欠的实现」说成了「不是我们提供的」—— \
                 那与 `K38` 矛盾（`KR65D2` 逐字：不会被读成「不该我们装」）",
            r.tool_id
        );
        assert!(
            r.effect_label.contains("还没写") || r.effect_label.contains("没写"),
            "`{}` 的「我们做什么」没说清装口是欠着的，实得 {:?}",
            r.tool_id,
            r.effect_label
        );
    }
}

// ===== 建表：七个字段都真被用上（T01 审计 I2 的验收点） =====

#[test]
fn rows_cover_every_touched_file_and_use_all_spec_fields() {
    let f = empty_probe();
    let h = home();
    let rows = build_rows(&env_with(&h, &f, Some("/usr/bin")));
    // 〔`K-R60`〕人群换成闭集之后，行数 = 有 ToolSpec 那一半的 touches 数
    //   + 手写那一半每项一行。**两半都现算**，不写死一个数〔`13b`〕。
    let expected: usize = TOOLS.iter().map(|t| t.touches().count()).sum::<usize>()
        + crate::tool_registry::UNMANAGED_ENV.len();
    assert_eq!(
        rows.len(),
        expected,
        "有 ToolSpec 的每条 touches 一行、手写的每项一行"
    );
    for r in &rows {
        assert!(!r.tool_id.is_empty() && !r.tool_name.is_empty()); // id / display_name
        assert!(!r.source_label.is_empty()); // source
        assert!(!r.effect_label.is_empty()); // touches[].effect
        assert!(!r.path_declared.is_empty()); // touches[].path
    }
    // destination：远端那条必须解析不出本机路径
    // 🔴 〔`K-R81`〕`remote-daemon` → `backend`，而它今天有三行 ⇒ 点名远端那一行。
    let daemon = rows
        .iter()
        .find(|r| r.tool_id == "backend" && r.host_label == host_label(HostScope::Remote))
        .unwrap();
    assert!(daemon.path_resolved.is_none());
    // installable / uninstallable：三种组合都真出现在表里 —— 两个字段都得有区分力
    // 〔`K-R60` 订正：cc-bus 原先在这里被当成「两者都 false」的样本，
    //  而那个 false 是一处**假申报**（部署 08-13 就实现了）。样本换成 `claude-code`
    //  —— 它是真的装不了（Claude Code 不该由 cc-monitor 装）。〕
    let ccbus = rows.iter().find(|r| r.tool_id == "cc-bus").unwrap();
    assert!(ccbus.installable && !ccbus.uninstallable, "装得了、卸不了");
    let ccm = rows.iter().find(|r| r.tool_id == "ccm").unwrap();
    assert!(ccm.installable && ccm.uninstallable);
    let cc = rows.iter().find(|r| r.tool_id == "claude-code").unwrap();
    assert!(!cc.installable && !cc.uninstallable, "装不了、也就无所谓卸");
    // note：至少两个工具用上了
    let with_note: std::collections::HashSet<_> = rows
        .iter()
        .filter(|r| r.note.is_some())
        .map(|r| r.tool_id)
        .collect();
    assert!(
        with_note.len() >= 2,
        "note 至少两个工具用上，实得 {with_note:?}"
    );
}

/// `host` 必须进到行里（T04）——不上屏的话用户分不出说的是哪台机器。
#[test]
fn rows_carry_the_host_label() {
    let h = home();
    let fs = empty_probe();
    let rows = build_rows(&env_with(&h, &fs, Some("/usr/bin")));
    for r in &rows {
        assert!(!r.host_label.is_empty(), "{} 缺 host 标签", r.path_declared);
    }
    // 四档措辞各不相同，且能看出"哪台"
    let labels: std::collections::HashSet<_> = rows.iter().map(|r| r.host_label).collect();
    assert!(
        labels.len() >= 3,
        "至少三种 host 出现在表里，实得 {labels:?}"
    );
    let ps = rows.iter().find(|r| r.path_declared == "$PROFILE").unwrap();
    assert_eq!(ps.host_label, "本机");
    let ccm = rows
        .iter()
        .find(|r| r.path_declared == "~/.local/bin/ccm")
        .unwrap();
    assert_eq!(ccm.host_label, "远端");
}

/// `GenerateOnly` 的措辞必须**明确说我们不写**——这是用户定的调，写错了就是失信。
#[test]
fn generate_only_wording_says_we_do_not_write() {
    let l = effect_label(TouchEffect::GenerateOnly);
    assert!(l.contains("不写"), "实得 {l:?}");
    assert!(l.contains("待贴文本"));
    assert!(effect_label(TouchEffect::ReadOnly).contains("不写"));
}

// ===== B04 登记项：settings 的多个作用域 =====

#[test]
fn settings_scopes_include_local_and_admit_project_is_unchecked() {
    let f = FsProbe {
        meta: &|p| {
            if p.to_string_lossy().ends_with("settings.json") {
                Some((false, 10))
            } else {
                None
            }
        },
        list: &|_| None,
    };
    let read = |p: &Path| {
        if p.to_string_lossy().ends_with("settings.local.json") {
            Some("{\"hooks\":{\"SessionStart\":\"cc-register\"}}".to_string())
        } else {
            Some("{}".to_string())
        }
    };
    let s = build_settings_scopes(&home(), None, &no_dir, &read, &f);
    assert_eq!(s.len(), 3);
    // E67①：**按「路径分量」比，不按斜杠比**。原来写的是
    // `s[0].path.ends_with("/.claude/settings.json")`，在 Windows 上恒假 ——
    // `PathBuf::from("/h").join(".claude")` 产出的是 `/h\.claude`（`join` 用平台分隔符），
    // 于是这条测试**在主平台上从来没绿过**（CI 连红两个版本的那一条）。
    // `Path::ends_with` 比的是完整分量，跨平台成立，而且比字符串后缀更严
    // （`xx.claude/settings.json` 这种半个分量的巧合匹配不上）。
    assert!(Path::new(&s[0].path).ends_with(Path::new(".claude").join("settings.json")));
    assert!(Path::new(&s[1].path).ends_with(Path::new(".claude").join("settings.local.json")));
    // local 里有钩子字样 → 必须报 true（B04 的病：只看第一份会说"没装"）
    assert_eq!(s[1].has_cc_bus_hooks, Some(true));
    assert_eq!(s[0].has_cc_bus_hooks, Some(false));
    // 项目级：**明说没查**，且不给 has_cc_bus_hooks 一个假答案
    assert_eq!(s[2].scope, "项目级");
    assert_eq!(s[2].has_cc_bus_hooks, None);
    match &s[2].state {
        SurfaceState::Undetermined { why } => {
            assert!(why.contains("没查"), "实得 {why}");
            assert!(why.contains("可能是错的"), "要点明结论可能错，实得 {why}");
        }
        other => panic!("项目级不该有确定结论，实得 {other:?}"),
    }
}

/// 读不到文件时 `has_cc_bus_hooks` 必须是 `None`（**不猜 false**）。
#[test]
fn unreadable_settings_does_not_claim_absence_of_hooks() {
    let s = build_settings_scopes(&home(), None, &no_dir, &|_| None, &empty_probe());
    assert_eq!(s[0].has_cc_bus_hooks, None);
    assert_eq!(s[1].has_cc_bus_hooks, None);
}

/// **本模块只准读，不准写**（红线）。守法是**白名单**而不是"不许出现哪些写法"
/// ——本会话的教训：黑名单版本被审计用五种我没想到的写法绕过，
/// 而白名单枚举每一处 `fs::` 用法并要求它们**都**在允许集合里，新写法自动被拦。
/// ★ **`PathResolution` 的分派不许有兜底臂**〔audit-0805 08-06〕。
///
/// # 它钉的是「谁是被偶然守住的」那一类
///
/// 本页（配置面审计）把每一项的解析结果渲染给用户。原来那条 match 是
/// `_ => None` —— 七个变体里**四个**落进兜底、显示为空，而这在界面上与
/// 「这一项不存在」**长得一模一样**：用户读不出区别，判据也不会红。
///
/// 已把兜底换成四条具名臂（零行为变更）。⇒ 第 8 个变体会**编译失败**。
/// 但「靠编译器」本身是个**没人盯的前提**：谁再加一条 `_`，穷尽性当场消失。
/// 本条就钉这一件事，与 `watcher.rs` 那条同型（daemon 侧七路信号分派）。
#[test]
fn the_path_resolution_dispatch_has_no_catch_all_arm() {
    // 与隔壁 `this_module_only_reads` 用同一种剥法（按首个 cfg-test 切），
    // 免得同一文件里出现第二套口径。
    let src = include_str!("../../src/bridge/src/config_surface.rs");
    let prod = src
        .split(concat!("#[cfg", "(test)]"))
        .next()
        .unwrap_or(src)
        .to_string();
    let prod = prod.as_str();
    // ⚠ **第一版锚在 `PathResolution::Local(p) =>` 上，而那个字符串在本文件里
    //   出现两次** —— 它命中的是更早的另一处 match，于是判据一直在看错对象：
    //   给我真正要守的那处加回兜底臂，本条**照样绿**（变异实测）。
    //   ⇒ 与 F19 同族（断言指的不是它自称的那个东西）。
    //   改成扫**所有** `PathResolution` 分派，不再挑一个锚点。
    let lines: Vec<&str> = prod.lines().collect();
    let ind = |l: &str| l.len() - l.trim_start().len();
    let mut arms = 0usize;
    let mut offenders: Vec<usize> = Vec::new();
    for (n, l) in lines.iter().enumerate() {
        if l.trim_start().starts_with("PathResolution::") {
            arms += 1;
            continue;
        }
        if !l.trim_start().starts_with("_ =>") {
            continue;
        }
        let d = ind(l);
        for k in (0..n).rev() {
            let prev = lines[k];
            if prev.trim().is_empty() {
                continue;
            }
            if ind(prev) < d {
                break;
            }
            if ind(prev) == d && prev.trim_start().starts_with("PathResolution::") {
                offenders.push(n + 1);
                break;
            }
        }
    }
    assert!(
        arms >= 8,
        "只扫到 {arms} 条 `PathResolution::` 臂（08-06 实测 10+）—— 抽取坏了，本条此刻是空转的"
    );
    assert!(
        offenders.is_empty(),
        "`PathResolution` 的分派里出现了兜底臂（生产段第 {offenders:?} 行）。\n\
             ⚠ 后果不是报错，是**这一项在审计页上显示为空** —— 与「它不存在」看起来一样。\n\
             新增变体请写成具名臂；确实不显示也请显式写出来并加一句为什么。"
    );
}

#[test]
fn this_module_only_reads() {
    let src = include_str!("../../src/bridge/src/config_surface.rs");
    let code = src.split(concat!("#[cfg", "(test)]")).next().unwrap_or(src);
    // 剥掉注释与文档：注释里出现 `fs::write` 这个词不该判红（本会话踩过：
    // 把注释当代码，一条守卫数出 3 处而实际只有 1 处）
    let stripped: String = code
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//")
        })
        .collect::<Vec<_>>()
        .join("\n");
    // 反向自检：剥完还得看得见真代码，否则守卫在空转
    assert!(
        stripped.contains("pub fn resolve_touched_path"),
        "剥过头了，守卫在空转"
    );
    // **扫任意前缀的 `fs::`，不只 `std::fs::`**（T02 审计重要 2 实测可绕）：
    // 注入 `use std::fs;` + `fs::write(...)` 后旧守卫 17/17 全绿，因为它只找字面
    // `std::fs::` 前缀、而 `write` 也不在那 4 个禁用词里。同类绕法还有
    // `tokio::fs::write`、`std::os::unix::fs::symlink`。
    let mut uses: Vec<String> = Vec::new();
    for (i, _) in stripped.match_indices("fs::") {
        let rest = &stripped[i + "fs::".len()..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            uses.push(name);
        }
    }

    // ★★ **「把写交给别人」也算写**〔audit-0805 08-07〕。
    //
    // 上面扫的是 `fs::` 前缀 —— 那只认「自己写」。08-07 实测：本模块加一句
    // `crate::utils::atomic_write_json(p, v)`，**全仓 982 条判据一条不红**，
    // 而本条的名字逐字写着「只读 / 不写」。与 F44（`tokio::io::copy` 绕过写流判据）
    // 同一族：**动作类判据锚在「这个动作长什么样」上，就漏掉别的做法**。
    //
    // 「谁在写」不在这里各写一张清单（那是下一个漂移源），而是问唯一那张表：
    // `write_site_registry::WRITE_SITES`。它自己由默认拒绝的人群守着。
    let delegated =
        crate::write_site_registry::writers::called_by(&stripped, "config_surface.rs");
    assert!(
        delegated.is_empty(),
        "本模块调了已登记的**写者**：{delegated:?}\n\
             ⚠ 前缀扫描看不见这种写法 —— 写发生在被调方，本文件里一个 `fs::` 都不出现。\n\
             真要写盘：先想清楚本模块「只读」这条性质还成不成立，\n\
             再把落点登记进 `write_site_registry::WRITE_SITES`。"
    );
    // 自检：那张表非空，否则上面一句是空转。
    assert!(
        !crate::write_site_registry::writers::names().is_empty(),
        "`WRITE_SITES` 是空的 —— 上面那条在空转"
    );
    // 允许集合就这三个，全部只读
    for u in &uses {
        assert!(
            matches!(u.as_str(), "metadata" | "read_dir" | "read_to_string"),
            "本模块只准只读的 fs 调用，发现 fs::{u}"
        );
    }
    // 计数自检（要件 3）：一处都没扫到 = 守卫失效了，而不是代码变干净了
    assert!(
        uses.len() >= 3,
        "只扫到 {} 处 fs:: 用法——守卫可能失效了（期望 metadata/read_dir/read_to_string 各至少一处）",
        uses.len()
    );
    // **钉死 `use` 列表**（要件 4：逃生口的定义必须逐字钉住）。不钉的话
    // `use tokio::fs as fs;` 之类能把上面的白名单整体架空。
    let uses_lines: Vec<&str> = stripped
        .lines()
        .map(|l| l.trim())
        .filter(|l| l.starts_with("use "))
        .collect();
    assert_eq!(
        uses_lines,
        vec![
            "use crate::tool_registry::{",
            "use std::path::{Path, PathBuf};",
        ],
        "本模块的 use 列表被改了——它是上面那条 fs:: 白名单的前提，改了要重新论证"
    );
    // ★〔audit-0805 08-06 复核〕**上面那条「use 列表钉死」已实测验过**：
    // 往本模块插 `use std::fs::{self as _f, write};` ⇒ 当场红，诊断逐字
    // 「本模块的 use 列表被改了——它是上面那条 fs:: 白名单的前提」。
    // ⚠ 为什么专门来验：同一天在 daemon 侧实测到**同一个改写绕过了那边的 fs:: 白名单**
    //（`readonly_guard`，六条判据全绿），原因就是那边**没有**钉 use 列表这一手。
    // ⇒ 本模块用对了 T01 要件 4（逃生口的定义要逐字钉住），而它是被 daemon 那次反衬出来的。
    // 且明确不许出现这些（即便将来换成别的前缀写法，上面的白名单也已经兜住 std::fs::）
    for bad in [
        "OpenOptions",
        "create_dir",
        "remove_file",
        "set_permissions",
    ] {
        assert!(!stripped.contains(bad), "本模块不得出现 {bad}——这一页只读");
    }
}
