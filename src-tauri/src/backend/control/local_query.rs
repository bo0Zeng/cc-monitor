//! F10a：**本机一次性查询的传输** —— 「本地 = 不走 ssh 的远端」那句话的落地。
//!
//! # 它是什么、为什么形态是这样
//!
//! daemon 的**读面**是 14 条一次性查询子命令（`--list-projects` / `--usage` / `--search` …）。
//! ⚠ 那批读**不在常驻通道上**：流连接的 hello 声明的 `commands` 是
//! `["cancel","kill","launch","ping","resolve"]` —— **一条读命令都没有**。
//! ⇒ 「把本机读面切到后端」的意思是 **exec 一次 sidecar 拿 stdout**，
//! **不是**跟那个被监护的常驻进程说话（那个是给 observe / 控制用的）。
//!
//! 远端那条路早就是这个形态（`ssh host <daemon> --list-projects`），
//! 而本机一直缺这一跳 —— `doc/ARCHITECTURE.md §1.1` 里写着
//! 「POSIX 本地 = 不走 ssh 的远端，同一套分解，只是没有 SSH 那一跳」，本模块就是那一跳的本地版。
//!
//! ⚠ **协议一个字都不用改**（定框 C1「一份代码两种承载」在读面上的最省形态）。
//! 那一点是刻意的：加读命令要动 hello 的 `commands` 集 = 动**仓外 aterm 也在读的协议面**，
//! 而对面今天有通报闸门 —— **在没法沟通的时候改共享契约是最坏的时机。**
//!
//! # 诚实降级不是可选项（定框 §5）
//!
//! sidecar 可能**不在**：开发树里今天就没有（`externalBin` 只在发版 `--config` 时注入，F05b）。
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
            // 保留原样（含尾部换行由调用方决定怎么显示）——daemon 把失败原因写在 stderr 上，
            // 而 `--fork-session` 那族的经验是：截断/改写它等于把用户能看懂的原因弄丢。
            stderr,
        },
    }
}

/// 跑一条本机一次性查询。`args` 是子命令及其参数，例如 `["--usage"]`。
///
/// ⚠ **不做重试、不做超时**：这两件都属调用方的策略（历史面愿意等、UI 探针不愿意），
/// 而在这一层写死会让两种调用方之一必然错。如实记为诚实边界。
// F10b 第一批起有生产调用方（`usage.rs`），不再需要 `allow(dead_code)`。
pub(crate) fn run_query(target_triple: &str, args: &[&str]) -> QueryOutcome {
    let bin: PathBuf = match super::local_backend::resolve_beside_this_exe(target_triple) {
        super::local_backend::Resolved::Found(p) => p,
        super::local_backend::Resolved::Missing { reason, looked_at } => {
            return QueryOutcome::NoBackend(format!("{reason}；找过 {looked_at:?}"));
        }
    };
    match std::process::Command::new(&bin).args(args).output() {
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
mod tests {
    use super::*;

    /// 三态各自的判定口径。
    #[test]
    fn exit_zero_is_the_only_success_and_stderr_survives_failure() {
        assert_eq!(
            classify(Some(0), "line1\nline2\n".into(), String::new()),
            QueryOutcome::Ok("line1\nline2\n".into())
        );
        // ⚠ 退出码非 0 时 stdout 里可能**也有内容**（daemon 边写边失败），但那不是成功。
        assert_eq!(
            classify(Some(2), "partial\n".into(), "boom\n".into()),
            QueryOutcome::Failed {
                code: Some(2),
                stderr: "boom\n".into()
            }
        );
        // 被信号杀掉：`code()` 是 None，仍是失败而不是「后端不在」。
        assert_eq!(
            classify(None, String::new(), String::new()),
            QueryOutcome::Failed {
                code: None,
                stderr: String::new()
            }
        );
    }

    /// ★ **「后端不在」必须与「查询失败」分得开** —— 压成一个 `Err` 就是让上层猜。
    ///
    /// 这条钉的是**类型上的可区分性**，不是某个字符串：`Failed` 那支带得出退出码，
    /// `NoBackend` 那支带得出「找过哪些路径」。
    #[test]
    fn no_backend_is_not_a_failed_query() {
        let missing = run_query("x86_64-unknown-linux-gnu-does-not-exist", &["--usage"]);
        match &missing {
            QueryOutcome::NoBackend(reason) => {
                // 理由里必须能看出「找过哪儿」，否则 UI 只能说一句「不可用」。
                assert!(
                    reason.contains("cc-monitor-remote") || reason.contains("找过"),
                    "「后端不在」的理由里看不出找过哪些路径：{reason}"
                );
            }
            other => panic!(
                "开发树里没有 sidecar，本条应当走 `NoBackend`，实得 {other:?}\n\
                 ⚠ 如果这台机器上**确实**有 sidecar（比如刚跑过发版构建），\
                 那本条会误报 —— 那时该把它改成注入一个不存在的 triple，而不是放宽断言。"
            ),
        }
        assert!(!matches!(missing, QueryOutcome::Failed { .. }));
    }

    /// ★ **前提触发器 —— 已经触发过一次，这是它的后继形态**（F10b 第一批，2026-08-04）。
    ///
    /// # 原形是什么、为什么换
    ///
    /// 原形：断言本模块**零生产调用方**，一有调用方就红并喊「回去把棘轮往下拧」。
    /// F10b 第一批（`usage.rs` 改走 `--usage`）时它**确实红了**，而且红得对 ——
    /// 棘轮当场从 11 拧到 10、`usage.rs` 那条登记删掉。
    ///
    /// ⇒ 换成后继形态：**每一个调用方都必须是「已退役」的那批**。
    /// 判定不靠手写清单：拿本模块的生产调用方集合，与
    /// `local_read_surface_registry::REGISTERED` 里**还挂着 `reader`** 的文件集合求交 ——
    /// **交集必须为空**。一个文件既在调后端、又还记在「未退役」账上，那就是假账。
    ///
    /// ⚠ **这不是降强度**：原形只能红**一次**（第一个调用方），此后永远绿；
    /// 后继形态对**每一批迁移**都有效，而且它钉的是更难的那件事
    /// （「棘轮跟着动了」而不只是「有人调了」）。
    ///
    /// ⚠ 判定用的是**遍历 `src/` 生产段找 `run_query(`**，不是手写清单。
    #[test]
    fn every_caller_of_this_transport_is_already_off_the_read_surface_ledger() {
        let src_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        // 运行时拼，免得命中本文件自己。
        let verb = format!("run_{}(", "query");
        let mut callers = Vec::new();
        let mut stack = vec![src_root.clone()];
        let mut scanned = 0usize;
        let mut nested = 0usize;
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_some_and(|x| x == "rs") {
                    let raw = std::fs::read_to_string(&p).unwrap_or_default();
                    let prod = guard_core::production_code(&raw);
                    scanned += prod.len();
                    if p.parent() != Some(src_root.as_path()) {
                        nested += 1;
                    }
                    // 本文件自己的定义行不算调用方。
                    let rel = p
                        .strip_prefix(&src_root)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/");
                    if rel.ends_with("local_query.rs") {
                        continue;
                    }
                    if prod.contains(verb.as_str()) {
                        callers.push(rel);
                    }
                }
            }
        }
        // 中间量自检：遍历必须真的**递归**过。
        //
        // ⚠ **第一版只断言「扫到的字节数 > 200 000」，而那条挡不住「不递归」** ——
        // `src/` 顶层本身就有 60+ 个平铺 `.rs`（含 6000 行的 `ssh_source.rs`），
        // 字节地板光靠顶层就满足了。变异 Y4（把 `stack.push(p)` 删掉、只走一层）
        // **从那一版里活着走了出去**，而本条的头注恰恰声称它能逮住「遍历坏了」。
        // ⇒ 改成直接钉「递归发生了」：扫到的文件里必须有**嵌套**的
        //   （实测嵌套 17 个，分布在 `src/adapter`、`src/backend`、`src/backend/control`）。
        assert!(
            nested > 10,
            "只扫到 {nested} 个嵌套目录下的文件 —— 遍历没有递归，本条在空转\
             （实测嵌套 17 个：`adapter/` `backend/` `backend/control/`）"
        );
        assert!(
            scanned > 200_000,
            "剥完生产段只扫到 {scanned} 字节 —— 连顶层都没读到，遍历彻底坏了"
        );
        // 后继形态：调用方**不许**还挂在「未退役」账上。
        // ⚠ 账本的真相源是那个模块自己的 `REGISTERED`，这里**不抄一份文件名单** ——
        // 读它的源码把还标着 `reader` 的文件名抽出来（同一个数/同一张表只有一个家，定框 §4）。
        let ledger = std::fs::read_to_string(src_root.join("local_read_surface_registry.rs"))
            .expect("读不到 local_read_surface_registry.rs");
        let mut still_on_ledger: Vec<&String> = Vec::new();
        for c in &callers {
            // 登记表里的键是 `src/<rel>`。
            let key = format!("\"src/{c}\"");
            if let Some(at) = ledger.find(key.as_str()) {
                // 该条目的类别就在文件名之后不远处；只要它还标着 reader 就算「未退役」。
                let window = &ledger[at..(at + 120).min(ledger.len())];
                if window.contains("\"reader\"") {
                    still_on_ledger.push(c);
                }
            }
        }
        assert!(
            still_on_ledger.is_empty(),
            "这些文件**既在调本机后端、又还挂在「未退役」账上**：{still_on_ledger:?}\n\
             ⇒ 那是假账，而且是最坏的那种：它让下一个人以为工作量还在。\n\
             正解：把它那条登记删掉，并把 `local_read_surface_registry` 的递减棘轮往下拧一格\n\
             （那个数只有一个家 —— `every_registered_file_declares_what_kind_of_read_it_is`，\n\
             别在别处抄第二份）。"
        );
        // 反向锚点：抽取器真的读到了账本，否则上面那条零命中地绿。
        assert!(
            ledger.contains("\"reader\"") && ledger.len() > 3000,
            "账本只读到 {} 字节或里面没有 `reader` —— 抽取坏了，本条在空转",
            ledger.len()
        );
    }
}
