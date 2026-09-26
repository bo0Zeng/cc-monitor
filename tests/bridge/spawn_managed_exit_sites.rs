//! ★★ **「谁还自己造一个 `Command`」的逐处普查 —— 处数相等，理由逐条。**
//!
//! # 它问的不是「谁绕开了出口」，别与兄弟条混
//!
//! 同目录的 [`super::tests::the_spawn_verbs_and_platform_primitives_live_only_here`]
//! 问的是**绕开**：`.spawn()` 与三条平台原语只许长在 `spawn_managed.rs` 里，零容忍。
//! 它的头注逐字写过「为什么不是数 `Command::new(`」——**那句话对它是对的**：
//! 数「装东西那一下」接不住绕过，还会逼出一个什么都往里传的上帝函数。
//!
//! 本条问的是**另一个问题**，所以锚点也不同：
//! **今天还有几处自己造 `Command`、各是哪一处、各把那个 `Command` 交给出口的哪个入口。**
//! 这三样今天**一个都没人数**：
//!
//! | 已有的 | 它的粒度 | 它在「变少」方向上 |
//! |---|---|---|
//! | `the_spawn_verbs_and_platform_primitives_live_only_here` | **文件**（这份源码里有没有那几个动词） | 不适用（零容忍，不数人群） |
//! | [`super::super::write_site_registry::spawn_sites::SPAWNS`] 的 `every_local_spawn_is_declared` | **`文件::函数`**，而且 `dedup` 过 | 瞎 —— 它的自检是 `found.len() >= 5` |
//! | `the_three_policies_each_site_declares_match_the_code` | **文件** | 瞎 —— 它的自检是 `checked >= 13` |
//!
//! ⇒ 三条合起来**接不住的**恰好三族，而本条各给一个相等断言：
//!
//! 1. **往一个已申报的函数里再加一句裸 `Command::new`** —— `SPAWNS` 那条按
//!    `(文件, 函数)` 去重，新加的那一处与旧的**同键** ⇒ 它一声不吭。
//! 2. **一处落点消失** —— 上面两条自检都是地板（`>= N`），地板在「变少」方向上是瞎的。
//! 3. **某一处换了出口入口**（`spawn_managed_cmd` → 别的什么）—— 没有任何判据记着
//!    「这一处走哪个入口」，而那三条策略的答案就挂在入口上。
//!
//! # 两侧刻意不同源
//!
//! 一侧是 [`SITES`]：**手写**的，四列，第四列逐条写「为什么这一处还留着一句自己造的
//! `Command`」。另一侧是 [`census`]：**现扫源码**（`production_code` 剥过测试段与注释）。
//! 第三列那个入口名**再走一趟派生**（[`exit_entry_points`]，从 `spawn_managed.rs`
//! 的签名里取「返回类型带 `Managed` 的那些」）—— 手写一份入口名单的话，
//! 出口改名之后本条会安静地继续绿，而它声称对拍的那件事已经不存在了。
//!
//! # ⚠ 射程边界，写下来（别读成更强）
//!
//! - **接不住把交接搬进同文件的一个辅助函数**：那时第三列那个名字不再出现在外层函数体里
//!   ⇒ 本条**假红**。处置是回来改第三列（而那一刻确实该有人重看一眼这一处怎么起进程的），
//!   不是把这一列删掉。今天盘面上一处都没有这一形。
//! - **`build.rs` 刻意不在人群里**：它跑在构建期，`SPAWNS` 已经把它那两处登记成
//!   「不进那个出口」并写了理由。同一件事记两张表 ⇒ 两张会各自漂。
//!   ⇒ 本条的扫描面逐字是 `crate_src_root()` 一棵树，**构建期那一格不归它**。
//! - **同文件同函数内两处落点，本条区分得开处数、区分不开各自的策略**：
//!   `launch.rs::launch_powershell_window` 今天就有两跳（`wt.exe` / `powershell.exe`），
//!   本表为它留**两行同键**、处数对得上；但「哪一跳配哪三格」仍归
//!   `the_three_policies_each_site_declares_match_the_code`，而那一条自己也说了它看不出来。
//!   **如实记，两条都没买到那一格。**

use std::path::PathBuf;

/// `(相对本 crate 源码根的路径, 外层函数, 它把造好的 `Command` 交给出口的哪个入口,
/// 为什么这一处还得自己造一个 `Command`)`。**默认拒绝**：人群从源码派生。
///
/// # 第三列是什么
///
/// `spawn_managed.rs` 那个唯一出口的**入口名**，逐字。名单由 [`exit_entry_points`]
/// 从源码派生 —— 写错一个字母就红。带 `ManagedSpawn` 的两行是**注入**形：
/// 它们住 `backend/`，平台原语进不去（后端那半的禁针 ＋ 平台例外表的递减棘轮），
/// ⇒ 起进程那一下由宿主注进来的那个闭包做，本层只负责把 `Command` 装好。
///
/// # 第四列为什么必须逐条写
///
/// 「这一处还留着一句 `Command::new`」有两种完全不同的来路：
/// ① 它是**出口的入参**（出口收 `&mut Command`，argv / env / cwd / stdin / stdout
///    各落点自己的事）；② 它**绕开了出口**。两者在源码上长得一模一样 ——
/// 一行 `Command::new(...)`。⇒ 区别只能由「它交给谁」（第三列，可判）
/// ＋「为什么非它自己造不可」（本列，逐条）一起说清。一句总述盖不住十四处。
const SITES: &[(&str, &str, &str, &str)] = &[
    (
        "filewin/proc.rs",
        "spawn_window",
        "spawn_managed_cmd",
        "🔴〔第十三刀 2026-09-23〕文件管理窗口的独立进程。**它非自己造 `Command` 不可，\
         唯一的理由是 stdin**：那一屏（＋ 源 ＋ cwd ＋ reveal）走 stdin 递过去，\
         而出口只回答三条策略、不回答「跑什么」。\
         ⚠ 为什么不走环境变量（那样就能用出口最简那个形态）：一屏上限 5 万条，\
         JSON 是兆字节级，而 Linux 一条环境变量的上限是 32 页 ⇒ `execve` 直接 `E2BIG`；\
         argv 更不行（世界可读，而种子里带主机名 / 用户名 / 私钥路径）。\
         ⚠ **stdout 刻意不接**：接成管子而没人读，对面一写满就卡死。",
    ),
    (
        "ccm_probe.rs",
        "probe_with",
        "spawn_managed_cmd",
        "argv 是 `bash -lic <CCM_PROBE_CMD>`，而且 stdin 要 null、stdout 要 piped ——\
         这三样都是本处特有的（探针唯一有用的字节在 stdout 上）。出口只回答三条策略，\
         不回答「跑什么」⇒ `Command` 必须由这里装好再递进去。",
    ),
    (
        "ccm_probe.rs",
        "probe_binary_uncached",
        "spawn_managed_cmd",
        "与上一行**共用同一段等待与解析**（`probe_spawned`），差的只有「怎么起」：\
         这一跳是 `<我们放下去的那份 ccm> --ccm-probe`，一个 shell 都不起。\
         ⇒ 两条起法各造一个 `Command`，正是那个共用点存在的前提。",
    ),
    (
        "launch.rs",
        "launch_local_posix_via",
        "spawn_managed_cmd",
        "argv 来自 `local_posix_spawn_plan`（用户配置的终端出口），cwd 只在真是目录时才设，\
         env 还要带上 `backend_bin_env_for_window` 那一对 —— 三样都是本处特有。",
    ),
    (
        "launch.rs",
        "launch_powershell_window",
        "spawn_managed_cmd",
        "Plan A（`wt.exe`）那一跳。argv 是「`-d <目录>` ＋ `powershell.exe` ＋ 那三个参数」\
         拼出来的，与 Plan B 不同形 ⇒ 同一个函数里两跳各造一个 `Command`。\
         ⚠ 本表两行同键、处数对得上；**哪一跳配哪三格**不归本条（见模块头注的射程边界）。",
    ),
    (
        "launch.rs",
        "launch_powershell_window",
        "spawn_managed_cmd",
        "Plan B（`powershell.exe` 直起）那一跳 —— 全仓唯一一处 `ConsolePolicy::NewVisible`，\
         而且是刻意的。它的 argv 是那三个参数本身（不经 `wt.exe` 转交），\
         env 在这一跳**一定继承**（Plan A 未必）⇒ 两跳的 `Command` 内容真的不同。",
    ),
    (
        "launch.rs",
        "ssh_client_available",
        "spawn_managed_cmd",
        "`where.exe ssh` 探测：stdout 要 piped，因为它的输出**是返回值**\
         （`status.success()` 那一格）。这一处先前是裸 `.output()` —— 也就是\
         「要不要窗口」没人回答过；今天它自己造 `Command`、再由出口回答那一格。",
    ),
    (
        "local_backend_host.rs",
        "spawn_detached",
        "managed_spawner",
        "「三样一起才叫脱离」里有两样是 `Command` 的内容而不是策略：`env_remove(\"TMUX\")`\
         （漏了就会去改 monitor 恰好从哪个 tmux 里被启动的那个 server）与\
         监听口/令牌那两个 env ＋ stdin/stdout 全 null。⇒ 装在这层，\
         起那一下交给 `managed_spawner` 绑出来的那个闭包（经 `spawn_with_etxtbsy_retry`）。",
    ),
    (
        "local_backend_host.rs",
        "signal_term",
        "spawn_managed_cmd",
        "argv 是 `kill -TERM <pid>`，那个 pid 是我们自己算出来的、不吃用户输入 ——\
         它只有这里知道。⚠ 这一处**已登记**在 `SPAWNS`（那张表默认拒绝），\
         本条与它不同源、不同粒度，两条都要。",
    ),
    (
        "profile_installer.rs",
        "run_user_path_powershell",
        "spawn_managed_cmd",
        "argv 的四段是**承重的**：`-NoProfile`（这一跳的行为不许被用户 profile 左右，\
         而本件刚把我们自己那段从 profile 里删掉）· `-NonInteractive`（界面点一下不许挂住）\
         · `-Command <我们自己 render 出来的脚本>`。stdout 要 piped：它是返回值。",
    ),
    // 〔SR1a · 2026-09-24〕拨号代理宿主那一行**摘了**：它不再起 `--dial` 子进程（拨号挪进本机常驻后端，经流上的链路做）。
    (
        "ssh_source.rs",
        "resolve_ssh_host",
        "spawn_managed_cmd",
        "`ssh -G <别名>`，别名过了 `is_safe_alias` 才进 argv；stdout piped 因为要解析它。\
         整跳还得包在 `spawn_blocking` 里（同步阻塞调用不许卡 tokio 线程）——\
         这几样都在出口的射程之外。",
    ),
    (
        "backend/control/cc_bus.rs",
        "local_shell_read",
        "spawn_managed_tokio",
        "tokio 那一侧。argv 是 `bash -lc <cc-bus 的读串>`，而 `<读串>` 逐字知道\
         `~/.cc-bus/agents.tsv` 长什么样 —— 那份文件布局知识只许有一份，\
         所以它必须以 argv 的形式从这里进去。用 `-lc` 而不是 `-lic`：需求与探针不同。",
    ),
    (
        "backend/control/local_backend.rs",
        "supervise_with_stdio",
        "ManagedSpawn",
        "**注入形**：本层住 `backend/`，平台原语进不去 ⇒ 起进程那一下由宿主注进来的\
         `ManagedSpawn` 做（宿主那侧声明 `local_backend_supervised` 那三格）。\
         留在本层的是**本模块的协议**：`env_remove(\"TMUX\")`、argv、以及 stdin/stdout\
         那两根管子（stdout 的 EOF 是「进程死了」这个事件的唯一来源）——\
         那两根**不是**三条策略里的任何一条。",
    ),
    // 〔LOC1a · 第四波 4D〕一次性本机查询那一处（`local_query` 模块的 `run_query`〔散文墓碑〕）那一行删了（注入形第二处）：
    //   本机那几问改走 `<local>` 长连接，宿主那侧的一次性查询三格（`local_backend_one_shot_query`〔散文墓碑〕）随之删。
];

/// 本 crate 的源码根 —— 住址唯一源。
fn src_root() -> PathBuf {
    crate::guard_support::crate_src_root()
}

/// 那个唯一出口自己的住址（相对仓根）。**明写，不靠 `file!()` 自摘** ——
/// 那一刀在这一处**不生效**（本判据的根过 `repo_root()`，规范化过 ⇒ 串里没有 `..`
/// ⇒ 后缀比不命中）。⇒ 今天承重的就是这行明写的名字本身。
///
/// 🔴 本文件被 `#[path]` 引进来 ⇒ `file!()` 是一条带 `..` 的折返路径，
/// `scan_tree!` 的后缀比**恒不命中**，那一刀在本仓一处都不生效。
/// ⇒ 用 `scan_tree_excluding` 的明写名单：摘不到它时当场 panic，
/// 出口改名/搬家会出声，而不是安静地把出口自己算成一处「还在自己造 `Command`」。
const THE_EXIT: &str = "src/bridge/src/spawn_managed.rs";

/// 造 `Command` 那一下的针。**运行时拼**，免得命中本文件自己。
fn needle() -> String {
    concat!("Command::", "new(").to_string()
}

/// 某一行往回找到的**最近那个 `fn` 项**：`(函数名, 从 `fn` 那一行起到花括号收口的整段)`。
///
/// # 为什么不复用 `write_site_registry::spawn_sites` 那个同族 helper
///
/// 那一个只回**名字**（它只需要名字），而本条要的是**那一段文本** ——
/// 第三列那个入口名要在里面找。回名字的那一版把 `fn` 那一行的下标丢掉了，
/// 而丢掉的正是本条要的东西。⇒ 两者不是同一形状，不是「抄了第二份」。
/// ⚠ 真要收成一处得改另一条判据自己那份文件，那不在本轮的写区 —— **如实记**。
fn enclosing_fn_item(lines: &[&str], at: usize) -> Option<(String, String)> {
    let start = (0..=at).rev().find(|i| fn_name_on(lines[*i]).is_some())?;
    let name = fn_name_on(lines[start])?;
    let mut depth = 0i32;
    let mut opened = false;
    let mut item = String::new();
    for l in &lines[start..] {
        item.push_str(l);
        item.push('\n');
        for c in l.chars() {
            match c {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        if opened && depth <= 0 {
            break;
        }
    }
    Some((name, item))
}

/// 这一行上声明的函数名（`fn <名字>`）。
///
/// ⚠ 认的是**小写 `fn ` ＋ 词边界**：`dyn Fn(` 那种闭包类型（大写 `F`）不算，
/// 否则 `supervise_with_stdio` 那几个 `Arc<dyn Fn(...)>` 参数会把外层函数判成它们。
fn fn_name_on(line: &str) -> Option<String> {
    let rest = line
        .split(" fn ")
        .nth(1)
        .or_else(|| line.trim_start().strip_prefix("fn "))?;
    let n: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!n.is_empty()).then_some(n)
}

/// 现扫一趟：`(相对源码根的路径, 外层函数名, 那个函数项的整段文本)`，**一处一条、不去重**。
///
/// 🔴 「不去重」是本条与 `SPAWNS` 那条的分水岭：去重之后，往一个已申报的函数里
/// 再加一句裸 `Command::new` 就消失了。
fn census() -> (Vec<(String, String, String)>, usize) {
    let root = src_root();
    let files = guard_core::scan_tree_excluding(&root, &["rs"], &[THE_EXIT]);
    let needle = needle();
    let mut out: Vec<(String, String, String)> = Vec::new();
    let mut scanned = 0usize;
    for (path, raw) in &files {
        let prod = guard_core::production_code(raw);
        scanned += prod.len();
        let lines: Vec<&str> = prod.lines().collect();
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        for (i, l) in lines.iter().enumerate() {
            if !l.contains(&needle) {
                continue;
            }
            let (name, item) = enclosing_fn_item(&lines, i).unwrap_or_else(|| {
                panic!(
                    "{rel} 第 {} 行的 `Command` 不在任何 `fn` 项里 —— 抽取器看不懂它了",
                    i + 1
                )
            });
            out.push((rel.clone(), name, item));
        }
    }
    (out, scanned)
}

/// 出口的**入口名单** —— 从 `spawn_managed.rs` 的签名派生：
/// `pub fn` / `pub type` 里**返回类型带 `Managed`** 的那些。
///
/// # 为什么是「返回类型带 `Managed`」而不是「所有 `pub`」
///
/// `wait_with_output` / `wait_for_status` 也是 `pub fn`，但它们是**等**那一下、
/// 不是**起**那一下。把它们收进名单的话，某一处只要在函数体里出现
/// `wait_with_output` 就算「够到了出口」，而那句话可以在完全绕开出口的代码里出现。
/// ⇒ 判据一整条退化成恒真。这一格由
/// [`the_exit_entry_point_list_is_derived_and_does_not_say_yes_to_every_pub_fn`] 的阴性对照钉着。
fn exit_entry_points() -> Vec<String> {
    let src = guard_core::production_code(include_str!("../../src/bridge/src/spawn_managed.rs"));
    let lines: Vec<&str> = src.lines().collect();
    let mut out: Vec<String> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim_start();
        let (kind, rest) = if let Some(r) = t.strip_prefix("pub fn ") {
            ("fn", r)
        } else if let Some(r) = t.strip_prefix("pub type ") {
            ("type", r)
        } else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        // 签名可能跨行（本仓的 rustfmt 会把长参数表摊开）⇒ 收到收口那一行为止。
        let mut sig = String::new();
        for l2 in lines[i..].iter().take(16) {
            sig.push_str(l2);
            sig.push('\n');
            let closed = if kind == "fn" {
                l2.contains('{')
            } else {
                l2.contains(';')
            };
            if closed {
                break;
            }
        }
        let tail = match sig.rsplit_once("->") {
            Some((_, t)) => t.to_string(),
            None => continue,
        };
        if tail.contains("Managed") {
            out.push(name);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// ★★★ **相等断言：还在自己造 `Command` 的那几处 == 本表逐条解释过的那几处。**
///
/// 两侧不同源：一侧现扫源码（[`census`]），一侧手写（[`SITES`]）。
///
/// 🔴 **相等而不是地板**：地板（`>= N`）在「变少」方向上是瞎的，而「一处落点消失」
/// 正是本仓已有两条自检（`found.len() >= 5` · `checked >= 13`）共同的盲区。
#[test]
fn the_bare_command_sites_are_exactly_the_ones_this_ledger_explains() {
    let (found, scanned) = census();
    // 反向自检①：剥完还得有东西可扫。空集会让下面那条「零命中地绿」。
    assert!(
        scanned > 100_000,
        "剥掉测试段与注释后只剩 {scanned} 字节可扫 —— 本条此刻是空转的"
    );
    // 反向自检②：**那根针是真的**，而且**那一刀排除是承重的**。
    //
    // 出口自己那份源码里恰好有一处 `Command::new(`（`spawn_managed` 的最简形态替调用方造的那一个）。
    // 它不在上面那趟人群里 ⇒ 证明 `THE_EXIT` 那条排除真的落下去了；
    // 而它**存在**⇒ 证明针没写错。两件事一条断言。
    let in_the_exit =
        guard_core::production_code(include_str!("../../src/bridge/src/spawn_managed.rs"))
            .matches(needle().as_str())
            .count();
    assert_eq!(
        in_the_exit, 1,
        "出口自己的生产段里有 {in_the_exit} 处造 `Command` —— 09-21 现打恰好 1 处\
             （`spawn_managed` 那个最简形态替调用方造的那一个）。\n\
             ★ 两种来路，先分清：① 出口自己长出了第二处（那要先回答「为什么」）；\
             ② 针写错了 / 剥法坏了 ⇒ **上面那趟人群此刻不可信**。"
    );
    assert!(
        !found.iter().any(|(f, ..)| f.ends_with("spawn_managed.rs")),
        "出口自己被算进人群了 —— `THE_EXIT` 那条排除没落下去，\
             而本条会教人「把出口的实现改成调用出口」。现打人群：{:?}",
        found.iter().map(|(f, n, _)| (f, n)).collect::<Vec<_>>()
    );

    let mut disk: Vec<(String, String)> = found
        .iter()
        .map(|(f, n, _)| (f.clone(), n.clone()))
        .collect();
    let mut ledger: Vec<(String, String)> = SITES
        .iter()
        .map(|(f, n, ..)| (f.to_string(), n.to_string()))
        .collect();
    disk.sort();
    ledger.sort();

    // 双向差集 —— 多重集语义（同键两行必须两侧都有两行）。
    let diff = |a: &[(String, String)], b: &[(String, String)]| -> Vec<String> {
        let mut rest: Vec<(String, String)> = b.to_vec();
        let mut only = Vec::new();
        for k in a {
            match rest.iter().position(|x| x == k) {
                Some(i) => {
                    rest.remove(i);
                }
                None => only.push(format!("  {}::{}", k.0, k.1)),
            }
        }
        only
    };
    let unexplained = diff(&disk, &ledger);
    assert!(
        unexplained.is_empty(),
        "这些地方**自己造了一个 `Command`，而本表没有逐条解释过它**：\n{}\n\n\
             ⚠ 「它是出口的入参」与「它绕开了出口」在源码上长得一模一样（都是一行 \
             `Command::new(...)`）⇒ 不许静默跳过。\n\
             ⇒ 往 `SITES` 加一行：**第三列**写它把那个 `Command` 交给出口的哪个入口\
             （名单从 `spawn_managed.rs` 的签名派生），**第四列**写为什么这一处非自己造不可。\n\
             🔴 若答不出第三列 —— 那就不是「没登记」，是**真的绕开了出口**，\
             该做的是把它改成走 `spawn_managed_cmd` / `spawn_managed_tokio`，\
             或者（`backend/` 那一半）收一个 `ManagedSpawn` 注入参数。",
        unexplained.join("\n")
    );
    let vanished = diff(&ledger, &disk);
    assert!(
        vanished.is_empty(),
        "本表登记的这几处**在盘面上找不到了**：\n{}\n\n\
             ★ 三种来路，先分清再动手：\n\
             ① 那一处真的改走别的路了（好事）⇒ 删掉这一行；\n\
             ② 函数改名 / 文件搬家 ⇒ 更新这一行（改名也该红：名字变了就该有人重看一眼它起的是什么）；\n\
             ③ **扫描面缩了 / 剥法变了** ⇒ 别去改表，去看 `src_root` 与 `production_code`。\n\
             本趟扫到 {} 字节生产段、人群 {} 处。",
        vanished.join("\n"),
        scanned,
        disk.len()
    );
    // ★ 头条：**处数相等**。上面两条差集为空时它必然成立 ——
    //   留着是因为它是这件事的**标题**，而且它的文案说得出「多了还是少了」。
    assert_eq!(
        disk.len(),
        SITES.len(),
        "还在自己造 `Command` 的处数（现扫 {}）与本表的行数（{}）不相等。\n\
             🔴 **这条刻意不是地板**：`>= N` 在「变少」方向上是瞎的，\
             而本仓已有两条自检（`found.len() >= 5` · `checked >= 13`）恰好都栽在那一格。",
        disk.len(),
        SITES.len()
    );
    eprintln!(
        "〔自己造 `Command` 的落点 · 本趟现扫〕{} 处：\n  {}",
        disk.len(),
        disk.iter()
            .map(|(f, n)| format!("{f}::{n}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// ★★ **第三列可判：每一处都得说出它把 `Command` 交给了出口的哪个入口。**
///
/// ① 那个名字必须真的是 `spawn_managed.rs` 的入口之一（名单**派生**，不手写）；
/// ② 它必须真的出现在**那一处的外层函数项**里。
///
/// # 它接得住什么（兄弟条都接不住的）
///
/// 某一处悄悄换了入口（比如从 `spawn_managed_cmd` 换成一个将来才有的别的东西），
/// 或者交接被挪走而账本没跟 —— 处数不变、动词不变，
/// `the_spawn_verbs_and_platform_primitives_live_only_here` 与 `SPAWNS` 两条都不会响。
#[test]
fn every_bare_command_site_names_the_exit_entry_it_hands_off_to() {
    let entries = exit_entry_points();
    let (found, _) = census();
    let mut bad: Vec<String> = Vec::new();
    for (file, func, route, _) in SITES {
        if !entries.iter().any(|e| e == route) {
            bad.push(format!(
                "  {file}::{func} 第三列写的是 `{route}`，而 `spawn_managed.rs` 今天的入口是 {entries:?}"
            ));
            continue;
        }
        // 同键可能有两行（同一个函数里两跳）⇒ 只要那个函数项里有这个名字就算。
        let item = found
            .iter()
            .find(|(f, n, _)| f == file && n == func)
            .map(|(_, _, item)| item.clone());
        match item {
            None => bad.push(format!(
                "  {file}::{func} —— 盘面上找不到这一处（上一条判据会先说这件事）"
            )),
            Some(item) if !item.contains(route) => bad.push(format!(
                "  {file}::{func} 的账本写着「交给 `{route}`」，而那个函数项里找不到这个名字"
            )),
            Some(_) => {}
        }
    }
    assert!(
        bad.is_empty(),
        "第三列与盘面对不上：\n{}\n\n\
             ★ 两种来路，先分清：\n\
             ① 那一处真的改了交接方式 ⇒ **回来改第三列**（改了行为不回来改理由，\
                账本当天就开始撒谎）；\n\
             ② 交接被挪进了同文件的一个辅助函数 ⇒ 本条会在这里假红，\
                处置写在模块头注的射程边界里，**不是把这一列删掉**。",
        bad.join("\n")
    );
}

/// ★ **入口名单的阴性对照 —— 它不是「所有 `pub fn`」。**
///
/// 没有这一条的话，派生法哪天松成「凡 `pub fn` 皆入口」是**静默**的：
/// 上一条照旧全绿，而它声称对拍的那件事（「交给了真正起进程的那个入口」）已经没了。
#[test]
fn the_exit_entry_point_list_is_derived_and_does_not_say_yes_to_every_pub_fn() {
    let entries = exit_entry_points();
    for must in ["spawn_managed", "spawn_managed_cmd", "spawn_managed_tokio"] {
        assert!(
            entries.iter().any(|e| e == must),
            "派生出来的入口名单里没有 `{must}` —— 要么它搬家/改名了（那 `SITES` 第三列该跟着改），\
                 要么派生法坏了（那上一条此刻在空转）。现打：{entries:?}"
        );
    }
    // 🔴 阴性那半：**等**那一下不是**起**那一下。
    for never in ["wait_with_output", "wait_for_status"] {
        assert!(
            !entries.iter().any(|e| e == never),
            "`{never}` 混进入口名单了 —— 它是「等它退出」，不是「起它」。\n\
                 ⇒ 派生法此刻松成了「凡 `pub fn` 皆入口」，而那让上一条整条退化成恒真：\
                 任何函数只要出现过 `{never}` 就算「够到了出口」。现打：{entries:?}"
        );
    }
    // 注入那一格必须在名单里 —— `backend/` 那两行全靠它。
    assert!(
        entries.iter().any(|e| e == "ManagedSpawn"),
        "`ManagedSpawn` 不在名单里 —— `backend/` 那两处（它们不许认平台，只收注入参数）\
             的第三列就无从对拍了。现打：{entries:?}"
    );
}

/// ★ **第四列不许是一句空话。**
///
/// 「刻意」也会过期：没有理由的登记，下一轮没人判得了真伪。
#[test]
fn every_site_writes_its_own_reason_and_no_two_share_one() {
    let mut thin: Vec<String> = Vec::new();
    for (file, func, _, why) in SITES {
        if why.chars().count() < 40 {
            thin.push(format!(
                "  {file}::{func} —— 只有 {} 个字",
                why.chars().count()
            ));
        }
    }
    assert!(
        thin.is_empty(),
        "这几行的第四列太短，读不出「为什么这一处非自己造 `Command` 不可」：\n{}",
        thin.join("\n")
    );
    // 🔴 逐条 ≠ 一句总述抄十四遍。同一段文字复用两次 ⇒ 至少有一处没被真的想过。
    let mut seen: Vec<&str> = SITES.iter().map(|(_, _, _, w)| *w).collect();
    let before = seen.len();
    seen.sort();
    seen.dedup();
    assert_eq!(
        seen.len(),
        before,
        "第四列里有重复的整段文字 —— 那是「一句总述抄了 N 遍」，\
             而本表存在的理由逐字是「一句总述盖不住十四处」。"
    );
}
