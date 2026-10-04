//! 通道 · **交接件**：外部前端连上通道要的全部东西（地址 ＋ 钥匙 ＋ 帧长上限）的形状。
//!
//! 绑 `127.0.0.1:0`、造钥匙、定帧长与认证等待时长那几件是宿主的事（`C4` / `C5` 不许通信层做），住壳里 `chan/host.rs`；
//! 这里只留两端都要认的那个形状（`filewin-contract` 的开窗种子带着它）。

use super::wire::Key;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// 外部前端连上通道要的全部东西：地址 ＋ 钥匙 ＋ 帧长上限。
///
/// 它整份过一次进程边界（走子进程的 stdin，见模块头注），所以能序列化。
#[derive(Clone, Serialize, Deserialize)]
pub struct Handoff {
    /// 回环上的那个口。
    pub addr: SocketAddr,
    /// 那把钥匙。
    pub key: Key,
    /// 帧头 / 帧体各自的字节上限（两端必须同一个数）。
    pub frame: usize,
}

impl std::fmt::Debug for Handoff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handoff")
            .field("addr", &self.addr)
            .field("key", &self.key)
            .field("frame", &self.frame)
            .finish()
    }
}
