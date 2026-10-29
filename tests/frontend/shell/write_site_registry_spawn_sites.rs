//! 本机起进程的落点：一处一行，每一处写清起的是什么、为什么必须起进程、交给唯一出口
//! （`spawn_managed.rs`）的哪个入口、选了哪三条策略。人群从源码现扫，两向按处相等。
//!
//! 一处落点 ＝ 生产段里一句 `Command::new(`，或一次调用出口最简那一形 `spawn_managed(`（出口替它造 `Command`）。
//! 同一个函数里两句 `Command::new` 是两处、两行；往已登记的函数里再加一句，处数就对不上。
//! 后端那一半起进程由它自己的 `readonly_guard::spawn_registry` 登记，这里只扫壳。

use std::path::{Path, PathBuf};

/// `(相对壳源码根的路径, 外层函数, 交给出口的哪个入口, 起的是什么, 为什么必须起进程, 三条策略)`。
///
/// 第三列是 `spawn_managed.rs` 的入口名，逐字（名单由 [`exit_entry_points`] 从签名派生）；
/// 出口自己那一行写 `——`。
///
/// 第六列两种写法：
/// - `"<Console> · <Lifetime> · <Stderr>"` —— 三个枚举变体名，逐字，与这一处的源码对拍；
///   带 `（宿主注入：<构造器>）` 后缀的，这一处自己不写策略（它在 `backend/`，平台原语进不去），
///   答案由 `spawn_managed.rs` 里那个具名构造器给。
/// - `"—— …"` 开头 —— 出口自己，没有「它选了哪三条」这回事。
const SPAWNS: &[(&str, &str, &str, &str, &str, &str)] = &[
    (
        "shots_shell.rs",
        "start",
        "spawn_managed_cmd",
        "截图台架给的整条命令（沙箱里的真后端）",
        "截图台架的无头壳：只在特性 `shots` 下编（例子 `ccm-shots-shell`，不进安装包），不在用户机器上跑。\
         每台机器起一个真后端，stdin / stdout 两根管子交产品那条本机读循环（`local_backend::local_stdio_consumer`）。",
        "Hidden · JobKillOnClose · Inherit",
    ),
    (
        "platform/console.rs",
        "run_helper",
        "spawn_managed_cmd",
        "`<本二进制> --ccm-console <件> <进程号…>`（不经 shell）",
        "认窗口要借别人的控制台（`AttachConsole`），而挂控制台是整个进程的状态 ⇒ 交给一个自己没有控制台、\
         用完即退的小助手借（本二进制的隐藏子命令）。参数只有子命令名与进程号 / 窗口句柄，答话在 stdout 上。\
         `Detached`：到点不掐，小助手自己有 2 秒预算、写回标题再退。",
        "Hidden · Detached · Null",
    ),
    (
        "backend_once.rs",
        "ask_once",
        "spawn_managed_cmd",
        "`<我们那份 ccm / 要放的那份暂存件> -- --<帧命令>`（不经 shell；入参 JSON 走 stdin）",
        "本机后端放下去之前要它自己判放不放（`place-verdict`，那一刻还没有常驻后端可连）⇒ 直接跑那份字节问一次。\
         stderr 是答话的一半（错信封 `{code, message}`）⇒ `Captured`。",
        "Hidden · JobKillOnClose · Captured",
    ),
    (
        "platform/terminal.rs",
        "launch_local_posix_via",
        "spawn_managed_cmd",
        "用户配置的终端 argv[0]",
        "在用户的终端里起会话，这是本产品的主用途。argv 来自 `local_posix_spawn_plan`，cwd 只在真是目录时设，\
         env 带上 `backend_bin_env_for_window` 那一对。`Hidden` 在 POSIX 上是空的：窗口是终端出口自己开的。",
        "Hidden · Detached · Null",
    ),
    (
        "platform/terminal.rs",
        "launch_powershell_window",
        "spawn_managed_cmd",
        "`wt.exe`（常量名）",
        "Windows 上开会话窗口的第一跳：把请求转交给 Windows Terminal。argv 是「`-d <目录>` ＋ `powershell.exe` ＋ 那三个参数」，\
         一个 creation flag 都不带（`Inherit`）。用户的终端不随 monitor 一起死（`Detached`）。",
        "Inherit · Detached · Inherit",
    ),
    (
        "platform/terminal.rs",
        "launch_powershell_window",
        "spawn_managed_cmd",
        "`powershell.exe`（常量名）",
        "`wt.exe` 起不来时的第二跳：直接起 `powershell.exe`，全仓唯一一处 `NewVisible`，刻意要一个用户看得见的新控制台。\
         argv 是那三个参数本身，env 一定继承。",
        "NewVisible · Detached · Inherit",
    ),
    (
        "lib.rs",
        "open_with",
        "spawn_managed",
        "`cmd` / `open` / `xdg-open`",
        "按平台打开日志目录：三个名字都是常量，路径是 monitor 自己的目录。`Detached` 是承重的：\
         fire-and-forget，`JobKillOnClose` 会在本函数返回、句柄一丢的瞬间把刚打开的文件管理器杀掉。",
        "Hidden · Detached · Inherit",
    ),
    (
        "local_backend.rs",
        "supervise_with_stdio",
        "ManagedSpawn",
        "被监护的后端二进制",
        "本机后端监护：二进制路径来自 `candidates`。本层住 `backend/`，平台原语进不去 ⇒ 起那一下由宿主注进来的 \
         `ManagedSpawn` 做；留在本层的是协议：`env_remove(\"TMUX\")`、argv、stdin / stdout 两根管子\
         （stdout 的 EOF 是「进程死了」这个事件的唯一来源）。",
        "Hidden · JobKillOnClose · ToLog（宿主注入：local_backend_supervised）",
    ),
    (
        "local_backend_host.rs",
        "spawn_detached",
        "managed_spawner",
        "被脱离起来的后端二进制",
        "本机后端脱离宿主起：stdio 全 null、协议走回环监听口；二进制路径来自 `local_backend::resolve_or_extract` 那一份解析。\
         住宿主知识层不住 `backend/`：`process_group` 来自 `std::os::unix`，在 `platform_home_guard` 的禁针里。\
         `Hidden`：常驻实例在 Windows 上留一个可关的黑框，等于常驻当场没了。",
        "Hidden · Detached · Null",
    ),
    (
        "local_backend_host.rs",
        "run_resident_stop_within",
        "spawn_managed_cmd",
        "`<本机后端> --resident-stop`（不经 shell）",
        "停本机常驻后端：在这台机器上跑一次那个一次性子命令，由它做「请它收尾 → 宽限期内等 → 到点强杀」，\
         monitor 只拿回结局。参数是我们自己记下的那个二进制与一个常量 flag；失败那句 `{code,message}` 在 stderr 上。",
        "Hidden · JobKillOnClose · Captured",
    ),
    (
        "filewin/proc.rs",
        "spawn_window",
        "spawn_managed_cmd",
        "`cc-monitor-filewin`（文件窗口包的 `[[bin]]`；一个窗口一个进程）",
        "winit 一个进程只许一个事件循环，「关掉就销毁」与「关掉之后还能再打开」在同进程形态下不可同时成立 ⇒ 一个窗口一个进程。\
         种子走 stdin：环境变量装不下（一屏上限 5 万条），argv 世界可读而种子里带主机名 / 用户名 / 私钥路径。\
         `Detached`：Tauri 命令一返回句柄就丢；`Captured`：「窗口为什么没立起来」那几句在 stderr 上。",
        "Hidden · Detached · Captured",
    ),
    (
        "spawn_managed.rs",
        "spawn_managed",
        "——",
        "调用方给的那个二进制 ＋ argv",
        "唯一出口：三条策略（要不要窗口 · 要不要随我死 · 错误往哪去）在这里落成平台原语，全仓只此一份；\
         `.spawn()` 与那几条平台原语只许长在这里由 `spawn_managed::tests` 钉。",
        "—— 它就是那个出口本身",
    ),
];

/// 出口自己那份源码（相对壳源码根）。
const THE_EXIT: &str = "spawn_managed.rs";

fn src_root() -> PathBuf {
    guard_core::addr::shell_src_root()
}

/// 语料 = 壳源码根那一棵（连 manifest 明写的兄弟包）＋ `build.rs`，`(模块住址, 生产段)`。
/// `build.rs` 每次 `cargo build` 都在开发者机器上真跑，也算「谁能碰这台机器」。
fn corpus() -> Vec<(String, String)> {
    let root = src_root();
    let mut out: Vec<(String, String)> = guard_core::scan_tree!(&root, &["rs"])
        .into_iter()
        .map(|(p, raw)| {
            (
                guard_core::module_address(&root, &p),
                guard_core::production_code(&raw),
            )
        })
        .collect();
    let bs = Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs");
    let raw = std::fs::read_to_string(&bs).expect("读不到 build.rs —— 它在本表的人群里");
    out.push(("build.rs".to_string(), guard_core::production_code(&raw)));
    out.sort();
    out
}

/// 这一行上声明的函数名（小写 `fn ` ＋ 词边界；`dyn Fn(` 不算）。
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

/// 某一行往回找到的最近那个 `fn` 项：`(函数名, 起始行下标, 收口行下标)`。
fn enclosing_fn_item(lines: &[&str], at: usize) -> Option<(String, usize, usize)> {
    let start = (0..=at).rev().find(|i| fn_name_on(lines[*i]).is_some())?;
    let name = fn_name_on(lines[start])?;
    let (mut depth, mut opened) = (0i32, false);
    let mut end = lines.len() - 1;
    for (k, l) in lines.iter().enumerate().skip(start) {
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
            end = k;
            break;
        }
    }
    Some((name, start, end))
}

/// 一处落点：`(文件, 外层函数, 外层函数整段, 从这一处到同一函数里下一处之前的那一段)`。
type Site = (String, String, String, String);

/// 现扫：一处一条、不去重。针运行时拼，免得命中本文件自己。
fn sites(files: &[(String, String)]) -> Vec<Site> {
    let build = concat!("Command::", "new(");
    let call = concat!("spawn_", "managed(");
    let mut out: Vec<Site> = Vec::new();
    for (file, prod) in files {
        let lines: Vec<&str> = prod.lines().collect();
        let is_the_exit = file == THE_EXIT;
        let hits: Vec<usize> = (0..lines.len())
            .filter(|&i| {
                let l = lines[i];
                // 出口自己那份只按 `Command::new` 数：`spawn_managed(` 在那里是它的签名与实现。
                l.contains(build) || (!is_the_exit && l.contains(call) && fn_name_on(l).is_none())
            })
            .collect();
        for (n, &i) in hits.iter().enumerate() {
            let (name, start, end) = enclosing_fn_item(&lines, i).unwrap_or_else(|| {
                panic!(
                    "{file} 第 {} 行的落点不在任何 `fn` 项里 —— 抽取器看不懂它了",
                    i + 1
                )
            });
            let stop = hits
                .get(n + 1)
                .copied()
                .filter(|&j| j <= end)
                .unwrap_or(end + 1);
            out.push((
                file.clone(),
                name,
                lines[start..=end].join("\n"),
                lines[i..stop].join("\n"),
            ));
        }
    }
    out
}

/// 多重集差：`a` 里比 `b` 多出来的那几个键。
fn only_in(a: &[(String, String)], b: &[(String, String)]) -> Vec<String> {
    let mut rest = b.to_vec();
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
}

fn keys(found: &[Site]) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = found
        .iter()
        .map(|(f, n, ..)| (f.clone(), n.clone()))
        .collect();
    v.sort();
    v
}

fn ledger() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = SPAWNS
        .iter()
        .map(|(f, n, ..)| (f.to_string(), n.to_string()))
        .collect();
    v.sort();
    v
}

/// 出口的入口名单：`spawn_managed.rs` 里 `pub fn` / `pub type` 中返回类型带 `Managed` 的那些。
/// 不是「所有 `pub`」：`wait_for_status` 是等那一下、不是起那一下。
fn exit_entry_points() -> Vec<String> {
    let src = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/spawn_managed.rs"
    ));
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
        // 签名可能跨行 ⇒ 收到收口那一行为止。
        let mut sig = String::new();
        for l2 in lines[i..].iter().take(16) {
            sig.push_str(l2);
            sig.push('\n');
            if (kind == "fn" && l2.contains('{')) || (kind == "type" && l2.contains(';')) {
                break;
            }
        }
        if sig
            .rsplit_once("->")
            .is_some_and(|(_, t)| t.contains("Managed"))
        {
            out.push(name);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// `spawn_managed.rs` 里某个枚举的变体名（从源码派生）。
fn variants_of(enum_name: &str) -> Vec<String> {
    let src = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/spawn_managed.rs"
    ));
    let head = format!("pub enum {enum_name} {{");
    let i = src
        .find(&head)
        .unwrap_or_else(|| panic!("`spawn_managed.rs` 里找不到 `{head}` —— 枚举改名或搬家了"));
    let rest = &src[i + head.len()..];
    let end = rest
        .find("\n}")
        .unwrap_or_else(|| panic!("`pub enum {enum_name}` 的花括号没收口 —— 抽取器看不懂它了"));
    rest[..end]
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_suffix(','))
        .filter(|l| !l.is_empty() && l.chars().all(|c| c.is_alphanumeric()))
        .map(str::to_string)
        .collect()
}

/// →**起进程的落点与本表按处相等**（多重集，两向）。
#[test]
fn every_local_spawn_is_declared() {
    let files = corpus();
    assert!(
        files.iter().any(|(f, _)| f == "lib.rs") && files.iter().any(|(f, _)| f == THE_EXIT),
        "语料里没有 `lib.rs` / `{THE_EXIT}`（{} 份）—— 遍历坏了，本条在空转",
        files.len()
    );
    let found = sites(&files);
    let (disk, table) = (keys(&found), ledger());
    let missing = only_in(&disk, &table);
    assert!(
        missing.is_empty(),
        "这些地方会在用户机器上起一个进程，但本表没有那一行（一处一行）：\n{}\n\
         ⇒ 往 `SPAWNS` 加一行：起的是什么 · 为什么必须起进程 · 交给出口哪个入口 · 选了哪三条策略。\
         答不出第三列 ⇒ 它绕开了出口，改成走 `spawn_managed_cmd`（`backend/` 那一半收一个 `ManagedSpawn`）。",
        missing.join("\n")
    );
    let stale = only_in(&table, &disk);
    assert!(
        stale.is_empty(),
        "本表这几行在盘面上找不到了：\n{}\n\
         ⇒ 那一处真走了 ⇒ 删这一行；改名 / 搬家 ⇒ 改这一行。本趟语料 {} 份、现扫 {} 处。",
        stale.join("\n"),
        files.len(),
        disk.len()
    );
}

/// 正控：往已登记的函数里再加一句裸 `Command::new`，处数就对不上；`spawn_managed(` 那一形也算一处。
#[test]
fn a_second_bare_command_in_one_function_is_a_second_site() {
    let one = "fn f() {\n    let mut c = std::process::Command::new(a);\n    spawn_managed_cmd(&mut c);\n}\n";
    let two = "fn f() {\n    let mut c = std::process::Command::new(a);\n    spawn_managed_cmd(&mut c);\n\
               let mut d = std::process::Command::new(b);\n    spawn_managed_cmd(&mut d);\n}\n\
               fn g() {\n    crate::spawn_managed::spawn_managed(p, &[], X, Y, Z);\n}\n";
    let at = |src: &str| sites(&[("x.rs".to_string(), src.to_string())]);
    assert_eq!(keys(&at(one)), vec![("x.rs".into(), "f".into())]);
    let k = keys(&at(two));
    assert_eq!(
        k,
        vec![
            ("x.rs".to_string(), "f".to_string()),
            ("x.rs".to_string(), "f".to_string()),
            ("x.rs".to_string(), "g".to_string()),
        ],
        "同一个函数里第二句 `Command::new` 没被数成第二处 —— 本表此刻按函数去重了"
    );
    assert_eq!(
        only_in(&k, &keys(&at(one))).len(),
        2,
        "多重集差没数出多出来的那两处"
    );
}

/// →**第三列可判**：入口名在派生名单里，且真出现在那一处的外层函数项里。
#[test]
fn every_site_names_the_exit_entry_it_hands_off_to() {
    let entries = exit_entry_points();
    let found = sites(&corpus());
    let mut bad: Vec<String> = Vec::new();
    let mut seen: Vec<(String, String)> = Vec::new();
    for (file, func, entry, ..) in SPAWNS {
        let key = (file.to_string(), func.to_string());
        let nth = seen.iter().filter(|k| **k == key).count();
        seen.push(key.clone());
        if *entry == "——" {
            if *file != THE_EXIT {
                bad.push(format!(
                    "  {file}::{func} 第三列写 `——`，只有出口自己那一行许这么写"
                ));
            }
            continue;
        }
        if !entries.iter().any(|e| e == entry) {
            bad.push(format!(
                "  {file}::{func} 第三列写的是 `{entry}`，而 `spawn_managed.rs` 今天的入口是 {entries:?}"
            ));
            continue;
        }
        let item = found
            .iter()
            .filter(|(f, n, ..)| *f == key.0 && *n == key.1)
            .nth(nth)
            .map(|(_, _, item, _)| item.clone());
        match item {
            None => bad.push(format!(
                "  {file}::{func} —— 盘面上没有这一处（上一条先说这件事）"
            )),
            Some(s) if !s.contains(entry) => bad.push(format!(
                "  {file}::{func} 写着交给 `{entry}`，那个函数项里找不到这个名字"
            )),
            Some(_) => {}
        }
    }
    assert!(
        bad.is_empty(),
        "第三列与盘面对不上：\n{}\n⇒ 那一处真的改了交接方式 ⇒ 改第三列；交接挪进了同文件的辅助函数 ⇒ 这里会假红，改回在原处交接。",
        bad.join("\n")
    );
}

/// 入口名单的阴性对照：它不是「所有 `pub fn`」。
#[test]
fn the_exit_entry_point_list_is_derived_and_does_not_say_yes_to_every_pub_fn() {
    let entries = exit_entry_points();
    for must in ["spawn_managed", "spawn_managed_cmd", "ManagedSpawn"] {
        assert!(
            entries.iter().any(|e| e == must),
            "派生出来的入口名单里没有 `{must}`：搬家 / 改名了就改 `SPAWNS` 第三列，否则派生法坏了。现打：{entries:?}"
        );
    }
    assert!(
        !entries.iter().any(|e| e == "wait_for_status"),
        "`wait_for_status` 混进入口名单了 —— 派生法松成了「凡 `pub fn` 皆入口」，上一条会恒真。现打：{entries:?}"
    );
}

/// →**第六列与盘面对拍**：三个变体名从枚举派生，且写在那一处的那一段里（宿主注入的写在 `spawn_managed.rs`）。
#[test]
fn the_three_policies_each_site_declares_match_the_code() {
    let consoles = variants_of("ConsolePolicy");
    let lifetimes = variants_of("Lifetime");
    let sinks = variants_of("StderrSink");
    assert_eq!(
        (consoles.len(), lifetimes.len(), sinks.len()),
        (3, 2, 4),
        "三个枚举的变体数变了（现打 {consoles:?} / {lifetimes:?} / {sinks:?}）—— 改完这一行，逐条看第六列"
    );
    let files = corpus();
    let found = sites(&files);
    let exit_src = &files
        .iter()
        .find(|(f, _)| f == THE_EXIT)
        .expect("语料里没有出口那份")
        .1;
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut checked = 0usize;
    for (file, func, _, _, _, policy) in SPAWNS {
        let key = (file.to_string(), func.to_string());
        let nth = seen.iter().filter(|k| **k == key).count();
        seen.push(key.clone());
        if policy.starts_with("——") {
            assert_eq!(
                *file, THE_EXIT,
                "{file}::{func} 说自己不进那个出口 —— 只许出口自己那一行"
            );
            continue;
        }
        let triple: Vec<&str> = policy
            .split('（')
            .next()
            .unwrap_or(policy)
            .split(" · ")
            .map(str::trim)
            .collect();
        assert_eq!(
            triple.len(),
            3,
            "{file}::{func} 的第六列不是三格：{policy:?}"
        );
        let seg = if policy.contains("宿主注入") {
            exit_src.clone()
        } else {
            found
                .iter()
                .filter(|(f, n, ..)| *f == key.0 && *n == key.1)
                .nth(nth)
                .map(|(.., s)| s.clone())
                .unwrap_or_else(|| panic!("{file}::{func} 盘面上没有这一处"))
        };
        for (i, (name, allowed)) in [
            ("ConsolePolicy", &consoles),
            ("Lifetime", &lifetimes),
            ("StderrSink", &sinks),
        ]
        .iter()
        .enumerate()
        {
            let v = triple[i];
            assert!(
                allowed.iter().any(|a| a == v),
                "{file}::{func} 第 {} 格写的是 `{v}`，而 `{name}` 今天的变体是 {allowed:?}",
                i + 1
            );
            let needle = format!("{name}::{v}");
            assert!(
                seg.contains(&needle),
                "{file}::{func}（同函数第 {} 处）的账本写着 `{needle}`，那一段源码里找不到它 —— \
                 那一处改了策略 ⇒ 改第六列；`backend/` 那两处自己不写策略 ⇒ 第六列带「（宿主注入：…）」",
                nth + 1
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "一个带策略的落点都没对拍到 —— 本条此刻在空转");
}
