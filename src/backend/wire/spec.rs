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
    /// 要文件管理写句柄（改用户的文件）：判据拿它对写面登记的命令表，只 [`needs::Face`] 是。
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
    impl Needs for Face {
        type Given = crate::platform::fs::Face;
        #[cfg(any(test, feature = "test-support"))]
        const FACE: bool = true;
        fn give(_: &Hands) -> Option<crate::platform::fs::Face> {
            Some(super::issue_face())
        }
    }
    impl Needs for OwnHub {
        type Given = (crate::platform::Own, HubFn);
        fn give(h: &Hands) -> Option<Self::Given> {
            Some((h.own?.clone(), h.hub))
        }
    }
}

/// 文件管理写句柄只在这一处造、只活这一问：[`needs::Face`] 那一份（帧面 · CLI 面），
/// 和资产域那扇门（`doors::LocalFiles` 问写的那几样，进程内的那一次分派）都经它。
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
    /// 怎么跑 · 要什么能力 · 交出去跑的那一份（入参 · 回包 · 码表都从类型来，`typed.rs`）。
    pub run: super::typed::Typed,
}
