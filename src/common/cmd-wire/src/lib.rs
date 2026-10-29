//! 帧命令两端必须对上的那几样（契约类：只放形状，不放判定）。
//!
//! - [`bookmarks`]：书签那两条（`bookmarks-read` · `-edit`）的入参与回包 · 目录归一的唯一一份；
//! - [`call`]：一条命令的身份（[`Call`]：名字 · 回包挂在入参类型上）与那一张 `calls!` 表 —— 命令身份唯一的声明；
//! - [`deploy`]：装 · 卸远端后端那两条（`deploy-install` · `deploy-uninstall`）的入参与回包；
//! - [`files`]：文件管理那一族（`files-*`）的入参与回包，路径走原始字节（[`files::WirePath`]）；
//! - [`front`]：↗ 切到终端的结局与浮层那一张（壳 · 本机后端 `session-window` 两个写家都调它，句子只在这一处取）；
//! - [`frame`]：后端发给客户端的帧（每种一个具名结构，记录那一格是类型参数）与一行解一帧的 [`frame::decode`]；
//! - [`live`]：活卡（`live_card` 帧）的线上形与归一流里一块的种类（[`live::BlockKind`]）；
//! - [`machine`]：「这一趟问的是哪台机器」的唯一类型 [`Machine`]（线上就是一个字符串，本机逐字 `"<local>"`）；
//! - [`dial`]：拨号核心写、读它的那一侧读的那几行（阶段 · ack · 收全的结果）；
//! - [`agent`]：线上一家 agent 的名字 [`AgentKind`]（kind；后端里面的路由名不上线）；
//! - [`attach`]：接常驻流那一段握手的两行（attach 行 · 回话）与新旧三态 [`attach::Relation`]；
//! - [`attachment`]：聊天里随一句发的附件（`session-attachment-*` 三条的入参与回包 · 记录里认出来的那一项）；
//! - [`watch`]：「这条流在看哪几个会话」（`stream-watch` 的入参与应答；attach 行带的就是同一份入参）；
//! - [`accounts`]：账号库那几条命令（`accounts-*`）的入参与成品；
//! - [`quota`]：`quota-read` 的回包（额度账的显示态 · 写好的几行 · 开窗那一判）；
//! - [`rotation`]：轮换那几条（`rotation-*`）的回包与失败带的 `data`；
//! - [`cells`]：成品格的两种标记类型（写好的字 [`cells::Words`] · 语气 [`cells::Tone`]）；
//! - [`session`] · [`needs`] · [`listing`] · [`record`]：手机问的那几条的回包（会话族 · 需手动 · 历史清单 · 读记录连同通用记录）；
//! - [`phone`]：手机嵌的 Rust 核心与 Kotlin 之间那一层（JNI 交上来的几样事 · 几个口的回值；Kotlin 读从这里生成的类）；
//! - [`plan`]：计划反查 `plan-files` 的回包（文件窗口按它解）；
//! - [`replies`]：「命令 ⇒ 回包类型」那一张表（判据对后端 · 手机生成的类都照它）；记录那一格的认不认得（[`replies::mend_records`]）；
//! - [`transfer`]：传输台开单那两条（`transfer-upload` · `-download`）：窗口说给 monitor 的 · monitor 补上拨号转给本机后端的，两层入参各一个类型；
//! - [`shapes`]：每条 typed 命令（入参 · 回包 · 错误码都从 Rust 类型来的那几条）的形状指纹。
//!   后端在 hello 里交出它自己那一份，客户端拿自己编进来的这一份比：对得上就直接按类型用，对不上就说「那台后端版本不同」。
//!   表由后端的判据从类型现算、写进 `src/shapes.rs`（过期即红），这里只是那份表本身。
//! - [`search`]：全文搜索那一族（一台的回包 · 几台合好的那一份）与几台合一份那一步（[`search::merge`]，本机后端与手机核心调同一份）；
//! - [`rewind`]：编辑已发的一句（发送时选怎么退）随那句带的那一份 · 发送那一下的选项（`session-rewind-options`）；
//! - [`queue`]：会话输入队那三条（`session-queue-*`）的入参与回执 · 一条会话的名单（也是 `changed {session_queue}` 的体）；
//! - [`tasks`]：`tasks-list` 的回包（一个会话的任务列表，也是 `changed {tasks}` 的体）；
//! - [`sync`]：带键集合的同步形（记号 · 一次答 · 客户端照答换的那一步）；
//! - [`fnv`]：FNV-1a（只求稳定、不防篡改）的唯一一份：形状指纹 · 记录摘要 · 骨架文件名 · 目录锁名等都调它。

pub mod accounts;
pub mod agent;
pub mod attach;
pub mod attachment;
pub mod batch;
pub mod board;
pub mod bookmarks;
pub mod bus;
pub mod call;
pub mod cells;
pub mod deploy;
pub mod dial;
pub mod file_class;
pub mod files;
#[cfg(any(test, feature = "ts"))]
pub mod fingerprint;
pub mod fnv;
pub mod frame;
pub mod frame_shapes;
pub mod front;
pub mod listing;
pub mod live;
pub mod machine;
pub mod needs;
pub mod phone;
pub mod plan;
pub mod queue;
pub mod quota;
pub mod record;
pub mod replies;
pub mod rewind;
pub mod rotation;
pub mod search;
pub mod session;
pub mod shapes;
pub mod sid;
pub mod sync;
pub mod tasks;
pub mod transfer;
pub mod watch;

pub use agent::AgentKind;
pub use call::Call;
pub use machine::{Machine, Route, LOCAL};
pub use sid::Sid;

/// 两端形状指纹对不上的那条 op 当「这台做不到」时用的码（客户端事前就拒、一个字节不发；给人看的那一句 `peerVersion.said.differs`）。
pub const SHAPE_DIFFERS: &str = "backend_differs";

/// 后端自己那一份设置文件此刻什么样（偏好 · 机器表同这一形）：没设过 · 读得懂 · 读不懂（读不懂也照答缺省那一份，不是失败）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "lowercase")]
pub enum FileState {
    Absent,
    Ok,
    Unreadable,
}

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
