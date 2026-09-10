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
//! # `observe/` 那个决定：🔴 **它已经被叫醒了，只是没人听见**〔订正 2026-09-10〕
//!
//! 〔墓碑 —— 原话逐字（两段，先后写于 P4a 与 F18）：
//!
//!  「⚠ **`observe/` 今天刻意还没建**：monitor 侧的读面（`local_accounts.rs` 30KB +
//!  `history_query.rs` 一族）正是 **U7 要退役的那批** —— 现在把它们搬进来、再由 U7 删掉
//!  是纯搬运；只建一个空目录则是装饰。⇒ 先建 `control/`，`observe/` 等那批读面退役。」
//!
//!  「⚠⚠ **谁来叫醒这个决定**〔F18 补〕：上面只说了「等 U7」——那是个**没有触发器的等待**，
//!  而 U7 的正题被「安装包里真有本机 daemon」挡着（F05b，要真 Windows 机）。
//!  真正会叫醒它的是 `local_read_surface_registry` 里那条**前提触发器**：
//!  `tauri.conf.json` 一出现 `externalBin`，它就红 —— 那一刻本机才真有个后端可切，
//!  「把读面搬进 `observe/` 再退役」才不是纯搬运。**别再把这句读成「等某个人想起来」。**」〕
//!
//! ## 为什么说它已经被叫醒了（三条，逐条有出处）
//!
//! ① **前提翻了**：F05b 已落地并随 v3.7.0 发出去。09-10 干净 win11 上现打（PM 的读数）——
//!    装出来那份 `cc-monitor-remote.exe` **2 个进程在跑**、裸 `monitor.exe` **0 个**。
//! ② **不只是「有后端可切」，是已经切过两次**：`local_read_surface_registry` 的棘轮史逐字
//!    记着 `11 → 10`（F10b 第一批，`usage.rs` 改走本机后端的 `--usage`）→ `9`
//!    → `8`（F10b 第二批·下半，`local_accounts.rs` 改走 sidecar 的 `--session-accounts`）
//!    → `7` → `8`（`P8a` 新增）。接线点今天就在生产段上：`usage.rs::aggregate_usage_all`
//!    （一个 `#[tauri::command]`）直接调 `backend::control::local_query::run_query(…, ["--usage"])`。
//! ③ 🔴 **`backend/` 里今天已经住着读面代码，只是挂在 `control` 线上** ——
//!    `control/local_query.rs` 头注第一句逐字是「daemon 的**读面**是 14 条一次性查询子命令」。
//!    ⇒ 「只建一个空目录是装饰」那条反对理由**今天不成立**：`observe/` 一建出来就有真住户，
//!    而那个住户此刻住在**错的能力线**上 —— 那正是这条边界当初要防的事。
//!
//! ⚠ 墓碑第一段里另有三处已经不成立的事实，一并记下（同族，S11「描述当下的字段最易腐」）：
//! `local_accounts.rs` 的读面**已经退役了**，它不再是「U7 要退役的那批」；
//! `history_query.rs` **不在 monitor 侧** —— 它住 `remote-daemon-proto/src/observe/history_query.rs`，
//! 是 daemon 的文件，monitor 从来没有过这个文件（`git log --all` 对该路径零提交）；
//! 「30KB」今天是 62KB。
//!
//! ## 下一步是什么（有形状的一件活，不是「等某个人想起来」）
//!
//! **建 `observe/`，把 `control/local_query.rs` 挪成 `observe/local_query.rs`**，
//! 连 `BACKEND_FILES` 那条登记的能力线一起改成 `observe`。那是**真搬运**不是装饰：
//! 它本来就是读面的传输，挂在 `control` 上只是因为当初没有第二个地方可挂。
//! ⚠ **本拍没做这一步**（写区不含 `control/mod.rs` 与那批 `use`）。挪的时候有两条判据看着：
//! 下面的 `every_file_under_backend_is_registered_with_a_reason`（目录与登记表两个方向都查）
//! 与 `every_file_under_backend_lives_on_a_capability_line`（它**认** `observe/`，
//! 也只认 `control` / `observe` 两条线）。这条**不是遗漏**，`observe/` 一建出来就自动纳入。
//!
//! ## 🔴 但原句的后半（「把**那批读面**搬进来再退役」）没有被叫醒 —— 今天没有触发器
//!
//! 挡着它的是两样有名有姓的东西，**F05b 一条都没动**：
//!
//! - **daemon 侧的查询集缺口**：剩下 8 条 `reader` 每一条的退役条件写的都是 daemon 侧一个
//!   具体缺口（`--list-projects` 缺会话 sid 清单 · daemon 侧没有 codex 的项目枚举 ·
//!   `--search` 没索引 · 缺 `--list-marketplaces` · 删会话 daemon 侧无对侧）。
//!   `local_read_surface_registry` 头注逐字：「至此本机读面**在现有 daemon 查询集下已无可退**」。
//! - **宿主耦合**：`backend/` 有一道宿主无关守卫（下面 `the_backend_layer_stays_host_agnostic`，
//!   禁 `AppHandle` / `tauri::Window` / `WebviewWindow` / `State<` / `.emit(` / `Emitter` / `Manager`）。
//!   8 条 `reader` 里**有 2 条今天就带着这些把手**（09-10 现打：`tasks.rs` 有
//!   `use tauri::{AppHandle, Emitter}` 与一处 `app.emit(`；`search.rs` 有三处 `tauri::State<`）
//!   ⇒ 它们**原样搬不进来**，得先把宿主耦合剥掉。
//!
//! ⇒ **今天没有触发器**：仓里没有任何一条判据会在「那 8 条能退役了」的那一刻红。
//! 🔴 **不许再在这里编一个** —— 上面墓碑第二段就是那么来的：一个不会响的闹钟比一句过期的话更贵，
//! 因为它让人以为有人在看着。
//! ⚠ 尤其**别**把 `local_read_surface_registry` 那条 sidecar 判据当成它的闹钟：那条 2026-08-04
//! 就换过靶（它自陈「已经触发过一次，这是它的后继形态」），今天盯的是**配置文件的形状**
//! （`externalBin` 不在 `tauri.conf.json` · 在 `tauri.sidecar.conf.json` · stem 与 `SIDECAR_STEM`
//! 一致），**一格都不读「本机后端起没起来」**；而且「`tauri.conf.json` 一出现 `externalBin`
//! 就红」这句今天**语义是反的** —— 它红是「有人把它搬回主配置了」（回归），不是「那一刻到了」。

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
        "control/local_query.rs",
        "control",
        "F10a：**本机一次性查询的传输** —— 「本地 = 不走 ssh 的远端」那一跳的本地版。\
         daemon 的读面是 14 条一次性子命令，**不在常驻通道上**（hello 的 `commands` 里\
         一条读命令都没有）⇒ 切读面 = exec 一次 sidecar 拿 stdout。协议一个字不改。\
         ★ **它是本目录里唯一的读面文件** —— 挂在 `control` 线上只因为 `observe/` 还没建，\
         见本文件头注「下一步是什么」那一节。\
         〔订正 2026-09-10 —— 本格原话逐字：「⚠ 今天**零生产调用方**（F10b 接线），\
         由它自己那条前提触发器盯着 —— 一有调用方就红，逼 `local_read_surface_registry` \
         的棘轮跟着往下拧」。**两句今天都不成立**：那条触发器 2026-08-04 就已经响过一次并\
         换成了后继形态（`every_caller_of_this_transport_is_already_off_the_read_surface_ledger`，\
         改成「每个调用方都必须已经从棘轮账上下来」）；而生产调用方 09-10 现打**不是零** —— \
         `usage.rs::aggregate_usage_all` 与 `local_accounts.rs::list_local_session_accounts` \
         都在调它。⇒ 这里不再写「有几个调用方」这种会腐的数，那个数的家在那条判据里〕",
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

    /// ★★ 住在 `backend/` **之外**、但属于 backend 那一半的文件 —— 必须一起受两道守卫管。
    ///
    /// # 为什么需要这张表〔G2，Phase G 整体设计审计的产物〕
    ///
    /// 两道守卫（宿主无关 / 平台无关）此前只扫 `src/backend` 子树。
    /// 而 `inbound_client.rs` **物理位置在 `src/` 顶层** ⇒ **整个在扫描面之外**。
    /// 审计当天实测它是干净的（无 `AppHandle` / `State<` / `.emit(`）——
    /// **但那是巧合，不是被钉住**。它承担的正是 **C1**「一份代码两种承载」里
    /// 「本机进程」那一半最关键的传输层（`daemon_kill` / `daemon_launch` /
    /// `daemon_send_keys` 共同依赖它）。它一旦长出宿主耦合，backend 的护栏体系**整体看不见**。
    ///
    /// ⚠ **为什么是加进扫描面而不是挪文件**：挪 1183 行的文件会动到一大批 `use` 路径与
    /// `mod` 声明，半径远大于收益，且与 `cross_half_edge_registry` 的边登记表相互作用。
    /// **先把它纳入管辖，挪不挪是另一件事**（若将来挪进 `backend/`，把这一行删掉即可）。
    const EXTRA_BACKEND_FILES: &[(&str, &str)] = &[(
        "inbound_client.rs",
        "daemon 流通道的 wire 客户端 —— C1「本机进程」那一半的传输层，\
         被 daemon_kill / daemon_launch / daemon_send_keys 共同依赖。\
         它不在 backend/ 下是历史位置，不是它不属于这一半。",
    )];

    /// 两道守卫共同的扫描面：`backend/` 全部 `.rs` + [`EXTRA_BACKEND_FILES`]。
    /// 返回 `(展示名, 绝对路径)`。
    fn guarded_files() -> Vec<(String, PathBuf)> {
        let root = backend_dir();
        let mut out: Vec<(String, PathBuf)> = backend_files()
            .into_iter()
            .map(|f| {
                let p = root.join(&f);
                (f, p)
            })
            .collect();
        let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for (f, _) in EXTRA_BACKEND_FILES {
            out.push((format!("（表外）{f}"), src_root.join(f)));
        }
        out
    }

    /// ★ 表外那几个必须真的存在 —— 挡「文件改名/挪走后登记留成僵尸」。
    /// 僵尸的后果不是红，是**那一行悄悄不再扫任何东西**（扫描面缩水且无人知道）。
    #[test]
    fn the_extra_backend_files_are_not_ghosts() {
        let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        assert!(
            !EXTRA_BACKEND_FILES.is_empty(),
            "表空了 —— 若真的把它们都挪进了 `backend/`，连这条一起删；\n\
             但别留一张空表假装还有人管"
        );
        for (f, why) in EXTRA_BACKEND_FILES {
            assert!(
                src_root.join(f).is_file(),
                "`src/{f}` 不存在 —— 登记成了僵尸，那一行从此不扫任何东西"
            );
            assert!(
                why.len() > 30,
                "`{f}` 的理由太短（{} 字）—— 这张表的价值全在「为什么它属于 backend 那一半」",
                why.len()
            );
        }
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
        "control/local_backend.rs",
        "env::consts::",
        "`EXE_SUFFIX` 是**没有 cfg 的平台原语**（Windows `.exe` / 别处空串）。         它没被搬进 `platform/`，但**平台差异已经收敛成一个注入参数**：         `resolve_beside_this_exe` 把它读出来喂给 `resolve_with`，         而 `resolve_with`（逻辑那半）与平台无关、在任何平台上都能测。         ⇒ 出路②「建 backend/platform/」为它一个常量建一层目录不划算；走出路③，登记在此。",
        // ⚠ 锚点要**不含糊**：第一版写的是 `"exe_suffix: &str"`，而同文件的
        // `sidecar_candidates` 也有同名参数 ⇒ 把 `resolve_with` 的参数改名，
        // 判据**照样绿**（变异实测）。改成多行签名片段。
        // ★ 与 F05「起流/起流程」、F16「remote-daemon-proto/-X」同族：**匹配单位比事实小**。
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
    /// daemon 侧 CI 有一步 `cargo check --all-targets --target x86_64-pc-windows-msvc`，
    /// 逐字标着「平台线的真判据」。**monitor 照抄不了**：本机实测 exit=101 ——
    /// 挡路的**不是 monitor 的代码**（252 个 `.rmeta` 已经产出），
    /// 是某个 C 依赖的 build script 要 `lib.exe`（MSVC 的库工具），Linux 上没有。
    /// ⇒ monitor 侧「两个平台都编得过」这条性质**本来**由 CI 的两个 OS 各自原生编承担：
    /// `rust` job 在 windows-latest 跑 `cargo test --workspace --exclude code-picture-core` ·
    /// `linux-app-build` job 在 ubuntu-latest 跑 `cargo build`。
    ///
    /// ⚠ **「本来」两个字是 08-06 补的，它现在不成立**：`ci.yml` 只在 `push` / `pull_request`
    /// 上触发，而〔用 08-05〕裁定不再 push ⇒ 至今 70+ 个提交**一次都没跑过**。
    /// 也就是说 monitor 的 Windows 面已经很久没有被任何编译器看过，
    /// 而这段头注原文会让人以为它有人管。**这不是判据的洞，是判据的前提没了。**
    /// 实况与解锁条件记在 `ROADMAP §5` 的 3y；前提本身由
    /// `shared_crate_registry::the_windows_cross_target_signal_covers_only_the_daemon`
    /// 盯着（daemon 那步被删 / monitor 那侧补上 / vendor 依赖变 optional，三种都会红）。
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
        let root = backend_dir();
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
        let root = backend_dir();
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
    /// grep -rEc '#\[cfg.*(windows|unix|target_os)' --include=*.rs src-tauri/src
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
    /// 08-06 实测顺带纠正一处口误：那个数是**全 `src-tauri/src` 的总量**，
    /// 不是「backend 之外那一半」—— 后者要再减掉 backend 测试段里的那几处。
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
