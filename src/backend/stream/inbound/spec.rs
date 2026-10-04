//! 一条入方向命令的登记形状。各族的命令表（`registry/`）只 use 这一份。

use crate::stream::wire::Request;

/// 处理器：拿走 [`Request`]，返回一个可以在**独立 task** 上跑的 future。
pub(super) type Handler = Box<dyn FnOnce(Request) -> BoxFut + Send>;
/// 同步阻塞处理器（见 [`Disposition::SpawnBlocking`]）。
pub(super) type BlockingHandler = Box<dyn FnOnce(Request) -> Outcome + Send>;
pub(super) type CmdResult = Result<Option<serde_json::Value>, (String, String)>;
/// 一条命令的结局（应答那一帧由它出）：成功的返回值 · 失败（码 ＋ 原话 ＋ 按码定形的 `data`）。
pub(crate) type Outcome = Result<Option<serde_json::Value>, Fail>;

/// 命令级失败。`data` 只给在协议文档里按码定了形的那几个码（多数码没有 ⇒ 应答里不出这一格）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fail {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) data: Option<serde_json::Value>,
}

impl From<(String, String)> for Fail {
    fn from((code, message): (String, String)) -> Self {
        Fail {
            code,
            message,
            data: None,
        }
    }
}
pub(super) type BoxFut =
    std::pin::Pin<Box<dyn std::future::Future<Output = CmdResult> + Send + 'static>>;

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
    Blocking(fn(Request) -> CmdResult),
    /// 同 [`Run::Blocking`]，只是失败可以带按码定形的 `data`（起会话那几条：选不了号时带上要的号与替代）。
    BlockingData(fn(Request) -> Outcome),
    Builtin,
}

/// 一条入方向命令的登记。**名字与处理器绑在同一个值里。**
///
/// `doc_anchor` / `codes` / `fields` **只被护栏读**（`protocol_doc_guard` 与入方向的
/// `structure_guards`）—— 那正是它们存在的理由：把「这条命令的契约面」写成**数据**，
/// 好让机检对着它比。非测试构建里它们确实没有读者，故精确 allow 而不是给整个类型开口子。
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy)]
pub(crate) struct CommandSpec {
    /// 线上命令名。
    pub name: &'static str,
    /// 它在 `src/doc/IPC-PROTOCOL.md` §10 里那一小节的标题**逐字**；`None` = 没有自己的小节
    /// （只要求名字出现在「入方向」节里）。**有 `fields` 就必须有小节** —— 由机检钉住。
    ///
    /// ⚠ **这是约定不是事实**：护栏只能查「标题在、字段名在它下面出现」，查不了写得对不对。
    /// 这不是新增局限，是把 `protocol_doc_guard` 早就登记过的那条局限**局部化**
    /// （从「§10 全节任意反引号」收到「本命令那一小节」），强度只升不降。
    pub(crate) doc_anchor: Option<&'static str>,
    /// 本命令**自己**可能回的 code。**协议级 code 不许出现在这里**（由 R4 的零命中钉住）。
    pub codes: &'static [&'static str],
    /// 本命令 `args` / `data` 的字段名。空 = 无载荷（如 `ping`）。
    ///
    /// ⚠ 它是**手写镜子**，本身就是一个新的漂移源 —— 所以必须再钉一层：
    /// 与真正的解析器/输出构造器实测对拍（`launch_fields_match_its_parser_and_output`）。
    /// 用一个手写清单去证明另一个手写清单是没有意义的。
    pub(crate) fields: &'static [&'static str],
    /// 这条命令**收不收入方向载荷**（CLI 面据此决定读不读 stdin）。
    ///
    /// # ★★ 为什么这是显式的，而不是从 `fields` 派生〔P4f 08-13 实测〕
    ///
    /// 原来 `cli_control::reads_stdin` 写成 `!fields.is_empty()`。那是个**代用品**：
    /// `fields` 的定义是「`args` **和** `data` 的字段名」，而 `kill`/`launch`/`resolve`
    /// 恰好都有输入、`ping` 恰好零字段 ⇒ 代用品当时全对。
    ///
    /// `bus-list` 是第一条**无输入、却有输出字段**的命令 ⇒ 代用品判它要读 stdin
    /// ⇒ **它挂住等一个永远不来的输入**。实测：`--ping` 120ms 回，`--bus-list` 6 秒
    /// 被掐死、一个字都没输出。而 CLI 面正是给第三方 skill 调的。
    ///
    /// ⚠ 这条病仓里**修过一次**（`--ping` 第一版无条件读 stdin，`cli_control` 的头注逐字：
    /// 「问『你活着吗』的那条命令，答案是挂住 —— 所有失败里最坏的一种」）。
    /// 它换了扇门回来，因为守它的判据是**恒真**的（`fields.is_empty()` ⟺ `!reads_stdin`
    /// 两边是同一个表达式，两个分支都不可能红）。
    ///
    /// ⇒ 改成每条命令自己说。真不真由**行为**判据验（`tests/e2e/backend-cc-bus.sh`：
    /// 声明无输入的命令，在 stdin 不关时必须秒回）。
    pub(crate) takes_input: bool,
    pub(crate) run: Run,
}
