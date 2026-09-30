//! 要求住址：`设计/90 §0.5.3` ⑰「文件窗口成独立包 `src/frontend/filewin/`（V72；先把通道客户端 · `copy_table` · `spawn_managed` 抽成共享 crate）」
//! ＋ `60 §2.3`「窗口是又一个前端：窗口 ↔ monitor 一条只绑本机回环的通道」。
//!
//! 通道两端（monitor 里的宿主 ＋ 进程外的前端）编同一份线上词汇与帧：从前两端都编自 `monitor_lib`，
//! 文件窗口独立成包之后改由本 crate 交给两边。住在这里的只有**搬字节**的那几份（零业务判断，`05` C1–C5）：
//! - 通信层成员（原地住 `src/comms/inward/`）：[`origin`] · [`chan::wire`] · [`chan::client`] · [`chan::router`]；
//! - 非成员两份：[`chan::dial`]（外部前端按交接件拨号）· [`chan::handoff`]（交接件的形状 ＋ 绑回环口、造钥匙）。
//! monitor 那一侧的生产句柄（按 `origin` 转给后端客户端 · 传输台那一口）不在这里，仍住壳里 `chan/host.rs`。

#[path = "../../../comms/inward/origin.rs"]
pub mod origin;

pub mod chan;
