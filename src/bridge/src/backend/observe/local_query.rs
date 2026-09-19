//! F10a：**本机一次性查询的传输** —— 「本地 = 不走 ssh 的远端」那句话的落地。
//!
//! # 它是什么、为什么形态是这样
//!
//! backend 的**读面**是 14 条一次性查询子命令（`--list-projects` / `--usage` / `--search` …）。
//! ⚠ 那批读**不在常驻通道上**：流连接的 hello 声明的 `commands` 是
//! `["cancel","kill","launch","ping","resolve"]` —— **一条读命令都没有**。
//! ⇒ 「把本机读面切到后端」的意思是 **exec 一次本机后端拿 stdout**，
//! **不是**跟那个被监护的常驻进程说话（那个是给 observe / 控制用的）。
//!
//! 远端那条路早就是这个形态（`ssh host <backend> --list-projects`），
//! 而本机一直缺这一跳 —— `src/doc/ARCHITECTURE.md §1.1` 里写着
//! 「POSIX 本地 = 不走 ssh 的远端，同一套分解，只是没有 SSH 那一跳」，本模块就是那一跳的本地版。
//!
//! ⚠ **协议一个字都不用改**（定框 C1「一份代码两种承载」在读面上的最省形态）。
//! 那一点是刻意的：加读命令要动 hello 的 `commands` 集 = 动**仓外 aterm 也在读的协议面**，
//! 而对面今天有通报闸门 —— **在没法沟通的时候改共享契约是最坏的时机。**
//!
//! # 诚实降级不是可选项（定框 §5）
//!
//! local_backend 可能**不在**：开发树里今天就没有（`externalBin` 只在发版 `--config` 时注入，F05b）。
//! ⇒ 本模块的返回值是 **tagged 三态**，不是 `Result<String, String>`：
//! 调用方必须能分开「后端不在」（该回落/该提示装）与「后端在但这条查询失败了」（该报原因）。
//! 把两者压成一个 `Err(String)` 就是让上层猜 —— 那正是 F14 那次「静默回落」的形状。
//!
//! # 调用方与账本必须同步（F10b）
//!
//! F10a 交付传输，F10b 逐批把那 5 个**有对侧**的 reader 迁过来（第一批：`usage.rs`）。
//! 下面那条判据钉住**每一个调用方都已经从「未退役」账上下来了** ——
//! 一个文件既在调后端、又还记在账上，就是「切了后端但棘轮没动」的假账。

use std::path::PathBuf;

/// 一次本机查询的结局。**三态**，理由见模块头注。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QueryOutcome {
    /// 查询成功，`stdout` 原样带回（逐行 JSON，由调用方按自己那条查询的形状解析）。
    Ok(String),
    /// **本机后端不在** —— 不是失败，是「今天这台机器上没有对侧」。
    /// `reason` 逐字带上找过哪些路径，供 UI 直说而不是猜。
    NoBackend(String),
    /// 后端在、查询跑了、但它失败了。`code` 是退出码，`stderr` 原样带回。
    Failed { code: Option<i32>, stderr: String },
}

/// 把一次 exec 的三样东西折成 [`QueryOutcome`]。
///
/// ⚠ **抽出来是为了能不 spawn 进程就单测**：判定口径（什么算成功、stderr 怎么带）
/// 是这一层唯一的决策，而 spawn 本身没什么可判的。
pub(crate) fn classify(code: Option<i32>, stdout: String, stderr: String) -> QueryOutcome {
    match code {
        Some(0) => QueryOutcome::Ok(stdout),
        other => QueryOutcome::Failed {
            code: other,
            // 保留原样（含尾部换行由调用方决定怎么显示）——backend 把失败原因写在 stderr 上，
            // 而 `--fork-session` 那族的经验是：截断/改写它等于把用户能看懂的原因弄丢。
            stderr,
        },
    }
}

/// 跑一条本机一次性查询。`args` 是子命令及其参数，例如 `["--list-accounts"]`。
///
/// ⚠ **不做重试、不做超时**：这两件都属调用方的策略（历史面愿意等、UI 探针不愿意），
/// 而在这一层写死会让两种调用方之一必然错。如实记为诚实边界。
// F10b 第一批起有生产调用方（`usage.rs`），不再需要 `allow(dead_code)`。
/// ⚠ **`spawn` 是注入进来的**〔`15 §5.1 A3`，09-18〕：起进程那一下的三个答案
/// （要不要窗口 · 要不要随我死 · 错误往哪去）要落成平台原语，而平台原语进不了本层
/// （`the_backend_half_stays_platform_agnostic` 的禁针 ＋ 平台例外表的递减棘轮）。
/// ⇒ 形状照 `control::local_backend::start_or_extract` 的 `make_executable` 那个先例。
pub(crate) fn run_query(
    target_triple: &str,
    args: &[&str],
    spawn: &crate::spawn_managed::ManagedSpawn,
) -> QueryOutcome {
    // 🔴 这是本文件唯一一条**跨能力线**的引用（`observe → control`），`K-R71` 归位时才显形 ——
    // 先前它写成 `super::local_backend::…`，因为两个文件当时同住 `control/`。
    // 方向是对的（backend 侧 `layering_guard` 逐字：`observe → control` 许、反向一条都不许），
    // ★〔`K-R73` 09-12，`DECISIONS.md#R29` 裁定三〕**这条边现在有登记的家了**：
    // `backend/mod.rs` 的 `layering` 模块照后端的形立了两条判据 ——
    // 正向逐条列举、条数被等号钉住（下面三个符号各占一条），反向零容忍。
    // 〔本段原话逐字，留作来历：「⚠ 但**monitor 侧今天没有任何判据在数这条边**：backend
    //  那侧要求「接口面显式列举、条数钉住」（`ALLOWED_OBSERVE_TO_CONTROL`），
    //  monitor 侧的对应物**不存在**。如实记，不假装钉住了。」〕
    // ⚠ 加一处新的 `control::` 引用**会红** —— 那不是坏了，是要你先回答
    // 「为什么这件事非得由读面发起」，再把它写进那张表。
    let bin: PathBuf =
        match crate::backend::control::local_backend::resolve_beside_this_exe(target_triple) {
            crate::backend::control::local_backend::Resolved::Found(p) => p,
            crate::backend::control::local_backend::Resolved::Missing { reason, looked_at } => {
                return QueryOutcome::NoBackend(format!("{reason}；找过 {looked_at:?}"));
            }
        };
    let mut cmd = std::process::Command::new(&bin);
    cmd.args(args).stdout(std::process::Stdio::piped());
    match spawn(&mut cmd).and_then(|c| c.wait_with_output()) {
        Ok(out) => classify(
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ),
        // 起不来（权限 / 文件损坏 / 架构不符）也算「后端不在」——对调用方的意义相同：
        // 今天这台机器上没有可用的对侧。
        Err(e) => QueryOutcome::NoBackend(format!("起 {} 失败：{e}", bin.display())),
    }
}

#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/observe/local_query_tests.rs"]
mod tests;
