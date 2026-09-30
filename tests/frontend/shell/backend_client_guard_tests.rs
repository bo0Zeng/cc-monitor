//! 要求住址：`设计/00 §2.2`「monitor 侧的 `backend/` 目录有一道宿主无关判据（禁 `AppHandle`/`State<`/`.emit(`）与一道平台无关判据」·
//! 〔THIN〕`99 §2.3`「壳里 `src/frontend/shell/src/backend/` 九份逐份回真住址、`backend` 目录名从壳里消失」。
//!
//! 〔THIN〕从前本文件住 `backend/mod.rs` 的测试段（`backend_tests.rs`〔散文墓碑〕），人群是「`backend/` 目录下的全部 `.rs`」，
//! 外加两条按目录认的登记（目录 == `BACKEND_FILES`〔散文墓碑〕两向 · 每份都住在 `control/` / `observe/` 能力线上）与一份层间方向判据
//! （`backend_layering.rs`〔散文墓碑〕：`observe/` 早删了，只剩一条线）。目录没了 ⇒ 按目录认的三条随之退役；
//! 宿主无关 · 平台无关两条**人群一个不少**：改成逐个点名的那一组（[`GUARDED`]），两向钉住「点名的都在」。
use std::fs;
use std::path::{Path, PathBuf};

/// 从前 `backend/` 目录那一组（〔THIN〕今天都住壳根，通信层成员 `backend_route` 住 `src/comms/inward/`），相对仓根。
/// 删了的两份（`tmux.rs` · `gate2_parity.rs`，子步 1）不在。**加一份调后端的客户端 / 宿主进来就在这里加一行。**
const GUARDED: &[&str] = &[
    // 〔P1〕`agent_profile_parity.rs` 出列：monitor 那份适配表删了，对拍随家进了后端 `agents_tests.rs`。
    "src/frontend/shell/src/backend_control.rs",
    "src/frontend/shell/src/cc_bus.rs",
    "src/frontend/shell/src/frame_query.rs",
    "src/frontend/shell/src/inbound_client.rs",
    "src/frontend/shell/src/local_backend.rs",
    "src/comms/inward/backend_route.rs",
];

fn repo_root() -> PathBuf {
    crate::guard_support::repo_root()
}

/// 两道守卫共同的扫描面：`(展示名, 绝对路径)`。
fn guarded_files() -> Vec<(String, PathBuf)> {
    GUARDED
        .iter()
        .map(|rel| (rel.to_string(), repo_root().join(rel)))
        .collect()
}

/// 那一组的 `(绝对路径, 全文)`（别的判据借这一份人群，别各读各的）。读不到 ⇒ 空串（调用方的地板认得出）。
pub(crate) fn guarded_sources() -> Vec<(PathBuf, String)> {
    guarded_files()
        .into_iter()
        .map(|(_, p)| {
            let s = fs::read_to_string(&p).unwrap_or_default();
            (p, s)
        })
        .collect()
}

/// ★ 点名的都在（读不到的文件只会静默返回空串，下面两条会零命中地绿）。
#[test]
fn every_guarded_file_exists() {
    for (f, p) in guarded_files() {
        assert!(
            p.is_file(),
            "点名的 {f} 不在了 —— 搬走 / 改名就改 `GUARDED`，删了就摘掉那一行"
        );
    }
    assert!(Path::new(&repo_root()).is_dir());
}

/// ★ **宿主无关**：那一组的生产段里不许出现 GUI 宿主的把手。
///
/// 这条是「一份代码两种宿主」在今天**唯一可机检的形态**：一旦这里的代码抓了窗口把手
/// 或自己 emit 事件，它就只能跑在 GUI 进程里 —— 而 U8a-2c / U9b 的前提正是它能被
/// 换个宿主跑起来。
///
/// ⚠ **`#[tauri::command]` 是允许的**（登记在案的例外）：它是 IPC 入口的**标注**，
/// 标注之下的函数体仍须宿主无关 —— 那正是本条查的东西。
/// ⚠ 它是**约定型守卫**（同 `readonly_guard` 一族）：查的是符号名的源码形态，
/// 挡得住「顺手 `app.emit` 一下」，挡不住「换个名字继续错」。**比没有强，别读成证明。**
#[test]
fn the_backend_layer_stays_host_agnostic() {
    const FORBIDDEN: &[&str] = &[
        "AppHandle",
        "tauri::Window",
        "WebviewWindow",
        "State<",
        ".emit(",
        "Emitter",
        "Manager",
    ];
    let mut offenders = Vec::new();
    let mut scanned = 0usize;
    for (f, path) in guarded_files() {
        let src = guard_core::production_code(&fs::read_to_string(&path).unwrap_or_default());
        scanned += src.len();
        for needle in FORBIDDEN {
            if src.contains(needle) {
                offenders.push(format!("  {f}: `{needle}`"));
            }
        }
    }
    // 剥完还得有东西可扫 —— 否则这条零命中变绿。
    assert!(
        scanned > 4000,
        "剥掉测试段后只剩 {scanned} 字节可扫，这条会零命中变绿"
    );
    assert!(
        offenders.is_empty(),
        "`backend/` 的生产段抓了 GUI 宿主的把手 —— 那它就只能跑在 GUI 进程里，\n\
             而「一份代码两种宿主」的前提是它能被换个宿主跑起来：\n{}",
        offenders.join("\n")
    );
}

/// F18 要查的那批形态：平台 `cfg` 与平台原语。运行时拼，免得命中本文件自己。
fn platform_needles() -> Vec<String> {
    let cfg = "cfg";
    vec![
        format!("#[{cfg}(windows)"),
        format!("#[{cfg}(unix)"),
        format!("#[{cfg}(not(windows)"),
        format!("#[{cfg}(not(unix)"),
        format!("{}_os = \"", "target"),
        format!("{}_family", "target"),
        "libc::".to_string(),
        format!("std::os::{}", "unix"),
        format!("std::os::{}", "windows"),
        format!("{}_sys::", "windows"),
        "winapi::".to_string(),
        // ⚠ **这三条是反向锚点逼出来的。** 第一版形态集只有 `libc::` / `std::os::*` /
        // `windows_sys::` / `winapi::` —— 而本仓的 Windows 面走的是 **`windows` crate**
        // （`Cargo.toml` 的 `[target.'cfg(windows)'.dependencies] windows = "0.56"`）。
        // ⇒ 有人往 `backend/` 里写一行 `use windows::Win32::...` 时，
        // 上面那条判据会**零命中地绿**。反向锚点当场红了，才发现这个洞。
        format!("{}::Win32", "windows"),
        format!("{}::core", "windows"),
        format!("use {}::", "windows"),
        // ⚠ **第四批，audit-0805 F19 下半补的**：`std::env::consts::*` 是
        // **没有 `cfg` 的平台原语** —— `EXE_SUFFIX` 在 Windows 上是 `.exe`、别处是空串。
        // 上面那十几条形态一个都匹配不上它，于是 `local_backend.rs::resolve_beside_this_exe` 那处
        // **在生产段里逃逸了整整一轮**（F19 §4 点过名，但当时归因成「管辖面太窄」，
        // 实际是**形态集太窄**）。
        // ★ 这已经是形态集第二次被扩：第三批是反向锚点逼出来的，这批是逐条读代码读出来的。
        format!("env::{}::", "consts"),
    ]
}

/// 一份源码里命中的平台形态。
/// **平台原语的单点例外**：`(文件, 形态, 为什么允许, 「已收敛」的机检锚点)`。
///
/// ⚠ 这不是豁免清单 —— 每条都要满足两件事，各有一条判据看着：
/// ① **单点**（该形态在该文件生产段里恰好出现 **1** 次）；
/// ② **「已收敛」不是散文** —— 第四列是那句话的机检锚点，锚点没了就红。
#[allow(clippy::type_complexity)]
const PLATFORM_EXCEPTIONS: &[(&str, &str, &str, &str)] = &[(
    "src/frontend/shell/src/local_backend.rs",
    "env::consts::",
    "`EXE_SUFFIX` 是**没有 cfg 的平台原语**（Windows `.exe` / 别处空串）。         它没被搬进 `platform/`，但**平台差异已经收敛成一个注入参数**：         `resolve_beside_this_exe` 把它读出来喂给 `resolve_with`，         而 `resolve_with`（逻辑那半）与平台无关、在任何平台上都能测。         ⇒ 出路②「建 backend/platform/」为它一个常量建一层目录不划算；走出路③，登记在此。",
    // ⚠ 锚点要**不含糊**：第一版写的是 `"exe_suffix: &str"`，而同文件的
    // `local_backend_candidates` 也有同名参数 ⇒ 把 `resolve_with` 的参数改名，
    // 判据**照样绿**（变异实测）。改成多行签名片段。
    // ★ 与 F05「起流/起流程」、F16「src/backend/-X」同族：**匹配单位比事实小**。
    "pub fn resolve_with(\n    exe_dir: &Path,\n    target_triple: &str,\n    exe_suffix: &str,",
)];

fn platform_hits(prod: &str) -> Vec<String> {
    platform_needles()
        .into_iter()
        .filter(|n| prod.contains(n.as_str()))
        .collect()
}

/// ★★ **F18 / C10 在 monitor 侧的落点**：`backend/` 的生产段里不许有平台 `cfg`
/// 与平台原语。
///
/// # 摸底把这件的前提证伪了一半
///
/// 路线图原写「C10 在 monitor 侧零落地，而且**没有任何判据、登记表或诚实边界提到过它**」。
/// 实测：`backend/` 的**生产段零平台 cfg、零平台原语** —— 那 3 处
/// 平台 cfg 全在 `control/local_backend.rs` 的**测试段**（660 / 722 / 726 行，
/// chmod 0o755 与 kill/taskkill，都是夹具在收拾自己起的子进程）。
///
/// ⇒ C10 在它该管的范围里**已经成立**。缺的不是「落地」，是
/// **① 没人认领 ② 没有判据钉住它不退化** ——
/// ★ 与 F17 那条「危害不是『无人看管』而是『无人认领』」**完全同形**。
///
/// # 范围：为什么不是整个 monitor crate
///
/// 定框 §5 逐字写的是「monitor 侧同名镜像（`src/backend/`）」⇒ C10 管的是 backend 那一半。
/// 另一半（`bind.rs` 的窗口把手 · `launch.rs` 的开窗 · `session_map.rs` 的进程身份）
/// 是 **C9** 的活：「在用户桌面上开一个终端窗口」本身就是平台特定的，
/// 把它搬进 `platform/` 不会让它变得可移植，只会让 C10 变成一句摆设。
/// 实测那一半有 **47 处**平台原语命中（`session_map.rs` 18 · `bind.rs` 9 · `launch.rs` 4 …）
/// —— 本条**刻意不管它们**，而且正好拿它们当反向锚点（见下）。
///
/// # C10 说「判据是跨 target 编译」，那 monitor 侧的那一半在哪
///
/// backend 侧 CI 有一步 `cargo check --all-targets --target x86_64-pc-windows-msvc`，
/// 逐字标着「平台线的真判据」。**monitor 照抄不了**：本机实测 exit=101 ——
/// 挡路的**不是 monitor 的代码**（252 个 `.rmeta` 已经产出），
/// 是某个 C 依赖的 build script 要 `lib.exe`（MSVC 的库工具），Linux 上没有。
/// ⇒ monitor 侧「两个平台都编得过」这条性质**本来**由 CI 的两个 OS 各自原生编承担：
/// `rust` job 在 windows-latest 跑 `cargo test --workspace` ·
/// `linux-app-build` job 在 ubuntu-latest 跑 `cargo build`。
///
/// ⚠ **「本来」两个字是 08-06 补的，它现在不成立**：`ci.yml` 只在 `push` / `pull_request`
/// 上触发，而〔用 08-05〕裁定不再 push ⇒ 至今 70+ 个提交**一次都没跑过**。
/// 也就是说 monitor 的 Windows 面已经很久没有被任何编译器看过，
/// 而这段头注原文会让人以为它有人管。**这不是判据的洞，是判据的前提没了。**
/// 实况与解锁条件记在 `ROADMAP §5` 的 3y；前提本身由
/// `shared_crate_registry::the_windows_cross_target_signal_covers_only_the_backend`
/// 盯着（backend 那步被删 / monitor 那侧补上 / vendor 依赖变 optional，三种都会红）。
/// 本条是它的**源码形态那一半**：编译只能证明「今天两边都过」，
/// 挡不住「往 backend 里塞一段 `#[cfg]` 分叉、两边各编一半」——那才是 C10 真正怕的。
#[test]
fn the_backend_half_stays_platform_agnostic() {
    let mut offenders = Vec::new();
    let mut scanned = 0usize;
    for (f, path) in guarded_files() {
        let raw = fs::read_to_string(&path).unwrap_or_default();
        let prod = guard_core::production_code(&raw);
        guard_core::assert_no_test_code(&f, &prod);
        scanned += prod.len();
        for hit in platform_hits(&prod) {
            let excused = PLATFORM_EXCEPTIONS
                .iter()
                .any(|(ef, en, _, _)| f.ends_with(ef) && hit.contains(en));
            if excused {
                continue;
            }
            offenders.push(format!("  {f}: `{hit}`"));
        }
    }
    assert!(
        scanned > 4000,
        "剥掉测试段后只剩 {scanned} 字节可扫，这条会零命中变绿"
    );
    assert!(
        offenders.is_empty(),
        "`backend/` 的生产段出现了平台 cfg 或平台原语：\n{}\n\
             ⚠ C10：`platform/` 是**唯一**允许它们的地方，而 backend 侧今天还没有 `platform/`。\n\
             三条出路，别默认第一条：① 这段其实属于 frontend 那一半（开窗 / 窗口把手 ⇒ C9），搬回去；\n\
             ② 它真是 backend 要的平台原语 ⇒ 建 `backend/platform/` 并把它收进去；\n\
             ③ 都不是 ⇒ 说清为什么，进诚实边界总账。",
        offenders.join("\n")
    );
}

/// ★ 例外必须是**单点**，而且「已收敛」那句话得有锚点。
///
/// 没有这条，例外表就是豁免清单：写一行理由，那个文件里就能随便加平台代码。
#[test]
fn every_platform_exception_is_a_single_point_and_its_claim_is_anchored() {
    let root = repo_root();
    for (file, needle, why, anchor) in PLATFORM_EXCEPTIONS {
        let raw = fs::read_to_string(root.join(file))
            .unwrap_or_else(|e| panic!("例外表里的 {file} 读不到：{e} —— 搬走了就把这条删掉"));
        let prod = guard_core::production_code(&raw);
        let n = prod.matches(needle).count();
        assert_eq!(
            n, 1,
            "`{file}` 的生产段里 `{needle}` 出现 {n} 次 —— 例外**只许单点**。\n\
                 多出一处就不再是「平台差异收敛在一个注入点」，而是「这个文件开始长平台分支了」\n\
                 ⇒ 回去走出路①/②（搬回 frontend / 建 `backend/platform/`），别在例外表里加行。"
        );
        assert!(
            prod.contains(anchor),
            "`{file}` 里找不到锚点 `{anchor}` —— 例外的理由是「平台差异已收敛成一个注入参数」，\n\
                 而那句话的**唯一证据**就是这个签名。锚点没了，理由就成了散文。\n\
                 理由原文：{why}"
        );
    }
}

/// ★ 例外的形态必须**真的在形态集里** —— 否则这条例外是句空话，
/// 而且「那个形态压根不被扫」这件事会**悄悄回来**。
///
/// ⚠ 变异实测：把 `env::consts::` 从 `platform_needles()` 里删掉，
/// 主判据与两条例外判据**全都照样绿** —— 因为例外只描述「允许什么」，
/// 不保证「那东西真的被扫」。这条补上那一格。
#[test]
fn every_exception_names_a_needle_that_is_actually_scanned_for() {
    let needles = platform_needles();
    for (file, needle, ..) in PLATFORM_EXCEPTIONS {
        assert!(
            needles
                .iter()
                .any(|n| n.contains(needle) || needle.contains(n.as_str())),
            "例外表给 `{file}` 登记的形态 `{needle}` **不在 `platform_needles()` 里**。\n\
                 ⇒ 这条例外是句空话：那个形态根本不被扫，写不写都一样。\n\
                 更糟的是**它反过来也成立** —— 有人把形态从集合里删掉时，\n\
                 主判据会安静地少扫一类东西，而例外表看起来还好好的。"
        );
    }
}

/// 例外表不许长草：登记的形态必须**真的还在命中**（否则它是条死规则）。
#[test]
fn the_platform_exception_table_is_not_dead_wood() {
    let root = repo_root();
    for (file, needle, ..) in PLATFORM_EXCEPTIONS {
        let raw = fs::read_to_string(root.join(file)).unwrap_or_default();
        let prod = guard_core::production_code(&raw);
        assert!(
            prod.contains(needle),
            "例外表里的 `{file}` 已经不含 `{needle}` 了 —— 删掉这条。\n\
                 留着就是一条永远不匹配的死规则，而死规则会在下次真有人写它时**悄悄放行**。"
        );
    }
    // 例外只许少不许多（**递减棘轮**）。
    assert!(
        PLATFORM_EXCEPTIONS.len() <= 1,
        "平台例外涨到 {} 条了 —— 只许降。C10 的意思是「平台原语有唯一的家」，\
             例外每多一条，那句话就弱一分。",
        PLATFORM_EXCEPTIONS.len()
    );
}

/// ★ **反向锚点：那套形态不是瞎的。**
///
/// 上一条是「什么都没发生」型断言 —— 它零命中地绿，可能是因为 backend 真干净，
/// 也可能是因为那套形态一个都匹配不上。⇒ 拿 monitor **另一半**里平台面最重的两个文件
/// 当标的：它们**必须**命中。
///
/// ⚠ 锚点按实测选（F18 摸底逐文件数过）：`bind.rs` 平台 cfg 21 处 / 原语 9 处 ·
/// `session_map.rs` 原语 18 处。**这两处不是 bug** —— 它们是 C9 那一半，本来就该有平台代码。
///
/// # ★ 「平台 cfg 有多少处」的**口径**（08-06 补：此前只有数，没有数法）
///
/// ```text
/// grep -rEc '#\[cfg.*(windows|unix|target_os)' --include=*.rs src/frontend/shell/src
/// ```
///
/// 即**按行数**、`#[cfg…]` 里出现那三个词之一就算一处。别的数法会给出别的答案：
/// 只认 `cfg(windows)`/`cfg(unix)`/`cfg(target_os…)` 三种完整形态的话，同一份
/// `bind.rs` 只有一半左右 —— 差在 `#[cfg(all(…))]`/`#[cfg(any(…))]`/`#[cfg(not(…))]`
/// 这些复合写法上。
///
/// 上面 `bind.rs` 那个数就是用这个口径量的，它同时是**校准锚点**：换个数法对不上 21，
/// 说明用错了口径。
///
/// ⚠ **总数刻意不写在这里，也不写进计划**：它每加一个平台分支就变，
/// 而没有任何判据读它 ⇒ 抄到哪里就在哪里腐（`plan-lint` 判据 3.9 那一族）。
/// 计划侧（`ROADMAP §5 2h`、`features/F19-*`）只说「C9 那一半占绝大多数」并指到这里。
/// 08-06 实测顺带纠正一处口误：那个数是**全 `src/frontend/shell/src` 的总量**，
/// 不是「backend 之外那一半」—— 后者要再减掉 backend 测试段里的那几处。
#[test]
fn the_platform_needles_actually_match_the_platform_heavy_half() {
    let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    // 地板按**实测的「命中几种形态」**写，不是「命中几次」——
    // ⚠ 第一版把 21 处 / 9 处（次数）当成了种类数，判据当场红。
    // 实测种类数：`utils.rs` **6** · `bind.rs` **4**。
    // 地板留一格余量（少一种形态不算警报，少两种就说明形态集在烂）。
    // 〔LOC1b · 第四波 4D〕`session_map.rs`（原 4 种）那个锚摘了：monitor 自己那份进程身份判活（`/proc` · `GetProcessTimes`）
    //   随本机判活改由本机后端的帧来删了，那份文件今天零平台形态 —— 它不再是「平台重的那一半」，拿它当锚会恒红。
    for (rel, least) in [("utils.rs", 5usize), ("bind.rs", 3usize)] {
        let p = src_root.join(rel);
        assert!(
            p.is_file(),
            "反向锚点 {} 不存在 —— 读不到的文件只会静默返回空串，那会让上一条判据的\
                 「零命中」失去意义",
            p.display()
        );
        let prod = guard_core::production_code(&fs::read_to_string(&p).unwrap_or_default());
        let hits = platform_hits(&prod);
        assert!(
            hits.len() >= least,
            "反向锚点 `src/{rel}` 只命中 {} 种平台形态（至少要 {least}）：{hits:?}\n\
                 那套形态多半坏了 —— 而它一坏，`the_backend_half_stays_platform_agnostic`\n\
                 就变成一条永远绿的空判据。",
            hits.len()
        );
    }
}
