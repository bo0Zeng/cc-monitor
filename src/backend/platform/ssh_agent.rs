//! **连本机的 ssh-agent** —— 拨号核心（`ssh-dial` 的 `connect.rs`）没配私钥路径时经 [`LocalAgent`] 用它鉴权。
//!
//! 两个平台两种接法，这正是本层存在的理由：
//! - Unix：`SSH_AUTH_SOCK` 指的那个 Unix 域套接字（由 `russh` 的 `connect_env` 读那个变量）；
//! - Windows：Win10+/Win11 自带的 OpenSSH agent 监听**固定的命名管道**（Windows 上 `SSH_AUTH_SOCK` 不是标准）。
//!
//! ⚠ Windows 那一臂只在交叉编译上编得过，**零真机读数**。

use copy_core::copy_text;
use russh::keys::agent::client::AgentClient;

/// 连上的 agent（两个平台的流类型抹成同一个；形状住拨号核心）。
pub(crate) type Agent = ssh_dial::env::AgentClient;

/// 本机的 ssh-agent，交给拨号核心（[`ssh_dial::env::Agent`]）。
pub(crate) struct LocalAgent;

impl ssh_dial::env::Agent for LocalAgent {
    fn connect<'a>(
        &'a self,
        sock: Option<&'a str>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Agent, copy_core::said::Said>> + Send + 'a>,
    > {
        Box::pin(connect(sock))
    }
}

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
