//! 轮换的**策略文件**：加载 · 沙箱 · 调用。策略只算优先级（`priority(ctx)`），偏好全在它里头；引擎只管事实与机械动作。
//!
//! - 一份策略 ＝ 一个 `.rhai` 文件，文件名去 `.rhai` 就是 id；必需四个函数 `meta` · `texts` · `describe` · `priority`。
//! - 两个来源、同一个读法（[`load`]）：内置三份（`src/shared/rotation-strategies/{1,2,3}.rhai`，编进引擎，只读）·
//!   本机 `~/.cc-monitor/strategies/`。共用小函数库 `lib.rhai` 也编进引擎，挂成静态模块 `lib`，不当策略加载。
//! - 沙箱：`Engine::new_raw()` ＋ 标准包；没有读文件 · 连网 · 取时间 · 随机数（没注册就没有；`no_time` 连取时间的函数都不编进来）；
//!   `print` / `debug` 吞掉；不给模块解析器（`import` 不可用）；操作数 · 调用深度 · 表达式深度 · 字符串 · 数组 · 表都有上限，另按墙钟兜底。
//!
//! 引擎怎么用它：[`ctx`] 拼上下文 → [`run`] 调 `priority` 取回每号的优先级与备忘（出错照内置策略 1 兜底）→ [`words`] 把 `why` /
//! `describe` 片段写成字；[`form`] 校验参数声明与参数值。

pub(crate) mod ctx;
pub(crate) mod form;
pub(crate) mod run;
pub(crate) mod words;

use form::Decl;
use rhai::packages::{Package, StandardPackage};
use rhai::{Dynamic, Engine, EvalAltResult, Module, Scope, AST};
use serde::Serialize;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// 共用小函数库的源码（随引擎走，只读；策略列表顶上那一行照它给源码）。
pub(crate) const LIB_SOURCE: &str = include_str!("../../../../shared/rotation-strategies/lib.rhai");

/// 内置三份策略：（id, 源码）。随安装包带的就是这三份文件；单可执行文件里编进来，与本机目录走同一个读法。
pub(crate) const BUILTIN: [(&str, &str); 3] = [
    (
        "1",
        include_str!("../../../../shared/rotation-strategies/1.rhai"),
    ),
    (
        "2",
        include_str!("../../../../shared/rotation-strategies/2.rhai"),
    ),
    (
        "3",
        include_str!("../../../../shared/rotation-strategies/3.rhai"),
    ),
];

/// 出错兜底用的那一份（内置策略 1）。
pub(crate) const FALLBACK_ID: &str = "1";

/// 一判至多几次操作（同样的输入在同一处停）。
pub(crate) const MAX_OPERATIONS: u64 = 200_000;
/// 墙钟兜底：一次调用至多跑多久（毫秒）。**主上限是操作数 · 调用深度 · 大小**（同样的输入停在同一处，跟机器忙不忙无关）；
/// 墙钟只兜那几样都没掐住的极端情形，放宽到秒级，免得机器一忙就判超时、换到兜底那份去挑号。真撞上了照超时办、带 `strategyError` 交出来。
/// 不是定时器：只在脚本每走一步时（`on_progress`）比一下，不让谁自己醒来。
const WALL_MS: u128 = 1_000;
/// 一份文件至多多大。
pub(crate) const MAX_SOURCE_BYTES: usize = 64 * 1024;
/// 本机目录里至多读几份。
pub(crate) const MAX_FILES: usize = 64;
/// id 至多几个字。
pub(crate) const MAX_ID_CHARS: usize = 32;
/// 本机策略没起名时从几编起（内置占了 1–3）。
const LOCAL_NAME_FROM: usize = 4;

thread_local! {
    /// 这一次调用从什么时候起（调用前设、调用后清）。
    static STARTED: Cell<Option<std::time::SystemTime>> = const { Cell::new(None) };
    /// 这一次调用到此用了几次操作（`on_progress` 记）。
    static OPS: Cell<u64> = const { Cell::new(0) };
}

/// 不带 `lib` 的沙箱（`lib.rhai` 自己就在它里头编）。
fn bare() -> Engine {
    let mut e = Engine::new_raw();
    e.register_global_module(StandardPackage::new().as_shared_module());
    e.set_max_operations(MAX_OPERATIONS)
        .set_max_call_levels(16)
        .set_max_expr_depths(64, 32)
        .set_max_string_size(4096)
        .set_max_array_size(1024)
        .set_max_map_size(256);
    e.on_print(|_| {});
    e.on_debug(|_, _, _| {});
    e.on_progress(|n| {
        OPS.with(|c| c.set(n));
        let late = STARTED.with(|d| {
            d.get().is_some_and(|t| {
                crate::common::time::now()
                    .duration_since(t)
                    .is_ok_and(|e| e.as_millis() >= WALL_MS)
            })
        });
        late.then(|| Dynamic::from(WALL_TAG))
    });
    e
}

/// 墙钟掐断时 `on_progress` 交回的那个值（认它就知道是超时，不是脚本自己 `throw`）。
const WALL_TAG: &str = "@wall";

/// 挂好 `lib` 的那一个沙箱（进程里一份；`sync` ⇒ 跨线程共用）。`lib.rhai` 编不过 ⇒ `Err`（判据 S4 跑它，走不到）。
pub(crate) fn engine() -> Result<&'static Engine, String> {
    static E: OnceLock<Result<Engine, String>> = OnceLock::new();
    E.get_or_init(|| {
        let mut e = bare();
        let ast = e.compile(LIB_SOURCE).map_err(|x| x.to_string())?;
        let m = Module::eval_ast_as_new(Scope::new(), &ast, &e).map_err(|x| x.to_string())?;
        e.register_static_module("lib", m.into());
        Ok(e)
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// 调一份策略里的一个函数（带墙钟期限）；回值 ＋ 用了几次操作。
pub(crate) fn call(
    ast: &AST,
    name: &str,
    args: impl rhai::FuncArgs,
) -> (Result<Dynamic, Box<EvalAltResult>>, u64) {
    let e = match engine() {
        Ok(e) => e,
        Err(m) => return (Err(m.into()), 0),
    };
    STARTED.with(|d| d.set(Some(crate::common::time::now())));
    OPS.with(|c| c.set(0));
    let r = e.call_fn_with_options::<Dynamic>(
        rhai::CallFnOptions::new().eval_ast(false),
        &mut Scope::new(),
        ast,
        name,
        args,
    );
    STARTED.with(|d| d.set(None));
    (r, OPS.with(Cell::get))
}

/// 一次调用为什么没成。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "StrategyFailKind"
    )
)]
pub enum FailKind {
    /// 编不过 · 跑出错（`line` 是第几行）。
    #[serde(rename = "failed")]
    Broke,
    /// 操作数超了 · 墙钟到了。
    Timeout,
    /// 回的形状不对。
    BadShape,
    /// 配置写的那份策略此刻没读到（文件没了 · 读不懂）。
    Gone,
}

/// 一次调用没成：哪一种 · 第几行（说得出才有）· 原话（英文诊断，不进文案表）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "StrategyFail"
    )
)]
pub struct Fail {
    pub kind: FailKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub col: Option<usize>,
    pub msg: String,
}

impl Fail {
    pub(crate) fn shape(msg: impl Into<String>) -> Self {
        Self {
            kind: FailKind::BadShape,
            line: None,
            col: None,
            msg: msg.into(),
        }
    }

    /// 跑出来的错：超操作数 / 墙钟 ⇒ 超时（带出处）；其余 ⇒ 出错（带行）。
    pub(crate) fn of_eval(e: &EvalAltResult) -> Self {
        let pos = e.position();
        let timeout = match e {
            EvalAltResult::ErrorTooManyOperations(_) => true,
            EvalAltResult::ErrorTerminated(v, _) => {
                v.clone().into_string().ok().as_deref() == Some(WALL_TAG)
            }
            _ => false,
        };
        Self {
            kind: if timeout {
                FailKind::Timeout
            } else {
                FailKind::Broke
            },
            line: pos.line(),
            col: pos.position(),
            msg: if matches!(e, EvalAltResult::ErrorTerminated(..)) && timeout {
                format!("wall clock over {WALL_MS} ms (the machine was too busy or the script too slow)")
            } else {
                e.to_string()
            },
        }
    }
}

/// 一份策略自己的字：`zh` · `en` 各一张（键 → 句）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Texts {
    pub(crate) zh: BTreeMap<String, String>,
    pub(crate) en: BTreeMap<String, String>,
}

/// 照哪种语言写字（策略自己的字有两种；引擎的字今天只有 zh）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Lang {
    #[default]
    Zh,
    En,
}

impl Lang {
    /// 请求里的 `lang`：`en` ⇒ 英文，别的 ⇒ 中文。
    pub(crate) fn of(v: Option<&str>) -> Self {
        match v {
            Some(s) if s.starts_with("en") => Lang::En,
            _ => Lang::Zh,
        }
    }
}

impl Texts {
    pub(crate) fn get(&self, lang: Lang, k: &str) -> Option<&str> {
        match lang {
            Lang::Zh => self.zh.get(k),
            Lang::En => self.en.get(k),
        }
        .map(String::as_str)
    }

    /// 两种语言都有这个键。
    pub(crate) fn has(&self, k: &str) -> bool {
        self.zh.contains_key(k) && self.en.contains_key(k)
    }
}

/// 读进来的一份策略。读不懂 ⇒ `error` 有、`ast` 缺（列表里照实写、不能被选）。
#[derive(Clone)]
pub(crate) struct Strategy {
    pub(crate) id: String,
    /// 显示名：内置 `内置策略 {id}`；本机 `meta().name`，没有 ⇒ `策略 N`。
    pub(crate) name: String,
    pub(crate) builtin: bool,
    /// 本机那一份的路径（内置 ⇒ 缺）。
    pub(crate) path: Option<PathBuf>,
    pub(crate) source: String,
    /// 配置里号的先后是不是这份策略的参数。
    pub(crate) ordered: bool,
    pub(crate) params: Vec<Decl>,
    pub(crate) acct_params: Vec<Decl>,
    pub(crate) texts: Texts,
    pub(crate) ast: Option<Arc<AST>>,
    pub(crate) error: Option<Fail>,
}

impl std::fmt::Debug for Strategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Strategy")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("builtin", &self.builtin)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl Strategy {
    pub(crate) fn usable(&self) -> bool {
        self.ast.is_some() && self.error.is_none()
    }

    /// 整份参数的缺省（按声明的 `init`）。
    pub(crate) fn param_init(&self) -> serde_json::Map<String, serde_json::Value> {
        form::init_of(&self.params)
    }

    /// 每号参数的缺省（按声明的 `init`）。
    pub(crate) fn acct_init(&self) -> serde_json::Map<String, serde_json::Value> {
        form::init_of(&self.acct_params)
    }
}

/// 目录里的一个文件：文件名 · 读到的字（读不出 ⇒ 原因）。
pub(crate) struct File {
    pub(crate) name: String,
    pub(crate) path: Option<PathBuf>,
    pub(crate) text: Result<String, String>,
}

/// id 能不能用：过路由段闸那一套字符、不以 `@` 开头、至多 32 字。
pub(crate) fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('@')
        && id.chars().count() <= MAX_ID_CHARS
        && crate::relay::segment_is_safe(id)
}

/// 第几行：源码里 `needle` 第一次出现在哪一行（声明出错时说「第 N 行」用；找不到 ⇒ `meta` 那一行）。
fn line_of(source: &str, needle: &str) -> Option<usize> {
    let at = source.find(needle).or_else(|| source.find("fn meta"))?;
    Some(source[..at].matches('\n').count() + 1)
}

fn fail_at(source: &str, needle: &str, msg: String) -> Fail {
    Fail {
        kind: FailKind::Broke,
        line: line_of(source, needle),
        col: None,
        msg,
    }
}

/// ★ 读一个目录里的策略（内置与本机同一处）：编 → 调 `meta` · `texts` · `describe(init, [])` 校验。`builtin` ⇒ 名字写「内置策略 N」。
/// `taken` ＝ 已有的 id（本机与内置同 id ⇒ 不加载，`dupId`）。
pub(crate) fn load(files: Vec<File>, builtin: bool, taken: &[String]) -> Vec<Strategy> {
    let mut files = files;
    files.sort_by(|a, b| a.name.cmp(&b.name));
    files.truncate(MAX_FILES);
    files
        .into_iter()
        .enumerate()
        .map(|(i, f)| {
            let id = f.name.strip_suffix(".rhai").unwrap_or(&f.name).to_string();
            let fallback_name = if builtin {
                copy_core::copy_text("beRotation.strategy.builtinName", &[("id", &id)])
            } else {
                copy_core::copy_text(
                    "beRotation.strategy.localName",
                    &[("n", &(LOCAL_NAME_FROM + i).to_string())],
                )
            };
            let mut l = Strategy {
                id: id.clone(),
                name: fallback_name,
                builtin,
                path: f.path,
                source: String::new(),
                ordered: false,
                params: Vec::new(),
                acct_params: Vec::new(),
                texts: Texts::default(),
                ast: None,
                error: None,
            };
            let bad = |msg: &str| Fail {
                kind: FailKind::Broke,
                line: None,
                col: None,
                msg: msg.to_string(),
            };
            match f.text {
                Err(why) => l.error = Some(bad(&why)),
                Ok(text) if text.len() > MAX_SOURCE_BYTES => {
                    l.error = Some(bad(&format!("file is over {MAX_SOURCE_BYTES} bytes")))
                }
                Ok(text) if !id_ok(&id) => {
                    l.source = text;
                    l.error = Some(bad("bad file name: id must be a route segment, ≤ 32 chars, not starting with @"));
                }
                Ok(text) if taken.contains(&id) => {
                    l.source = text;
                    l.error = Some(Fail {
                        kind: FailKind::Broke,
                        line: None,
                        col: None,
                        msg: DUP_ID.to_string(),
                    });
                }
                Ok(text) => {
                    l.source = text;
                    if let Err(e) = read_into(&mut l, builtin) {
                        l.error = Some(e);
                        l.ast = None;
                    }
                }
            }
            l
        })
        .collect()
}

/// 与内置同 id 的那句原话（列表里认它写「与内置同名」）。
pub(crate) const DUP_ID: &str = "dupId";

/// 编 · 调 `meta` / `texts` / `describe` 校验，填进 `l`。
fn read_into(l: &mut Strategy, builtin: bool) -> Result<(), Fail> {
    let e = engine().map_err(|m| Fail::shape(m))?;
    let ast = e.compile(&l.source).map_err(|x| {
        let p = x.position();
        Fail {
            kind: FailKind::Broke,
            line: p.line(),
            col: p.position(),
            msg: x.to_string(),
        }
    })?;
    for f in ["meta", "texts", "describe", "priority"] {
        if !ast.iter_functions().any(|d| d.name == f) {
            return Err(fail_at(
                &l.source,
                "fn meta",
                format!("`fn {f}` is missing"),
            ));
        }
    }
    let to_json = |d: Dynamic, what: &str| -> Result<serde_json::Value, Fail> {
        rhai::serde::from_dynamic::<serde_json::Value>(&d)
            .map_err(|x| fail_at(&l.source, &format!("fn {what}"), x.to_string()))
    };
    let (meta, _) = call(&ast, "meta", ());
    let meta = to_json(meta.map_err(|x| Fail::of_eval(&x))?, "meta")?;
    let (texts, _) = call(&ast, "texts", ());
    let texts = to_json(texts.map_err(|x| Fail::of_eval(&x))?, "texts")?;
    l.texts = form::texts_of(&texts).map_err(|m| fail_at(&l.source, "fn texts", m))?;
    let m = meta
        .as_object()
        .ok_or_else(|| fail_at(&l.source, "fn meta", "meta() must return a map".into()))?;
    if m.get("format").and_then(serde_json::Value::as_i64) != Some(1) {
        return Err(fail_at(
            &l.source,
            "format",
            "meta().format must be 1".into(),
        ));
    }
    l.ordered = m
        .get("ordered")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !builtin {
        if let Some(n) = m
            .get("name")
            .and_then(serde_json::Value::as_str)
            .filter(|n| !n.trim().is_empty())
        {
            l.name = n.trim().to_string();
        }
    }
    let src = l.source.clone();
    let decl_err = |(k, msg): (String, String)| fail_at(&src, &format!("\"{k}\""), msg);
    l.params = form::decls_of(m.get("params"), &l.texts, form::Scope::Whole).map_err(decl_err)?;
    l.acct_params = form::decls_of(m.get("acct_params"), &l.texts, form::Scope::PerAccount)
        .map_err(decl_err)?;
    // 描述：缺省参数 ＋ 没有号也得跑得通，片段的键都得有字。
    let init = rhai::serde::to_dynamic(serde_json::Value::Object(l.param_init()))
        .map_err(|x| Fail::shape(x.to_string()))?;
    let (d, _) = call(&ast, "describe", (init, rhai::Array::new()));
    let d = to_json(d.map_err(|x| Fail::of_eval(&x))?, "describe")?;
    words::check_fragments(&d, &l.texts).map_err(|m| fail_at(&l.source, "fn describe", m))?;
    l.ast = Some(Arc::new(ast));
    Ok(())
}

/// 内置三份（进程里读一次）。
pub(crate) fn builtins() -> &'static [Arc<Strategy>] {
    static B: OnceLock<Vec<Arc<Strategy>>> = OnceLock::new();
    B.get_or_init(|| {
        let files = BUILTIN
            .iter()
            .map(|(id, src)| File {
                name: format!("{id}.rhai"),
                path: None,
                text: Ok((*src).to_string()),
            })
            .collect();
        load(files, true, &[]).into_iter().map(Arc::new).collect()
    })
}

/// 本机策略目录：`<家>/strategies/`（家同轮换那份文件）。
pub(crate) fn dir_from(rotation_path: &Path) -> Option<PathBuf> {
    rotation_path.parent().map(|p| p.join(DIR_NAME))
}

/// 本机策略目录的名字。
pub(crate) const DIR_NAME: &str = "strategies";

/// 读一个目录里的 `.rhai`（不在 ⇒ 空）。
pub(crate) fn files_in(dir: &Path) -> Vec<File> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<File> = rd
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("rhai"))
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let path = e.path();
            let text = match std::fs::metadata(&path) {
                Ok(m) if m.len() > MAX_SOURCE_BYTES as u64 => {
                    Err(format!("file is over {MAX_SOURCE_BYTES} bytes"))
                }
                _ => std::fs::read_to_string(&path).map_err(|x| x.to_string()),
            };
            Some(File {
                name,
                path: Some(path),
                text,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 目录的戳：每个文件的名 · 改动时刻 · 长度（变了才重读）。
type DirStamp = Vec<(String, Option<std::time::SystemTime>, u64)>;

fn dir_stamp(dir: &Path) -> DirStamp {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: DirStamp = rd
        .filter_map(Result::ok)
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((
                e.file_name().to_str()?.to_string(),
                m.modified().ok(),
                m.len(),
            ))
        })
        .collect();
    v.sort();
    v
}

/// 此刻加载的全部策略：内置在前、本机按文件名在后（本机目录的戳变了才重读）。`dir = None` ⇒ 只有内置。
pub(crate) fn all(dir: Option<&Path>) -> Arc<Vec<Arc<Strategy>>> {
    type Cache = BTreeMap<PathBuf, (DirStamp, Arc<Vec<Arc<Strategy>>>)>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let b = builtins();
    let Some(dir) = dir else {
        return Arc::new(b.to_vec());
    };
    let st = dir_stamp(dir);
    let cache = CACHE.get_or_init(|| Mutex::new(Cache::new()));
    let mut g = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((s, v)) = g.get(dir) {
        if *s == st {
            return v.clone();
        }
    }
    let taken: Vec<String> = b.iter().map(|l| l.id.clone()).collect();
    let mut v: Vec<Arc<Strategy>> = b.to_vec();
    v.extend(load(files_in(dir), false, &taken).into_iter().map(Arc::new));
    let v = Arc::new(v);
    g.insert(dir.to_path_buf(), (st, v.clone()));
    v
}

/// 按 id 找一份（读不懂的也回，调用方看 `usable`）。
pub(crate) fn find(all: &[Arc<Strategy>], id: &str) -> Option<Arc<Strategy>> {
    all.iter().find(|l| l.id == id).cloned()
}

/// 兜底那一份（内置策略 1）。走不到 `None`：内置三份的判据（S3）跑它。
pub(crate) fn fallback() -> Option<Arc<Strategy>> {
    builtins()
        .iter()
        .find(|l| l.id == FALLBACK_ID && l.usable())
        .cloned()
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/strategy/strategy_tests.rs"]
mod tests;
