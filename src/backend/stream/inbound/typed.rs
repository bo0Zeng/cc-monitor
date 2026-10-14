//! **typed 命令**：入参 · 回包 · 错误码各是一个 Rust 类型（第 8 批）。
//!
//! 一条命令 = 一个零大小的标记类型，实现 [`Command`]（名字 · 一句话 · 三个类型）＋ [`Blocking`] 或 [`Async`]（怎么跑）。
//! 登记表里一行 `typed::blocking::<X>("一句话")` / `typed::asynchronous::<X>("一句话")`：
//! - 入参只在 [`take`] 这一处从 `args` 解成 `Req`（`deny_unknown_fields`）：形状不对 ⇒ `bad_args`，原话里带解码器那一句；处理器里没有 `args.get`。
//! - 码表从错误枚举来（[`codes!`] 声明的那一份），登记的 `codes` 就是它，不另写。
//! - 字段不手写（`fields` 空）：协议参考与形状指纹都从类型生成（测试档，经 ts-rs）。
//! - 跑起来就是 [`super::Run::Typed`] 那一档，按档分派照旧只在 [`super::spec::prepare`] 一处。

use super::spec::{CommandSpec, DataFut, Fail, Outcome, Run};
use crate::stream::wire::Request;

/// 测试档才要的 TS 派生（生成类型 · 形状指纹）；发布二进制里它什么都不要求。
#[cfg(test)]
pub(crate) trait Wire: ts_rs::TS + 'static {}
#[cfg(test)]
impl<T: ts_rs::TS + 'static> Wire for T {}
#[cfg(not(test))]
pub(crate) trait Wire {}
#[cfg(not(test))]
impl<T> Wire for T {}

/// 一条 typed 命令的三个类型。
pub(crate) trait Command: 'static {
    /// 线上命令名。
    const NAME: &'static str;
    /// 收不收入方向载荷（CLI 面据此决定读不读 stdin；同 [`CommandSpec::takes_input`]）。
    const TAKES_INPUT: bool = true;
    type Req: serde::de::DeserializeOwned + Wire;
    type Reply: serde::Serialize + Wire;
    type Err: Codes;
}

/// 不收入参的那几条命令的入参：缺 · `null` · `{}` 都认，多一个键 ⇒ `bad_args`。
#[derive(Debug, Clone, Copy, Default, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct NoArgs {}

/// 下层那一份失败（码 ＋ 那一句，`(&'static str, String)` 那一形）⇒ 这一条的错误枚举（同 [`of_fail`]）。
pub(crate) fn of_pair<E: Codes>(fallback: fn(String) -> E) -> impl Fn((&'static str, String)) -> E {
    move |(code, said)| of_fail(Fail::new(code, said), fallback)
}

/// 一条命令自己可能回的那几个码（[`codes!`] 声明）。
pub(crate) trait Codes: Sized {
    /// 全部码，声明次序。
    const ALL: &'static [&'static str];
    /// 投出去的那一份。
    fn into_fail(self) -> Fail;
    /// 下层交上来的码 ⇒ 这一条的那个变体（只认不带 `data` 的那几种）；不在码表里 ⇒ 原样交回那一句（调用方定落哪一格）。
    fn of_code(code: &str, said: String) -> Result<Self, String>;
}

/// 下层那一份失败（码 ＋ 那一句 ＋ 原话）⇒ 这一条的错误枚举：码在这一条的码表里 ⇒ 照交；不在 ⇒ 落 `fallback`
/// （这一条自己的那一格失败：资产域那扇门把文件管理那一族的码原样交上来，外层命令的码表不必都有），那一句照旧，原码与原话进原话。
/// 原话跟着那一句走（[`Fail::into_said`]），投出去时照原样拆回复制详情。
pub(crate) fn of_fail<E: Codes>(f: Fail, fallback: fn(String) -> E) -> E {
    let code = f.code.clone();
    let (message, raw) = (f.message.clone(), f.raw.clone());
    E::of_code(&code, f.into_said()).unwrap_or_else(|_| {
        tracing::warn!("typed: 下层回了码表里没有的码 `{code}`，落这一条自己的那一格");
        let lower = match raw {
            Some(r) => format!("{code}: {message}\n{r}"),
            None => format!("{code}: {message}"),
        };
        fallback(crate::common::contract::joined(message, Some(lower)))
    })
}

/// 同步阻塞的那一种（读盘 · 扫全库）：进阻塞池，开跑之后打不断。
pub(crate) trait Blocking: Command {
    fn run(req: Self::Req, cx: &Cx) -> Result<Self::Reply, Self::Err>;
}

/// 文件管理面（同 `faces::launch_face::Files`）：生产那一份住 `stream/inbound`（`LocalFiles`），由命令表那一行递进来
/// —— 写面只有 `stream/inbound/` 够得着（`readonly_guard` 第三层），本文件不点它的名。
pub(crate) type Files = &'static (dyn crate::common::door::Door + Sync);

/// 同 [`Blocking`]，多递一份**文件管理面**（起之前预标信任经它写）。处理器签名里写着它要这一份。
pub(crate) trait WithFiles: Command {
    fn run(req: Self::Req, cx: &Cx, files: Files) -> Result<Self::Reply, Self::Err>;
}

/// 同 [`WithFiles`]，再多递一份**写自家状态的句柄**（`~/.cc-monitor` 底下）。分派那一层手里没有这一份的入口
/// （文件门 · 小成品重问）交不出来（[`super::spec::prepare`] 回 `None`）。
pub(crate) trait WithOwn: Command {
    fn run(
        req: Self::Req,
        cx: &Cx,
        files: Files,
        own: &crate::platform::Own,
    ) -> Result<Self::Reply, Self::Err>;
}

/// 同 [`Blocking`]，只多递一份**写自家状态的句柄**（`~/.cc-monitor` 底下），不带文件管理面。
/// 分派那一层手里没有写句柄的入口交不出来（同 [`WithOwn`]）。
pub(crate) trait Owning: Command {
    fn run(req: Self::Req, cx: &Cx, own: &crate::platform::Own) -> Result<Self::Reply, Self::Err>;
}

/// 同 [`Blocking`]，多递一份**文件管理写句柄**（[`crate::platform::fs::Face`]：改用户的文件）。只有文件管理写面那几条用这一档
/// （`readonly_guard::only_the_file_manager_commands_are_handed_the_face`）；句柄由分派那一层造了递进来（[`super::spec::prepare`]）。
pub(crate) trait Facing: Command {
    fn run(
        req: Self::Req,
        cx: &Cx,
        face: &crate::platform::fs::Face,
    ) -> Result<Self::Reply, Self::Err>;
}

/// 真异步的那一种：交给独立 task，`cancel` 能在 await 点打断。
pub(crate) trait Async: Command {
    fn run(
        req: Self::Req,
        cx: Cx,
    ) -> impl std::future::Future<Output = Result<Self::Reply, Self::Err>> + Send + 'static;
}

/// 同 [`Async`]，多递一份**文件管理面**（同 [`WithFiles`]：那一份是命令表那一行递的）。
pub(crate) trait AsyncFiles: Command {
    fn run(
        req: Self::Req,
        cx: Cx,
        files: Files,
    ) -> impl std::future::Future<Output = Result<Self::Reply, Self::Err>> + Send + 'static;
}

/// 两台之间那几件的枢纽问**这台自己**的那一跳（生产 = `doors::hub_here`，由命令表那一行递进来，同 [`Files`]）：
/// 按引起这一趟的请求时区与写句柄造一份。
pub(crate) type Hub =
    fn(&crate::Tz, &crate::platform::Own) -> std::sync::Arc<dyn crate::assets::hub::Here>;

/// 同 [`Async`]，多递一份**写自家状态的句柄**（按值：交出去的 future 要 `'static`；同 [`super::Run::AsyncOwn`]）
/// 与枢纽问这台自己的那一跳（[`Hub`]，命令表那一行递的）。分派那一层手里没有写句柄的入口交不出来（同 [`Owning`]）。
pub(crate) trait AsyncOwning: Command {
    fn run(
        req: Self::Req,
        cx: Cx,
        own: crate::platform::Own,
        hub: Hub,
    ) -> impl std::future::Future<Output = Result<Self::Reply, Self::Err>> + Send + 'static;
}

/// 同 [`Async`]，只多递一份**写自家状态的句柄**（按值：交出去的 future 要 `'static`），不带枢纽那一跳。
/// 分派那一层手里没有写句柄的入口交不出来（同 [`Owning`]）。
pub(crate) trait AsyncOwn: Command {
    fn run(
        req: Self::Req,
        cx: Cx,
        own: crate::platform::Own,
    ) -> impl std::future::Future<Output = Result<Self::Reply, Self::Err>> + Send + 'static;
}

/// 信封里除了 `args` 那几样：看的那一台的时区 · 分派那一层换算好的截止时刻。
pub(crate) struct Cx {
    pub(crate) tz: crate::common::time::Tz,
    #[allow(dead_code)] // 第一批 typed 命令里还没有装总期限的；起进程那几族换过来时用。
    pub(crate) until: Option<crate::platform::child::Until>,
}

/// 声明一条命令的错误枚举：每个变体带那一句（同 [`Fail::new`]：契约错的诊断跟在句子后面 ⇒ 拆进原话），码就是右边那个字面量。
/// 按码定了 `data` 形状的那几个码写成 `Var = "code" => 类型`：变体多带那一份，投出去时进失败信封的 `data`。
/// 码表恒以 `bad_args` 打头、枚举里恒有 `BadArgs`：入参解不出来是每条 typed 命令都会回的（[`take`]），
/// 入参解得出、组合不对（「两个里恰好给一个」那一类）的由处理器自己回 `BadArgs`。
macro_rules! codes {
    ($(#[$m:meta])* $vis:vis enum $name:ident { $($body:tt)* }) => {
        $crate::stream::inbound::typed::codes!(@munch [$(#[$m])*] [$vis] $name [] [] [] [] $($body)*);
    };
    // 带 `data` 的那一种。
    (@munch $attrs:tt $vis:tt $name:ident [$($vars:tt)*] [$($arms:tt)*] [$($codes:tt)*] [$($of:tt)*]
        $(#[$vm:meta])* $var:ident = $code:literal => $data:ty $(, $($rest:tt)*)?) => {
        $crate::stream::inbound::typed::codes!(@munch $attrs $vis $name
            [$($vars)* $(#[$vm])* $var(String, $data),]
            [$($arms)* $name::$var(said, data) => $crate::stream::inbound::spec::Fail {
                data: ::serde_json::to_value(&data).ok(),
                ..$crate::stream::inbound::spec::Fail::new($code, said)
            },]
            [$($codes)* $code,]
            [$($of)*]
            $($($rest)*)?);
    };
    // 只有那一句的那一种。
    (@munch $attrs:tt $vis:tt $name:ident [$($vars:tt)*] [$($arms:tt)*] [$($codes:tt)*] [$($of:tt)*]
        $(#[$vm:meta])* $var:ident = $code:literal $(, $($rest:tt)*)?) => {
        $crate::stream::inbound::typed::codes!(@munch $attrs $vis $name
            [$($vars)* $(#[$vm])* $var(String),]
            [$($arms)* $name::$var(said) => $crate::stream::inbound::spec::Fail::new($code, said),]
            [$($codes)* $code,]
            [$($of)* ($code, $var)]
            $($($rest)*)?);
    };
    (@munch [$(#[$m:meta])*] [$vis:vis] $name:ident [$($vars:tt)*] [$($arms:tt)*] [$($codes:tt)*] [$(($ocode:literal, $ovar:ident))*]) => {
        $(#[$m])*
        #[derive(Debug)]
        $vis enum $name {
            /// 入参组合不对（形状不对的那一种在解入参那一步就回了，不经这里）。
            #[allow(dead_code)] // 不是每条命令都有「组合不对」这一种；码表里它恒在（解入参那一步）。
            BadArgs(String),
            $($vars)*
        }
        impl $crate::stream::inbound::typed::Codes for $name {
            const ALL: &'static [&'static str] = &["bad_args", $($codes)*];
            #[allow(unreachable_patterns, clippy::match_single_binding)]
            fn of_code(code: &str, said: String) -> Result<Self, String> {
                Ok(match code {
                    "bad_args" => $name::BadArgs(said),
                    $($ocode => $name::$ovar(said),)*
                    _ => return Err(said),
                })
            }
            fn into_fail(self) -> $crate::stream::inbound::spec::Fail {
                match self {
                    $name::BadArgs(said) => $crate::stream::inbound::spec::Fail::new("bad_args", said),
                    $($arms)*
                }
            }
        }
    };
}
pub(crate) use codes;

/// 那一份 typed 的跑法（装进 [`Run::Typed`]）。
#[derive(Clone, Copy)]
pub(crate) struct Typed {
    pub(super) run: TypedRun,
    /// 生成类型与形状指纹用（测试档）。
    #[cfg(test)]
    pub(crate) shape: fn() -> tests::Shape,
}

impl Typed {
    /// 这一份拿不拿得到文件管理写句柄（[`Facing`] 那一档）。
    pub(crate) fn hands_the_face(&self) -> bool {
        matches!(self.run, TypedRun::BlockingFace(_))
    }
}

#[derive(Clone, Copy)]
pub(super) enum TypedRun {
    Blocking(fn(Request) -> Outcome),
    /// 阻塞、要文件管理面（[`WithFiles`]）：那一份是命令表那一行递的。
    BlockingFiles(fn(Request, Files) -> Outcome, Files),
    /// 阻塞、要文件管理面 ＋ 写句柄（[`WithOwn`]）：写句柄由分派那一层递。
    BlockingOwn(fn(Request, Files, &crate::platform::Own) -> Outcome, Files),
    /// 阻塞、只要写句柄（[`Owning`]）：写句柄由分派那一层递。
    BlockingOwning(fn(Request, &crate::platform::Own) -> Outcome),
    /// 阻塞、要文件管理写句柄（[`Facing`]）：句柄由分派那一层造了递。
    BlockingFace(fn(Request, &crate::platform::fs::Face) -> Outcome),
    Async(fn(Request) -> DataFut),
    /// 异步、要文件管理面（[`AsyncFiles`]）。
    AsyncFiles(fn(Request, Files) -> DataFut, Files),
    /// 异步、要写句柄 ＋ 枢纽那一跳（[`AsyncOwning`]）：写句柄由分派那一层递，枢纽那一跳是命令表那一行递的。
    AsyncOwning(fn(Request, crate::platform::Own, Hub) -> DataFut, Hub),
    /// 异步、只要写句柄（[`AsyncOwn`]）：写句柄由分派那一层递。
    AsyncOwn(fn(Request, crate::platform::Own) -> DataFut),
}

/// 入参 ⇒ `R`。缺 ＝ `{}`；多一个键 · 缺一个必填 · 类型不对 ⇒ `bad_args`（解码器那一句进原话）。
pub(crate) fn take<R: serde::de::DeserializeOwned>(args: &serde_json::Value) -> Result<R, Fail> {
    let v = if args.is_null() {
        serde_json::Value::Object(Default::default())
    } else {
        args.clone()
    };
    serde_json::from_value(v).map_err(|e| {
        Fail::new(
            "bad_args",
            crate::common::contract::malformed(&e.to_string()),
        )
    })
}

fn settle<C: Command>(got: Result<C::Reply, C::Err>) -> Outcome {
    let reply = got.map_err(Codes::into_fail)?;
    super::spec::wire::<_, Fail>(&reply).map(Some)
}

fn run_blocking<C: Blocking>(r: Request) -> Outcome {
    let req = take::<C::Req>(&r.args)?;
    let cx = Cx {
        tz: r.tz,
        until: r.until,
    };
    settle::<C>(C::run(req, &cx))
}

fn run_with_files<C: WithFiles>(r: Request, files: Files) -> Outcome {
    let req = take::<C::Req>(&r.args)?;
    let cx = Cx {
        tz: r.tz,
        until: r.until,
    };
    settle::<C>(C::run(req, &cx, files))
}

fn run_with_own<C: WithOwn>(r: Request, files: Files, own: &crate::platform::Own) -> Outcome {
    let req = take::<C::Req>(&r.args)?;
    let cx = Cx {
        tz: r.tz,
        until: r.until,
    };
    settle::<C>(C::run(req, &cx, files, own))
}

fn run_owning<C: Owning>(r: Request, own: &crate::platform::Own) -> Outcome {
    let req = take::<C::Req>(&r.args)?;
    let cx = Cx {
        tz: r.tz,
        until: r.until,
    };
    settle::<C>(C::run(req, &cx, own))
}

fn run_facing<C: Facing>(r: Request, face: &crate::platform::fs::Face) -> Outcome {
    let req = take::<C::Req>(&r.args)?;
    let cx = Cx {
        tz: r.tz,
        until: r.until,
    };
    settle::<C>(C::run(req, &cx, face))
}

fn run_async<C: Async>(r: Request) -> DataFut {
    Box::pin(async move {
        let req = take::<C::Req>(&r.args)?;
        let cx = Cx {
            tz: r.tz,
            until: r.until,
        };
        settle::<C>(C::run(req, cx).await)
    })
}

fn run_async_files<C: AsyncFiles>(r: Request, files: Files) -> DataFut {
    Box::pin(async move {
        let req = take::<C::Req>(&r.args)?;
        let cx = Cx {
            tz: r.tz,
            until: r.until,
        };
        settle::<C>(C::run(req, cx, files).await)
    })
}

fn run_async_owning<C: AsyncOwning>(r: Request, own: crate::platform::Own, hub: Hub) -> DataFut {
    Box::pin(async move {
        let req = take::<C::Req>(&r.args)?;
        let cx = Cx {
            tz: r.tz,
            until: r.until,
        };
        settle::<C>(C::run(req, cx, own, hub).await)
    })
}

fn run_async_own<C: AsyncOwn>(r: Request, own: crate::platform::Own) -> DataFut {
    Box::pin(async move {
        let req = take::<C::Req>(&r.args)?;
        let cx = Cx {
            tz: r.tz,
            until: r.until,
        };
        settle::<C>(C::run(req, cx, own).await)
    })
}

/// 阻塞档的一行登记。`summary` ＝ 一句话：这条命令做什么（住命令表，同别的命令）。
pub(crate) const fn blocking<C: Blocking>(summary: &'static str) -> CommandSpec {
    spec::<C>(summary, TypedRun::Blocking(run_blocking::<C>))
}

/// 阻塞、要文件管理面的一行登记（`files` ＝ 命令表递的那一份）。
pub(crate) const fn with_files<C: WithFiles>(summary: &'static str, files: Files) -> CommandSpec {
    spec::<C>(summary, TypedRun::BlockingFiles(run_with_files::<C>, files))
}

/// 阻塞、要文件管理面 ＋ 写句柄的一行登记。
pub(crate) const fn with_own<C: WithOwn>(summary: &'static str, files: Files) -> CommandSpec {
    spec::<C>(summary, TypedRun::BlockingOwn(run_with_own::<C>, files))
}

/// 阻塞、只要写句柄的一行登记。
pub(crate) const fn owning<C: Owning>(summary: &'static str) -> CommandSpec {
    spec::<C>(summary, TypedRun::BlockingOwning(run_owning::<C>))
}

/// 阻塞、要文件管理写句柄的一行登记（只有文件管理写面那一族用）。
pub(crate) const fn facing<C: Facing>(summary: &'static str) -> CommandSpec {
    spec::<C>(summary, TypedRun::BlockingFace(run_facing::<C>))
}

/// 异步、要文件管理面的一行登记。
pub(crate) const fn async_with_files<C: AsyncFiles>(
    summary: &'static str,
    files: Files,
) -> CommandSpec {
    spec::<C>(summary, TypedRun::AsyncFiles(run_async_files::<C>, files))
}

/// 异步、要写句柄 ＋ 枢纽那一跳的一行登记（`hub` ＝ 命令表递的那一份）。
pub(crate) const fn async_owning<C: AsyncOwning>(summary: &'static str, hub: Hub) -> CommandSpec {
    spec::<C>(summary, TypedRun::AsyncOwning(run_async_owning::<C>, hub))
}

/// 异步、只要写句柄的一行登记。
pub(crate) const fn async_own<C: AsyncOwn>(summary: &'static str) -> CommandSpec {
    spec::<C>(summary, TypedRun::AsyncOwn(run_async_own::<C>))
}

/// 异步档的一行登记。
pub(crate) const fn asynchronous<C: Async>(summary: &'static str) -> CommandSpec {
    spec::<C>(summary, TypedRun::Async(run_async::<C>))
}

const fn spec<C: Command>(summary: &'static str, run: TypedRun) -> CommandSpec {
    CommandSpec {
        name: C::NAME,
        summary,
        codes: <C::Err as Codes>::ALL,
        fields: &[],
        takes_input: C::TAKES_INPUT,
        run: Run::Typed(Typed {
            run,
            #[cfg(test)]
            shape: tests::shape_of::<C>,
        }),
    }
}

// 一条 typed 命令的形状（入参 · 回包的 TS 声明 ＋ 码表）与它的指纹：测试档才要，住测试树（`typed_tests.rs`）。
#[cfg(test)]
#[path = "../../../../tests/backend/stream/typed_tests.rs"]
pub(crate) mod tests;
