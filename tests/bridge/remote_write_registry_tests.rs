use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// 写原语。**刻意含本机也会用的那几个** —— 人群是超集，分类靠登记。
///
/// ⚠ **运行时拼**，照 `local_read_surface_registry::needles()` 的既有先例。
/// 首跑实测：写成 `const &[&str]` 字面量之后，`atomic_replace_registry` 当场把本文件
/// 报成「有原子替换调用点没登记：`remote_write_registry.rs [rename] 1 处`」——
/// 它用 `strip_comment_lines` 剥注释（所以头注里那串没事），但**不剥 `cfg(test)` 段**，
/// 于是这里的字符串字面量被当成了一处真实 `.rename(` 调用。
/// ⇒ 判据的针写成字面量，就会被**别的判据**当成靶子。
fn write_prims() -> Vec<String> {
    let dot = ".";
    [
        "write",
        "create",
        "remove_file",
        "remove_dir",
        "rename",
        "create_dir",
        "set_metadata",
        "write_all",
    ]
    .iter()
    .map(|m| format!("{dot}{m}("))
    .collect()
}

/// 落地何方。**刻意是封闭集合** —— 多出第三种就得回来论证。
/// 〔SR1b · 2026-09-24〕**两档都摘了**：「本机」那一档唯一的成员（下载落地）随传输台搬进本机常驻后端；
/// 「远端」那一档随 monitor 零 SFTP 会话一起空了（部署经本机后端的 `files` 链路，写只许两处）。
/// 「封闭集合不许长草」那条逐字要这一刀 —— 哪天 monitor 又拿到一个 SFTP 会话，先回来答它落在哪。
const LANDINGS: &[&str] = &[];

/// `(相对 src 的文件, 函数, 落地何方, 写什么 + 路径由谁定 + 什么围栏着)`
///
/// ⚠ **登记表不是豁免清单**：新增一处没登记的 ⇒ 下面第一条红。
#[allow(clippy::type_complexity)]
const REMOTE_WRITES: &[(&str, &str, &str, &str)] = &[
    // 🔴 〔SR1b · 2026-09-24〕**表空了：monitor 进程一个 SFTP 会话都不拿**（用户 V89「SFTP 进本机常驻后端」）。
    //   从前这里五行都在 `sftp.rs`：原子上传的核心 · 逐级建目录 · 卸载后端 · 入口那两个 `fenced_block::Store` 原语
    //   （`upload_atomic`〔散文墓碑〕· `SftpFile`〔散文墓碑〕那一对）。执行那一半整段进了本机后端 `dial/sftp.rs`
    //   （写只许 `~/.cc-monitor/staging/` 与 `~/.cc-monitor/bin/`，每处先过 `fenced_remote`，由后端
    //   `readonly_guard::remote_write_layer` 钉）；monitor 这一侧只剩判定，经 `dial_host::RemoteFs` 一问一答。
    //   ⇒ 本表的人群根（`capability_holders`）今天是**空集**，由 `the_remote_write_capability_is_still_confined_to_three_files`
    //   钉成零命中（带正控）。更早那几段的来历（F10 / F11 / F89a 经远端后端 · 传输台进后端）见 git 历史。
    // 更早摘掉的几行各留一块：F11 那条远端直删 `remove_remote_file`〔散文墓碑〕（RW1 改经远端后端删）·
    //   入口助手自己逐级建目录的 `install_remote_ccm_helper`〔散文墓碑〕（AL1 收进 `fenced_block::apply`）·
    //   传输台那两处 `upload_to_staging`〔散文墓碑〕· `download_inner`〔散文墓碑〕（SR1b 进本机后端 `control/transfer.rs`）。
];

/// 从三个「持有 SFTP 会话」的文件里抠出 `(文件, 函数)` —— 函数体内调了写原语。
fn write_sites() -> Vec<(String, String)> {
    let root = repo_root();
    let prims = write_prims();
    let mut out = Vec::new();
    for f in capability_holders() {
        let raw = std::fs::read_to_string(root.join("src/bridge/src").join(&f))
            .unwrap_or_else(|e| panic!("{f} 读不到：{e} —— 文件搬了就把登记一起改"));
        let prod = guard_core::production_code(&raw);
        let mut cur = String::new();
        for line in prod.lines() {
            let t = line.trim_start();
            // `fn 名(` 起一个新作用域。闭包不算（闭包里的写归外层函数）。
            //
            // ⚠ **不许用前缀白名单**〔本条首跑就红在这里〕：第一版列了
            // `["pub async fn ", "pub fn ", "async fn ", "fn "]`，**漏掉 `pub(crate) async fn`** ——
            // 于是 `sftp.rs` 里那个逐级建目录的助手（正是这个可见性；〔SR1b〕它随执行那一半搬去了本机后端）没起新作用域，
            // 它那处 `create_dir` 被记到**上一个函数** `read_profile_text` 头上，
            // 而那个函数一个写原语都没有。诊断当场点名了 `read_profile_text`。
            // ⇒ 改成**按结构判**：在 `fn ` 之前的东西必须全是可见性/修饰符 token。
            // 这样 `pub(in crate::x) const unsafe fn` 之类的写法也认得。
            if let Some((head, rest)) = t.split_once("fn ") {
                let modifiers_only = head.split_whitespace().all(|w| {
                    w == "pub"
                        || w.starts_with("pub(")
                        || w == "async"
                        || w == "const"
                        || w == "unsafe"
                        || w == "extern"
                        || w.starts_with('"')
                });
                if modifiers_only {
                    if let Some(name) = rest.split(['(', '<']).next() {
                        cur = name.trim().to_string();
                    }
                }
            }
            if cur.is_empty() {
                continue;
            }
            if prims.iter().any(|p| line.contains(p.as_str()))
                && !out.iter().any(|(g, m)| *g == f && *m == cur)
            {
                out.push((f.clone(), cur.clone()));
            }
        }
    }
    out.sort();
    out
}

/// 「持有远端写能力」= **拿到过一个 SFTP 会话**。这是人群的根。
///
/// ★★〔G 审计逮到的〕**第一版按「源码里出现 SFTP 会话类型名」判，恒绿。**
///
/// 漏掉的是 `acct_iso_deploy.rs`：它 `let conn = connect_sftp(&cfg).await?;`
/// `let sftp = &conn.sftp;` 然后往**用户给的 `dest_dir`** 写文件
/// （`#[tauri::command] deploy_remote_acct_iso`）——**类型靠推断，那个名字一次都没写出来**。
///
/// ⇒ 又一次「按拼法取样」，而本条的 doc 注释里逐字写着
/// 「**凡「本条是某一族唯一的哨兵」的判据，人群必须按事实取样，因为它没有第二道网**」。
/// **写下那句话的判据，自己犯了那句话说的错** —— 本区第三次量到同一条。
///
/// ⇒ 改成按**能力从哪来**取样：SFTP 会话在本仓只有两个出处 ——
/// `sftp::connect_sftp()`（自己开一条）与 `sftp_pool::with_sftp()`（从池里借）。
/// 拿不到会话就写不了远端，这是结构事实，绕不过去。
/// 类型名仍然留在针里（`sftp.rs`/`sftp_pool.rs` 自己要靠它进人群）。
///
/// ⚠ 仍然失效的形态如实登记：把会话再包一层自己的 newtype 传出去，本条看不见那一层。
/// 真要堵死得走可见性（`pub(in crate::sftp) struct RemoteWriter(SftpSession)`），
/// 那样「远端写能力在哪几个模块」就是**编译器答案**而不是 grep 答案。已登记，未做。
fn capability_holders() -> Vec<String> {
    let root = repo_root().join("src/bridge/src");
    let mut out = Vec::new();
    for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
        let prod = guard_core::production_code(&src);
        // 拼出来的，免得命中本文件自己的说明。
        let ty = format!("Sftp{}", "Session");
        // 会话的两个出处：自己开一条 / 从池里借。拿不到会话就写不了远端。
        let opens = format!("connect_{}(", "sftp");
        let borrows = format!("with_{}(", "sftp");
        if prod.contains(&ty)
            || prod.contains("russh_sftp")
            || prod.contains(&opens)
            || prod.contains(&borrows)
        {
            out.push(
                path.strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    out.sort();
    out
}

/// ★ 正题一：**默认拒绝** —— 那三个文件里每一处写原语调用都得说清落地何方。
#[test]
fn every_write_primitive_in_a_capability_holder_is_classified() {
    let sites = write_sites();
    // 〔SR1b · 2026-09-24〕地板（「只抠到 N 处写点 —— 抽取器坏了」）翻成**零命中**：人群根（持 SFTP 会话的文件）
    //   今天是空集 ⇒ 写点也是零。「抽取器没瞎」那一半由下面那条人群根判据的正控钉（同一把针喂合成语料必亮）。
    //   〔从前的读数史：08-10 实测 10；步 23b +1 = 11；`sftp_chmod` +1 = 12；RW1 / SR1b 搬走之后 6 → 0。〕
    assert!(
        sites.is_empty(),
        "monitor 里又有持 SFTP 会话的文件在调写原语了：{sites:?} —— 远端写只许住本机后端 `dial/sftp.rs`"
    );
    let missing: Vec<String> = sites
        .iter()
        .filter(|(f, n)| !REMOTE_WRITES.iter().any(|(g, m, ..)| g == f && m == n))
        .map(|(f, n)| format!("  {f}::{n}"))
        .collect();
    assert!(
        missing.is_empty(),
        "这些写点没登记：\n{}\n\n\
             ★ 它们在一个**持有 SFTP 会话**的文件里 —— 也就是说这段代码手上有\n\
             「往别人机器上写」的能力。要回答三件事，都不许留空：\n\
             ① **落地何方**（{LANDINGS:?}）—— 机器分不清 handle 来历，这是人的答案；\n\
             ② **路径由谁定**（代码定的固定落点？还是用户在面板里选的？）；\n\
             ③ **什么围栏着**（用户选路径的必须过 Claude 数据防误伤围栏，见第三条判据）。",
        missing.join("\n")
    );
    // 反向：登记的必须真在（改名/删了就该删条目）。
    for (f, n, ..) in REMOTE_WRITES {
        assert!(
            sites.iter().any(|(g, m)| g == f && m == n),
            "登记表里的 `{f}::{n}` 已经不在源码里了 —— 删掉这条，别留僵尸账"
        );
    }
}

/// ★ 正题二：落地何方在封闭集合里，且说明不许是占位。
#[test]
fn a_registered_write_says_where_it_lands_and_who_picks_the_path() {
    for (f, n, landing, why) in REMOTE_WRITES {
        assert!(
            LANDINGS.contains(landing),
            "`{f}::{n}` 的落地写的是「{landing}」，不在 {LANDINGS:?} 里。\
                 多出一种就得回来论证 —— 本表的全部意义是分清「写谁的机器」。"
        );
        assert!(
            why.chars().count() > 20,
            "`{f}::{n}` 的说明太短，像是占位：「{why}」"
        );
        // 「路径由谁定」是本表的核心信息之一，措辞可以变，但必须提到「路径」。
        assert!(
            why.contains("路径") || why.contains("落点"),
            "`{f}::{n}` 没说**路径由谁定**（代码定的固定落点 / 用户选的）。\
                 那决定了它要不要围栏 —— 说明里必须提到「路径」或「落点」。"
        );
    }
    // 封闭集合不许长草。
    for l in LANDINGS {
        assert!(
            REMOTE_WRITES.iter().any(|(_, _, x, _)| x == l),
            "落地集合里的「{l}」今天一处都没人用 —— 删掉它"
        );
    }
}

/// `sftp_pool.rs` 今天对外开了哪几条 `#[tauri::command]`（已排序）。
///
/// 🔴 **这个数本身就是一条现打订正**，所以它只许有一个住址：
/// `设计/99 §4.6.4` 写的是 14，而那第 14 个匹配住在 `sftp_chmod` 的头注里
/// —— 「错的 grep 与截断的 grep 是同一种失败」。
///
/// 〔步 H2 09-21〕从 [`the_file_window_uses_exactly_the_pool_commands_it_registers`]
/// 的函数体里提出来，**一行逻辑没改**。提出来的唯一理由：
/// [`every_pool_command_is_either_a_registered_write_or_a_registered_read`] 要拿
/// **同一份**人群去做「写 / 读 两分」那条相等断言，而一个闭集只许有一个住址。
fn pool_commands() -> Vec<String> {
    let pool = std::fs::read_to_string(repo_root().join("src/bridge/src/sftp_pool.rs"))
        .expect("sftp_pool.rs 读不到");
    let commands = tauri_commands_of(&guard_core::production_code(&pool));
    // 抽取器自检：`sftp_pool.rs` 到底有几条对外命令。
    assert_eq!(
        commands.len(),
        // 〔第四波 S4〕1 → 0：最后一条（零流量复制）随门禁 `f3-copy` 那一格退役一起删了。
        //   〔F7c 收尾 09-24〕13 → 1 那一拍删了其余十二条（`设计/60 §13b`）。
        0,
        "`sftp_pool.rs` 现打 {} 条 `#[tauri::command]`（〔第四波 S4〕现打 **0**；〔F7c 收尾〕1；2026-09-21 现打 13；\
         `设计/99 §4.6.4` 写的是 14，而含注释的 grep 数出来正是 14 —— \
         「错的 grep 与截断的 grep 是同一种失败」）。实得：{commands:?}",
        commands.len()
    );
    // 🔴 正控：人群今天是空集 ⇒ 抽取器同一段逻辑喂一份合成语料，证明它认得出命令
    //    （否则「0 条」与「抽取器坏了」在这里长得一模一样）。
    let attr = format!("#[tauri::{}]", "command");
    let fake = format!(
        "{attr}\npub async fn alpha(x: u8) {{}}\n{attr}\npub fn beta<T>() {{}}\nfn gamma() {{}}\n"
    );
    assert_eq!(
        tauri_commands_of(&fake),
        vec!["alpha".to_string(), "beta".to_string()],
        "抽取器认不出合成语料里那两条命令 —— 上面那个 0 不可信"
    );
    commands
}

/// 一段生产代码里带 `#[tauri::command]` 的函数名（排好序）。
fn tauri_commands_of(pool_prod: &str) -> Vec<String> {
    let attr = format!("#[tauri::{}]", "command");
    let mut commands: Vec<String> = Vec::new();
    let lines: Vec<&str> = pool_prod.lines().collect();
    for (i, l) in lines.iter().enumerate() {
        if l.trim() != attr {
            continue;
        }
        // 属性的下一行就是那条命令的签名（rustfmt 下如此）。
        let Some(sig) = lines.get(i + 1) else {
            continue;
        };
        let Some((_, rest)) = sig.split_once("fn ") else {
            continue;
        };
        if let Some(name) = rest.split(['(', '<']).next() {
            commands.push(name.trim().to_string());
        }
    }
    commands.sort();
    commands
}

/// 一个顶层 `fn` 的函数体到哪一行为止。
///
/// ★ 用**第一行行首的 `}`** 判 —— 那是 rustfmt 下顶层项的真实收尾。
/// ⚠ 第一版用「下一行以 `pub ` 或 `#[tauri::command]` 开头」，两个洞：
/// ① 非 `pub` 的顶层 `fn`（本仓的 `fn guard_write` / `async fn upload_inner`）不终止窗口
///    ⇒ `sftp_write_text` 的窗口吞进了 `guard_write` 的**定义**，判据当场假绿；
/// ② `sftp_upload` 的窗口宽到 109 行、把整个 `upload_inner` 吞了进去。
/// ⇒ 又一次「终止条件按拼法列白名单」。
fn fn_body_end(lines: &[&str], start: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, l)| l.starts_with('}'))
        .map(|(i, _)| i + 1)
        .unwrap_or(lines.len())
}

// 〔FN1 · 第四波 4C · 2026-09-25 · 用户 V119〕这里原来是「正题三」两条：用户选路径的远端写必须过 Claude 数据防误伤围栏
//   （写死的写入口人群，函数体里必须有那道拒绝）· 两个路径参数的写入口两个参数各自过一次（死值验 `M7` 逼出来的那条）。
//   用户「文件管理器全部都可以改. 不需要任何围栏」⇒ 那道拒绝（`claude_data_fence` 里的拒绝那一半）连同它最后一个调用方
//   （`sftp_pool::transfer_call` 开下载单那一判）删了；两条的人群在那之前已经是「一条」与「零条」，今天都是零 ⇒ 靶子不在，一起退役。
//   ⚠ 退役的是**围栏的存在性**这一维；池子里再长出命令（写或读）仍由下面那条两分判据与 `sftp_family_registry_tests` 逼它归档。

/// ★ 正题四：**接线层** —— 对外 IPC 入口必须真的转发到一个已登记的原语点。
///
/// # 为什么单列
///
/// 〔audit-0805 V7-2〕当初点名的十个名字里有五个是**IPC 包装**
/// （`sftp_write_text` / `sftp_upload` / `deploy_remote_backend` /
/// `write_remote_mcp_server` / `delete_remote_history_session`）——
/// 它们自己**不调写原语**，所以按能力边界派生的人群里**一个都没有**。
///
/// ⚠ 〔步 12·C 收尾 09-20〕上面那句话是 audit-0805 当时的**现打**，留着不改；
/// 但今天它里面的 `write_remote_mcp_server` 与 `delete_remote_history_session`
/// **已经不是 IPC 命令**了（`origin` 归一把它们并进了本机同族那条）。
/// 它们仍留在下面那张路由表里，而且**必须留** —— 本表这一条判的是
/// 「**按得到的那一层 ↔ 真正写盘的那一层**」这条边，而合并之后那条边是
/// `write_project_mcp_server`（命令）→ `write_remote_mcp_server`（远端分支）
/// → `upload_atomic`（写点）。本表是**一跳**的（中间垫一层，「按钮 ↔ 真实写点」这条边就表达不出来），
/// 所以它钉的是后半跳；前半跳由 `origin_tests::every_origin_taking_command_splits_local_through_route`
/// 钉着（吃 `Origin` 的命令 ↔ 走 `route` 的命令，两向集合相等）。
///
/// ⇒ 两份名单只重合五个。那不是谁错了，是**两个不同的层**：
/// 「谁按下按钮」与「谁真的写」。本表的人群是后者，而前者是外面看得见的那一层
/// ⇒ 必须有一条边把它们连起来，否则改了转发目标没人会红。
#[test]
fn the_ipc_entry_points_route_through_a_registered_write_site() {
    let root = repo_root();
    // `(入口所在文件, 入口名, 它该转发到的已登记写点)`
    const ROUTES: &[(&str, &str, &str)] = &[
        // 〔SR1b · 2026-09-24〕**空了**：`deploy_remote_backend → upload_atomic` · `deploy_remote_acct_iso → ensure_dir_all`
        //   两条边的后半跳（monitor 里的 SFTP 写点）整段进了本机后端；按钮今天经 `dial_host::RemoteFs` 交给后端执行，
        //   「按钮 ↔ 真实写点」那条边跨进了后端那棵树（它的写点由后端 `readonly_guard::remote_write_layer` 钉）。
        // 〔RW1 · 第四波 09-24〕更早走掉的几条（F89a · F11 · F10 经远端后端写）见 git 历史。
    ];
    // 〔AL1〕**落点类型**：一个 `fenced_block::Store` 的远端实现，它的写原语方法全在 `REMOTE_WRITES` 里。
    // 〔SR1b〕今天空：那个落点类型（`RemoteFile`，从前叫 `SftpFile`）的写经本机后端，不在本表人群里。
    const STORES: &[(&str, &[&str])] = &[];
    for (ty, methods) in STORES {
        for m in *methods {
            assert!(
                REMOTE_WRITES
                    .iter()
                    .any(|(f, n, ..)| *f == "sftp.rs" && n == m),
                "落点类型 `{ty}` 的写原语 `{m}` 不在 `REMOTE_WRITES` 里 —— 这条边指向账外"
            );
        }
    }
    for (file, entry, target) in ROUTES {
        // 转发目标必须是本表登记过的写点（或登记过的落点类型）—— 否则这条边指向账外。
        assert!(
            REMOTE_WRITES.iter().any(|(_, n, ..)| n == target)
                || STORES.iter().any(|(t, _)| t == target),
            "路由表说 `{entry}` 转发到 `{target}`，可 `{target}` 不在 `REMOTE_WRITES` 里 —— \
                 那这条边指向账外，等于没连"
        );
        let raw = std::fs::read_to_string(root.join("src/bridge/src").join(file))
            .unwrap_or_else(|e| panic!("{file} 读不到：{e}"));
        let prod = guard_core::production_code(&raw);
        let lines: Vec<&str> = prod.lines().collect();
        let Some(start) = lines
            .iter()
            .position(|l| l.trim_start().contains(&format!("fn {entry}(")))
        else {
            panic!("`{file}` 里找不到入口 `{entry}` —— 改名/搬走了就把路由表一起改");
        };
        let end = fn_body_end(&lines, start);
        let body = lines[start..end].join("\n");
        assert!(
            guard_core::contains_word(&body, target),
            "`{file}::{entry}` 不再转发到 `{target}` 了。\n\
                 ★ 它是**用户按得到**的那一层，而真正写盘的是被转发的那一层。\n\
                 换了转发目标就要把路由表改对 —— 否则「按钮 ↔ 真实写点」这条边断了没人知道。"
        );
    }
}

/// ★ 正题五：**前提触发器** —— 拿得到远端写能力的文件仍然只有那四个。
///
/// 本表的人群根是「谁拿得到一个 SFTP 会话」。那个根一旦长大，
/// 上面四条判据的覆盖面就跟着变，而**没有任何东西会提醒**。
///
/// ⚠⚠ **第一版按「源码里出现会话类型名」判，恒绿，被 G 审计逮到**
/// （`acct_iso_deploy.rs` 靠类型推断，那个名字一次都没写出来，
/// 而它往用户给的 `dest_dir` 写 8 个文件 + 2 个目录）。
/// 而本条的注释当时就逐字写着「凡『唯一的哨兵』的判据人群必须按事实取样」——
/// **写下那句话的判据自己犯了那句话说的错**，本区第三次。
/// ⇒ 现在按**能力从哪来**取样（`connect_sftp` / `with_sftp` 两个出处），见
/// [`capability_holders`] 的头注。
#[test]
fn the_remote_write_capability_is_still_confined_to_three_files() {
    // 〔SR1b · 2026-09-24〕**三 → 零**：monitor 进程一个 SFTP 会话都不拿（用户 V89）。名字里那个 three 是立表那天的数，
    //   刻意没改（别处按名字引用它）；活的数住在下面那条断言里。
    let holders = capability_holders();
    assert_eq!(
        holders,
        Vec::<String>::new(),
        "monitor 里又有文件拿到了 SFTP 会话：{holders:?}\n\n\
         ★ 用户 V89：SFTP 住本机常驻后端、界面进程零 SSH。远端写只许住后端 `dial/sftp.rs`（只许两处）；\
         monitor 要远端文件就走 `dial_host::RemoteFs`（部署）或 `transfer-*`（传输）。"
    );
    // 正控：同一把针（会话类型名 · crate 路径 · 两个取会话的出处）喂合成语料，每一根都亮 —— 零命中不是瞎了。
    let ty = format!("Sftp{}", "Session");
    let opens = format!("connect_{}(", "sftp");
    let borrows = format!("with_{}(", "sftp");
    let crate_path = format!("russh{}sftp", "_");
    for (needle, sample) in [
        (&ty, format!("fn f(s: &{ty}) {{}}")),
        (
            &opens,
            format!("async fn g(c: &C) {{ let x = {opens}c).await; }}"),
        ),
        (
            &borrows,
            format!("async fn h() {{ {borrows}|s| async {{}}).await; }}"),
        ),
        (&crate_path, format!("use {crate_path}::client;")),
    ] {
        assert!(
            sample.contains(needle.as_str()),
            "正控语料里没有 `{needle}` —— 正控本身坏了"
        );
    }
    // 扫描面没塌（零命中 ≠ 没扫）：`src/bridge/src` 生产段的份数有地板。
    let n = guard_core::scan_tree!(&repo_root().join("src/bridge/src"), &["rs"]).len();
    assert!(
        n >= 100,
        "只扫到 {n} 份 monitor 源文件 —— 扫描面塌了，零命中在空转"
    );
}

/// ★ 正题六：🔴 **原生文件窗口用了池子哪几条 —— 逐条登记，两向相等。**
///
/// # 它补的洞
///
/// 前五条判据管的是「**池子**那一侧每一处写盘有没有申报、有没有围栏」。
/// 它们**一条都不管「谁按得下去」**：`filewin/` 那棵树把哪几条命令接到了界面上，
/// 在本条之前**零判据**。后果有两个方向，都出现过：
///
/// - **变多**：往窗口上接一条新的写命令而不申报 ⇒ 没人回来读一遍
///   「它的路径由谁定 · 什么围栏着」（第三条判据只看池子那一侧的入口，
///   看不见窗口新长出来的按钮）。
/// - 🔴 **变少**：某条命令的接线被顺手删掉 ⇒ 用户当场少一件事，而**一条判据都不会红**。
///   `设计/99 §4.6.5` 逐字记着这一形的代价：老面板先退役就会「用户这六件事当场没了」。
///
/// ⇒ 本条是一条**相等**断言（两向差集空 ＋ 处数相等）。**地板不行** ——
/// 地板在「变少」那个方向上是瞎的，而那正是贵的那个方向。
///
/// # 🔴 现打订正了 `设计/99 §4.6.4` 那张表两处（2026-09-21，接线前现打）
///
/// 那一节写着「池子 **14** 条命令，窗口只接了 **6** 条」，接上了那一栏点名
/// `sftp_list_dir` · `sftp_stat` · **`sftp_realpath`** · `sftp_upload` · `sftp_copy` ·
/// **`copy_remote_path`**。逐条现打：
///
/// | 那张表 | 现打 | 差在哪 |
/// |---|---|---|
/// | 池子 14 条 | **13** 条 | 第 14 个匹配是 `sftp_chmod` 头注里提到 `#[tauri::command]` 那句**散文** |
/// | 窗口接了 6 条 | **4** 条 | `sftp_realpath` 在 `filewin/` 里**零处**（只有老面板 `panel.ts` 在用）；`copy_remote_path` **不是命令**，而且 `filewin/` 被判据明禁调它（理由住 `copy.rs` 头注：它的第一个参数是一条裸会话，绕过 `sftp_copy` 就同时丢掉围栏 ＋ 取消登记 ＋ 车道预算） |
///
/// ⇒ 那张表的「6 → 11」这个算式两侧都错。**真实读数是 4 → 9**（第七刀之后 **10**），
/// 而本条就是那个数的住址。
/// 🔴 〔第七刀 09-21 补记〕上面那张表里「`sftp_realpath` 在 `filewin/` 里**零处**」
/// 这句**已经不是实况了** —— 那一格正是第七刀补上的（落点是 `source.rs` 里一个问 home 的 async 函数；
/// 〔F7a · 第三波 09-24〕那一问又换成了后端 `files-home`，`sftp_realpath` 从窗口那棵树上又没了，见下表）。
/// 那句话留着是因为它说清了**当时为什么是零**（窗口寄生在老面板的寻址上），
/// 而它今天的真伪由本条的相等断言替它保鲜。
/// ⚠ `设计/` 是唯一真相源、不是 git 仓 ⇒ 这条订正只落在本判据的诊断里，
/// 由本条的相等断言替它保鲜。
#[test]
fn the_file_window_uses_exactly_the_pool_commands_it_registers() {
    /// `(池子里的名字, 它是什么, 窗口为什么要它)`。
    ///
    /// ⚠ 「它是什么」那一栏**不是人的答案** —— 下面按 `sftp_pool.rs` 里
    /// 有没有那个属性逐条核（写错了当场红）。
    // 〔F2 · 2026-09-24 · 文件窗口只经通道说 `call`〕走掉 8 行：`sftp_list_dir` · `SftpEntry`（列目录改问后端
    //   `files-ls`）· `sftp_stat`（「那儿有没有东西」改问 `files-stat`）· `sftp_mkdir` / `sftp_delete` /
    //   `sftp_rename` / `sftp_chmod` / `sftp_write_text`（改走后端写面那五条）。留下的 10 行里 9 行是
    //   窗口进程那一侧**还不是通道**的欠账（跨机传输 §8.4 未拍 · 后端缺 `files-copy` · 本地预判围栏），
    //   逐条住 `tests/bridge/filewin/boundary_tests.rs::WINDOW_SIDE`；〔F7a 09-24〕`sftp_realpath` 那一行已搬去后端 `files-home`（见 `MOVED_TO_BACKEND`）。
    // 〔F7c 收尾 09-24〕走掉三行〔已删：`sftp_download` · `TRANSFER_LANE_CAP` · `sftp_cancel_transfer`〔散文墓碑〕〕：
    //   上传 / 下载经通道开单、订阅进度（`设计/60 §13`），复制那一腿的取消随复制走后端一起删了。
    //   ⇒ 窗口接的池子**命令**从此是 0；剩下这一行是那道围栏判定的旧住址（一条函数，不是命令）。
    // 〔第四波 S4〕剩下那一行（围栏判定的旧住址 `sftp_pool::` 那一份转出）也走了：窗口改指围栏本家
    //   `claude_data_fence`，池子里那行转出一起删了 ⇒ **窗口那棵树碰池子的地方从此是零**。
    //   表留着、今天是空的：窗口哪天又回头用池子，它得来这里登记（下面那条相等当场红）。
    const SITES: &[(&str, &str, &str)] = &[];
    /// 「它是什么」那一栏的**封闭集合**。多出第四种就得回来论证。
    /// 〔F7a · 第三波 09-24〕原来还有「类型」一档（唯一一条是复制那一趟的裁决类型），
    /// 复制换到后端之后那一条走了 ⇒ 这一档没人用，删掉（下面那条「每一档都有人用」逐字要求）。
    // 〔F7c 收尾 09-24〕「命令」「常量」两档今天都没人用了（窗口接的池子命令是 0）⇒ 只剩「函数」。
    // 〔第四波 S4〕「函数」那一档唯一的一行也走了 ⇒ 集合空（下面那条「每一档都有人用」逐字要求删掉没人用的档）。
    const KINDS: &[&str] = &[];

    let root = repo_root();
    let dir = root.join("src/bridge/src/filewin");
    // 🔴 走 `guard_core` 而不是裸 `read_dir`（`scanning_guard_registry` 那条纪律：
    //    扫描型判据不许自己遍历）。本条不需要摘掉自己 —— 调用者住
    //    `tests/bridge/`，压根不在被扫的那棵树里。
    let files = guard_core::scan_tree!(&dir, &["rs"]);
    // 抽取器自检①：**采集量**。射程被改窄在本条上是静默的（少扫一份 ⇒ 那一份
    // 接的命令从 `used` 里消失 ⇒ 差集非空 ⇒ 会红；但少扫**全部**就两边都空了）。
    assert_eq!(
        files.len(),
        // 〔F7c · 合主线 09-24〕主线 19 → 20，多的是 `upload.rs`（工具栏「上传」那一问；一个池子符号都不碰）。
        // 〔FW34 · 第四波 09-24〕20 → 21，多的是 `bookmarks.rs`（书签：monitor 自己的状态文件，一个池子符号都不碰）；
        //   21 → 22，多的是 `workspace.rs`（标签页 ＋ 双栏 ＋ 复制到另一栏；复制经通道问后端 `files-copy`，一个池子符号都不碰）。
        //   22 → 23，多的是 `preview.rs`（预览：经通道问后端 `files-read-text`，一个池子符号都不碰）。
        // 〔W5-FILES · 第五波〕23 → 24，多的是 `size.rs`（算大小：经通道问后端 `files-size`，一个池子符号都不碰）；
        //   24 → 25，多的是 `picker.rs`（原生选文件框：只碰本机选择框，一个池子符号都不碰）。
        25,
        "`filewin/` 那棵树现扫到 {} 份 `.rs`（2026-09-22 现打 14：copy · corpus · **download** · **editor** · entry · \
         find · fonts · mod · rows · scale · shell · source · transfer · writeops）\
         〔第十三刀 09-23：14 → 16，多的是 **proc** 与 **win_main**（窗口改独立进程：\
          一个窗口一个进程）。⚠ `win_main.rs` **不是一个模块** —— 它是那个 `[[bin]]` 的 \
          crate 根，`mod.rs` 刻意不 `mod` 它；它照旧进本条的射程，因为本条问的是\
          「`filewin/` 这棵树上谁在碰池子那几条命令」，而那个问题对一份 bin 入口一样要问〕\
         〔F7b 09-24：16 → 18，多的是 **create** 与 **bigfile**（「新建空文件」；它走后端 `files-create`，\
          一条池子命令都不碰 ⇒ 下面 `used` 那一摞不因它变）〕—— \
         〔F9 09-24（与 F7b 同拍合并，现打 18）多的是 **bigfile**（大文件模式；它一条池子命令都不碰 ——\
          读上限仍经 `editor.rs` 那一处 `MAX_EDIT_BYTES`）〕—— \
         〔FW1+FW2 09-24：18 → 19（与 F7b / F9 同拍合并，现打 19），多的是 **select**（选中态 · 键位 · \
          右键菜单那张表；它一条池命令都不碰，写操作经 `shell.rs` 那几个 `begin_*` 走）〕—— \
         不等就是射程变了，先查扫描面再改这个数",
        files.len()
    );
    let mut used: Vec<String> = Vec::new();
    let mut where_of: Vec<(String, String)> = Vec::new();
    for (path, src) in &files {
        let prod = guard_core::production_code(src);
        let rel = path
            .file_name()
            .expect("扫到的每一项都是文件")
            .to_string_lossy()
            .to_string();
        for name in pool_refs_in(&prod) {
            if !used.contains(&name) {
                used.push(name.clone());
            }
            where_of.push((name, rel.clone()));
        }
    }
    used.sort();
    // 🔴〔第四波 S4〕正控：人群今天是空集 ⇒ 同一个抽取器喂一份合成语料，证明它认得出
    //    （否则「零处」与「抽取器坏了」在这里长得一模一样）。
    let fake = format!(
        "let x = crate::sftp_{p}::foo_bar(1); let y = sftp_{p}::BAZ;",
        p = "pool"
    );
    assert_eq!(
        pool_refs_in(&fake),
        vec!["foo_bar".to_string(), "BAZ".to_string()],
        "抽取器认不出合成语料里那两处池子引用 —— 下面那个 0 不可信"
    );
    // 抽取器自检②：**处数地板**。剥法把生产段剥没了 ⇒ 两边都空 ⇒ 相等断言恒真。
    // 〔F7a · 第三波 09-24〕地板改成相等：10 → 9（少了 `sftp_realpath`：开窗前解 home 换成后端 `files-home`）
    //   → 7（少了读文本那条命令与它的上限常量：换成后端 `files-read-text`，常量搬回 `editor.rs`）
    //   → 5（少了复制那条命令与它的裁决类型：换成后端 `files-copy`）。
    assert_eq!(
        used.len(),
        // 〔F7c · 合主线 ＋ 收尾 09-24〕5 → 1：`sftp_download` · `sftp_upload` · `TRANSFER_LANE_CAP`（经通道）·
        //   `sftp_cancel_transfer`〔散文墓碑〕（复制那一腿的取消，随复制走后端删了）走掉；剩那道围栏判定的旧住址。
        // 〔第四波 S4〕1 → 0：那道围栏判定改指本家 `claude_data_fence`。
        0,
        "抠到 {} 处 `sftp_pool::…` 引用 —— 与现打的条数不等：抽取器坏了，或接线变了（实得 {used:?}）",
        used.len()
    );

    let mut declared: Vec<String> = SITES.iter().map(|(n, ..)| n.to_string()).collect();
    declared.sort();
    // 🔴 **相等断言，两向**（处数相等 ＋ 双向差集空，由 `assert_eq!` 一并给出）。
    assert_eq!(
        declared,
        used,
        "「窗口那棵树用到的池子那几样」与登记表对不上。\n  \
         登记了而树上没有（接线被删了？那用户当场少一件事）：{:?}\n  \
         树上有而没登记（新接了一条却没人回来读它的围栏与路径归属）：{:?}\n\n\
         ★ 这一条**不是地板**：在「变少」那个方向上地板是瞎的，\n\
         而 `设计/99 §4.6.5` 逐字记着那个方向的代价。",
        declared
            .iter()
            .filter(|d| !used.contains(d))
            .collect::<Vec<_>>(),
        used.iter()
            .filter(|u| !declared.contains(u))
            .collect::<Vec<_>>()
    );

    // 说明不许是占位，落地分类在封闭集合里。
    for (name, kind, why) in SITES {
        assert!(
            KINDS.contains(kind),
            "`{name}` 的分类写的是「{kind}」，不在 {KINDS:?} 里"
        );
        assert!(
            why.chars().count() > 20,
            "`{name}` 的说明太短，像是占位：「{why}」"
        );
    }
    for k in KINDS {
        assert!(
            SITES.iter().any(|(_, x, _)| x == k),
            "分类集合里的「{k}」今天一处都没人用 —— 删掉它"
        );
    }

    // 🔴 「它是什么」那一栏**由机器核**，不是人说了算。住址唯一源：[`pool_commands`]。
    let commands = pool_commands();
    let declared_cmds: Vec<String> = SITES
        .iter()
        .filter(|(_, k, _)| *k == "命令")
        .map(|(n, ..)| n.to_string())
        .collect();
    for n in &declared_cmds {
        assert!(
            commands.contains(n),
            "登记表说 `{n}` 是一条命令，可 `sftp_pool.rs` 里没有这么一条 \
             `#[tauri::command]` —— 那这一栏是假的"
        );
    }
    for (n, k, _) in SITES.iter().filter(|(_, k, _)| *k != "命令") {
        assert!(
            !commands.contains(&n.to_string()),
            "登记表说 `{n}` 是「{k}」，可它其实是一条 `#[tauri::command]`"
        );
    }
    // 🔴〔F2 · 2026-09-24〕**从「13 条全接上」变成「6 条 ＋ 7 条搬去后端」**。
    //   窗口成了独立进程、只经通道说 `call` 之后，池子那 13 条里有 7 条在后端有了对应命令，
    //   窗口不再碰它们（`D11`：没有退路）。剩下 6 条是窗口进程那一侧**还不是通道**的欠账。
    //   ⇒ 两件事各一条**两向相等**：窗口还用的 == 6 条；搬走的 == 下表 7 条（逐条写清搬去了哪）。
    // 〔F7c 收尾 09-24〕这张表的人群是「池子里**还在**、而窗口不再用」的命令 ⇒ 池子收到只剩 `sftp_copy` 之后
    //   它只剩一行。搬走过的十二条〔已删：`sftp_list_dir`→`files-ls` · `sftp_stat`→`files-stat` ·
    //   `sftp_mkdir` · `sftp_delete` · `sftp_rename` · `sftp_chmod` · `sftp_write_text`→写面 ·
    //   `sftp_realpath`→`files-home` · `sftp_read_text_for_edit`〔散文墓碑〕→`files-read-text`（F7a）·
    //   `sftp_upload` / `sftp_download`→通道后面的传输台 · `sftp_cancel_transfer`〔散文墓碑〕→停订（F7c）〕连本体一起删了。
    // 〔第四波 S4〕最后一行（同机复制 → 后端 `files-copy`）随池子那条命令一起删了 ⇒ 表空。
    const MOVED_TO_BACKEND: &[(&str, &str)] = &[];
    assert_eq!(
        declared_cmds.len(),
        // 〔F7c 收尾 09-24〕3 → 0：窗口一条池子命令都不接了（上传 · 往外拖经通道；取消随复制走后端）。
        0,
        "窗口今天接了 {} 条池子命令（〔F2 09-24〕13 → 6：上传 · 往外拖 · 取消 · 读文本进编辑器 ·\
         同机复制 · 开窗前解 home；〔F7a 09-24〕6 → 5：开窗前解 home 换成后端 `files-home`；\
         5 → 4：读文本进编辑器换成后端 `files-read-text`；4 → 3：同机复制换成后端 `files-copy`）",
        declared_cmds.len()
    );
    let missing: std::collections::BTreeSet<&str> = commands
        .iter()
        .filter(|c| !declared_cmds.contains(c))
        .map(String::as_str)
        .collect();
    let moved: std::collections::BTreeSet<&str> =
        MOVED_TO_BACKEND.iter().map(|(c, _)| *c).collect();
    assert_eq!(
        missing, moved,
        "窗口不再用的池子命令 ≠ 「搬去后端的那几条」（F2 7 条 ＋ F7a 逐条加的）。多出来的 ＝ 一条接线掉了而没有后端对应；\
         少了的 ＝ 窗口又回头用池子了（`D11` 不许）"
    );
    // 〔第四波 S4〕原先这里钉「复制核心不在窗口那棵树上」；那个核心本身删了，这一圈跟着走。
    // 🔴 把「谁在哪儿用」印出来（`--nocapture` 下可见）——
    //    本条对「采到了它而它过了」与「压根没扫到」原本输出相同，那正是静默的绿。
    where_of.sort();
    where_of.dedup();
    println!("窗口 ↔ 池子的接线（{} 处）：{where_of:?}", where_of.len());
}

/// 一段生产代码里 `sftp_pool::X` 的那些 `X`（按出现顺序，可重复）。
fn pool_refs_in(prod: &str) -> Vec<String> {
    // 拼出来的针，免得命中本文件自己的说明（同 `capability_holders` 那条手法）。
    let needle = format!("sftp_{}::", "pool");
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(k) = prod[from..].find(needle.as_str()) {
        let at = from + k + needle.len();
        let end = prod[at..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .map_or(prod.len(), |d| at + d);
        let name = prod[at..end].to_string();
        if !name.is_empty() {
            out.push(name);
        }
        from = at;
    }
    out
}

// ════════════════════════════════════════════════════════════════════════════
//  〔步 H2 · 2026-09-21〕围栏拆成独立一族那一刀留下的两条
// ════════════════════════════════════════════════════════════════════════════

/// 池子里**不写任何路径**的那几条命令 —— `(命令, 它做什么, 为什么不需要围栏)`。
///
/// 🔴 **它是「默认拒绝」的另一半。** 上面第三条判据的人群
/// （`USER_CHOSEN_ENTRIES`，七条）是**写死**的 —— 那是刻意的（「哪些路径是用户选的」
/// 没有语法特征），但它单独存在时有一个洞：
/// **往 `sftp_pool.rs` 加第 14 条命令、而它是一条写命令，那七个名字里没有它 ⇒ 一条不红。**
///
/// ⇒ 本表把那个洞封上：命令的全集由 [`pool_commands`] 从文件里现打，
/// 而全集必须**恰好**分成「已登记的写」与「已登记的读」两堆，一条都不许落在外面。
const NON_WRITING_COMMANDS: &[(&str, &str, &str)] = &[
    // 〔F7c 收尾 09-24〕这一堆五行全走了〔已删：`sftp_realpath` · `sftp_list_dir` · `sftp_stat` ·
    //   `sftp_read_text_for_edit` · `sftp_cancel_transfer`〔散文墓碑〕〕（读侧今天是后端 `files-*`；取消是停订）。
    //   表留着、今天是空的：它是「默认拒绝」的另一半 —— 池子里哪天再长出一条读命令，它得来这里写清理由。
];

/// 🔴 **池子里每一条命令，要么是已登记的写、要么是已登记的读 —— 相等断言，两向。**
///
/// # 它补的洞：「现打有几处写操作」此前是**写死**的
///
/// 第三条判据（〔FN1〕已退役，见上面那块注释）的人群是
/// 七个写死的名字。它钉得住「这七条各自有围栏」，钉不住「今天恰好就是这七条」。
/// ⇒ 加第 14 条命令而它是写命令 ⇒ 那七个名字里没有它 ⇒ **全仓一条不红**，
/// 而它写的是用户选的远端路径。
///
/// 本条把两个数对起来：
///
/// ```text
/// 从文件现打的命令全集（13）  ==  带围栏的写（7）  +  已登记的读（6）
/// ```
///
/// 三个数**都不是人手填的**：全集来自 [`pool_commands`]，
/// 「写」那一堆从函数体里现打（带 `guard_write(` 调用的那几条），
/// 「读」那一堆来自 [`NON_WRITING_COMMANDS`]（每一行要写清**为什么不需要围栏**）。
///
/// ⚠ 它钉不了什么（如实登记）：**一条新命令被归进「读」那一堆而其实它写**。
/// 那一栏是人的答案（同 `REMOTE_WRITES` 的「落地何方」）—— 挡这一形的是
/// 第一条判据（写原语调用点默认拒绝）与第四条判据（IPC 入口必须转发到已登记的写点）。
/// 本条挡的是**落在两堆之外**那一形，那是「加了一条命令而没人回来读它的围栏」的痕迹。
#[test]
fn every_pool_command_is_either_a_registered_write_or_a_registered_read() {
    let pool = std::fs::read_to_string(repo_root().join("src/bridge/src/sftp_pool.rs"))
        .expect("sftp_pool.rs 读不到");
    let prod = guard_core::production_code(&pool);
    let lines: Vec<&str> = prod.lines().collect();
    let all = pool_commands();
    // 「写」那一堆**从函数体里现打**，不抄第三条判据那七个名字 ——
    // 抄一份的话，「人群少一个」与「登记少一行」会被同一次编辑一起改掉 ⇒ 恒真。
    let mut fenced: Vec<String> = Vec::new();
    for cmd in &all {
        let Some(start) = lines.iter().position(|l| {
            let t = l.trim_start();
            t.starts_with(&format!("pub async fn {cmd}("))
                || t.starts_with(&format!("pub fn {cmd}("))
        }) else {
            panic!("命令 `{cmd}` 的签名找不到 —— 抽取器坏了");
        };
        let end = fn_body_end(&lines, start);
        let body = lines[start..end].join("\n");
        if body
            .lines()
            .any(|l| l.contains("guard_write(") && !l.trim_start().starts_with("fn "))
        {
            fenced.push(cmd.clone());
        }
    }
    let mut readers: Vec<String> = NON_WRITING_COMMANDS
        .iter()
        .map(|(n, ..)| n.to_string())
        .collect();
    readers.sort();
    fenced.sort();

    // 🔴 两个数相等 —— 全集 = 写 + 读，一条都不许落在两堆之外。
    let mut covered: Vec<String> = fenced.iter().chain(readers.iter()).cloned().collect();
    covered.sort();
    assert_eq!(
        covered,
        all,
        "`sftp_pool.rs` 的命令全集与「写 / 读」两堆对不上。\n  \
         全集里没归档的（🔴 **最贵的就是这一格**）：{:?}\n  \
         归档了而全集里没有（改名 / 退役了 ⇒ 把登记一起改）：{:?}\n\n\
         ★ 落在两堆之外意味着：它要么是一条**没人回来读它围栏**的写命令\n\
         （后果：那台远端机上正被 Claude 打开的 jsonl 能被它删掉 / 改走 / 改成不可读），\n\
         要么是一条读命令而没人写下「为什么它不需要围栏」。\n\
         ⇒ 处置：写命令 ⇒ 〔FN1 · V119〕池子不该再长写命令（SFTP 只做传输，写经后端文件管理面）—— 先问能不能不要；\n\
         读命令 ⇒ 加进 `NON_WRITING_COMMANDS` 并写清理由。",
        all.iter()
            .filter(|c| !covered.contains(c))
            .collect::<Vec<_>>(),
        covered
            .iter()
            .filter(|c| !all.contains(c))
            .collect::<Vec<_>>()
    );
    // 两堆不许重叠（一条命令不能既是写又是读 —— 那说明有一栏是假的）。
    let both: Vec<&String> = fenced.iter().filter(|f| readers.contains(f)).collect();
    assert!(
        both.is_empty(),
        "这几条命令同时出现在两堆里：{both:?} —— \
         `NON_WRITING_COMMANDS` 说它不写，可它函数体里有 `guard_write`。哪一栏是假的？"
    );
    // 现打读数：两个数各自钉住，别只钉和。
    assert_eq!(
        (fenced.len(), readers.len()),
        // 〔F7c 收尾 09-24〕(8, 5) → (1, 0)：池子只剩一条带围栏的写命令，读命令一条都不剩。
        // 〔第四波 S4〕(1, 0) → (0, 0)：那一条（零流量复制）也删了。全集由 `pool_commands` 现打（带正控），
        //   池子哪天再长出一条命令，本条逼它归档。
        (0, 0),
        "现打：写 {} 条 · 读 {} 条（2026-09-21 现打 **8 ＋ 5 = 13**；\
         当日早先是 7 ＋ 6 —— `sftp_download` 从「读」那一堆搬到了「写」那一堆，\
         它一直在写盘，只是写的是**用户选的本机路径**而没人守，逐条来历见\
         `NON_WRITING_COMMANDS` 上方那段墓碑）。\
         实得写 {fenced:?} · 读 {readers:?}",
        fenced.len(),
        readers.len()
    );
    for (n, what, why) in NON_WRITING_COMMANDS {
        assert!(
            why.chars().count() > 10,
            "`{n}`（{what}）没写清为什么不需要围栏，像是占位：「{why}」"
        );
    }
}

// 〔FN1 · V119〕这里原来有一条「围栏在发往返之前就出声」（带拒绝那一道的池命令里，拒绝必须早于拿连接）。
//   自第四波 S4 起它的人群就是零（池子零条命令），FN1 把那道拒绝本身删了 ⇒ 靶子不在，退役。

// ══════════════════════════════════════════════════════════════════════════
// 🔴 〔RW1 · 第四波 · 2026-09-24〕**monitor 进程不经 SFTP 直写用户文件** —— 远端那一半的分类闭集
// ══════════════════════════════════════════════════════════════════════════
//
// 用户裁远端三处（F10 别名块 · F11 删会话 · F89a `.mcp.json`）「按推荐改」经远端后端写，F08 部署后端留在 SFTP。
// ⇒ 本表剩下的每一处 SFTP 写，都必须落在**不是用户文件**的那几类里（同本机那一半
//   `write_site_registry_tests::every_monitor_write_site_lands_outside_the_users_files`）：
//   闭集**没有「用户文件」这一档**；真是用户文件又一时搬不走的，记待收并指名谁来收。两向相等。

/// 一处远端写写的是什么。**闭集，刻意没有「用户文件」这一档。**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // 〔SR1b〕今天零成员（上面那张表空了）；闭集的形状留着，不是豁免。
enum RemoteLands {
    /// F08：我们的部署物（后端二进制 · 标记 · 入口 shim · 我们的脚本目录）—— 用户裁「留在 SFTP」。
    OwnDeployment,
    /// 我们自己的暂存区（`~/.cc-monitor/staging/`），落进用户目标的那一下在后端提交。
    /// 〔SR1b〕今天零成员（暂存区的写搬进了本机后端）；留着这一档是闭集的形状，不是豁免。
    #[allow(dead_code)]
    OwnStaging,
    /// **是**用户文件，但有主、在别的路收：`(谁来收, 为什么不在本路)`。
    /// 〔SR1b〕今天零成员（唯一那一格下载落地被 SR1b 收了）；形态留着，下一格要落时有地方落。
    #[allow(dead_code)]
    Pending(&'static str),
}

const REMOTE_CLASS: &[(&str, &str, RemoteLands)] = &[
    // 〔SR1b · 2026-09-24〕空：上面那张表空了（monitor 零 SFTP 会话），分类跟着空。F08 那五行（部署物）
    //   随执行搬进本机后端；「部署物」那一档的落点今天由后端的写根（`~/.cc-monitor/bin/`）答。
];

#[test]
fn every_remaining_sftp_write_lands_outside_the_users_files() {
    let registered: Vec<(&str, &str)> = REMOTE_WRITES.iter().map(|(f, n, _, _)| (*f, *n)).collect();
    // 〔SR1b〕表空了（monitor 零 SFTP 会话）⇒ 下面那条相等今天在两侧空集上成立 —— **这一次空集就是事实**，
    //   不是空转：人群根的零命中由 `the_remote_write_capability_is_still_confined_to_three_files` 带正控钉着。
    assert!(registered.is_empty(), "远端写表又长出来了：{registered:?}");
    let unclassified: Vec<String> = registered
        .iter()
        .filter(|(f, n)| !REMOTE_CLASS.iter().any(|(cf, cn, _)| cf == f && cn == n))
        .map(|(f, n)| format!("{f}::{n}"))
        .collect();
    let ghosts: Vec<String> = REMOTE_CLASS
        .iter()
        .filter(|(cf, cn, _)| !registered.iter().any(|(f, n)| f == cf && n == cn))
        .map(|(f, n, _)| format!("{f}::{n}"))
        .collect();
    assert!(
        unclassified.is_empty() && ghosts.is_empty(),
        "远端写与「它写的是什么」那张分类表对不上。\n  \
         没分类的（🔴 新的一处 SFTP 写先回答它是不是用户文件）：{unclassified:?}\n  \
         分类表里的幽灵（那一处没了，同轮摘行）：{ghosts:?}\n\n\
         用户裁远端三处（别名块 · 删会话 · `.mcp.json`）经远端后端写；F08 部署留在 SFTP。\n\
         ⇒ 分类是闭集，**没有「用户文件」这一档**：用户文件经那台远端的后端写（`user_files`）。"
    );
    for (f, n, c) in REMOTE_CLASS {
        if let RemoteLands::Pending(owner) = c {
            assert!(
                owner.chars().count() >= 10,
                "`{f}::{n}` 记成待收，却没写清谁来收"
            );
        }
    }
    // 远端三处用户文件的写，今天一处都不在本表的人群里（它们不再拿 SFTP 会话）。
    for gone in [
        "install_remote_alias_block",
        "uninstall_remote_alias_block",
        "remove_remote_file",
        "write_remote_mcp_server",
        "remove_remote_mcp_server",
    ] {
        assert!(
            !write_sites().iter().any(|(_, n)| n == gone),
            "`{gone}` 又经 SFTP 写了 —— 用户裁它经远端后端写"
        );
    }
}
