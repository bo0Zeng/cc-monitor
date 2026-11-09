//! 一条入方向命令的登记形状。各族的命令表（`registry/`）只 use 这一份。

use crate::common::said::Said;
use crate::wire::FrameMake as _;
use crate::wire::Request;

/// 同步阻塞处理器（分派交进阻塞池：开跑之后打不断）。
pub type BlockingHandler = Box<dyn FnOnce(Request) -> Outcome + Send>;
/// 一条命令的结局（应答那一帧由它出）：成功的返回值 · 失败（码 ＋ 那一句 ＋ 原话 ＋ 按码定形的 `data`）。
pub type Outcome = Result<Option<serde_json::Value>, Said>;

/// 命令级失败：码 ＋ 那一句（收 `String`，也收已带原话的 [`Said`]；外层的码换掉里层的）。
/// 契约错的诊断跟在句子后面（`contract::malformed`）⇒ 拆开放进原话（往下交给只认一句的那一层时也不带它）。
pub fn fail(code: &str, said: impl Into<Said>) -> Said {
    let s: Said = said.into();
    let (sentence, diag) = crate::common::contract::parts(&s.said, None);
    let raw = s.raw.clone();
    Said {
        said: sentence,
        raw: diag,
        ..s
    }
    .coded(code)
    .and_raw(raw.as_deref())
}

/// 失败投出去的那两口（两个面都经这里：帧面的应答 · CLI 面的信封）。
pub trait Settle {
    /// 这次失败投出去的那一份。`cmd` 是命令名，进详情的「命令」那一项；码缺着 ⇒ 兜底码。
    fn settle(self, cmd: &str) -> crate::wire::detail::Failed;
    /// 这次失败的应答帧（[`Settle::settle`] 那一份装进 `reply`）。
    fn into_reply(self, id: String, cmd: &str) -> crate::wire::Frame;
}

impl Settle for Said {
    fn settle(self, cmd: &str) -> crate::wire::detail::Failed {
        let code = self.code_or_fallback().to_string();
        crate::wire::detail::failed(Some(cmd), &code, self.said, self.raw.as_deref(), self.data)
    }

    fn into_reply(self, id: String, cmd: &str) -> crate::wire::Frame {
        crate::wire::Frame::failed(id, self.settle(cmd))
    }
}
/// 异步处理器的 future（结局可带按码定形的 `data`）。
pub type DataFut = std::pin::Pin<Box<dyn std::future::Future<Output = Outcome> + Send + 'static>>;
/// 异步处理器（交给独立 task，`cancel` 能在 await 点打断）。
pub type DataHandler = Box<dyn FnOnce(Request) -> DataFut + Send>;

/// 一条登记命令交出去跑的那一份。按档分派只此一处（[`invoke`]）：分派 · 撤不动名单 · 文件门 · 枢纽 · 小成品重问 · CLI 面都经它。
pub enum Prepared {
    /// 真异步：交给独立 task，`cancel` 能在 await 点打断。
    Async(Box<dyn FnOnce(Request) -> DataFut + Send>),
    /// 同步阻塞：进阻塞池（或在调用方自己的阻塞线程上跑），开跑之后打不断。
    Blocking(Box<dyn FnOnce(Request) -> Outcome + Send>),
}

/// 分派那一层手里的能力：写自家状态的句柄（文件门 · 小成品重问那几处没有 ⇒ `None`）· 文件管理面 · 枢纽问这台自己那一跳。
/// 后两样是这台的单例，由入方向那一层（`stream::inbound::hands`）装上；本模块不点它们的名。
pub struct Hands<'a> {
    pub own: Option<&'a crate::platform::Own>,
    pub files: super::typed::Files,
    pub hub: super::typed::Hub,
}

/// 一条命令要的能力（[`super::typed::Handle::Needs`]）：处理器拿到的那一份（`Given`）· 怎么从 [`Hands`] 里取。
/// 取不到（要写句柄、手里没有）⇒ `None`：那条命令在这个入口交不出来。
pub trait Needs: 'static {
    type Given: Send + 'static;
    /// 要文件管理写句柄（改用户的文件）：判据拿它对写面登记的命令表，只 [`needs::Face`] · [`needs::FaceOwn`] 是。
    #[cfg(any(test, feature = "test-support"))]
    const FACE: bool = false;
    fn give(h: &Hands) -> Option<Self::Given>;
}

/// 能力的那几样（[`Needs`]）。
pub mod needs {
    use super::{Hands, Needs};
    use crate::wire::typed::{Files as Door, Hub as HubFn};

    /// 什么都不要。
    pub struct Nothing;
    /// 这台的文件管理面（只读 · 起之前预标信任那几样写经它）。
    pub struct Files;
    /// 写自家状态的句柄（`~/.cc-monitor` 底下）。
    pub struct Own;
    /// 文件管理面 ＋ 写自家状态的句柄。
    pub struct FilesOwn;
    /// 文件管理写句柄（改用户的文件）：分派那一层造一份、只活这一问。
    pub struct Face;
    /// 写自家状态的句柄 ＋ 枢纽问这台自己那一跳。
    pub struct OwnHub;
    /// 文件管理写句柄 ＋ 写自家状态的句柄（删会话：删用户那份记录，连带删自家那一条注解）。
    pub struct FaceOwn;

    impl Needs for Nothing {
        type Given = ();
        fn give(_: &Hands) -> Option<()> {
            Some(())
        }
    }
    impl Needs for Files {
        type Given = Door;
        fn give(h: &Hands) -> Option<Door> {
            Some(h.files)
        }
    }
    impl Needs for Own {
        type Given = crate::platform::Own;
        fn give(h: &Hands) -> Option<crate::platform::Own> {
            h.own.cloned()
        }
    }
    impl Needs for FilesOwn {
        type Given = (Door, crate::platform::Own);
        fn give(h: &Hands) -> Option<Self::Given> {
            Some((h.files, h.own?.clone()))
        }
    }
    #[expect(
        clippy::disallowed_types,
        reason = "那一种能力的定义本身：分派照它递文件管理写句柄"
    )]
    impl Needs for Face {
        type Given = crate::platform::fs::Face;
        #[cfg(any(test, feature = "test-support"))]
        const FACE: bool = true;
        fn give(_: &Hands) -> Option<crate::platform::fs::Face> {
            Some(super::issue_face())
        }
    }
    #[expect(
        clippy::disallowed_types,
        reason = "那一种能力的定义本身：分派照它递文件管理写句柄"
    )]
    impl Needs for FaceOwn {
        type Given = (crate::platform::fs::Face, crate::platform::Own);
        #[cfg(any(test, feature = "test-support"))]
        const FACE: bool = true;
        fn give(h: &Hands) -> Option<Self::Given> {
            Some((super::issue_face(), h.own?.clone()))
        }
    }
    impl Needs for OwnHub {
        type Given = (crate::platform::Own, HubFn);
        fn give(h: &Hands) -> Option<Self::Given> {
            Some((h.own?.clone(), h.hub))
        }
    }
}

/// 文件管理写句柄只在这一处造、只活这一问：[`needs::Face`] · [`needs::FaceOwn`] 那一份（帧面 · CLI 面），
/// 和资产域那扇门（`doors::LocalFiles` 问写的那几样，进程内的那一次分派）都经它。
#[expect(
    clippy::disallowed_methods,
    reason = "文件管理写句柄只在分派那一层造：按档装处理器的那一档 · 资产域那扇门的进程内分派都经这一处，句柄只活那一问"
)]
fn issue_face() -> crate::platform::fs::Face {
    crate::platform::fs::Face::issue()
}

/// 同 [`issue_face`]，句柄只活这一个闭包。
pub fn with_face<T>(f: impl FnOnce(&crate::platform::fs::Face) -> T) -> T {
    f(&issue_face())
}

/// 阻塞档的本体在调用方这条线程上跑完：本体是同步代码包成的 future，一次就完；
/// 真等到了什么（不该有）就停在这条线程上等它醒（不借 tokio 的 `block_on`：枢纽那一跳会在异步任务里同步问本机一条阻塞命令）。
pub fn finish<F: std::future::Future>(fut: F) -> F::Output {
    struct Wake(std::thread::Thread);
    impl std::task::Wake for Wake {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(std::sync::Arc::new(Wake(std::thread::current())));
    let mut cx = std::task::Context::from_waker(&waker);
    let mut fut = std::pin::pin!(fut);
    loop {
        if let std::task::Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::park();
    }
}

/// 按声明把能力交进去、装成 [`Prepared`]：分派 · 撤不动名单 · 文件门 · 枢纽 · 小成品重问 · CLI 面都经这一处。
/// 帧面硬臂那几条（[`super::typed::Typed::on_frame_arm`]）在分派的硬臂里，这里是 `None`；要写句柄、`hands` 里没有 ⇒ `None`。
pub fn invoke(run: super::typed::Typed, hands: &Hands) -> Option<Prepared> {
    (run.go)(hands)
}

/// typed 应答 → 应答 `data`（`serde_json::Value` 的对象按键排，与此前现拼的 `json!` 逐字节同形）。
/// 序列化不出（实际到不了：格都是普通值）⇒ `failed`，原话记日志。
pub fn wire<T: serde::Serialize>(t: &T) -> Result<serde_json::Value, Said> {
    serde_json::to_value(t).map_err(|e| {
        tracing::warn!("reply does not serialize: {e}");
        Said::new(
            "failed",
            crate::common::contract::malformed("reply does not serialize"),
        )
    })
}

/// 一条入方向命令的登记。**名字与处理器绑在同一个值里**；协议文档的命令那一半也从这里生成。
#[derive(Clone, Copy)]
pub struct CommandSpec {
    /// 线上命令名。
    pub name: &'static str,
    /// 一句话：这条命令做什么。
    pub summary: &'static str,
    /// 本命令**自己**可能回的 code。**协议级 code 不许出现在这里**（由 R4 的零命中钉住）。
    pub codes: &'static [&'static str],
    /// 这条命令**收不收入方向载荷**（CLI 面据此决定读不读 stdin）：那条命令的 [`super::typed::Command::TAKES_INPUT`]。
    /// 真不真由行为判据验（`tests/e2e/backend-cc-bus.sh`：声明无输入的命令，在 stdin 不关时必须秒回）。
    pub takes_input: bool,
    /// 应答里成品住的那几处 `(路径, 成品名)`（外层的成品排在它里面那件之前；路径写法同格目录，空串 ＝ 整份应答）。
    /// 请求信封的 `view` 只许点这几件（`stream::inbound::views`）；没写 ⇒ 空：带 `view` 一律拒。登记行上 [`CommandSpec::places`] 设。
    pub places: &'static [(&'static str, &'static str)],
    /// 阻塞档命令的总期限上限：发起方在请求信封里带它这一发愿意等多久（`within_ms`），分派按它收紧、装在执行这条命令的线程上
    /// （`stream::inbound::caps`）。没写 ⇒ 不装，发起方带的期限对它不起作用。登记行上 [`CommandSpec::capped`] 设；上限常量住那条命令旁。
    pub cap: Option<crate::platform::child::Deadline>,
    /// 上不上一次性 CLI 面（[`Cli`]）。登记行上 [`CommandSpec::stream_only`] · [`CommandSpec::ui_only`] 设，理由写在那一行。
    pub cli: Cli,
    /// 怎么跑 · 要什么能力 · 交出去跑的那一份（入参 · 回包 · 码表都从类型来，`typed.rs`）。
    pub run: super::typed::Typed,
}

/// 登记行上补的那几格（`typed::blocking::<X>("…").places(&[..])`）：一条命令的登记只住这一行，不按命令名另记一张表。
impl CommandSpec {
    /// 应答里成品住的那几处（[`CommandSpec::places`] 那一格）。
    #[must_use]
    pub const fn places(self, places: &'static [(&'static str, &'static str)]) -> Self {
        Self { places, ..self }
    }
    /// 总期限上限（[`CommandSpec::cap`] 那一格）。
    #[must_use]
    pub const fn capped(self, cap: crate::platform::child::Deadline) -> Self {
        Self {
            cap: Some(cap),
            ..self
        }
    }
    /// 一次性进程里结构上答不了 ⇒ 不上 CLI 面（[`Cli::StreamOnly`]）；`why` 写为什么。
    #[must_use]
    pub const fn stream_only(self, why: &'static str) -> Self {
        Self {
            cli: Cli::StreamOnly(why),
            ..self
        }
    }
    /// 答得了、只是除了界面没人用得着 ⇒ 暂不上 CLI 面（[`Cli::UiOnly`]）；`why` 写为什么。
    #[must_use]
    pub const fn ui_only(self, why: &'static str) -> Self {
        Self {
            cli: Cli::UiOnly(why),
            ..self
        }
    }
}

/// 一条命令上不上一次性 CLI 面（帧面硬臂那一档与占了 ccm 的词另判，见 `stream::cli::cli_exposed`）。
/// 不上的两种理由分开记：混在一起时后一种被读成前一种，第二个前端（只有一次性 CLI 与流两条路进后端）就够不着。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cli {
    /// 上（缺省）。
    Open,
    /// **一次性进程里结构上答不了**：答出来是假话（读的是常驻进程才有的状态：watcher · 中转监听 · 转发账 · 可达表），
    /// 或做的事活不过这个进程（换号重启要拿退出排空的票钉住常驻进程）。只收这一种理由。
    StreamOnly(&'static str),
    /// **答得出真话、只是除了界面没人用得着**：关于「调用方是谁」的产品判断，不是结构限制 ——
    /// 有新的调用方（第二个前端）要它，就挪出去。理由不许是「命令行那一侧直接敲 `ccm` 就是它」：那挡的是第二个**入口**，挡不住第二个**前端**。
    /// 2026-10-09 复核（第二个前端的审计 §4）：起新会话框三问 · tab 栏批量停 / 起 / 问样子 · 铸终端名 · 开终端那一串 · 各台搜索结果合并
    /// 九条放出（事实全在这台：tmux 名单现探、会话快照问一次重探一次、记录在不在现读、合并是纯计算 ⇒ 一次性进程答的与常驻那一个同样是真话）。
    /// 留着这一种的是 ↗ 拉前那一问（本机后端答 `session-window`，远端那一半问那台的 `session-terminals`）与装卸远端后端那两条。
    UiOnly(&'static str),
}
