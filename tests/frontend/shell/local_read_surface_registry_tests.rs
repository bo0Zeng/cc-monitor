use std::fs;
use std::path::{Path, PathBuf};

/// 本机读面登记表：`(相对 src/frontend/shell 的路径, 外层函数 / 项, 类别, 为什么 + 退役归谁)`。
///
/// 每一处碰本机 claude 目录的源码按「文件 × 外层项」认（[`crate::guard_support::enclosing_fn`]）：
/// 新的一处 ⇒ 下面那条红（直读点不许无声地长）；退役了 ⇒ **也红**（把那一行删掉）。
const REGISTERED: &[(&str, &str, &str, &str)] = &[
    (
        "src/stream_source/run.rs",
        "stream_loop",
        "remote",
        "流循环 hello 那一臂解出的**远端** `claude_dir`（打日志用）。说的是远端主机的 claude 目录 ⇒ 不属本机读面。",
    ),
    (
        "src/stream_source/frame.rs",
        "InboundFrame",
        "remote",
        "backend `hello` 帧的 `claude_dir` 字段（冻结的线上格）。说的是远端主机的 claude 目录 ⇒ 不属本机读面。",
    ),
    (
        "src/stream_source/frame.rs",
        "parse_frame",
        "remote",
        "解 `hello` 帧时取 `claude_dir` 那一格。远端主机的目录 ⇒ 不属本机读面。",
    ),
    (
        "src/stream_source/frame.rs",
        "claude_home_from_hello",
        "remote",
        "hello 里远端后端自陈的 Claude home（优先 `homes`、回退 `claude_dir`）。远端的目录 ⇒ 不属本机读面。",
    ),
    (
        "src/config.rs",
        "claude_dir_override",
        "hub",
        "**路径源头**：设置里 `claudeDir` 覆盖原值那一格，只回答「用户填了什么」，自己不读内容；\
             Claude 目录本身只由那台后端解析 ⇒ 不属退役范围。",
    ),
    (
        "src/local_backend_host.rs",
        "CLAUDE_DIR_ENV",
        "non-read",
        "交给后端的那个环境变量名。一个字节的用户数据都不读 ⇒ 不属退役范围。",
    ),
    (
        "src/local_backend_host.rs",
        "backend_env",
        "non-read",
        "把设置里填的 Claude 目录原样交给后端（起常驻后端、一次性 `--resident-stop` 都走它）。不读内容 ⇒ 不属退役范围。",
    ),
    (
        "src/local_backend_host.rs",
        "backend_env_from",
        "non-read",
        "同上一行的纯函数那一半：有就塞进环境变量。不读内容 ⇒ 不属退役范围。",
    ),
];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// 一处「碰本机 claude 目录」的源码形态。运行时拼，免得命中本文件自己的说明。
fn needles() -> Vec<String> {
    let c = "claude";
    vec![
        format!("{c}_dir"),
        format!("CLAUDE_CONFIG_DIR"),
        format!(".{c}/projects"),
        "records_dir".to_string(),
        // **`.claude` 这个目录名本身也要算**。
        //
        // 原来四个针里最"宽"的是 `.claude/projects` —— 于是
        // `home.join(".claude")`、`~/.claude/settings.json`、`.claude.json`
        // 这些**同样是本机 claude 面**的写法一个都不在人群里。
        // 实测加上它之后：本机读面从 60 行涨到 78 行（+18），
        // **没有新文件**，全落在已登记的五个文件上 ——
        // 也就是说漏的不是"某个没人知道的模块"，而是**已登记文件里没被数到的那些行**。
        // 那更坏：登记表看起来是全的，数字却比事实小，而 F10 的工作量正是按这个数估的。
        format!(".{c}"),
    ]
}

/// 一份生产段里命中的那些行各自所在的函数名（去重前）。
fn hit_fns(prod: &str) -> Vec<String> {
    let ns = needles();
    // 行尾注释里的提及不算读点：先走共享剥法砍掉行尾注释（行数不变，行号与外层函数的对应不动）。
    let code = guard_core::strip_trailing_comments(prod);
    let lines: Vec<&str> = code.lines().collect();
    (0..lines.len())
        .filter(|&i| ns.iter().any(|n| lines[i].contains(n.as_str())))
        .map(|i| crate::guard_support::enclosing_fn(&lines, i))
        .collect()
}

/// 递归遍历 `src/`，**不是硬编码文件名单**。
fn rust_files() -> Vec<(String, String)> {
    let src = root().join("src");
    let mut out = Vec::new();
    // 人群 ＝ 本 crate 的 `src/` ＋ manifest 明写的兄弟包（窗口包 · 通道 · 宿主原语 · 开窗契约，
    //   `guard_core::population_trees`）—— monitor 的代码搬进去了，人群不变；兄弟包的键带包名。
    let trees =
        std::iter::once(("src".to_string(), src.clone())).chain(guard_core::population_trees(&src));
    for (label, tree) in trees {
        let mut stack = vec![tree.clone()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p: PathBuf = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_some_and(|x| x == "rs") {
                    let rel = format!(
                        "{label}/{}",
                        p.strip_prefix(&tree)
                            .unwrap_or(&p)
                            .to_string_lossy()
                            .replace('\\', "/")
                    );
                    out.push((rel, fs::read_to_string(&p).unwrap_or_default()));
                }
            }
        }
    }
    out.sort();
    out
}

/// ★★ **谁伸手进用户的 home —— 一张覆盖索引**〔audit-0805 08-08，Phase G 第 88 件〕。
///
/// # 为什么再加一张表（它不是第四个权威源）
///
/// 「谁能读」这一侧本会话补了两块：`.ssh`（第 85 件）与本模块的 claude 目录棘轮。
/// 但两块的人群都是**按目录名**取的 —— 也就是说，**下一个被伸手的目录仍然不在任何人群里**
///（08-08 实测：`~/.config` / `~/.local` 本机读面今天为 0，那是「碰巧没有」不是「有人守着」）。
///
/// ⇒ 换一个**真正派生**的人群：生产段里每一处 `home_dir()` 调用。
/// 它按构造覆盖 claude / `.ssh` / profile / `.codex` / 数据目录 / 将来任何新目录。
/// 第四列写的是**这一处归谁管** —— 本表只回答「有没有人管」，
/// 具体守法仍在各自那张表里（E3：本表不复制它们的内容）。
///
/// ⚠ **扫描面只有 monitor 树**（本模块的 `rust_files()` 就是这么定的）。
/// backend 侧另有 3 处 `home_dir()`（`observe/accounts_query.rs`），**刻意不并进来**：
/// 那是**远端那台机器上**的 home，语义不同（monitor 碰的是用户自己的机器），
/// 而后端的写侧由它自己的 `readonly_guard` 整个禁掉。
/// 把两侧混进一张表会让「这一处归谁管」这一列失去意义。
const HOME_REACHES: &[(&str, &str, &str, &str)] = &[
    // monitor 那份 Codex 适配器取 `~/.codex` 那一行摘了：适配器随适配表删了（`~/.codex` 住哪只剩后端 `agents/codex/`）。
    (
        "footprint_client.rs",
        // 足迹判定进了后端，monitor 只答它自己那台那几行的事实（`with_monitor_probe`〔散文墓碑〕删了）。
        "footprint_client_facts",
        "足迹里 monitor 自己那几行的根（`home` 那一格）",
        "只读诊断页；只交 monitor 自己进程的几条事实，不 stat，落点由本模块的 claude 棘轮数着",
    ),
    // 数据位置页列本机后端住在家里的那几样：只 stat（在不在 · 多大），不读内容。
    (
        "data_paths.rs",
        "collect",
        "`~/.cc-monitor` 里后端那几样（`bin/` · `staging/` · 两把钥匙）",
        "**不是伸手拿用户的东西**：cc-monitor 自己的家（后端与 monitor 同住）。只 `is_file` / `is_dir` / 长度，\
             不读字节 —— 两把钥匙的内容一个字节都不碰；`home_dir()` 只为「每个用户各一份」",
    ),
    // `data_paths.rs` 探 `$PROFILE` 候选目录那一行摘了：界面经通道问本机后端（`$PROFILE` 在哪只有后端方言答）。
    // 钩子诊断那一行摘了：本机那份读盘进了本机后端（`hooks-diag`），monitor 不再伸手进用户 home。
    // 这里原来有 `aliases_read`〔散文墓碑〕一行（它自己 `home_dir()` 再直读）。读回口改问那台后端
    //   （`files-home` / `files-peek`，本机远端同一条），这一条不再伸手进用户 home ⇒ 摘行。
    // 这里原来还有 `aliases_install`〔散文墓碑〕一行（它自己 `home_dir()`、再交本进程落盘）。
    // 别名那一族整个进了那台后端（`aliases-*`）。
    //   写改走本机后端之后，home 由后端答（`user_files::Door::home`），这一条不再伸手进用户 home ⇒ 摘行。
    (
        "ccm_probe.rs",
        "local_ccm_entry_now",
        "`~/.cc-monitor/bin/<本机 ccm 入口名>`（`K-R69`：在不在 + 它自报的身份）",
        "**不是伸手拿用户的东西**：这是 monitor 自己的目录，那一份也是我们自己放下去的\
             （写侧登记在 `write_site_registry` 的 `local_backend.rs::extract_embedded_to`；它就是后端本身）。\
             `home_dir()` 只为「每个用户各一份」。\
             🔴 **它刻意够不到 `~/.local/bin/ccm`** —— 用户那份旧的由产品**一个字节都不碰**\
             （`K34` 逐字：原本的配置要手动删除）；那一份的存在与否是靠**跑一次 `--ccm-probe`**\
             问出来的，不是靠 stat 一个路径（比路径认不出同名不同物）。",
    ),
    // 文件窗口程序的落点：monitor 旁边没有它时，把自带那份放到 `~/.cc-monitor/bin/` 再起。
    (
        "proc.rs",
        "landing_dir",
        "`~/.cc-monitor/bin/cc-monitor-filewin[.exe]`（文件窗口程序）",
        "**不是伸手拿用户的东西**：monitor 自己的目录（与自释放出来的本机后端同一个）。只在开窗而 exe 旁边没有它时放；\
             写侧登记在 `write_site_registry` 的 `local_backend.rs::place_local_program`",
    ),
    // `("local_backend_host.rs", "cc_monitor_dir")` 那一行摘了：钥匙与「谁在听」改住这台的家（`config::resolve_monitor_data_dir`，
    //   门牌只跟着家走），不再单独 `home_dir()` 拼一份。
    // 拨号代理的二进制解析那一行**摘了**：monitor 不再找 / 起拨号代理（拨号挪进本机常驻后端）。
    (
        "local_backend_host.rs",
        "start_local_backend",
        "`~/.cc-monitor/bin`（P2z 自释放内嵌后端的落点）",
        "**不是伸手拿用户的东西**：这是 monitor 自己的缓存目录，只有我们写、只有我们读。\
             它用 `home_dir()` 只是为了「每个用户各一份」。写侧登记在 `write_site_registry` 的\
             `local_backend.rs::extract_embedded_to`；释放出来的文件按 build_id 命名 ⇒ 幂等、不覆盖别版",
    ),
    // `("local_accounts.rs", "local_accts_dir")` 那一行同拍去掉 —— 它自己写的退役条件兑现了：
    //   钉契约目录名的那条判据搬到了后端（`accounts_query_tests.rs::the_accounts_library_lives_under_the_contract_directory_name`，
    //   对 `acct-core` 的常量与后端缺省解析那一处），本机那份参照实现连同这一处 `home_dir()` 一起删了。
    // `mcp.rs` 那一行（`.claude.json` 三候选的 `home_dir()`〔散文墓碑〕）去掉：MCP 列表改问那台后端，monitor 不再伸手进 home 找它。
    // `("config.rs", "resolve_claude_dir")` 那一行摘了：Claude 目录只由那台后端解析。
    (
        "config.rs", // 原 `paths.rs`
        "resolve_monitor_data_dir",
        "`~/.cc-monitor`",
        "monitor 自己的数据目录；写侧在 `write_site_registry`",
    ),
    // `profile_installer.rs` 那一行（围栏拿 `home_dir()` 当基准）摘了：围栏的 home 今天问那台后端（`fence_on`）。
    (
        // 原生文件管理窗口那颗「本机」按钮的落脚点。
        "shell.rs",
        "local_home",
        "home 本身（当**起点路径**）",
        "它不是「伸手拿东西」，是给那个窗口一个开始浏览的地方 —— \
             之后列哪个目录**由用户走到哪决定**，而列目录那一条是 \
             `filewin::source::list_local`（只读 `read_dir` ＋ `metadata`，不落盘）。\
             ⚠ 与 profile 围栏（今天是后端 `block.rs::fence`）**不是同一类**：\
             那一条拿 home 划界（围栏），这一条只是起点，**它不围任何东西** —— \
             也就是说「用户能在这个窗口里浏览到 home 之外」是设计如此，不是漏了围栏。\
             写侧归 `filewin::transfer`（上传经通道：monitor 的传输台只写远端暂存区，\
             落进用户目录那一下是后端 `files-commit-upload`，先过围栏）",
    ),
    // `~/.ssh/config` 那两行出表：读 ssh config 与 `~` 展开随导入搬进后端（`dial/ssh_config.rs`）。
];

/// ★ 正题：**每一处 `home_dir()` 都要在表里，且表里不留死行**。
#[test]
fn every_reach_into_the_user_home_is_indexed() {
    let files = rust_files();
    let needle = format!("home_{}()", "dir");
    let mut found: Vec<(String, String)> = Vec::new();
    for (rel, src) in &files {
        let prod = guard_core::production_code(src);
        let lines: Vec<&str> = prod.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            if !l.contains(&needle) || l.trim_start().starts_with("fn ") {
                continue;
            }
            // 归属按「上一处 `fn 名字`」判（粗，但归错会红在名字对不上上）。
            let mut fname = "<找不到外层函数>".to_string();
            for prev in lines[..=i].iter().rev() {
                if let Some(rest) = prev
                    .split(" fn ")
                    .nth(1)
                    .or_else(|| prev.strip_prefix("fn "))
                {
                    fname = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    break;
                }
            }
            let stem = rel.rsplit('/').next().unwrap_or(rel).to_string();
            found.push((stem, fname));
        }
    }
    found.sort();
    found.dedup();
    let mut declared: Vec<(String, String)> = HOME_REACHES
        .iter()
        .map(|(f, n, _, _)| (f.to_string(), n.to_string()))
        .collect();
    declared.sort();
    assert_eq!(
        found, declared,
        "伸手进用户 home 的落点变了。\n\
             ★ 本表回答的是「**有没有人管**」，不是「怎么管」——多出来的那一处，\n\
             要在第四列写清它归哪张表（claude 棘轮 / `.ssh` 读面表 / profile 围栏 / 写点表 …）。\n\
             ⚠ 之所以按 `home_dir()` 取人群而不是按目录名：按目录名取的话，\n\
             **下一个被伸手的目录仍然不在任何人群里** —— 08-08 实测 `~/.config`/`~/.local`\n\
             本机读面为 0，那是「碰巧没有」不是「有人守着」。\n\
             ⚠ 少了的：那一处被删/改名了 ⇒ 删登记；若是**扫描面缩了**（`rust_files()` 少扫了），\n\
             先修扫描面，别改表。"
    );
}

/// ★ 抽取器自检。
#[test]
fn the_scan_actually_reads_the_monitor_tree() {
    let files = rust_files();
    // 遍历器正控：入口 `lib.rs` 与窗口包里那份必在人群里（manifest 明写的兄弟包也要被走到）。
    for must in ["src/lib.rs"] {
        assert!(
            files.iter().any(|(n, _)| n == must),
            "遍历器没走到 `{must}`（走到 {} 份）—— 本条会零命中地绿",
            files.len()
        );
    }
    let me = files
        .iter()
        .find(|(n, _)| n == "src/local_read_surface_registry.rs")
        .map(|(_, s)| s.as_str())
        .expect("扫不到本文件");
    assert!(
        guard_core::production_code(me).len() < me.len() / 2,
        "本文件剥完还剩一半以上 —— 剥法没生效，说明文字会被当成命中"
    );
}

/// ★ 目录内容 == 登记表：每一处命中按「文件 × 外层函数」两向相等。
#[test]
fn the_local_read_surface_matches_the_registry_line_for_line() {
    let mut want: Vec<(String, String)> = REGISTERED
        .iter()
        .map(|(f, func, ..)| ((*f).to_string(), (*func).to_string()))
        .collect();
    want.sort();
    want.dedup();
    let mut got: Vec<(String, String)> = rust_files()
        .into_iter()
        .flat_map(|(rel, raw)| {
            hit_fns(&guard_core::production_code(&raw))
                .into_iter()
                .map(move |f| (rel.clone(), f))
        })
        .collect();
    got.sort();
    got.dedup();
    assert_eq!(
        got, want,
        "\n本机读面与登记表对不上（按「文件 × 外层函数」认）。\n\
             **多一处** = 直读点在增长 —— 先回答它属哪一类（`hub` 路径源头 / `reader` 真读内容 / `payload` 只拼串），\n\
             `reader` 还要写退役归属。\n\
             **少一处** = 退役了 —— 把登记表那条删掉。"
    );
}

/// ★ `reader` 这一类**必须**写退役归属；`hub`/`payload` 必须说清为什么不属退役范围。
#[test]
fn every_reader_names_its_retirement_owner() {
    let mut readers = 0;
    for (f, _, kind, why) in REGISTERED {
        assert!(
            matches!(
                *kind,
                "hub"
                    | "reader"
                    | "payload"
                    | "remote"
                    | "non-read"
                    | "fence"
                    | "write"
                    | "no-counterpart"
            ),
            "{f} 的类别 `{kind}` 不在三类里 —— 新类别要先在模块头注那张表里定义"
        );
        match *kind {
            "reader" => {
                readers += 1;
                assert!(why.contains("退役归"), "{f} 记成 reader 却没说谁退役它");
            }
            _ => assert!(
                why.contains("不属"),
                "{f} 记成 `{kind}` 却没说清为什么不属退役范围 —— \
                     那样它会被下一个人当成 F10 的工作量"
            ),
        }
    }
    // 本机直读点（`reader`）今天是零：monitor 不再自己读 claude 目录的内容，读都问后端。
    assert_eq!(
        readers, 0,
        "又登记了 `reader`（monitor 自己读 claude 目录的内容）—— 读面归后端，先问能不能改问后端"
    );
}

/// ★ **前提触发器 —— 已经触发过一次，这是它的后继形态**（F05b，2026-08-04）。
///
/// # 它原来长什么样、为什么要换
///
/// 原形：断言 `tauri.conf.json` 里**没有** `externalBin`，一出现就红并喊
/// 「F05b 落地了 ⇒ F10 的正题现在能做了」。
///
/// F05b 落地时它**确实红了**，而且红得对。但落地形态与它预设的不同：
/// `externalBin` **没有**进主配置 —— 因为 `tauri-build` 要求**当前 target** 的 local_backend
/// 在编译期就存在，进主配置会让 `cargo test` 也需要一份后端二进制，
/// 那正是 C2 反面（两半不许在构建期互相咬住）刚钉住的东西。
/// ⇒ 它住进**发版补丁配置** `tauri.sidecar.conf.json`，只在 `tauri build --config` 时注入。
///
/// ⇒ 本条换成后继形态：**盯新的家**，并且钉住「棘轮一格没放」。
/// ⚠ **这不是降强度**：断言从「一条」变成「三条」（local_backend 契约有家 · stem 与
/// `LOCAL_BACKEND_STEM` 一致 · 棘轮上限没被放宽），而且扫描面从主配置**换到了它真正的家** ——
/// 留在旧扫描面上才是降强度（它永远不会再红）。
#[test]
fn the_local_backend_contract_has_exactly_one_home_and_f10s_ratchet_is_untouched() {
    // ① local_backend 契约必须有家，而且**不在主配置里**（进主配置 = 每个编译点都要一份二进制）。
    let key = format!("external{}", "Bin"); // 运行时拼，免得命中本文件自己的说明
    let main_conf =
        fs::read_to_string(root().join("tauri.conf.json")).expect("读不到 tauri.conf.json");
    assert!(
        main_conf.contains("\"identifier\""),
        "tauri.conf.json 里没有 `identifier` —— 读到的不是那份配置？"
    );
    assert!(
        !main_conf.contains(key.as_str()),
        "`{key}` 回到了主配置 —— 那会让 `cargo test` 也需要一份当前 target 的后端二进制\n\
             （实测报错：`resource path binaries/cc-monitor-backend-<triple> doesn't exist`），\n\
             等于把两半在**构建期**绑死。它的家是 `tauri.sidecar.conf.json`，只在发版时 `--config` 注入。"
    );
    let patch = fs::read_to_string(root().join("tauri.sidecar.conf.json"))
        .expect("读不到 tauri.sidecar.conf.json —— local_backend 契约没有家了");
    assert!(
        patch.contains(key.as_str()),
        "发版补丁配置里没有 `{key}` —— 那安装包里就不会带上本机后端（C7）"
    );

    // ② stem 与 Rust 侧的 `LOCAL_BACKEND_STEM` 必须是同一个（同一个名字不许两侧各写一份，定框 §4）。
    let stem = crate::local_backend::LOCAL_BACKEND_STEM;
    assert!(
        patch.contains(&format!("binaries/{stem}")),
        "补丁配置里的本机后端路径与 Rust 侧的 `LOCAL_BACKEND_STEM`（{stem:?}）对不上 —— \n\
             消费侧 `resolve_with` 找的是 `{stem}-<triple>` 与裸 `{stem}`，\n\
             两边写不一样 ⇒ 安装包里带了一个谁也找不到的文件。"
    );

    // ⚠ **刻意不在这里再钉一遍 `reader` 的条数。**
    // 那个数（今天 11）已经由同模块的
    // `every_reader_names_its_retirement_owner` 钉着（这里原先点的名字全仓不存在，改指真在数它的那一条）；
    // 在这里抄第二份就是「判据存了源头的副本」（定框 §4 逐字禁止）——
    // F11 的 E4 变异就是被那种副本骗过去的。
    //
    // ⇒ F10 的交接写在本条头注与里，不写成第二个数字：
    // **F05b 已落地、本机后端真的起起来了**（真机实测日志逐字为
    // `本机后端: Started { pid: 6072, attempt: 1 }`），所以 F10 的正题现在能做 ——
    // 把那些 `reader` 直读点切到后端，然后把那条棘轮往下拧。
    // ⚠ **F01b 留的那条死限已由 P3 刀 0 解除**（原文：「本地 sid 一进 `tmux_raw_registry`〔散文墓碑〕，
    // `/branch` 的灰点 bug 会回来」）。当时成立，是因为本地那条 diff **只产 `Gone`**；
    // P3 刀 0 让它按 `pid + procStart` 判出 `Superseded`（要正面证据，缺 `procStart` 退回 `Gone`）
    // ⇒ 进表之后 `/branch` 会走 `(Some(origin), Superseded)` = 归档，不再是灰点。
    // 判出它的今天是本机后端（`session_removed.cause` ⇒ 会话账本裁成 `session_state`），monitor 只转交成品。
    // ★ 留着这段而不是删掉：**限制解除的理由本身是要交代的** ——
    // 否则下一个人只看到限制没了，不知道换了什么在保证它。
}
