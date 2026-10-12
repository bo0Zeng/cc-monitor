//! 后端测试档里的 SFTP 台架（合成服务端，住拨号核心 `ssh_dial::sftp::rig`）：起会话之前先装上本后端交给拨号核心的那一套
//! （`dial::host`：写按部署那两处），别的原样转手。整份只在 `cfg(test)` 下存在。

pub(crate) use ssh_dial::sftp::rig::*;

/// 在台架上起一条后端的 SFTP 会话（先装本后端那一套）。
pub(crate) async fn session_on(
    fs: std::sync::Arc<std::sync::Mutex<Fs>>,
) -> ssh_dial::sftp::Session {
    crate::dial::host();
    ssh_dial::sftp::rig::session_on(fs).await
}
