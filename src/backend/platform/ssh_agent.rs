//! **连本机的 ssh-agent** —— 拨号代理（`dial/connect.rs`）没配私钥路径时用它鉴权。
//!
//! 两个平台两种接法，这正是本层存在的理由：
//! - Unix：`SSH_AUTH_SOCK` 指的那个 Unix 域套接字（由 `russh` 的 `connect_env` 读那个变量）；
//! - Windows：Win10+/Win11 自带的 OpenSSH agent 监听**固定的命名管道**（Windows 上 `SSH_AUTH_SOCK` 不是标准）。
//!
//! 〔搬自界面侧 `stream_source::authenticate_via_agent`〔散文墓碑〕 的连接那一半 —— 那一份只有 Windows 臂，
//! 非 Windows 直接报「不支持 ssh-agent」；拨号搬进后端之后两个平台都有了。〕
//!
//! ⚠ Windows 那一臂只在交叉编译上编得过，**零真机读数**。

use copy_core::copy_text;
use russh::keys::agent::client::{AgentClient, AgentStream};

/// 连上的 agent（两个平台的流类型抹成同一个）。
pub(crate) type Agent = AgentClient<Box<dyn AgentStream + Send + Unpin + 'static>>;

/// Windows OpenSSH agent 的命名管道。
#[cfg(windows)]
const OPENSSH_AGENT_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";

/// 连本机 ssh-agent。连不上 ⇒ `Err(人话)`，**不猜别的位置**。
///
/// `sock` = 界面进程交过来的 agent 套接字路径（Unix）。常驻后端活得比任何一个界面都长，
/// 自己身上那份 `SSH_AUTH_SOCK` 可能早就不指向活的 agent 了 ⇒ 给了就用给的；没给才读本进程的环境。
/// Windows 上 agent 是固定的命名管道，这个参数不用。
/// 连不上：那一句（不带原话）＋ 下层原话另带（调用方放进复制详情）。
pub(crate) async fn connect(sock: Option<&str>) -> Result<Agent, copy_core::said::Said> {
    use copy_core::said::Said;
    #[cfg(unix)]
    {
        match sock.map(str::trim).filter(|s| !s.is_empty()) {
            Some(path) => AgentClient::connect_uds(path)
                .await
                .map(AgentClient::dynamic)
                .map_err(|e| {
                    Said::with_raw(
                        copy_text("beSshAgent.connect.givenSocket", &[("path", path)]),
                        e,
                    )
                }),
            None => AgentClient::connect_env()
                .await
                .map(AgentClient::dynamic)
                .map_err(|e| Said::with_raw(copy_text("beSshAgent.connect.authSock", &[]), e)),
        }
    }
    #[cfg(windows)]
    {
        let _ = sock;
        AgentClient::connect_named_pipe(OPENSSH_AGENT_PIPE)
            .await
            .map(AgentClient::dynamic)
            .map_err(|e| {
                Said::with_raw(
                    copy_text("beSshAgent.connect.pipe", &[("pipe", OPENSSH_AGENT_PIPE)]),
                    e,
                )
            })
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = sock;
        Err(Said::from(copy_text("beSshAgent.connect.unsupported", &[])))
    }
}
