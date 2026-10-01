//! 通道 · **交接件与绑口**（非成员；原住壳里 `chan/host.rs`，随通道编进 `chan-core`，逐字搬来）。
//!
//! 绑 `127.0.0.1:0`、造钥匙、定帧长与认证等待时长 —— 那几件 `C4` / `C5` 不许通信层成员做（理由住壳里 `chan/host.rs` 头注那张表）。
//! 两个用户：monitor 起它自己那个口（`chan/host.rs::start`）· 文件窗口的判据起一个挂着合成句柄的口。

use super::router::{self, Backends, Ended, Terms};
use super::wire::Key;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

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

/// 造一把钥匙：两枚 v4 UUID（各 122 位来自 OS 随机源）拼成 64 位十六进制。
pub fn mint_key() -> Key {
    Key(format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    ))
}

/// 在回环上起一个通道口，把每条接进来的连接交给路由器。回交接件。
///
/// `frame` 与 `hello_within` 由调用方给（它们是策略值）。判据用它起一个挂着合成句柄的口。
///
/// # Errors
///
/// 回环口绑不上。
pub async fn start_with(
    backends: Arc<dyn Backends>,
    key: Key,
    frame: usize,
    hello_within: Duration,
) -> std::io::Result<Handoff> {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let addr = listener.local_addr()?;
    let terms = Terms {
        key: key.clone(),
        frame,
        hello_within,
    };
    tokio::spawn(async move {
        loop {
            // `accept` 是内核事件，不是定时器；单次失败（fd 顶满之类）不许把整个口带走。
            let (stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("通道：accept 失败（{e}），这一条放过，口照开");
                    continue;
                }
            };
            let terms = terms.clone();
            let backends = Arc::clone(&backends);
            tokio::spawn(async move {
                match router::serve(stream, terms, backends).await {
                    Ended::Left => {}
                    Ended::Denied => tracing::warn!("通道：一条连接没过认证，已关"),
                    Ended::Broken(why) => tracing::warn!("通道：一条连接坏了，已关（{why}）"),
                }
            });
        }
    });
    Ok(Handoff { addr, key, frame })
}
