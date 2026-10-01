//! ssh-agent 这一族的平台差异〔阶段 H：；原是 `dial_host.rs::agent_sock` 里那一句 `cfg!(unix)`〕。

/// 本机 ssh-agent 经不经 `SSH_AUTH_SOCK` 那个套接字找（Unix 是）；Windows 上 agent 是固定的命名管道，界面不交。
pub const AGENT_VIA_SOCKET_ENV: bool = cfg!(unix);
