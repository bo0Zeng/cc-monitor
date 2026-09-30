//! 〔RM1c · 第四波〕`cc-monitor-panorama` —— 只装代码全景引擎的**独立小程序**（用户 09-24 V108 选 B）。
//!
//! # 它是什么、谁起它
//!
//! vendored `code-picture-core` ＋ 一问一答的 JSON CLI。起它的是**那台机器上的后端**，经插件
//! 通用调用口（找它 → 问它会什么 → 传 argv 起它，期限走 `timeout` 前缀）按需起；
//! 后端本体仍零 code-picture（`C21` / `95 §0` 照旧）。索引落后端交给它的那个目录
//! （那台机器上后端自己的数据目录），**不落进被分析的仓**。
//!
//! # 线上契约（1 exec = 1 请求 = 1 行应答）
//!
//! - `cc-monitor-panorama --probe` ⇒ 插件口那套 `key=value` 方言：首行 `name=`（身份）·
//!   `version=`（只进诊断）· `capabilities=`（逗号列表 = [`OPS`] 的名字，集合语义）·
//!   `long=`（长活档的那几个，后端按档给期限）·
//!   `shape=`（形状代号 [`shape_code`]；期望值由发起方从生成物 `src/frontend/ui/panorama/engine-contract.json` 带来，后端只比对）。
//! - `cc-monitor-panorama <op> [--repo <仓>] [--store <索引根>] [--args <JSON>]` ⇒ stdout **恰一行**：
//!   成功 `{"ok":true,"data":…}`（退出码 0）；失败 `{"ok":false,"code":…,"message":…}`，
//!   退出码 [`EXIT_BAD_ARGS`]（调用方给错了东西）/ [`EXIT_FAILED`]（仓打不开 / 引擎报错）。
//!   失败那句话同时写 stderr 一行（调用口摘诊断时 stderr 优先）。
//! - ⚠ **退出码的语义只在这里定义一次**；起它的那一侧（后端适配层）自己持一张码 → 语义码的表，
//!   插件口本身不翻码（那条纪律住 `src/backend/plugin/mod.rs` 头注）。
//!
//! # 引擎只算、文件管理来写（〔RM1d〕用户 09-24 V110）
//!
//! 批注增 / 提 / 批 / 删与文档关联写 / 删，写的是**被分析仓里的文件**（`<仓>/.codepicture/annotations/`、
//! 文档的 frontmatter）—— 用户文件。用户 09-24 裁「只允许后端的文件管理部分写用户文件」⇒
//! 本程序对这六样**只算不写**：`plan_*` 那几个 op 读盘上现状、调上游 `edits::plan_*`，把算好的
//! `{value, edit: {rel, before, after, parents}}` 原样交回；落盘由起它的那一侧交给那台机器后端的
//! 文件管理（`files-put` 带 `expect = before` / `files-delete`）。
//! 判据：`tests::the_program_never_calls_an_engine_method_that_writes_user_files`（上游写盘那一层
//! 零命中 ＋ 正控）· `tests::planning_ops_leave_the_repo_byte_identical`。
//! 本程序唯一的写是**索引**：落 `--store` 给的目录（后端自己的数据目录），不是用户文件。
//!
//! # 应答形状与 monitor 进程内那 23 条命令**逐字同形**
//!
//! 引擎直出的类型原样透出（snake_case，同 monitor `panorama.rs`）；`status` 那三格是本仓自己的
//! DTO，沿用 monitor `PanoramaStatus` 的 camelCase（`stale` / `indexedAt` / `symbols`）；
//! `diagram` = `{diagram, mermaid}`（同 monitor `PanoramaDiagram`）。
//! ⇒ 前端「按形状渲染」那一层本机与远端同一份，不为远端另写一套。

use code_picture_core::diagram::{self, DiagramKind, DiagramRequest};
use code_picture_core::{edits, model, Engine, EngineOpts};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

/// 身份行的值（`--probe` 首行 `name=` 后面那个）。起它的那一侧逐字比对。
pub const NAME: &str = "cc-monitor-panorama";

/// 能力探测旗标（插件口那套方言）。
pub const PROBE_FLAG: &str = "--probe";

/// 调用方给错了东西：未知 op / 缺仓 / 参数 JSON 不合形 / 多了不认识的旗标。
pub const EXIT_BAD_ARGS: i32 = 2;
/// 给的东西形状对，但做不成：仓打不开 / 引擎报错 / 索引根建不出来。
pub const EXIT_FAILED: i32 = 3;

/// 一个 op 要不要一个仓、要哪一档锁。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// 不碰仓、不碰索引（今天只有图种注册表 —— 它编在二进制里）。
    Nothing,
    /// 〔RM1d〕只读仓里那一两份文件、算出新内容：要仓，**不要索引根、不上锁、不开引擎**。
    Repo,
    /// 读：要仓与索引根，**共享**锁（多个读可以并行）。
    Read,
    /// 建索引：要仓与索引根，**独占**锁（同一个索引根上同一时刻只许一个在写）。
    Build,
}

impl Need {
    /// 长活档（`--probe` 的 `long=`）：建索引那一族。其余是短活档。
    pub fn is_long(self) -> bool {
        self == Need::Build
    }
}

/// ★ **op 表 —— 本程序会什么的唯一住址**。`--probe` 报的能力由它派生；
/// 分派臂与它两向相等（判据从源码抽臂，异源）；〔PANO〕后端不再存 op 表（按 `--probe` 自报的能力与档办），
/// 前端与各判据读的是它的生成物 `src/frontend/ui/panorama/engine-contract.json`（判据 `the_frontend_contract_is_generated_from_this_program`）。
///
/// ⚠ 词表只说**查询语义**（`protocol_doc_guard` 那条 `P7c-2` 约束：不暴露存储、grammar、解析开关）。
pub const OPS: &[(&str, Need)] = &[
    ("status", Need::Read),
    ("index", Need::Build),
    ("reindex", Need::Build),
    ("overview", Need::Read),
    ("node", Need::Read),
    // 〔PANO · CP1〕以某符号为心的邻域，每个符号带「距根几跳」（〔P7〕上游 `Engine::neighborhood` 直出；前端只分组，不算）。
    ("neighborhood", Need::Read),
    ("callers", Need::Read),
    ("callees", Need::Read),
    ("impact", Need::Read),
    ("search", Need::Read),
    ("docs_for", Need::Read),
    ("touching", Need::Read),
    ("symbols_in_file", Need::Read),
    ("drift", Need::Read),
    ("list_annotations", Need::Read),
    ("diagram_kinds", Need::Nothing),
    ("diagram", Need::Read),
    // 〔RM1d〕只算不写：回一份编辑计划，落盘归那台机器的后端文件管理（头注）。
    ("plan_add_annotation", Need::Repo),
    ("plan_propose_annotation", Need::Repo),
    ("plan_approve_annotation", Need::Repo),
    ("plan_remove_annotation", Need::Repo),
    ("plan_write_doc_link", Need::Repo),
    ("plan_remove_doc_link", Need::Repo),
    // 〔RM1d〕外面落了 `.md` 的 `covers:` 之后让索引跟上（只写索引）。
    ("refresh_doc_links", Need::Build),
];

/// ★〔PANO · V158「后端不带引擎知识」〕**写那几种 → 落盘之后还要跑哪一个 op** —— 唯一住址（原住后端 `panorama_edit.rs` 那张表）。
///
/// 键 = `Need::Repo` 那几个「算」op（判据两向）；值 = 写成之后要跑的 op（文档关联那两种要让索引跟上）。
/// `--probe` 以 `plans=` 自报（`<算 op>[><之后>]` 逗号列表），那台后端的 `panorama-edit` 只照它走；前端从生成物取、按档给期限。
pub const PLANS: &[(&str, Option<&str>)] = &[
    ("plan_add_annotation", None),
    ("plan_propose_annotation", None),
    ("plan_approve_annotation", None),
    ("plan_remove_annotation", None),
    ("plan_write_doc_link", Some("refresh_doc_links")),
    ("plan_remove_doc_link", Some("refresh_doc_links")),
];

/// `plans=` 那一行的值（也进形状代号）。
fn plans_line() -> String {
    PLANS
        .iter()
        .map(|(p, then)| then.map_or(p.to_string(), |t| format!("{p}>{t}")))
        .collect::<Vec<_>>()
        .join(",")
}

/// 〔PANO〕本程序**自己的**应答形状（引擎直出之外的那几样）：`status` 那三格。
#[derive(Serialize, Default)]
struct StatusReply {
    stale: bool,
    #[serde(rename = "indexedAt")]
    indexed_at: Option<u64>,
    symbols: usize,
}

/// `diagram` 的应答：上游 `Diagram`（形状归 vendored pin）＋ 上游 Mermaid 文本。
#[derive(Serialize, Default)]
struct DiagramReply {
    diagram: Value,
    mermaid: String,
}

/// ★ 自己的应答形状 → 一份样本（键结构进形状代号：只改 DTO 不改 op 名也换代）。
/// **本文件每个 `Serialize` 结构体都要在这里出现**（判据从源码抽；分派里不许手搓 `json!({…})` 对象）。
fn own_dtos() -> Vec<(&'static str, Value)> {
    let sample = |v: Result<Value, serde_json::Error>| v.expect("样本序列化不出来");
    vec![
        (
            "status",
            sample(serde_json::to_value(StatusReply::default())),
        ),
        (
            "diagram",
            sample(serde_json::to_value(DiagramReply::default())),
        ),
    ]
}

/// 一份样本的键结构（对象按键排序、数组取第一个元素、标量为空）。
fn key_shape(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut ks: Vec<String> = m
                .iter()
                .map(|(k, x)| format!("{k}:{}", key_shape(x)))
                .collect();
            ks.sort();
            format!("{{{}}}", ks.join(","))
        }
        Value::Array(a) => format!("[{}]", a.first().map(key_shape).unwrap_or_default()),
        _ => String::new(),
    }
}

/// 失败的两类（与两个退出码一一对应）。
#[derive(Debug, PartialEq, Eq)]
pub enum Fail {
    BadArgs(String),
    Failed(String),
}

impl Fail {
    fn code(&self) -> &'static str {
        match self {
            Fail::BadArgs(_) => "bad_args",
            Fail::Failed(_) => "failed",
        }
    }
    fn exit(&self) -> i32 {
        match self {
            Fail::BadArgs(_) => EXIT_BAD_ARGS,
            Fail::Failed(_) => EXIT_FAILED,
        }
    }
    fn message(&self) -> &str {
        match self {
            Fail::BadArgs(m) | Fail::Failed(m) => m,
        }
    }
}

/// 解析完的一次调用。
#[derive(Debug, PartialEq)]
pub enum Parsed {
    Probe,
    Call {
        op: String,
        repo: Option<PathBuf>,
        store: Option<PathBuf>,
        args: Value,
    },
}

/// argv（不含程序名）→ 一次调用。**不认识的旗标就拒**，不静默忽略。
pub fn parse_argv(argv: &[String]) -> Result<Parsed, Fail> {
    let Some(first) = argv.first() else {
        return Err(Fail::BadArgs(format!(
            "缺 op：用法 `{NAME} <op> [--repo <仓>] [--store <索引根>] [--args <JSON>]` 或 `{NAME} {PROBE_FLAG}`"
        )));
    };
    if first == PROBE_FLAG {
        if argv.len() > 1 {
            return Err(Fail::BadArgs(format!("`{PROBE_FLAG}` 不带参数")));
        }
        return Ok(Parsed::Probe);
    }
    let op = first.clone();
    let (mut repo, mut store, mut args) = (None, None, None);
    let mut it = argv[1..].iter();
    while let Some(flag) = it.next() {
        let Some(val) = it.next() else {
            return Err(Fail::BadArgs(format!("`{flag}` 后面缺值")));
        };
        let slot = match flag.as_str() {
            "--repo" => &mut repo,
            "--store" => &mut store,
            "--args" => &mut args,
            other => return Err(Fail::BadArgs(format!("不认识的旗标 `{other}`"))),
        };
        if slot.is_some() {
            return Err(Fail::BadArgs(format!("`{flag}` 给了两次")));
        }
        *slot = Some(val.clone());
    }
    let args = match args {
        None => json!({}),
        Some(s) => serde_json::from_str::<Value>(&s)
            .map_err(|e| Fail::BadArgs(format!("`--args` 不是 JSON：{e}")))?,
    };
    Ok(Parsed::Call {
        op,
        repo: repo.map(PathBuf::from),
        store: store.map(PathBuf::from),
        args,
    })
}

/// vendored 副本的 pin（`VENDOR.md` 里 `vendored commit:` 后那对反引号里的值）。
fn vendor_pin() -> &'static str {
    const MD: &str = include_str!("vendor/code-picture-core/VENDOR.md");
    let at = MD
        .find("vendored commit:`")
        .expect("VENDOR.md 没有 vendored commit 那一行");
    let rest = &MD[at + "vendored commit:`".len()..];
    &rest[..rest.find('`').expect("pin 没收尾")]
}

/// 〔FIX2 · `设计/97 §8` · `99 §2.1 ㉝①`〕**形状代号**：op 表（名 ＋ 档）＋ 写表（[`PLANS`]）＋ 自己的应答形状（[`own_dtos`]）
/// ＋ vendored pin 的摘要（FNV-1a 64）。能力表相同、某个 op 的应答形状变了（re-vendor，或〔PANO〕本程序自己的 DTO 改了）时它会变
/// ⇒ 后端按它判旧、回 `unsupported`、monitor 重放字节。
pub fn shape_code() -> String {
    let ops: Vec<String> = OPS
        .iter()
        .map(|(n, need)| format!("{n}:{need:?}"))
        .collect();
    let dtos: Vec<String> = own_dtos()
        .iter()
        .map(|(n, v)| format!("{n}={}", key_shape(v)))
        .collect();
    let canon = format!(
        "ops={};plans={};dtos={};vendor={}",
        ops.join(","),
        plans_line(),
        dtos.join(","),
        vendor_pin()
    );
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in canon.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// `--probe` 的全文（插件口方言，首行是身份）。
///
/// 〔PANO · V158「后端不带引擎知识」〕`long=` 自报长活档（建索引那几个，由 [`OPS`] 的 `Need::Build` 派生）；
/// 不在里面的是短活档。起它的后端按档给期限，自己不存 op 表。
pub fn probe_text() -> String {
    let caps: Vec<&str> = OPS.iter().map(|(n, _)| *n).collect();
    let long: Vec<&str> = OPS
        .iter()
        .filter(|(_, need)| need.is_long())
        .map(|(n, _)| *n)
        .collect();
    format!(
        "name={NAME}\nversion={}\ncapabilities={}\nlong={}\nplans={}\nshape={}\n",
        env!("CARGO_PKG_VERSION"),
        caps.join(","),
        long.join(","),
        plans_line(),
        shape_code()
    )
}

/// 按 op 表跑一次：找 op → 备仓与索引根 → 上锁 → 开引擎 → 分派。
pub fn run(
    op: &str,
    repo: Option<&Path>,
    store: Option<&Path>,
    args: Value,
) -> Result<Value, Fail> {
    let Some((_, need)) = OPS.iter().find(|(n, _)| *n == op) else {
        return Err(Fail::BadArgs(format!(
            "不认识的 op `{op}`（本程序会的：{}）",
            OPS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(" · ")
        )));
    };
    if *need == Need::Nothing {
        return dispatch_registry(op, args);
    }
    let repo = repo.ok_or_else(|| Fail::BadArgs(format!("op `{op}` 要一个仓（`--repo`）")))?;
    if !repo.is_absolute() {
        return Err(Fail::BadArgs(format!(
            "仓路径必须是绝对路径：`{}`",
            repo.display()
        )));
    }
    let canon = std::fs::canonicalize(repo)
        .map_err(|e| Fail::Failed(format!("仓路径无效（{}）：{e}", repo.display())))?;
    if !canon.is_dir() {
        return Err(Fail::Failed(format!(
            "仓路径不是目录：`{}`",
            repo.display()
        )));
    }
    if *need == Need::Repo {
        // 只算不写：不碰索引根（给了也不用）、不上锁、不开引擎。
        return dispatch_plan(op, &canon, args);
    }
    let store =
        store.ok_or_else(|| Fail::BadArgs(format!("op `{op}` 要一个索引根（`--store`）")))?;
    std::fs::create_dir_all(store)
        .map_err(|e| Fail::Failed(format!("建不出索引根 `{}`：{e}", store.display())))?;
    let _guard = lock_store(store, *need)?;
    let mut engine = Engine::open(
        &canon,
        EngineOpts {
            store_dir: Some(store.to_path_buf()),
        },
    )
    .map_err(|e| Fail::Failed(format!("打开全景引擎失败（{}）：{e}", repo.display())))?;
    dispatch(op, &mut engine, args)
}

/// 索引根上的一把文件锁：建索引独占、读共享。
///
/// 病：monitor 进程内那一版靠**进程内**互斥锁防「两条连接对同一索引库并发写」（`panorama.rs`
/// 头注那条真事故）。换成一问一答的独立进程之后，并发的两次调用是**两个进程** ⇒ 进程内的锁
/// 管不到，只能靠文件锁。锁随 `File` drop 释放（进程退出也释放，内核兜底）。
fn lock_store(store: &Path, need: Need) -> Result<std::fs::File, Fail> {
    let path = store.join(".lock");
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| Fail::Failed(format!("开不了索引根的锁文件 `{}`：{e}", path.display())))?;
    let got = match need {
        Need::Build => f.lock(),
        _ => f.lock_shared(),
    };
    got.map_err(|e| Fail::Failed(format!("锁不上索引根 `{}`：{e}", path.display())))?;
    Ok(f)
}

/// 按 op 取参数：**多一个不认识的字段就拒**（拼错的字段名不许被静默忽略）。
fn take<T: DeserializeOwned>(op: &str, args: Value) -> Result<T, Fail> {
    serde_json::from_value(args).map_err(|e| Fail::BadArgs(format!("op `{op}` 的参数不合形：{e}")))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OverviewArgs {
    budget: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SymbolArgs {
    symbol: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SymbolDepthArgs {
    symbol: String,
    depth: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    query: String,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TouchingArgs {
    files: Vec<String>,
    /// 1-based `[start, end]`；空 ⇒ 整文件所有符号（同 monitor `panorama_touching`）。
    ranges: Vec<(usize, usize)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileArgs {
    file: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiagramArgs {
    kind: String,
    request: DiagramRequest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnnotateArgs {
    /// 〔P7〕挂在谁身上：**整个**符号 id（文件级批注 = 裸文件路径）。截 `@行号`、取文件段归上游 `SymbolRef::of`，本程序不拆。
    target: String,
    body: String,
    author: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdArgs {
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DocLinkArgs {
    doc: String,
    target: String,
}

/// 序列化成线上的值（引擎直出的类型原样透出）。
fn out<T: serde::Serialize>(v: T) -> Result<Value, Fail> {
    serde_json::to_value(v).map_err(|e| Fail::Failed(format!("序列化应答失败：{e}")))
}

/// 不碰仓的那几个 op。
fn dispatch_registry(op: &str, args: Value) -> Result<Value, Fail> {
    match op {
        "diagram_kinds" => {
            take::<NoArgs>(op, args)?;
            out(diagram::kinds())
        }
        other => Err(Fail::BadArgs(format!("op `{other}` 要一个仓"))),
    }
}

/// 上游「算」那一层的失败 → 两类：给错了东西 / 盘上那份读不出来。
fn plan_fail(e: edits::PlanError) -> Fail {
    match e {
        edits::PlanError::Invalid(m) => Fail::BadArgs(m),
        edits::PlanError::Io(e) => Fail::Failed(format!("读不出盘上那一份：{e}")),
    }
}

/// 〔RM1d〕只算不写的那几个 op：调上游 `edits::plan_*`（读 ＋ 纯），把计划原样交回。
/// **这里没有一处写盘**（判据 `the_program_never_calls_an_engine_method_that_writes_user_files`）。
fn dispatch_plan(op: &str, repo: &Path, args: Value) -> Result<Value, Fail> {
    match op {
        "plan_add_annotation" => {
            let a: AnnotateArgs = take(op, args)?;
            let at = model::SymbolRef::of(&a.target);
            out(edits::plan_add_annotation(
                repo,
                &at.file,
                at.symbol.as_deref(),
                &a.body,
                &a.author,
            )
            .map_err(plan_fail)?)
        }
        "plan_propose_annotation" => {
            let a: AnnotateArgs = take(op, args)?;
            let at = model::SymbolRef::of(&a.target);
            out(edits::plan_propose_annotation(
                repo,
                &at.file,
                at.symbol.as_deref(),
                &a.body,
                &a.author,
            )
            .map_err(plan_fail)?)
        }
        "plan_approve_annotation" => {
            let a: IdArgs = take(op, args)?;
            out(edits::plan_approve_annotation(repo, &a.id).map_err(plan_fail)?)
        }
        "plan_remove_annotation" => {
            let a: IdArgs = take(op, args)?;
            out(edits::plan_remove_annotation(repo, &a.id).map_err(plan_fail)?)
        }
        "plan_write_doc_link" => {
            let a: DocLinkArgs = take(op, args)?;
            out(edits::plan_write_doc_link(repo, &a.doc, &a.target).map_err(plan_fail)?)
        }
        "plan_remove_doc_link" => {
            let a: DocLinkArgs = take(op, args)?;
            out(edits::plan_remove_doc_link(repo, &a.doc, &a.target).map_err(plan_fail)?)
        }
        other => Err(Fail::BadArgs(format!("op `{other}` 不在「算」那一组里"))),
    }
}

/// 在开好的引擎上跑一个 op。**这里只有读引擎的方法与建索引**（写用户文件的那几样只算不写，
/// 住 [`dispatch_plan`]，见头注）。
fn dispatch(op: &str, e: &mut Engine, args: Value) -> Result<Value, Fail> {
    match op {
        "status" => {
            take::<NoArgs>(op, args)?;
            out(StatusReply {
                stale: e.is_stale(),
                indexed_at: e.indexed_at(),
                symbols: e.symbol_count(),
            })
        }
        "index" => {
            take::<NoArgs>(op, args)?;
            out(e.index().map_err(|x| Fail::Failed(x.to_string()))?)
        }
        "reindex" => {
            take::<NoArgs>(op, args)?;
            out(e.reindex().map_err(|x| Fail::Failed(x.to_string()))?)
        }
        "overview" => {
            let a: OverviewArgs = take(op, args)?;
            out(e.overview(model::TokenBudget(a.budget.unwrap_or(4000))))
        }
        "node" => {
            let a: SymbolArgs = take(op, args)?;
            out(e.node(&a.symbol))
        }
        "neighborhood" => {
            let a: SymbolDepthArgs = take(op, args)?;
            out(e.neighborhood(&a.symbol, a.depth))
        }
        "callers" => {
            let a: SymbolDepthArgs = take(op, args)?;
            out(e.callers(&a.symbol, a.depth))
        }
        "callees" => {
            let a: SymbolDepthArgs = take(op, args)?;
            out(e.callees(&a.symbol, a.depth))
        }
        "impact" => {
            let a: SymbolArgs = take(op, args)?;
            out(e.impact(&a.symbol))
        }
        "search" => {
            let a: SearchArgs = take(op, args)?;
            out(e.search(&a.query, a.limit.unwrap_or(30)))
        }
        "docs_for" => {
            let a: SymbolArgs = take(op, args)?;
            out(e.docs_for(&a.symbol))
        }
        "touching" => {
            let a: TouchingArgs = take(op, args)?;
            let files: Vec<PathBuf> = a.files.into_iter().map(PathBuf::from).collect();
            let ranges: Vec<model::LineRange> = a
                .ranges
                .into_iter()
                .map(|(start, end)| model::LineRange { start, end })
                .collect();
            out(e.symbols_touching(&files, &ranges))
        }
        "symbols_in_file" => {
            // 同 monitor 旧的 `collect_symbols_in_file`〔散文墓碑〕（随内嵌引擎删了，RM1f）：core 没有公开的按文件查询口 ⇒
            // `symbols_touching`（ranges 空 = 整文件）＋ 逐 id `find_symbol`。
            let a: FileArgs = take(op, args)?;
            let refs = e.symbols_touching(&[PathBuf::from(&a.file)], &[]);
            let syms: Vec<model::Symbol> =
                refs.iter().filter_map(|r| e.find_symbol(&r.id)).collect();
            out(syms)
        }
        "drift" => {
            take::<NoArgs>(op, args)?;
            out(e.drift())
        }
        "list_annotations" => {
            take::<NoArgs>(op, args)?;
            out(e.list_annotations())
        }
        "refresh_doc_links" => {
            take::<NoArgs>(op, args)?;
            e.refresh_doc_links()
                .map_err(|x| Fail::Failed(x.to_string()))?;
            Ok(Value::Null)
        }
        "diagram" => {
            let a: DiagramArgs = take(op, args)?;
            let kind = DiagramKind::from_id(&a.kind).map_err(|x| Fail::BadArgs(x.to_string()))?;
            let d = e
                .draw(kind, &a.request)
                .map_err(|x| Fail::Failed(x.to_string()))?;
            out(DiagramReply {
                mermaid: diagram::to_mermaid(&d),
                diagram: out(d)?,
            })
        }
        other => Err(Fail::BadArgs(format!("op `{other}` 不在分派里"))),
    }
}

/// 一次调用 → (stdout 那一行, stderr 那一行（可空）, 退出码)。纯函数外壳，好测。
pub fn answer(argv: &[String]) -> (String, String, i32) {
    let parsed = match parse_argv(argv) {
        Ok(p) => p,
        Err(f) => return fail_line(&f),
    };
    match parsed {
        Parsed::Probe => (probe_text(), String::new(), 0),
        Parsed::Call {
            op,
            repo,
            store,
            args,
        } => match run(&op, repo.as_deref(), store.as_deref(), args) {
            Ok(data) => (
                format!("{}\n", json!({ "ok": true, "data": data })),
                String::new(),
                0,
            ),
            Err(f) => fail_line(&f),
        },
    }
}

fn fail_line(f: &Fail) -> (String, String, i32) {
    (
        format!(
            "{}\n",
            json!({ "ok": false, "code": f.code(), "message": f.message() })
        ),
        format!("{}\n", f.message()),
        f.exit(),
    )
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let (stdout, stderr, code) = answer(&argv);
    // 写不出去（管道已关）就只剩退出码能说话 —— 不 panic，照原码退。
    let _ = std::io::stdout().write_all(stdout.as_bytes());
    let _ = std::io::stderr().write_all(stderr.as_bytes());
    std::process::exit(code);
}

#[cfg(test)]
#[path = "../../tests/panorama-engine/cli_tests.rs"]
mod tests;
