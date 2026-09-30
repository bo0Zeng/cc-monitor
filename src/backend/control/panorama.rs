//! 〔RM1c · 第四波〕**代码全景**的帧命令 `panorama` —— 后端经插件通用调用口起那个只装引擎的
//! 独立小程序（用户 09-24 V108 选 B），自己一行引擎代码都不链。
//!
//! # 为什么是「起一个进程」，不是「链进来」
//!
//! 引擎 ≈19.6 MB / 架构（`调研/第四波记录/RM1b.md §3`）。编进后端本体 = 每一台远端都背它，
//! 且与 `C21`「code-picture 走独立二进制……**也不编进 backend**」、`95 §0`「不并进后端本体」正面相撞；
//! 那两条由 `plugin_class_registry` ②③ 与 `panorama_locus_guard` 正题①（链接面）· monitor 侧 `panorama_seam_registry::engine_port_scope`（取用面，〔TL1〕原 `panorama_locus_guard` 正题② 并进去了）钉着，本模块一条都不碰。
//! ⇒ 解析发生在**被起的那个进程**里；本模块只做插件口的四段里**属于这个插件**的那几样：
//!
//! | 段 | 本模块给的 |
//! |---|---|
//! | ① 找它 | 候选：后端自己那个可执行文件旁边 → `<家>/.cc-monitor/bin/`；**不兜 `PATH`**（同名的无关程序由身份行再挡一道） |
//! | ② 问它会什么 | 要的能力 = 这一次的 op（缺哪个说哪个 —— 「这台装的小程序太旧」与「调用失败」是两件事）；要的那一代 = 请求带来的 `shape` |
//! | ③ 起它 | 期限按它自报的档：长活档（`long=`）一档、其余一档（`u64` 秒，交给子进程的 `timeout` 前缀；**后端零定时器不破**）；〔P7〕它在 stderr 上写的进度行（插件口分拣）交发起方订的进度流 |
//! | ④ 码 → 语义 | **这张表只住这里**（插件口只抽骨架）：见 [`classify`] |
//!
//! # 后端不带引擎知识（〔PANO〕`99 §1` V158）
//!
//! 本模块**不存** op 表、不存形状代号：会哪些 op、哪个是长活，由小程序 `--probe` 自报（`capabilities=` · `long=`）；
//! 要的是哪一代，由发起方在请求里带上 `shape`（前端取自与小程序同源的生成物 `src/frontend/ui/panorama/engine-contract.json`）。
//! 未知 op / 形状对不上 ⇒ `unsupported`（放字节那条照旧接上）。本模块自己只留「档 → 秒数」那一张（[`deadline_for`]）。
//! 写（`panorama-edit`）也一样：哪几个 op 是「算」、写成之后还要跑哪一个，照小程序自报的 `plans=`（[`answer_plan`]）。
//! 〔墓碑 —— 此前这里写死 `SHAPE` 与 24 行 `OPS`（含期限档），re-vendor 一次后端就要改常量、每台重部署。〕
//!
//! # 只说查询语义（`protocol_doc_guard` 那条 `P7c-2` 约束）
//!
//! 线上只有一条命令名 `panorama`，`op` 只许小程序自报的词（== 签字白名单，判据读生成物）；
//! 存储、grammar、解析开关**一个都不上线**。
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
//!   再问一次（〔MIG-3b 续〕界面 `src/frontend/ui/panorama/api.ts::askOrPlace`）。本模块只认「在不在、是不是它、会不会这个 op」，
//!   不在就说 `not_installed` ＋ 查过哪儿 —— **这两个码是推字节的触发条件**（界面那一侧的 `PUSH_ON`（`src/frontend/ui/panorama/api.ts`）
//!   与下面 `discover::find` / `negotiate` 两处映射出的码两向相等，判据读本文件）。

use copy_core::copy_text;
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

/// 长活档（小程序自报 `long=` 的那几个：建索引一族）的期限（秒）。量级来自 `RM1b.md §3.1`：整个本仓（1212 文件）在忙机器上 107 s ⇒ 给 15 分钟。
pub(crate) const BUILD_DEADLINE_SECS: u64 = 900;

/// 其余（短活档）的期限（秒）。一问一答每次都要 `Engine::open`（很轻），overview 不跨进程缓存 ⇒ 大仓每次重算。
pub(crate) const QUERY_DEADLINE_SECS: u64 = 60;

/// **档 → 秒数**（只住这里）：按小程序自报的档（`long` = 它把这个 op 报在 `long=` 里）给这一次的期限。
pub(crate) fn deadline_for(long: bool) -> u64 {
    if long {
        BUILD_DEADLINE_SECS
    } else {
        QUERY_DEADLINE_SECS
    }
}

/// 小程序的退出码（它自己的契约，`src/panorama-engine/main.rs` 头注）：调用方给错了东西。
const PLUGIN_EXIT_BAD_ARGS: i32 = 2;
/// 小程序的退出码：形状对、做不成（仓打不开 / 引擎报错）。
const PLUGIN_EXIT_FAILED: i32 = 3;

/// 〔RM1f〕小程序每条输出流最多留多少字节（交给插件口 `run_abortable`，多出来的它照读照丢）：
/// 与「结果太大」那一格（[`classify`]）**同一个**上限 `read_face::LINES_CAP_BYTES` ⇒ `len() >` 它就是 `too_large`。
fn keep() -> u64 {
    crate::faces::read_face::LINES_CAP_BYTES as u64
}

/// 找不到时那句话的尾巴（这个插件自己的话）。
///
/// ⚠ 只说「没装」，不许说「重装后端就有了」：重装后端**不带**这份小程序（它只推给开过远端全景的机器）。
/// 〔RM1e〕界面听到 `not_installed` 会请 monitor 放一次字节再问（〔MIG-3b 续〕界面 `src/frontend/ui/panorama/api.ts::askOrPlace`），推完仍缺才把这句话交到人眼前。
static NOT_INSTALLED_HINT: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("bePanorama.notInstalledHint.say", &[]));

/// 命令级错误：`(code, message)`。
type CmdErr = (&'static str, String);

/// 家目录（`platform::paths::home_dir`）。
fn home() -> Option<PathBuf> {
    crate::platform::paths::home_dir()
}

/// 索引落哪：那台机器上后端自己的数据目录下 `panorama/`（**不是**被分析的仓）。
/// 〔P3〕住址只住契约 crate（`relay_route_core::PANORAMA_INDEX_REL`：monitor 的数据位置页按同一个常量列出它）。
pub(crate) fn store_dir(home: &Path) -> PathBuf {
    home.join(relay_route_core::PANORAMA_INDEX_REL)
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

/// 帧面入口。〔RM1f〕异步：`cancel` 命中 ⇒ 这个 future 被丢 ⇒ 小程序那一组子进程被杀。
/// 〔P7〕`progress`：小程序报的每一格进度（一个 JSON 对象，原样；格里是什么由界面解释）往哪儿交 ——
/// 帧面上是「推进本请求那张票的进度流」（`stream::inbound::Progress`），CLI 一次性进程里没人订、是空的。
pub(crate) async fn answer(
    args: &Value,
    progress: &(dyn Fn(Value) + Send + Sync),
) -> Result<Value, (String, String)> {
    let (fixed, store) = where_to_look()?;
    answer_with(&fixed, &store, args, progress)
        .await
        .map_err(|(c, m)| (c.to_string(), m))
}

/// 〔PANO〕「算」那一问（`panorama-edit` 用）：同 [`answer`]，另要求 `op` 在小程序自报的写表（`plans=`）里，
/// 应答多一格 `then`（写成之后要跑的 op，没有 = `null`）。不在表里 ⇒ `bad_args`、不起那个 op。
pub(crate) async fn answer_plan(args: &Value) -> Result<Value, (String, String)> {
    let (fixed, store) = where_to_look()?;
    answer_with_plan(&fixed, &store, args)
        .await
        .map_err(|(c, m)| (c.to_string(), m))
}

/// 去哪儿找小程序（固定候选）· 索引落哪（这台后端自己的数据目录）。
fn where_to_look() -> Result<(Vec<PathBuf>, PathBuf), (String, String)> {
    let home = home();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let fixed = fixed_candidates(exe_dir.as_deref(), home.as_deref());
    let Some(home) = home else {
        return Err((
            "failed".to_string(),
            copy_text("bePanorama.answer.noHome", &[]),
        ));
    };
    Ok((fixed, store_dir(&home)))
}

/// [`answer`] 的本体：候选与索引根是参数（判据拿夹具喂它，不去动进程级环境）；小程序报的进度格交 `progress`。
pub(crate) async fn answer_with(
    fixed: &[PathBuf],
    store: &Path,
    args: &Value,
    progress: &(dyn Fn(Value) + Send + Sync),
) -> Result<Value, CmdErr> {
    run_op(fixed, store, args, false, progress).await
}

/// [`answer_plan`] 的本体（同 [`answer_with`]）。
pub(crate) async fn answer_with_plan(
    fixed: &[PathBuf],
    store: &Path,
    args: &Value,
) -> Result<Value, CmdErr> {
    run_op(fixed, store, args, true, &|_| {}).await
}

/// 写表（`plans=` 的值，`<算 op>[><之后>]` 逗号列表）里 `op` 那一项：`Some(之后要跑的)`；不在表里 ⇒ `None`。
pub(crate) fn plan_of(plans: Option<&str>, op: &str) -> Option<Option<String>> {
    plans?.split(',').map(str::trim).find_map(|item| {
        let (p, then) = item
            .split_once('>')
            .map_or((item, None), |(p, t)| (p, Some(t)));
        (p == op).then(|| then.map(str::to_string))
    })
}

async fn run_op(
    fixed: &[PathBuf],
    store: &Path,
    args: &Value,
    plan: bool,
    progress: &(dyn Fn(Value) + Send + Sync),
) -> Result<Value, CmdErr> {
    let op = args.get("op").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `op` (a string)"),
    ))?;
    // 要的那一代由发起方带来（后端不存形状代号，头注「后端不带引擎知识」）。
    let shape = args.get("shape").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `shape` (a string)"),
    ))?;
    let repo = match args.get("repo") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.as_str()),
        Some(_) => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("`repo` must be a string"),
            ))
        }
    };
    let op_args = match args.get("args") {
        None | Some(Value::Null) => None,
        Some(v @ Value::Object(_)) => Some(v.to_string()),
        Some(_) => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("`args` must be a JSON object"),
            ))
        }
    };
    let bin = crate::plugin::discover::find(PLUGIN_NAME, fixed, false, &NOT_INSTALLED_HINT)
        .map_err(|m| ("not_installed", m))?;
    // ② 问它会什么：要的就是这一次的 op（未知的 op 也落在这里：它没自报 ⇒ `unsupported`）。
    // 〔RM1f〕两次起进程都走可打断的那一形（探测也是：它卡住时同样要能被撤掉）。
    let probe =
        crate::plugin::invoke::run_abortable(&bin, &[PROBE_FLAG], PROBE_DEADLINE_SECS, &[], keep())
            .await;
    let text = match probe {
        Ok(d) if d.code == Some(0) => String::from_utf8_lossy(&d.stdout).into_owned(),
        Ok(d) => {
            return Err((
                "failed",
                copy_text(
                    "bePanorama.answerWith.probeFailed",
                    &[
                        ("bin", &(bin.display()).to_string()),
                        ("how", &(describe_exit(d.code)).to_string()),
                        ("detail", &(d.diagnosis()).to_string()),
                    ],
                ),
            ))
        }
        Err(n) => return Err(not_run(&bin, n)),
    };
    let answer = crate::plugin::probe::negotiate(&text, PLUGIN_NAME, &[op], Some(shape)).map_err(
        |r| match r {
            Rejected::MissingCapability { .. } | Rejected::StaleShape { .. } => {
                ("unsupported", r.message())
            }
            Rejected::NotThePlugin { .. } => ("not_installed", r.message()),
        },
    )?;
    let then = if plan {
        let Some(then) = plan_of(answer.extra("plans"), op) else {
            return Err((
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "`{op}` is not a planning op this program reports (`plans=`)"
                )),
            ));
        };
        Some(then)
    } else {
        None
    };
    let deadline = deadline_for(answer.is_long(op));
    // ③ 起它。argv 直传不过 shell。
    let store_s = store.display().to_string();
    let mut argv: Vec<&str> = vec![op, "--store", store_s.as_str()];
    if let Some(r) = repo {
        argv.extend(["--repo", r]);
    }
    if let Some(a) = op_args.as_deref() {
        argv.extend(["--args", a]);
    }
    // 〔P7〕插件口分拣出来的进度行：只认一个 JSON 对象（一格），别的形不转（格是什么由界面严格收）。
    let mut on_line = |line: &str| {
        if let Ok(cell @ Value::Object(_)) = serde_json::from_str::<Value>(line) {
            progress(cell);
        }
    };
    let done = crate::plugin::invoke::run_abortable_reporting(
        &bin,
        &argv,
        deadline,
        &[],
        keep(),
        &mut on_line,
    )
    .await;
    let mut got = classify(op, deadline, done.map_err(|n| not_run(&bin, n))?)?;
    if let Some(then) = then {
        got["then"] = json!(then);
    }
    Ok(got)
}

/// 〔FIX4 · `97 §8` · 主会话 09-28 裁「受管工具都应可卸，照 SU1 装卸账」〕**卸掉这台上的全景小程序**（帧 `panorama-uninstall`）。
///
/// 装的那一下只写一份文件（落点 `~/.cc-monitor/bin/<`[`program_file_name`]`>`，`panorama_bytes` 放的）⇒ 卸只删那一份：
/// 先问它是不是全景小程序（`--probe` 首行认身份，认不出 / 跑不起来 ⇒ `not_ours`、一个字节不动），再经这台文件管理面带 CAS 删
/// （盘上逐字节 == 刚读到的那一份才删）。它跑出来的索引（[`store_dir`]）不是装时写的，不删，说出在哪。不在 ⇒ `removed: false`。
pub(crate) async fn answer_uninstall(
    door: impl crate::assets::door::Door + Send + Sync + 'static,
) -> Result<Value, (String, String)> {
    let Some(home) = home() else {
        return Err((
            "failed".to_string(),
            copy_text("bePanorama.answer.noHome", &[]),
        ));
    };
    uninstall_at(door, home).await
}

/// [`answer_uninstall`] 的本体：家目录是参数（判据拿临时目录喂它，不去动进程级环境）。
pub(crate) async fn uninstall_at(
    door: impl crate::assets::door::Door + Send + Sync + 'static,
    home: PathBuf,
) -> Result<Value, (String, String)> {
    let rel = format!(
        "{}/bin/{}",
        super::exit_policy::DIR_NAME,
        program_file_name()
    );
    let path = home.join(&rel);
    let index = store_dir(&home).display().to_string();
    // 读与删都是同步文件 I/O ⇒ 挪到阻塞线程池（同 `panorama_edit` 落盘那一步）。
    let at = path.clone();
    let read = tokio::task::spawn_blocking(move || std::fs::read(&at))
        .await
        .map_err(|e| ("failed".to_string(), e.to_string()))?;
    let bytes = match read {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(
                json!({ "removed": false, "path": path.display().to_string(), "index": index }),
            )
        }
        Err(e) => {
            return Err((
                "failed".to_string(),
                copy_text(
                    "bePanorama.uninstall.unreadable",
                    &[("path", &path.display().to_string()), ("e", &e.to_string())],
                ),
            ))
        }
    };
    let not_ours = |why: String| {
        (
            "not_ours".to_string(),
            copy_text(
                "bePanorama.uninstall.notOurs",
                &[("path", &path.display().to_string()), ("why", &why)],
            ),
        )
    };
    let probe = crate::plugin::invoke::run_abortable(
        &path,
        &[PROBE_FLAG],
        PROBE_DEADLINE_SECS,
        &[],
        keep(),
    )
    .await;
    let text = match probe {
        Ok(d) if d.code == Some(0) => String::from_utf8_lossy(&d.stdout).into_owned(),
        Ok(d) => return Err(not_ours(describe_exit(d.code))),
        Err(n) => return Err(not_ours(not_run(&path, n).1)),
    };
    crate::plugin::probe::negotiate(&text, PLUGIN_NAME, &[], None)
        .map_err(|r| not_ours(r.message()))?;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let root = home.display().to_string();
    tokio::task::spawn_blocking(move || {
        door.ask(
            "files-delete",
            json!({ "root": root, "rel": rel, "expect": { "b16": hex } }),
        )
    })
    .await
    .map_err(|e| ("failed".to_string(), e.to_string()))??;
    Ok(json!({ "removed": true, "path": path.display().to_string(), "index": index }))
}

/// 「根本没跑起来」那一类。
fn not_run(bin: &Path, n: NotRun) -> CmdErr {
    match n {
        NotRun::ArgListTooLong => (
            "too_large",
            copy_text("bePanorama.notRun.tooManyFiles", &[]),
        ),
        NotRun::Failed(m) => {
            tracing::warn!("起不来 {}：{m}", bin.display());
            (
                "failed",
                copy_text("bePanorama.notRun.spawnFailed", &[("m", &m.to_string())]),
            )
        }
    }
}

fn describe_exit(code: Option<i32>) -> String {
    match code {
        Some(c) => copy_text("bePanorama.describeExit.code", &[("c", &c.to_string())]),
        None => copy_text("bePanorama.describeExit.signal", &[]),
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
    if done.stdout.len() > crate::faces::read_face::LINES_CAP_BYTES {
        return Err((
            "too_large",
            copy_text(
                "bePanorama.classify.tooLarge",
                &[
                    ("op", &op.to_string()),
                    ("size", &(done.stdout.len()).to_string()),
                    (
                        "cap",
                        &(crate::faces::read_face::LINES_CAP_BYTES).to_string(),
                    ),
                ],
            ),
        ));
    }
    if done.timed_out() {
        return Err((
            "timed_out",
            copy_text(
                "bePanorama.classify.timedOut",
                &[("op", &op.to_string()), ("deadline", &deadline.to_string())],
            ),
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
                    copy_text(
                        "bePanorama.classify.badReply",
                        &[("op", op), ("said", &said)],
                    ),
                ));
            };
            Ok(json!({ "result": v.get("data").cloned().unwrap_or(Value::Null) }))
        }
        Some(PLUGIN_EXIT_BAD_ARGS) => Err(("bad_args", message())),
        Some(PLUGIN_EXIT_FAILED) => Err(("failed", message())),
        other => Err((
            "failed",
            copy_text(
                "bePanorama.classify.failed",
                &[
                    ("op", &op.to_string()),
                    ("how", &(describe_exit(other)).to_string()),
                    ("message", &(message()).to_string()),
                ],
            ),
        )),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/panorama_tests.rs"]
mod tests;
