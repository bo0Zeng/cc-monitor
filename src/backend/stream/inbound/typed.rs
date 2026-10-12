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

/// 一条命令自己可能回的那几个码（[`codes!`] 声明）。
pub(crate) trait Codes: Sized {
    /// 全部码，声明次序。
    const ALL: &'static [&'static str];
    /// 投出去的那一份。
    fn into_fail(self) -> Fail;
}

/// 同步阻塞的那一种（读盘 · 扫全库）：进阻塞池，开跑之后打不断。
pub(crate) trait Blocking: Command {
    fn run(req: Self::Req, cx: &Cx) -> Result<Self::Reply, Self::Err>;
}

/// 文件管理面（同 `faces::launch_face::Files`）：生产那一份住 `stream/inbound`（`LocalFiles`），由命令表那一行递进来
/// —— 写面只有 `stream/inbound/` 够得着（`readonly_guard` 第三层），本文件不点它的名。
pub(crate) type Files = &'static (dyn crate::assets::door::Door + Sync);

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

/// 真异步的那一种：交给独立 task，`cancel` 能在 await 点打断。
pub(crate) trait Async: Command {
    fn run(
        req: Self::Req,
        cx: Cx,
    ) -> impl std::future::Future<Output = Result<Self::Reply, Self::Err>> + Send + 'static;
}

/// 信封里除了 `args` 那几样：看的那一台的时区 · 分派那一层换算好的截止时刻。
pub(crate) struct Cx {
    pub(crate) tz: crate::common::time::Tz,
    #[allow(dead_code)] // 第一批 typed 命令里还没有装总期限的；起进程那几族换过来时用。
    pub(crate) until: Option<crate::platform::child::Until>,
}

/// 声明一条命令的错误枚举：每个变体带那一句（同 [`Fail::new`]：契约错的诊断跟在句子后面 ⇒ 拆进原话），码就是右边那个字面量。
/// 码表恒以 `bad_args` 打头、枚举里恒有 `BadArgs`：入参解不出来是每条 typed 命令都会回的（[`take`]），
/// 入参解得出、组合不对（「两个里恰好给一个」那一类）的由处理器自己回 `BadArgs`。
macro_rules! codes {
    ($(#[$m:meta])* $vis:vis enum $name:ident { $($(#[$vm:meta])* $var:ident = $code:literal),* $(,)? }) => {
        $(#[$m])*
        #[derive(Debug)]
        $vis enum $name {
            /// 入参组合不对（形状不对的那一种在解入参那一步就回了，不经这里）。
            #[allow(dead_code)] // 不是每条命令都有「组合不对」这一种；码表里它恒在（解入参那一步）。
            BadArgs(String),
            $($(#[$vm])* $var(String)),*
        }
        impl $crate::stream::inbound::typed::Codes for $name {
            const ALL: &'static [&'static str] = &["bad_args", $($code),*];
            fn into_fail(self) -> $crate::stream::inbound::spec::Fail {
                match self {
                    $name::BadArgs(said) => $crate::stream::inbound::spec::Fail::new("bad_args", said),
                    $($name::$var(said) => $crate::stream::inbound::spec::Fail::new($code, said)),*
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

#[derive(Clone, Copy)]
pub(super) enum TypedRun {
    Blocking(fn(Request) -> Outcome),
    /// 阻塞、要文件管理面（[`WithFiles`]）：那一份是命令表那一行递的。
    BlockingFiles(fn(Request, Files) -> Outcome, Files),
    /// 阻塞、要文件管理面 ＋ 写句柄（[`WithOwn`]）：写句柄由分派那一层递。
    BlockingOwn(fn(Request, Files, &crate::platform::Own) -> Outcome, Files),
    Async(fn(Request) -> DataFut),
}

/// 入参 ⇒ `R`。缺 ＝ `{}`；多一个键 · 缺一个必填 · 类型不对 ⇒ `bad_args`（解码器那一句进原话）。
fn take<R: serde::de::DeserializeOwned>(args: &serde_json::Value) -> Result<R, Fail> {
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
