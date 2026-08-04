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
//! 是纯搬运；只建一个空目录则是装饰。⇒ 先建 `control/`，`observe/` 等 U7。
//! 这条**不是遗漏**，下面 `every_file_under_backend_lives_on_a_capability_line` 那条判据
//! 认 `observe/`，它一建出来就自动纳入。

pub mod control;

/// 本目录下每个文件的**归属登记**：`(相对 `backend/` 的路径, 能力线, 一句为什么在这里)`。
///
/// 加文件不写理由 ⇒ 下面那条判据红。这就是当初 `src/` 摊成五十多个平铺文件的那道缺口 ——
/// 一个目录只要没有「什么该进来」的说法，它就会变成下一个平铺堆。
#[cfg(test)]
const BACKEND_FILES: &[(&str, &str, &str)] = &[
    ("mod.rs", "-", "边界本身的说明 + 两条机检"),
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
        "control/launch_wire.rs",
        "control",
        "前端结构化请求 → wire 适配 → ccm 调用行 / 裸载荷（两个 tauri 命令）",
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
}
