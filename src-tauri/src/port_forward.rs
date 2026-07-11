//! F58 本地端口转发管理台(-L)。把远端机(或其内网)端口映到本机 `127.0.0.1:localPort`,经
//! cc-monitor 已有 SSH 连接隧道(复用 `connect_session` → 自动继承 F45 竞速/F56 跳板 +
//! `channel_open_direct_tcpip`,同 F56)。**每转发一条独立 SSH 会话**(存注册表保活)+ 本地
//! `TcpListener` + accept 循环;每进来的 TCP 连接开一条 direct-tcpip channel 双向 `copy`。
//!
//! v1 **即席**(不持久化 config)。**停** = abort accept 任务(drop listener 停接受,本地端口释放)
//! + `session.disconnect(ByApplication)` **主动断连**。注意:**仅 drop session Arc 关不掉连接**——
//! russh `Handle::drop` 是 no-op,且每条在飞连接的 `ChannelStream` 各持会话 sender 的 clone 保活
//! 会话(D 审计 russh 源码实证);故必须主动 Disconnect,让服务端关连接 → 在飞 direct-tcpip
//! channel 全死 → 各 per-conn `copy_bidirectional` 报错收尾。

use crate::ssh_source::{self, SshSession};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::task::AbortHandle;

/// 前端下发的转发定义。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardSpec {
    pub origin: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

/// 转发状态(列表展示)。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardStatus {
    pub id: String,
    pub origin: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    /// "running"（accept 循环存活）| "error"（循环退出）。v1 = accept 循环存活性,非 session 健康。
    pub state: String,
    pub error: Option<String>,
    pub conn_count: u64,
}

struct ForwardEntry {
    spec: ForwardSpec,
    /// 保活:drop 则 russh 关连接 → 隧道死(同 F56 jump_holders 教训)。russh Handle 不 Clone,
    /// 用 `Arc` 在 accept 循环/per-conn 任务/注册表间共享(`channel_open_direct_tcpip(&self)`）。
    _session: Arc<SshSession>,
    accept_task: AbortHandle,
    conn_count: Arc<AtomicU64>,
}

fn registry() -> &'static Mutex<HashMap<String, ForwardEntry>> {
    static R: OnceLock<Mutex<HashMap<String, ForwardEntry>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_id() -> String {
    static N: AtomicU64 = AtomicU64::new(1);
    format!("fwd-{}", N.fetch_add(1, Ordering::Relaxed))
}

/// 校验转发定义（纯逻辑,便于单测）。
fn validate_spec(spec: &ForwardSpec) -> Result<(), String> {
    if spec.local_port == 0 {
        return Err("本地端口必须 > 0".to_string());
    }
    if spec.remote_host.trim().is_empty() {
        return Err("远端 host 不能为空".to_string());
    }
    if spec.remote_port == 0 {
        return Err("远端端口必须 > 0".to_string());
    }
    Ok(())
}

/// F58：启动一条本地端口转发。绑 `127.0.0.1:localPort` → 经 origin 的 SSH 会话隧道到
/// `remoteHost:remotePort`。校验/查配置/连接/绑定任一失败 → Err（不进注册表）。返回转发 id。
#[tauri::command]
pub async fn start_forward(spec: ForwardSpec) -> Result<String, String> {
    validate_spec(&spec)?;
    let cfg = crate::load_remote_config_by_label(&spec.origin)
        .ok_or_else(|| format!("未找到远端配置: {}", spec.origin))?;
    // 起转发专用会话(自动继承 F45 竞速 / F56 跳板)。
    let (session, _fp) = ssh_source::connect_session(&cfg, None, None)
        .await
        .map_err(|e| format!("连接 {} 失败: {e}", spec.origin))?;
    let session = Arc::new(session); // russh Handle 不 Clone → Arc 共享
                                     // 仅绑 127.0.0.1(不对外暴露)。
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", spec.local_port))
        .await
        .map_err(|e| format!("绑定本地端口 127.0.0.1:{} 失败: {e}", spec.local_port))?;

    let conn_count = Arc::new(AtomicU64::new(0));
    let accept_session = Arc::clone(&session);
    let remote_host = spec.remote_host.clone();
    let remote_port = spec.remote_port;
    let counter = Arc::clone(&conn_count);
    // accept 循环:每进来的 TCP 连接开一条 direct-tcpip channel + 双向 copy。
    let task = tokio::spawn(async move {
        loop {
            let (mut tcp, _peer) = match listener.accept().await {
                Ok(v) => v,
                // 瞬时错误(ECONNABORTED / EMFILE fd 耗尽 等)不杀这条转发:短暂 backoff 后重试
                // (本地已 bound 的 listener 无「永久失败」态,故不 break)。避免忙等。
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    continue;
                }
            };
            counter.fetch_add(1, Ordering::Relaxed);
            let session = Arc::clone(&accept_session);
            let host = remote_host.clone();
            tokio::spawn(async move {
                match session
                    .channel_open_direct_tcpip(host, remote_port as u32, "127.0.0.1".to_string(), 0)
                    .await
                {
                    Ok(channel) => {
                        let mut chs = channel.into_stream();
                        let _ = tokio::io::copy_bidirectional(&mut tcp, &mut chs).await;
                    }
                    Err(_) => { /* 开隧道失败 → 丢弃本连接 */ }
                }
            });
        }
    });

    let id = next_id();
    registry().lock().unwrap().insert(
        id.clone(),
        ForwardEntry {
            spec,
            _session: session,
            accept_task: task.abort_handle(),
            conn_count,
        },
    );
    Ok(id)
}

/// F58：停止一条转发——abort accept 循环(drop listener 停接受、本地端口释放)+ `session.disconnect`
/// 主动断连(仅 drop session Arc 关不掉连接:Handle::drop no-op + 在飞连接持 sender clone 保活,
/// D 审计 russh 源码实证)→ 服务端关 → 在飞 channel 全死 → copy 收尾。移除注册表条目。
#[tauri::command]
pub async fn stop_forward(id: String) -> Result<(), String> {
    let entry = registry()
        .lock()
        .unwrap()
        .remove(&id)
        .ok_or_else(|| format!("未找到转发: {id}"))?;
    entry.accept_task.abort(); // 停止接受新连接(drop listener,本地端口释放)
    let _ = entry
        ._session
        .disconnect(
            russh::Disconnect::ByApplication,
            "cc-monitor: stop forward",
            "",
        )
        .await;
    Ok(())
}

/// F58：列出当前所有转发状态。
#[tauri::command]
pub async fn list_forwards() -> Vec<ForwardStatus> {
    let reg = registry().lock().unwrap();
    reg.iter()
        .map(|(id, e)| ForwardStatus {
            id: id.clone(),
            origin: e.spec.origin.clone(),
            local_port: e.spec.local_port,
            remote_host: e.spec.remote_host.clone(),
            remote_port: e.spec.remote_port,
            state: if e.accept_task.is_finished() {
                "error".to_string()
            } else {
                "running".to_string()
            },
            error: None,
            conn_count: e.conn_count.load(Ordering::Relaxed),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(lp: u16, rh: &str, rp: u16) -> ForwardSpec {
        ForwardSpec {
            origin: "o".into(),
            local_port: lp,
            remote_host: rh.into(),
            remote_port: rp,
        }
    }

    #[test]
    fn validate_spec_guards() {
        assert!(validate_spec(&spec(15432, "localhost", 5432)).is_ok());
        assert!(validate_spec(&spec(0, "h", 80)).is_err()); // 本地端口 0
        assert!(validate_spec(&spec(8080, "", 80)).is_err()); // 空 remote host
        assert!(validate_spec(&spec(8080, "  ", 80)).is_err()); // 纯空白 remote host
        assert!(validate_spec(&spec(8080, "h", 0)).is_err()); // 远端端口 0
    }

    #[test]
    fn next_id_monotonic_and_prefixed() {
        let a = next_id();
        let b = next_id();
        assert_ne!(a, b);
        assert!(a.starts_with("fwd-"));
        assert!(b.starts_with("fwd-"));
    }
}
