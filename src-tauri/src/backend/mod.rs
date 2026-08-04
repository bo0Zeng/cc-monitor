//! `backend/` —— **monitor 侧的后端边界**（P4a，§1.4b，用户 2026-08-03 选 A）。
//!
//! # 病：这个边界的缺席，造出了第三个地方
//!
//! daemon 内部有 §1.1 的三分（`platform/` · `observe/` · `control/`），而 monitor 侧
//! **一个边界都没有** —— `src-tauri/src/` 是平铺的五十多个 `.rs`，唯一子目录是 `adapter/`。
//! 于是 U8c 一族要给「起会话的渲染」找个家时，只剩共享 crate 一条路 ⇒
//! **那个共享 crate（当时叫 `launch-core`）的存在是「因为没有第二个地方，才造了第三个地方」**
//! （架构审计 2026-08-03 实测：daemon 对整个 crate 的用量只有一行 `posix_quote`）。
//!
//! # 顶层架构里它是谁
//!
//! 「后端 = 一份代码、两种宿主（本机进程 / 远端进程）」。**这个目录是本机那种宿主**；
//! `remote-daemon-proto/` 是远端那种。两边**同一套分解**：读（observe）与控制（control）。
//!
//! ⚠ **`observe/` 今天刻意还没建**：monitor 侧的读面（`local_accounts.rs` 30KB +
//! `history_query.rs` 一族）正是 **U7 要退役的那批** —— 现在把它们搬进来、再由 U7 删掉
//! 是纯搬运；只建一个空目录则是装饰。⇒ 先建 `control/`，`observe/` 等那批读面退役。
//! 这条**不是遗漏**，下面 `every_file_under_backend_lives_on_a_capability_line` 那条判据
//! 认 `observe/`，它一建出来就自动纳入。
//!
//! ⚠⚠ **谁来叫醒这个决定**〔F18 补〕：上面只说了「等 U7」——那是个**没有触发器的等待**，
//! 而 U7 的正题被「安装包里真有本机 daemon」挡着（F05b，要真 Windows 机）。
//! 真正会叫醒它的是 `local_read_surface_registry` 里那条**前提触发器**：
//! `tauri.conf.json` 一出现 `externalBin`，它就红 —— 那一刻本机才真有个后端可切，
//! 「把读面搬进 `observe/` 再退役」才不是纯搬运。**别再把这句读成「等某个人想起来」。**

pub mod control;

/// 本目录下每个文件的**归属登记**：`(相对 `backend/` 的路径, 能力线, 一句为什么在这里)`。
///
/// 加文件不写理由 ⇒ 下面那条判据红。这就是当初 `src/` 摊成五十多个平铺文件的那道缺口 ——
/// 一个目录只要没有「什么该进来」的说法，它就会变成下一个平铺堆。
#[cfg(test)]
const BACKEND_FILES: &[(&str, &str, &str)] = &[
    (
        "mod.rs",
        "-",
        "边界本身的说明 + **六条**机检（F18 加了 C10 那条与它的反向锚点）。\
         ⚠ 这一格原本写「两条机检」，而实测当时已经有四条 —— **它在 F18 之前就过期了**，\
         是 S11 那一族（「描述当下」的字段最易腐）在本仓的又一处",
    ),
    ("control/mod.rs", "control", "写/控制面的说明"),
    (
        "control/ccm_invocation.rs",
        "control",
        "ctx → `ccm …` 调用行（维度注册表 + 诚实降级）。P4b 从共享 crate 搬回归属地",
    ),
    (
        "control/payload.rs",
        "control",
        "`env 前缀 → cd → argv → wrap` 载荷编译器。P4b 从共享 crate 搬回归属地",
    ),
    (
        "control/daemon_launch.rs",
        "control",
        "U8a-2c-1：daemon `launch` 的发送端（`send-into` 那半边；attach 留在用户终端）",
    ),
    (
        "control/daemon_route.rs",
        "control",
        "F04c：「这条命令能不能回落」的**唯一**判定（`kill` 与 `send-keys` 共用）。\
         分界线是「能不能**证明**这条命令根本没发出去」，不是「成功/失败」。\
         两份实现必漂，而漂开的后果是把一次**被门拒绝**洗成另一条路的成功",
    ),
    (
        "control/daemon_send_keys.rs",
        "control",
        "F04c：daemon `send-keys` 的发送端。★ `enter` 落在**两个 mode 名**上而不是一个字段 —— \
         `parse_request` 不 deny unknown fields ⇒ 旧 daemon 会静默忽略字段照样附 `Enter`，\
         把「打断当前回合」变成「提交用户输入框里排队的文本」",
    ),
    (
        "control/daemon_kill.rs",
        "control",
        "F04b：daemon `kill` 的发送端（C6 那条顺序的最后一步）。★ 结局是**三态**而不是两态 —— \
         分界线是「能不能证明这条命令根本没发出去」：能证明才许回落到过渡期的 SSH 路（C7），\
         否则一律不回落。把 `wrong_owner`/`too_many_windows` 当成「daemon 不可用」而回落，\
         等于把一次**被门拒绝**洗成另一条路的成功",
    ),
    (
        "control/local_backend.rs",
        "control",
        "F05a：本机后端进程的「起与看住」。决策那半（sidecar 路径解析 + 崩溃频率上限）是纯函数；\
         监护器用 `std::process::Command`，等子进程死靠**读它 stdout 到 EOF**（零定时器，C12）。\
         ⚠ 今天只认打包进安装包的 sidecar、不扫 dev 产物 —— 理由是 daemon 一起来就无条件\
         往 tmux server 装全局 hook 且没有开关（F05 摸底 §2.5）",
    ),
    (
        "control/launch_wire.rs",
        "control",
        "前端结构化请求 → wire 适配 → ccm 调用行 / 裸载荷（两个 tauri 命令）",
    ),
    (
        "control/agent_profile_parity.rs",
        "control",
        "F06：agent 适配表（`fixtures/agent-profile-golden.tsv`）的跨语言对拍 —— \
         **C4「ccm 变零决策」的前置**：搬之前先证明三份副本逐字一致。\
         另钉两条前提：ccm 独有的两个决策仍无 Rust 对侧 · ccm 仍拒绝 codex 的 subcommand 形 resume",
    ),
    (
        "control/gate2_parity.rs",
        "control",
        "F03：§34 Gate 2 判定表（`fixtures/gate2-golden.tsv`）在 monitor 这一侧的独立对拍。\
         同一张表另有两个读者：daemon 的 `control/gate.rs` 与 `e2e/daemon-gate2-acceptance.sh`",
    ),
    (
        "control/launch_cli_parity.rs",
        "control",
        "上面那条 ccm 调用行与 TS 黄金串的跨语言逐字节对拍",
    ),
    (
        "control/launch_payload_parity.rs",
        "control",
        "上面那条裸载荷与 TS 黄金串的跨语言逐字节对拍",
    ),
];

#[cfg(test)]
mod tests {
    use super::BACKEND_FILES;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn backend_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend")
    }

    /// `backend/` 下的所有 `.rs`，路径相对 `backend/`，`/` 分隔。
    fn backend_files() -> Vec<String> {
        let root = backend_dir();
        let mut out = Vec::new();
        walk(&root, &root, &mut out);
        out.sort();
        out
    }

    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(
                    p.strip_prefix(root)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    /// ★ 抽取器自检：遍历坏掉时下面两条会零命中零失败地绿。
    #[test]
    fn the_backend_scan_actually_finds_files() {
        let n = backend_files().len();
        assert!(
            n >= BACKEND_FILES.len(),
            "只扫到 {n} 个文件，登记表有 {} 条 —— 遍历器坏了",
            BACKEND_FILES.len()
        );
    }

    /// ★ 目录内容 == 登记表。**两个方向都查**：
    /// 多的文件没写理由 ⇒ 红；登记表里写了不存在的文件 ⇒ 也红（搬走/改名忘了改表）。
    #[test]
    fn every_file_under_backend_is_registered_with_a_reason() {
        let on_disk = backend_files();
        let mut registered: Vec<String> = BACKEND_FILES
            .iter()
            .map(|(f, _, _)| f.to_string())
            .collect();
        registered.sort();
        assert_eq!(
            on_disk, registered,
            "`backend/` 的内容与登记表不一致。\n\
             多出来的文件请在 `BACKEND_FILES` 里写明它属于哪条能力线、为什么在这里；\n\
             登记表里多出来的条目说明有文件被搬走/改名了。"
        );
        for (f, _, why) in BACKEND_FILES {
            assert!(!why.trim().is_empty(), "{f} 的理由是空的");
        }
    }

    /// ★ 每个文件都必须**住在一条能力线上** —— `backend/` 根下只允许 `mod.rs`。
    ///
    /// 这条钉的是 §1.1 那条线在 monitor 侧也成立：读与控制分开，不许有「既不是读也不是写」
    /// 的第三堆。daemon 侧同一条纪律由 `layering_guard` 管。
    #[test]
    fn every_file_under_backend_lives_on_a_capability_line() {
        for (f, line, _) in BACKEND_FILES {
            if *f == "mod.rs" {
                continue;
            }
            assert!(
                matches!(*line, "control" | "observe"),
                "{f} 的能力线是 {line:?} —— 只能是 control 或 observe"
            );
            assert!(
                f.starts_with(&format!("{line}/")),
                "{f} 登记为 {line} 线，却不在 `{line}/` 目录下"
            );
        }
        for f in backend_files() {
            assert!(
                f == "mod.rs" || f.starts_with("control/") || f.starts_with("observe/"),
                "`backend/` 根下只允许 mod.rs，`{f}` 既不在 control/ 也不在 observe/ —— \
                 一个既不是读也不是写的第三堆，就是边界开始溶解的样子"
            );
        }
    }

    /// ★ **宿主无关**：`backend/` 的生产段里不许出现 GUI 宿主的把手。
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
        let root = backend_dir();
        let mut offenders = Vec::new();
        let mut scanned = 0usize;
        for f in backend_files() {
            let src =
                guard_core::production_code(&fs::read_to_string(root.join(&f)).unwrap_or_default());
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
        ]
    }

    /// 一份源码里命中的平台形态。
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
    /// daemon 侧 CI 有一步 `cargo check --all-targets --target x86_64-pc-windows-msvc`，
    /// 逐字标着「平台线的真判据」。**monitor 照抄不了**：本机实测 exit=101 ——
    /// 挡路的**不是 monitor 的代码**（252 个 `.rmeta` 已经产出），
    /// 是某个 C 依赖的 build script 要 `lib.exe`（MSVC 的库工具），Linux 上没有。
    /// ⇒ monitor 侧「两个平台都编得过」这条性质由 **CI 的两个 OS 各自原生编**承担：
    /// `rust` job 在 windows-latest 跑 `cargo test --all` ·
    /// `linux-app-build` job 在 ubuntu-latest 跑 `cargo build`。
    /// 本条是它的**源码形态那一半**：编译只能证明「今天两边都过」，
    /// 挡不住「往 backend 里塞一段 `#[cfg]` 分叉、两边各编一半」——那才是 C10 真正怕的。
    #[test]
    fn the_backend_half_stays_platform_agnostic() {
        let root = backend_dir();
        let mut offenders = Vec::new();
        let mut scanned = 0usize;
        for f in backend_files() {
            let raw = fs::read_to_string(root.join(&f)).unwrap_or_default();
            let prod = guard_core::production_code(&raw);
            guard_core::assert_no_test_code(&f, &prod);
            scanned += prod.len();
            for hit in platform_hits(&prod) {
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

    /// ★ **反向锚点：那套形态不是瞎的。**
    ///
    /// 上一条是「什么都没发生」型断言 —— 它零命中地绿，可能是因为 backend 真干净，
    /// 也可能是因为那套形态一个都匹配不上。⇒ 拿 monitor **另一半**里平台面最重的两个文件
    /// 当标的：它们**必须**命中。
    ///
    /// ⚠ 锚点按实测选（F18 摸底逐文件数过）：`bind.rs` 平台 cfg 21 处 / 原语 9 处 ·
    /// `session_map.rs` 原语 18 处。**这两处不是 bug** —— 它们是 C9 那一半，本来就该有平台代码。
    #[test]
    fn the_platform_needles_actually_match_the_platform_heavy_half() {
        let src_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        // 地板按**实测的「命中几种形态」**写，不是「命中几次」——
        // ⚠ 第一版把 21 处 / 9 处（次数）当成了种类数，判据当场红。
        // 实测种类数：`utils.rs` **6** · `bind.rs` **4** · `session_map.rs` **4**。
        // 地板留一格余量（少一种形态不算警报，少两种就说明形态集在烂）。
        for (rel, least) in [
            ("utils.rs", 5usize),
            ("bind.rs", 3usize),
            ("session_map.rs", 3usize),
        ] {
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
}
