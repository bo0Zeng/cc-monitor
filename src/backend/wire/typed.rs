//! **typed 命令**：入参 · 回包 · 错误码各是一个 Rust 类型（第 8 批）；本体只有一种签名（减法第一刀）。
//!
//! 一条命令 = 一个零大小的标记类型，实现 [`Command`]（名字 · 三个类型）＋ [`Handle`]（它要的能力 · 本体）。
//! 登记表里一行 `typed::blocking::<X>("一句话")` / `typed::asynchronous::<X>("一句话")`：阻塞还是异步在这一行说。
//! - 入参只在 [`take`] 这一处从 `args` 解成 `Req`（`deny_unknown_fields`）：形状不对 ⇒ `bad_args`，原话里带解码器那一句；处理器里没有 `args.get`。
//! - 码表从错误枚举来（[`codes!`] 声明的那一份），登记的 `codes` 就是它，不另写。
//! - 字段不手写：协议参考与形状指纹都从类型生成（测试档，经 ts-rs）。
//! - 它要的能力（文件管理面 · 写句柄 · 文件管理写句柄 · 枢纽那一跳）写在 [`Handle::Needs`]，分派只在 [`super::spec::invoke`] 一处按它交进 `cx.needs`。
//! - 要碰本连接的票表与应答通道的那几条（帧面硬臂）登记成 `typed::frame::<X>("一句话")`、只实现 [`Command`]：
//!   分派在 `dispatch` 那条硬臂，硬臂那一发经 [`answer`] 一处（入参按类型收 · 回包按类型写 · 失败照码表）。

pub(crate) use super::spec::{needs, Needs};
use super::spec::{CommandSpec, Fail, Outcome};
use crate::wire::Request;

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

/// [`Codes::Data`] 的约束：测试档要算得出 TS 形状（`tests::DataTys`）；发布二进制里它什么都不要求。
#[cfg(test)]
pub(crate) trait DataTypes: tests::DataTys {}
#[cfg(test)]
impl<T: tests::DataTys> DataTypes for T {}
#[cfg(not(test))]
pub(crate) trait DataTypes {}
#[cfg(not(test))]
impl<T> DataTypes for T {}

/// [`Codes::Data`] 那一格：[`Codes::DATA`] 里每个码都带 `T` 这一形。
pub(crate) struct Each<T>(std::marker::PhantomData<T>);

/// 不收入参的那几条命令的入参：缺 · `null` · `{}` 都认，多一个键 ⇒ `bad_args`。
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
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
    /// 带 `data` 的那几个码（声明次序）。读它的只有形状指纹与协议参考（测试档）。
    #[cfg_attr(not(test), allow(dead_code))]
    const DATA: &'static [&'static str];
    /// 那几个码的 `data` 类型：元组，与 [`Codes::DATA`] 逐位对上（[`Each`] ＝ 每个码都带同一形）。测试档拿它算形状指纹与协议参考。
    type Data: DataTypes;
}

/// 同 [`of_fail`]，交成闭包（`map_err` 那一格）。
pub(crate) fn of_fail_with<E: Codes>(fallback: fn(String) -> E) -> impl Fn(Fail) -> E {
    move |f| of_fail(f, fallback)
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

/// 同 [`of_fail`]，那一句照这一条自己的「码 → 句」表换（`said` 按落定的码给句子；`None` ⇒ 照下层那句）：下层那句进原话。
/// 码表外的码先落 `fallback` 那一格再查表（同投出去时的码）。
pub(crate) fn of_fail_said<E: Codes>(
    f: Fail,
    fallback: fn(String) -> E,
    said: impl FnOnce(&str) -> Option<String>,
) -> E {
    let code = if E::ALL.contains(&f.code.as_str()) {
        f.code.clone()
    } else {
        fallback(String::new()).into_fail().code
    };
    let f = match said(&code) {
        Some(s) => f.retold(s),
        None => f,
    };
    of_fail(f, fallback)
}

/// 文件管理面（同 `faces::launch_account_face::Files`）：生产那一份住 `stream/inbound`（`LocalFiles`），由命令表那一行递进来
/// —— 写面只有 `stream/inbound/` 够得着（`readonly_guard` 第三层），本文件不点它的名。
pub(crate) type Files = &'static (dyn crate::common::door::Door + Sync);

/// 两台之间那几件的枢纽问**这台自己**的那一跳（生产 = `doors::hub_here`，由命令表那一行递进来，同 [`Files`]）：
/// 按引起这一趟的请求时区与写句柄造一份。
pub(crate) type Hub = fn(&crate::Tz, &crate::platform::Own) -> std::sync::Arc<dyn Here>;

/// 这台自己的那几条内层命令（生产 = `stream/inbound/doors.rs::LocalFrames`：本进程 `REGISTRY` 的 `run`，限那几条）。
/// 资产域的枢纽（`assets::hub`）经它问这台自己；实现在命令表那一层，这里只是那一口的形状。
pub(crate) trait Here: Send + Sync {
    fn ask(
        &self,
        cmd: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, (String, String)>;
    /// 看的人那一台的时区（引起这一趟的那条请求的信封 `tz`）：问别台时原样转过去，那台回的「几点」照它写。
    fn viewer_tz(&self) -> crate::Tz;
}

/// 一条命令的本体：只有这一种签名。它要的能力写在 [`Handle::Needs`]（`spec::needs` 里那几样），
/// 分派那一层（[`super::spec::invoke`]）按它把能力交进 `cx.needs`；阻塞还是异步在登记那一行说（[`blocking`] · [`asynchronous`]）。
/// 阻塞档的本体是同步代码包在 `async move` 里：分派把它放进阻塞池、在那条线程上跑完（开跑之后打不断）。
pub(crate) trait Handle: Command {
    /// 它要的能力（`spec::needs::Nothing` · `Files` · `Own` · `FilesOwn` · `Face` · `OwnHub`）。
    type Needs: Needs;
    fn run(
        req: Self::Req,
        cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Self::Reply, Self::Err>> + Send + 'static;
}

/// 信封里除了 `args` 那几样：看的那一台的时区 · 分派那一层换算好的截止时刻。
pub(crate) struct Cx<N: Needs = needs::Nothing> {
    pub(crate) tz: crate::common::time::Tz,
    #[allow(dead_code)] // 第一批 typed 命令里还没有装总期限的；起进程那几族换过来时用。
    pub(crate) until: Option<crate::platform::child::Until>,
    /// 分派那一层按 [`Handle::Needs`] 交进来的那几样。
    pub(crate) needs: N::Given,
}

impl<N: Needs> Cx<N> {
    /// 拆成信封那两样（不带能力的那一份，交给只认 `&Cx` 的下层）＋ 交进来的能力。
    pub(crate) fn split(self) -> (Cx, N::Given) {
        (
            Cx {
                tz: self.tz,
                until: self.until,
                needs: (),
            },
            self.needs,
        )
    }
}

/// 声明一条命令的错误枚举：每个变体带那一句（同 [`Fail::new`]：契约错的诊断跟在句子后面 ⇒ 拆进原话），码就是右边那个字面量。
/// 按码定了 `data` 形状的那几个码写成 `Var = "code" => 类型`：变体多带那一份，投出去时进失败信封的 `data`。
/// 码表恒以 `bad_args` 打头、枚举里恒有 `BadArgs`：入参解不出来是每条 typed 命令都会回的（[`take`]），
/// 入参解得出、组合不对（「两个里恰好给一个」那一类）的由处理器自己回 `BadArgs`。
macro_rules! codes {
    ($(#[$m:meta])* $vis:vis enum $name:ident { $($body:tt)* }) => {
        $crate::wire::typed::codes!(@munch [$(#[$m])*] [$vis] $name [] [] [] [] [] $($body)*);
    };
    // 带 `data` 的那一种。
    (@munch $attrs:tt $vis:tt $name:ident [$($vars:tt)*] [$($arms:tt)*] [$($codes:tt)*] [$($of:tt)*] [$($dat:tt)*]
        $(#[$vm:meta])* $var:ident = $code:literal => $data:ty $(, $($rest:tt)*)?) => {
        $crate::wire::typed::codes!(@munch $attrs $vis $name
            [$($vars)* $(#[$vm])* $var(String, $data),]
            [$($arms)* $name::$var(said, data) => $crate::wire::spec::Fail {
                data: ::serde_json::to_value(&data).ok(),
                ..$crate::wire::spec::Fail::new($code, said)
            },]
            [$($codes)* $code,]
            [$($of)*]
            [$($dat)* ($code, $data)]
            $($($rest)*)?);
    };
    // 只有那一句的那一种。
    (@munch $attrs:tt $vis:tt $name:ident [$($vars:tt)*] [$($arms:tt)*] [$($codes:tt)*] [$($of:tt)*] [$($dat:tt)*]
        $(#[$vm:meta])* $var:ident = $code:literal $(, $($rest:tt)*)?) => {
        $crate::wire::typed::codes!(@munch $attrs $vis $name
            [$($vars)* $(#[$vm])* $var(String),]
            [$($arms)* $name::$var(said) => $crate::wire::spec::Fail::new($code, said),]
            [$($codes)* $code,]
            [$($of)* ($code, $var)]
            [$($dat)*]
            $($($rest)*)?);
    };
    (@munch [$(#[$m:meta])*] [$vis:vis] $name:ident [$($vars:tt)*] [$($arms:tt)*] [$($codes:tt)*] [$(($ocode:literal, $ovar:ident))*] [$(($dcode:literal, $dty:ty))*]) => {
        $(#[$m])*
        #[derive(Debug)]
        $vis enum $name {
            /// 入参组合不对（形状不对的那一种在解入参那一步就回了，不经这里）。
            #[allow(dead_code)] // 不是每条命令都有「组合不对」这一种；码表里它恒在（解入参那一步）。
            BadArgs(String),
            $($vars)*
        }
        impl $crate::wire::typed::Codes for $name {
            const ALL: &'static [&'static str] = &["bad_args", $($codes)*];
            #[allow(unreachable_patterns, clippy::match_single_binding)]
            fn of_code(code: &str, said: String) -> Result<Self, String> {
                Ok(match code {
                    "bad_args" => $name::BadArgs(said),
                    $($ocode => $name::$ovar(said),)*
                    _ => return Err(said),
                })
            }
            fn into_fail(self) -> $crate::wire::spec::Fail {
                match self {
                    $name::BadArgs(said) => $crate::wire::spec::Fail::new("bad_args", said),
                    $($arms)*
                }
            }
            const DATA: &'static [&'static str] = &[$($dcode),*];
            type Data = ($($dty,)*);
        }
    };
}
pub(crate) use codes;

/// 怎么跑：登记那一行说。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum How {
    /// 同步阻塞（读盘 · 扫全库 · 起进程）：进阻塞池，开跑之后打不断（`cancel` 命中回 `not_cancellable`）。
    Blocking,
    /// 真异步：交给独立 task，`cancel` 能在 await 点打断。
    Async,
    /// 帧面硬臂（要碰本连接的票表 · 链路表 · 应答通道）：分派在 `dispatch` 那条硬臂，经 [`answer`] / [`answer_async`] 一处收发。
    /// 只在帧面（CLI 面不派生）、不进 [`super::spec::invoke`]。
    Frame,
    /// 同 [`How::Frame`]，只是那条硬臂把本体交进阻塞池（[`answer_blocking`]）：开跑之后打不断。
    FrameBlocking,
}

/// 一条登记命令跑起来的那一份（类型擦掉之后）。
#[derive(Clone, Copy)]
pub(crate) struct Typed {
    pub(crate) how: How,
    /// 要写句柄（自家状态 · 文件管理写面）：手里没有写句柄的入口（文件门 · 小成品重问）交不出它。
    pub(crate) writes: bool,
    /// 拿不拿文件管理写句柄（[`super::spec::needs::Face`]）。
    pub(crate) face: bool,
    /// 按声明把能力交进去、装成交出去跑的那一份（帧面硬臂 ⇒ `None`）。
    pub(crate) go: fn(&super::spec::Hands) -> Option<super::spec::Prepared>,
    /// 生成类型与形状指纹用（测试档）。
    #[cfg(test)]
    pub(crate) shape: fn() -> tests::Shape,
}

impl Typed {
    /// 开跑之后打不打得断（阻塞档打不断）：读它的只有形状指纹（撤不动的算进指纹，`cmd_wire::shapes::UNCANCELLABLE` 由它写）。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn blocking(&self) -> bool {
        matches!(self.how, How::Blocking | How::FrameBlocking)
    }
    /// 帧面硬臂那一档（只在帧面：CLI 面不派生、不进 [`super::spec::invoke`]）。
    pub(crate) fn on_frame_arm(&self) -> bool {
        matches!(self.how, How::Frame | How::FrameBlocking)
    }
    /// 写不写（后端自家状态 · 文件管理面改用户的文件；分派要递写句柄的那几条）。
    pub(crate) fn writes(&self) -> bool {
        self.writes || self.face
    }
}

/// 找出是哪个字段解码失败（不泄露原值）。
fn find_culprit_key<R: serde::de::DeserializeOwned>(v: &serde_json::Value) -> Option<String> {
    let map = v.as_object()?;
    if map.len() == 1 {
        return map.keys().next().cloned();
    }
    for (k, _) in map {
        let mut sub = map.clone();
        sub.remove(k);
        match serde_json::from_value::<R>(serde_json::Value::Object(sub)) {
            Ok(_) => return Some(k.clone()),
            Err(e2) => {
                let s2 = e2.to_string();
                if s2.contains(&format!("missing field `{k}`")) {
                    return Some(k.clone());
                }
            }
        }
    }
    None
}

/// 契约错只回「哪一格 · 期望什么形」，不回调用方给的原值；原值只进本机日志。
pub(crate) fn sanitize_de_error<R: serde::de::DeserializeOwned>(
    v: &serde_json::Value,
    raw: &str,
) -> String {
    let raw_clean = raw.split(" at line ").next().unwrap_or(raw).trim();
    if raw_clean.starts_with("invalid type:")
        || raw_clean.starts_with("invalid value:")
        || raw_clean.starts_with("unknown variant ")
    {
        let key = find_culprit_key::<R>(v);
        if let Some((_, exp)) = raw_clean.split_once(", expected ") {
            let exp = exp.trim();
            if let Some(k) = key {
                if raw_clean.starts_with("unknown variant ") {
                    format!("`{k}`: unknown variant, expected {exp}")
                } else {
                    format!("`{k}`: expected {exp}")
                }
            } else {
                format!("expected {exp}")
            }
        } else if let Some(k) = key {
            format!("`{k}`: invalid value")
        } else {
            "invalid value".to_string()
        }
    } else {
        raw_clean.to_string()
    }
}

/// 入参 ⇒ `R`。缺 ＝ `{}`；多一个键 · 缺一个必填 · 类型不对 ⇒ `bad_args`。
/// 契约错只回「哪一格 · 期望什么形」，不回调用方给的原值；原值只进本机日志。
pub(crate) fn take<R: serde::de::DeserializeOwned>(args: &serde_json::Value) -> Result<R, Fail> {
    let v = if args.is_null() {
        serde_json::Value::Object(Default::default())
    } else {
        args.clone()
    };
    serde_json::from_value::<R>(v.clone()).map_err(|e| {
        let raw = e.to_string();
        tracing::warn!("contract: bad_args deserialization failed: {raw}");
        let detail = sanitize_de_error::<R>(&v, &raw);
        Fail::new("bad_args", crate::common::contract::malformed(&detail))
    })
}

/// 本体那一份 ⇒ 结局：失败照码表投；零出参的回包（`()` ⇒ `null`）不带 `data`。
fn settle<C: Command>(got: Result<C::Reply, C::Err>) -> Outcome {
    let reply = got.map_err(Codes::into_fail)?;
    let v = super::spec::wire::<_, Fail>(&reply)?;
    Ok((!v.is_null()).then_some(v))
}

/// 按声明交能力 → 解入参 → 本体 → 回包（阻塞档在调用方那条阻塞线程上跑完）。
fn go<C: Handle>(h: &super::spec::Hands, how: How) -> Option<super::spec::Prepared> {
    let needs = <C::Needs as Needs>::give(h)?;
    let run = move |r: Request| async move {
        let req = take::<C::Req>(&r.args)?;
        let cx = Cx::<C::Needs> {
            tz: r.tz,
            until: r.until,
            needs,
        };
        settle::<C>(C::run(req, cx).await)
    };
    Some(match how {
        How::Blocking => {
            super::spec::Prepared::Blocking(Box::new(move |r| super::spec::finish(run(r))))
        }
        How::Async => super::spec::Prepared::Async(Box::new(move |r| Box::pin(run(r)))),
        How::Frame | How::FrameBlocking => return None,
    })
}

fn go_blocking<C: Handle>(h: &super::spec::Hands) -> Option<super::spec::Prepared> {
    go::<C>(h, How::Blocking)
}

fn go_async<C: Handle>(h: &super::spec::Hands) -> Option<super::spec::Prepared> {
    go::<C>(h, How::Async)
}

fn go_frame(_: &super::spec::Hands) -> Option<super::spec::Prepared> {
    None
}

/// 帧面硬臂那一档（[`How::Frame`]）的一发：入参按类型收 → `run` → 应答帧（成功带回包；失败照码表投，详情里带命令名）。
/// `dispatch` 的硬臂调本体时都经这里（本体签名里只有类型，没有 `args`）。
pub(crate) fn answer<C: Command>(
    id: &str,
    args: &serde_json::Value,
    run: impl FnOnce(C::Req) -> Result<C::Reply, C::Err>,
) -> crate::wire::Frame {
    match take::<C::Req>(args).and_then(|req| settle::<C>(run(req))) {
        Ok(data) => crate::wire::Frame::Reply(crate::wire::Reply {
            id: id.to_string(),
            ok: true,
            code: None,
            message: None,
            detail: None,
            data,
        }),
        Err(f) => f.into_reply(id.to_string(), C::NAME),
    }
}

/// 一发失败的应答帧（码表里那一格）：硬臂那几条自己发应答的（`link-data` 成功那一下由上行泵回）失败时经它。
pub(crate) fn refuse<C: Command>(id: &str, e: C::Err) -> crate::wire::Frame {
    e.into_fail().into_reply(id.to_string(), C::NAME)
}

/// 帧面硬臂那一档交给通用 spawn 那一路的一份（异步：`cancel` 能在 await 点打断）：入参按类型收 → 本体 future → 结局。
/// 本体拿到的是入参与信封那两样（[`Cx`]），硬臂要的本连接那几样由它在闭包里带进去。
pub(crate) fn answer_async<C, F, Fut>(run: F) -> super::spec::DataHandler
where
    C: Command,
    F: FnOnce(C::Req, Cx) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<C::Reply, C::Err>> + Send + 'static,
{
    Box::new(move |r: Request| {
        Box::pin(async move {
            let req = take::<C::Req>(&r.args)?;
            let cx = Cx {
                tz: r.tz,
                until: r.until,
                needs: (),
            };
            settle::<C>(run(req, cx).await)
        })
    })
}

/// 同 [`answer_async`]，入参已在硬臂里就地收好（要在读循环里当场生效的那一半先做了，如 `stream-watch` 换名单）：只剩等结局那一半交出去。
pub(crate) fn answer_later<C, Fut>(fut: Fut) -> super::spec::DataHandler
where
    C: Command,
    Fut: std::future::Future<Output = Result<C::Reply, C::Err>> + Send + 'static,
{
    Box::new(move |_r: Request| Box::pin(async move { settle::<C>(fut.await) }))
}

/// 同 [`answer_async`]，本体交进阻塞池（[`How::FrameBlocking`]：开跑之后打不断）。
pub(crate) fn answer_blocking<C, F>(run: F) -> super::spec::BlockingHandler
where
    C: Command,
    F: FnOnce(C::Req, Cx) -> Result<C::Reply, C::Err> + Send + 'static,
{
    Box::new(move |r: Request| {
        let req = take::<C::Req>(&r.args)?;
        let cx = Cx {
            tz: r.tz,
            until: r.until,
            needs: (),
        };
        settle::<C>(run(req, cx))
    })
}

/// 帧面硬臂那一档的一行登记（本体在 `dispatch` 那条硬臂里经 [`answer`] / [`answer_async`] 调）。
pub(crate) const fn frame<C: Command>(summary: &'static str) -> CommandSpec {
    spec::<C>(summary, How::Frame, false, false, go_frame)
}

/// 同 [`frame`]，那条硬臂把本体交进阻塞池（[`answer_blocking`]）。
pub(crate) const fn frame_blocking<C: Command>(summary: &'static str) -> CommandSpec {
    spec::<C>(summary, How::FrameBlocking, false, false, go_frame)
}

/// 阻塞档的一行登记。`summary` ＝ 一句话：这条命令做什么（住命令表，同别的命令）。
pub(crate) const fn blocking<C: Handle>(summary: &'static str) -> CommandSpec {
    spec::<C>(
        summary,
        How::Blocking,
        <C::Needs as Needs>::WRITES,
        <C::Needs as Needs>::FACE,
        go_blocking::<C>,
    )
}

/// 异步档的一行登记。
pub(crate) const fn asynchronous<C: Handle>(summary: &'static str) -> CommandSpec {
    spec::<C>(
        summary,
        How::Async,
        <C::Needs as Needs>::WRITES,
        <C::Needs as Needs>::FACE,
        go_async::<C>,
    )
}

const fn spec<C: Command>(
    summary: &'static str,
    how: How,
    writes: bool,
    face: bool,
    go: fn(&super::spec::Hands) -> Option<super::spec::Prepared>,
) -> CommandSpec {
    CommandSpec {
        name: C::NAME,
        summary,
        codes: <C::Err as Codes>::ALL,
        takes_input: C::TAKES_INPUT,
        run: Typed {
            how,
            writes,
            face,
            go,
            #[cfg(test)]
            shape: tests::shape_of::<C>,
        },
    }
}

// 一条 typed 命令的形状（入参 · 回包的 TS 声明 ＋ 码表）与它的指纹：测试档才要，住测试树（`typed_tests.rs`）。
#[cfg(test)]
#[path = "../../../tests/backend/wire/typed_tests.rs"]
pub(crate) mod tests;
