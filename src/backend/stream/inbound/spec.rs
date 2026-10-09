//! 一条入方向命令的登记形状。各族的命令表（`registry/`）只 use 这一份。

use crate::common::said::IntoNote as _;
use crate::stream::wire::Request;

/// 处理器：拿走 [`Request`]，返回一个可以在**独立 task** 上跑的 future。
pub(super) type Handler = Box<dyn FnOnce(Request) -> BoxFut + Send>;
/// 同步阻塞处理器（见 [`Disposition::SpawnBlocking`]）。
pub(super) type BlockingHandler = Box<dyn FnOnce(Request) -> Outcome + Send>;
pub(super) type CmdResult = Result<Option<serde_json::Value>, (String, String)>;
/// 一条命令的结局（应答那一帧由它出）：成功的返回值 · 失败（码 ＋ 原话 ＋ 按码定形的 `data`）。
pub(crate) type Outcome = Result<Option<serde_json::Value>, Fail>;

/// 命令级失败。`data` 只给在协议文档里按码定了形的那几个码（多数码没有 ⇒ 应答里不出这一格）。
/// `raw` 是下层原话（子进程 stderr · 系统报错）：不进 `message`，进应答的 `detail`（[`crate::stream::detail::of`]）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fail {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) data: Option<serde_json::Value>,
    pub(crate) raw: Option<String>,
}

impl From<(String, String)> for Fail {
    fn from((code, message): (String, String)) -> Self {
        Fail {
            code,
            message,
            data: None,
            raw: None,
        }
    }
}

impl From<(&'static str, String)> for Fail {
    fn from((code, message): (&'static str, String)) -> Self {
        Fail::new(code, message)
    }
}

impl From<(&'static str, crate::common::said::Said)> for Fail {
    /// 码 ＋ 那一句与它的原话（原话进复制详情）。
    fn from((code, s): (&'static str, crate::common::said::Said)) -> Self {
        Fail::new(code, s.said).with_raw(s.raw.as_deref())
    }
}

impl From<Fail> for crate::common::said::Said {
    /// 失败不成应答、往下交给只认「一句 ＋ 原话」的那一层（日志 · 拼进别的句子）：码不跟着走。
    fn from(f: Fail) -> Self {
        crate::common::said::Said {
            said: f.message,
            raw: f.raw,
        }
    }
}

impl Fail {
    /// 码 ＋ 那一句（没有原话、没有 `data`）。
    pub(crate) fn new(code: &str, message: String) -> Fail {
        Fail {
            code: code.to_string(),
            message,
            data: None,
            raw: None,
        }
    }

    /// 带上下层原话（进复制详情；空 / 缺 ⇒ 不带）。
    pub(crate) fn with_raw(mut self, raw: Option<&str>) -> Fail {
        self.raw = raw
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(str::to_string);
        self
    }

    /// 这次失败**不成应答**、只成别处的一句（同 `Said::into_note`：原话记一行日志）。
    pub(crate) fn into_note(self) -> String {
        crate::common::said::Said::from(self).into_note()
    }

    /// 这次失败的应答帧（`cmd` 是命令名，进详情的「命令」那一项）。
    /// 这条命令有「码 → 句」表（[`crate::stream::said::reword`]）⇒ 句子换成表里那一句，处理器原来那句进原话。
    pub(crate) fn into_reply(
        mut self,
        id: String,
        cmd: &str,
        args: &serde_json::Value,
    ) -> crate::stream::wire::Frame {
        if let Some(said) = crate::stream::said::reword(cmd, args, &self.code) {
            let was = std::mem::replace(&mut self.message, said);
            if self.raw.is_none() && !was.trim().is_empty() {
                self.raw = Some(was);
            }
        }
        let detail = crate::stream::detail::of(Some(cmd), &self.code, self.raw.as_deref());
        crate::stream::wire::Frame::Reply {
            id,
            ok: false,
            code: Some(self.code),
            message: Some(self.message),
            detail: Some(detail),
            data: self.data,
        }
    }
}
pub(super) type BoxFut =
    std::pin::Pin<Box<dyn std::future::Future<Output = CmdResult> + Send + 'static>>;
/// 同 [`BoxFut`]，结局可带按码定形的 `data`。
pub(super) type DataFut =
    std::pin::Pin<Box<dyn std::future::Future<Output = Outcome> + Send + 'static>>;
/// 异步处理器（失败可带 `data`）。
pub(super) type DataHandler = Box<dyn FnOnce(Request) -> DataFut + Send>;

/// 一条命令**怎么跑**。三档，缺一不可：
///
/// - [`Run::Async`]：真异步，有 await 点 ⇒ `cancel` 能在那儿把它打断。
/// - [`Run::Blocking`]：**同步阻塞**（起进程 / 扫全库）⇒ 进 `spawn_blocking` 的专用线程池。
///   ⚠ 它**开跑之后打不断** —— 这一档不是「修好了取消」，是**停止假装能取消**：
///   `cancel` 命中它时回 `not_cancellable`，而不是撒一条 `cancelled` 的谎。
/// - [`Run::Builtin`]：`dispatch` 里的硬臂（`cancel` ＋链路四条）。它要 `replies`/`running`，
///   与别的命令签名不同 —— 硬塞进统一签名等于给每条命令都递上「自己发帧 / 碰登记表」的能力，
///   而那条性质今天是成立的，不该为了整齐拆掉。**但它仍要在注册表里占一行**，
///   否则「镜子 == 注册表」覆盖不到它。
#[derive(Clone, Copy)]
pub(crate) enum Run {
    Async(fn(Request) -> BoxFut),
    /// 同 [`Run::Async`]，只是失败可以带按码定形的 `data`（换号重启：起失败时带终端名）。
    AsyncData(fn(Request) -> DataFut),
    Blocking(fn(Request) -> CmdResult),
    /// 同 [`Run::Blocking`]，只是失败可以带按码定形的 `data`（起会话那几条：选不了号时带上要的号与替代）。
    BlockingData(fn(Request) -> Outcome),
    Builtin,
}

/// 载荷字段的向：请求里的（`args`）· 应答里的（`data`）· 两边都有。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Dir {
    In,
    Out,
    Both,
}

/// 一条命令的一个载荷字段：线上名 ＋ 向 ＋ 一句话说明（协议文档 `IPC-COMMANDS.md` 由它生成）。
#[derive(Clone, Copy, Debug)]
pub(crate) struct Field {
    pub(crate) name: &'static str,
    // 这两格的读者只有协议文档生成器（`protocol_doc_gen.rs`，测试档），它写进 `src/doc/IPC-COMMANDS.md`。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) dir: Dir,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) doc: &'static str,
}

/// 请求里的字段。
pub(crate) const fn arg(name: &'static str, doc: &'static str) -> Field {
    Field {
        name,
        dir: Dir::In,
        doc,
    }
}
/// 应答里的字段。
pub(crate) const fn out(name: &'static str, doc: &'static str) -> Field {
    Field {
        name,
        dir: Dir::Out,
        doc,
    }
}
/// 请求与应答里都有的字段。
pub(crate) const fn both(name: &'static str, doc: &'static str) -> Field {
    Field {
        name,
        dir: Dir::Both,
        doc,
    }
}

/// 一条入方向命令的登记。**名字与处理器绑在同一个值里**；协议文档的命令那一半也从这里生成。
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy)]
pub(crate) struct CommandSpec {
    /// 线上命令名。
    pub name: &'static str,
    /// 一句话：这条命令做什么。
    pub(crate) summary: &'static str,
    /// 本命令**自己**可能回的 code。**协议级 code 不许出现在这里**（由 R4 的零命中钉住）。
    pub codes: &'static [&'static str],
    /// 本命令 `args` / `data` 的字段。空 = 无载荷（如 `ping`）。
    pub(crate) fields: &'static [Field],
    /// 这条命令**收不收入方向载荷**（CLI 面据此决定读不读 stdin）。
    ///
    /// 显式写、不从 `fields` 派生：`bus-list` 无输入却有输出字段，派生的话 CLI 面会挂住等 stdin。
    /// 真不真由行为判据验（`tests/e2e/backend-cc-bus.sh`：声明无输入的命令，在 stdin 不关时必须秒回）。
    pub(crate) takes_input: bool,
    pub(crate) run: Run,
}

impl CommandSpec {
    /// 全部字段名（`args` 与 `data` 合起来，登记次序）。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn field_names(&self) -> impl Iterator<Item = &'static str> {
        self.fields.iter().map(|f| f.name)
    }
}
