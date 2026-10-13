//! 轮换策略的**沙箱**：Rhai，没有文件 · 网络 · 钟 · 随机数（没注册就没有；`no_time` 连取时间的函数都不编进来）；
//! `print` / `debug` 吞掉；不给模块解析器（`import` 不可用）；操作数 · 调用深度 · 表达式深度 · 字符串 · 数组 · 表都有上限，另按墙钟兜底。
//! 策略文件的读写与 `decide(ctx)` 在 [`super::rules::script`]（它在这个沙箱上挂积木库 `lib`）。

use rhai::packages::{Package, StandardPackage};
use rhai::{Dynamic, Engine, EvalAltResult, Scope, AST};
use serde::Serialize;
use std::cell::Cell;

/// 一判至多几次操作（同样的输入在同一处停）。
pub(crate) const MAX_OPERATIONS: u64 = 200_000;
/// 墙钟兜底：一次调用至多跑多久（毫秒）。**主上限是操作数 · 调用深度 · 大小**（同样的输入停在同一处，跟机器忙不忙无关）；
/// 墙钟只兜那几样都没掐住的极端情形，放宽到秒级，免得机器一忙就判超时、换到兜底那份去挑号。真撞上了照超时办、带 `strategyError` 交出来。
/// 不是定时器：只在脚本每走一步时（`on_progress`）比一下，不让谁自己醒来。
const WALL_MS: u128 = 1_000;

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

/// 一个新的、照同样上限挂好的沙箱（第五版在它上面挂自己的积木库）。
pub(crate) fn fresh_engine() -> Engine {
    bare()
}

/// 在给定的沙箱里调（同 [`call`]）。
pub(crate) fn call_in(
    e: &Engine,
    ast: &AST,
    name: &str,
    args: impl rhai::FuncArgs,
) -> (Result<Dynamic, Box<EvalAltResult>>, u64) {
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
    /// 会话用的那份策略文件此刻没读到（文件没了 · 太大）。
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
