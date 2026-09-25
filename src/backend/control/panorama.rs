//! 〔RM1c · 第四波〕**代码全景**的帧命令 `panorama` —— 后端经插件通用调用口起那个只装引擎的
//! 独立小程序（用户 09-24 V108 选 B），自己一行引擎代码都不链。
//!
//! # 为什么是「起一个进程」，不是「链进来」
//!
//! 引擎 ≈19.6 MB / 架构（`调研/第四波记录/RM1b.md §3`）。编进后端本体 = 每一台远端都背它，
//! 且与 `C21`「code-picture 走独立二进制……**也不编进 backend**」、`95 §0`「不并进后端本体」正面相撞；
//! 那两条由 `plugin_class_registry` ②③ 与 `panorama_locus_guard` 正题② 钉着，本模块一条都不碰。
//! ⇒ 解析发生在**被起的那个进程**里；本模块只做插件口的四段里**属于这个插件**的那几样：
//!
//! | 段 | 本模块给的 |
//! |---|---|
//! | ① 找它 | 候选：后端自己那个可执行文件旁边 → `<家>/.cc-monitor/bin/`；**不兜 `PATH`**（同名的无关程序由身份行再挡一道） |
//! | ② 问它会什么 | 要的能力 = 这一次的 op（缺哪个说哪个 —— 「这台装的小程序太旧」与「调用失败」是两件事） |
//! | ③ 起它 | 期限按 op：建索引一档、其余一档（`u64` 秒，交给子进程的 `timeout` 前缀；**后端零定时器不破**） |
//! | ④ 码 → 语义 | **这张表只住这里**（插件口只抽骨架）：见 [`classify`] |
//!
//! # 只说查询语义（`protocol_doc_guard` 那条 `P7c-2` 约束）
//!
//! 线上只有一条命令名 `panorama`，`op` 只许 [`OPS`] 里的词（查询 ＋ 建索引）；
//! 存储、grammar、解析开关**一个都不上线**。[`OPS`] 与小程序那张 op 表两向相等
//! （判据运行时读小程序的源码，异源）。
//!
//! # 写面
//!
//! 后端进程自己**零写盘**。被起的那个进程只写**索引**，落 [`store_dir`]
//! （那台机器上后端自己的数据目录 `<家>/.cc-monitor/panorama/`，不是用户文件）。
//! 〔RM1d · 用户 09-24 V110「引擎只算、文件管理来写」〕批注 / 文档关联：小程序只有 `plan_*`
//! （读盘上现状 ＋ 算出新内容，交回 `{value, edit: {rel, before, after, parents}}`）；
//! 落盘是**另一条**命令 —— monitor 拿着计划去问同一台后端的 `files-put`（带 `expect = before`）/
//! `files-delete`，本命令里一个字节都不写。
//!
//! # 诚实边界
//!
//! - 〔RM1f〕**打得断**：异步档（`Run::Async`），两次起进程都走 `plugin::invoke::run_abortable` ——
//!   `cancel` 命中 ⇒ 处理器 future 被丢 ⇒ 小程序连同 `timeout` 前缀那一组子进程一起被杀、回 `cancelled`。
//!   期限照旧住子进程（[`BUILD_DEADLINE_SECS`]）；「长期限 ＋ 可取消」（`RM1b.md §3.3` ③）两半都齐了。
//!   〔墓碑 —— RM1c 那一版这里写着「**打不断**：阻塞档（起进程、等它退出），`cancel` 命中回 `not_cancellable`。
//!    建索引最长等到期限 —— 长期限 ＋ 可取消的口今天没有」。〕
//!   ⚠ 被杀的那一趟索引留在 SQLite 自己的事务语义下（小程序没有写到一半的中间态要收拾；
//!   锁是 `flock`，进程一死就放）。
//! - 机器上没有 `timeout(1)` ⇒ 插件口如实裸跑（没有期限），那一条降级由插件口自己的判据钉着。
//! - **字节怎么到那台机器上**不归本模块：〔RM1e〕monitor 听到本模块回 `not_installed` / `unsupported`
//!   ⇒ 经本机常驻后端那条 `files` 链路把内嵌字节推到 `<家>/.cc-monitor/bin/`（[`fixed_candidates`] 的第二个候选）
//!   再问一次（`panorama_call.rs::ask_or_push`）。本模块只认「在不在、是不是它、会不会这个 op」，
//!   不在就说 `not_installed` ＋ 查过哪儿 —— **这两个码是推字节的触发条件**（monitor 那一侧的 `PUSH_ON`
//!   与下面 `discover::find` / `negotiate` 两处映射出的码两向相等，判据读本文件）。

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::plugin::invoke::Done;
use crate::plugin::invoke::NotRun;
use crate::plugin::probe::Rejected;

/// 小程序的名字 = 探测身份行的值 = 盘上那个可执行文件的名字。
pub(crate) const PLUGIN_NAME: &str = "cc-monitor-panorama";

/// 它的探测旗标（插件口 `key=value` 方言）。
const PROBE_FLAG: &str = "--probe";

/// 探测的期限（秒）：只打三行字，给得很宽也只是「卡死时最多等这么久」。
const PROBE_DEADLINE_SECS: u64 = 10;

/// 建索引的期限（秒）。量级来自 `RM1b.md §3.1`：整个本仓（1212 文件）在忙机器上 107 s ⇒ 给 15 分钟。
pub(crate) const BUILD_DEADLINE_SECS: u64 = 900;

/// 其余查询的期限（秒）。一问一答每次都要 `Engine::open`（很轻），overview 不跨进程缓存 ⇒ 大仓每次重算。
pub(crate) const QUERY_DEADLINE_SECS: u64 = 60;

/// 小程序的退出码（它自己的契约，`src/panorama-engine/main.rs` 头注）：调用方给错了东西。
const PLUGIN_EXIT_BAD_ARGS: i32 = 2;
/// 小程序的退出码：形状对、做不成（仓打不开 / 引擎报错）。
const PLUGIN_EXIT_FAILED: i32 = 3;

/// ★ **线上认得的 op 与各自的期限** —— 词表只说查询语义。
///
/// 与小程序 `OPS` 两向相等（`tests::the_op_table_matches_the_program_one`，运行时读它的源码）；
/// 用建索引那一档期限的 op == 小程序里标成独占写的那几个（同一条判据）。
pub(crate) const OPS: &[(&str, u64)] = &[
    ("status", QUERY_DEADLINE_SECS),
    ("index", BUILD_DEADLINE_SECS),
    ("reindex", BUILD_DEADLINE_SECS),
    ("overview", QUERY_DEADLINE_SECS),
    ("node", QUERY_DEADLINE_SECS),
    ("subgraph", QUERY_DEADLINE_SECS),
    ("callers", QUERY_DEADLINE_SECS),
    ("callees", QUERY_DEADLINE_SECS),
    ("impact", QUERY_DEADLINE_SECS),
    ("search", QUERY_DEADLINE_SECS),
    ("docs_for", QUERY_DEADLINE_SECS),
    ("touching", QUERY_DEADLINE_SECS),
    ("symbols_in_file", QUERY_DEADLINE_SECS),
    ("drift", QUERY_DEADLINE_SECS),
    ("list_annotations", QUERY_DEADLINE_SECS),
    ("diagram_kinds", QUERY_DEADLINE_SECS),
    ("diagram", QUERY_DEADLINE_SECS),
    // 〔RM1d〕只算不写：小程序读盘上那一两份、回一份编辑计划；落盘不在这条命令里（头注「写面」）。
    ("plan_add_annotation", QUERY_DEADLINE_SECS),
    ("plan_propose_annotation", QUERY_DEADLINE_SECS),
    ("plan_approve_annotation", QUERY_DEADLINE_SECS),
    ("plan_remove_annotation", QUERY_DEADLINE_SECS),
    ("plan_write_doc_link", QUERY_DEADLINE_SECS),
    ("plan_remove_doc_link", QUERY_DEADLINE_SECS),
    // 〔RM1d〕外面落了 `.md` 之后让索引里的文档关联跟上（写的是索引，建索引那一档）。
    ("refresh_doc_links", BUILD_DEADLINE_SECS),
];

/// 〔RM1f〕小程序每条输出流最多留多少字节（交给插件口 `run_abortable`，多出来的它照读照丢）：
/// 与「结果太大」那一格（[`classify`]）**同一个**上限 `read_face::LINES_CAP_BYTES` ⇒ `len() >` 它就是 `too_large`。
fn keep() -> u64 {
    crate::read_face::LINES_CAP_BYTES as u64
}

/// 找不到时那句话的尾巴（这个插件自己的话）。
///
/// ⚠ 只说「没装」，不许说「重装后端就有了」：重装后端**不带**这份小程序（它只推给开过远端全景的机器）。
/// 〔RM1e〕monitor 听到 `not_installed` 会自己推一次再问（`panorama_call.rs`），推完仍缺才把这句话交到人眼前。
const NOT_INSTALLED_HINT: &str = "这台机器上还没装代码全景组件。";

/// 命令级错误：`(code, message)`。
type CmdErr = (&'static str, String);

/// 家目录：`HOME`，没有再退 `USERPROFILE`（同 `exit_policy::policy_path` 的口径）。
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|h| !h.is_empty()))
        .map(PathBuf::from)
}

/// 索引落哪：那台机器上后端自己的数据目录下 `panorama/`（**不是**被分析的仓）。
pub(crate) fn store_dir(home: &Path) -> PathBuf {
    home.join(super::exit_policy::DIR_NAME).join("panorama")
}

/// 〔RM1f〕盘上那个可执行文件的**文件名**：身份名 ＋ 这台机器的可执行后缀（Windows 上 `.exe`，别处空串）。
///
/// 身份（`--probe` 首行、[`PLUGIN_NAME`]）不带后缀；只有「去盘上哪儿找」这一格要它 ——
/// Windows 本机那一份是 `cc-monitor-panorama.exe`（monitor 按目标平台放下来的，`panorama_bytes::local_file_name`）。
/// 后端就跑在它要找的那台机器上 ⇒ 它自己的后缀就是那台的后缀。
pub(crate) fn program_file_name() -> String {
    format!("{PLUGIN_NAME}{}", std::env::consts::EXE_SUFFIX)
}

/// 固定候选 —— **纯函数**（不读环境，好测）。后端自己旁边优先（随后端一起铺的那一份），
/// 其次是部署落点 `<家>/.cc-monitor/bin/`。
pub(crate) fn fixed_candidates(exe_dir: Option<&Path>, home: Option<&Path>) -> Vec<PathBuf> {
    let file = program_file_name();
    let mut out = Vec::new();
    if let Some(d) = exe_dir {
        out.push(d.join(&file));
    }
    if let Some(h) = home {
        let p = h.join(super::exit_policy::DIR_NAME).join("bin").join(&file);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// 帧面入口。〔RM1f〕异步：注册表里是 `Run::Async`，`cancel` 命中 ⇒ 这个 future 被丢 ⇒ 小程序那一组子进程被杀。
pub(crate) async fn answer(args: &Value) -> Result<Value, (String, String)> {
    let home = home();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let fixed = fixed_candidates(exe_dir.as_deref(), home.as_deref());
    let Some(home) = home else {
        return Err((
            "failed".to_string(),
            "这台机器上解析不出家目录（HOME / USERPROFILE 都没有）—— 不知道把索引放哪".to_string(),
        ));
    };
    answer_with(&fixed, &store_dir(&home), args)
        .await
        .map_err(|(c, m)| (c.to_string(), m))
}

/// [`answer`] 的本体：候选与索引根是参数（判据拿夹具喂它，不去动进程级环境）。
pub(crate) async fn answer_with(
    fixed: &[PathBuf],
    store: &Path,
    args: &Value,
) -> Result<Value, CmdErr> {
    let op = args
        .get("op")
        .and_then(Value::as_str)
        .ok_or(("bad_args", "缺 `op`（要一个字符串）".to_string()))?;
    let Some((_, deadline)) = OPS.iter().find(|(n, _)| *n == op) else {
        return Err((
            "bad_args",
            format!(
                "不认识的全景 op `{op}`（认得的：{}）",
                OPS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(" · ")
            ),
        ));
    };
    let repo = match args.get("repo") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.as_str()),
        Some(_) => return Err(("bad_args", "`repo` 要一个字符串".to_string())),
    };
    let op_args = match args.get("args") {
        None | Some(Value::Null) => None,
        Some(v @ Value::Object(_)) => Some(v.to_string()),
        Some(_) => return Err(("bad_args", "`args` 要一个 JSON 对象".to_string())),
    };
    let bin = crate::plugin::discover::find(PLUGIN_NAME, fixed, false, NOT_INSTALLED_HINT)
        .map_err(|m| ("not_installed", m))?;
    // ② 问它会什么：要的就是这一次的 op。
    // 〔RM1f〕两次起进程都走可打断的那一形（探测也是：它卡住时同样要能被撤掉）。
    let probe =
        crate::plugin::invoke::run_abortable(&bin, &[PROBE_FLAG], PROBE_DEADLINE_SECS, &[], keep())
            .await;
    let text = match probe {
        Ok(d) if d.code == Some(0) => String::from_utf8_lossy(&d.stdout).into_owned(),
        Ok(d) => {
            return Err((
                "failed",
                format!(
                    "`{}` 的能力探测没答上来（{}）：{}",
                    bin.display(),
                    describe_exit(d.code),
                    d.diagnosis()
                ),
            ))
        }
        Err(n) => return Err(not_run(&bin, n)),
    };
    crate::plugin::probe::negotiate(&text, PLUGIN_NAME, &[op]).map_err(|r| match r {
        Rejected::MissingCapability { .. } => ("unsupported", r.message()),
        Rejected::NotThePlugin { .. } => ("not_installed", r.message()),
    })?;
    // ③ 起它。argv 直传不过 shell。
    let store_s = store.display().to_string();
    let mut argv: Vec<&str> = vec![op, "--store", store_s.as_str()];
    if let Some(r) = repo {
        argv.extend(["--repo", r]);
    }
    if let Some(a) = op_args.as_deref() {
        argv.extend(["--args", a]);
    }
    let done = crate::plugin::invoke::run_abortable(&bin, &argv, *deadline, &[], keep()).await;
    classify(op, *deadline, done.map_err(|n| not_run(&bin, n))?)
}

/// 「根本没跑起来」那一类。
fn not_run(bin: &Path, n: NotRun) -> CmdErr {
    match n {
        NotRun::ArgListTooLong => (
            "too_large",
            "这次要查的文件太多，一次传不过去，少选几个再试".to_string(),
        ),
        NotRun::Failed(m) => {
            tracing::warn!("起不来 {}：{m}", bin.display());
            ("failed", format!("代码全景组件没能启动：{m}"))
        }
    }
}

fn describe_exit(code: Option<i32>) -> String {
    match code {
        Some(c) => format!("异常退出，码 {c}"),
        None => "被中途终止".to_string(),
    }
}

/// ④ **码 → 语义**（这个插件自己的表，只住这里）。
///
/// | 码 | 语义 |
/// |---|---|
/// | 0 | 应答那一行的 `data` 原样装进 `result` |
/// | 小程序的「参数不对」 | `bad_args`（它自己那句话原样带回） |
/// | 小程序的「做不成」 | `failed`（同上） |
/// | 期限命令超时（`Done::timed_out`，那个码是通用层的事实） | `timed_out`（说清是哪一档期限） |
/// | 被信号打断 / 其余 | `failed` ＋ 摘一行诊断 |
pub(crate) fn classify(op: &str, deadline: u64, done: Done) -> Result<Value, CmdErr> {
    if done.stdout.len() > crate::read_face::LINES_CAP_BYTES {
        return Err((
            "too_large",
            format!(
                "全景 `{op}` 的结果超过 {} 字节上限，没有返回（{} 字节）",
                crate::read_face::LINES_CAP_BYTES,
                done.stdout.len()
            ),
        ));
    }
    if done.timed_out() {
        return Err((
            "timed_out",
            format!("全景 `{op}` 超过 {deadline} 秒没做完，已经停掉（这一档的期限）"),
        ));
    }
    let said = crate::plugin::invoke::first_line(&done.stdout);
    let reply: Option<Value> = serde_json::from_str(&said).ok();
    let message = || {
        reply
            .as_ref()
            .and_then(|v| v.get("message"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| done.diagnosis())
    };
    match done.code {
        Some(0) => {
            let Some(v) = reply.as_ref().filter(|v| v.get("ok") == Some(&json!(true))) else {
                return Err((
                    "failed",
                    format!("全景 `{op}` 退出码 0，但应答不是 `{{\"ok\":true,…}}` 那一行：{said}"),
                ));
            };
            Ok(json!({ "result": v.get("data").cloned().unwrap_or(Value::Null) }))
        }
        Some(PLUGIN_EXIT_BAD_ARGS) => Err(("bad_args", message())),
        Some(PLUGIN_EXIT_FAILED) => Err(("failed", message())),
        other => Err((
            "failed",
            format!(
                "全景 `{op}` 没做成（{}）：{}",
                describe_exit(other),
                message()
            ),
        )),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/panorama_tests.rs"]
mod tests;
