//! F58 本地端口转发管理台(-L)。把远端机(或其内网)端口映到本机 `127.0.0.1:localPort`。
//!
//! 〔C2 · `设计/05 §13`〕**绑口与隧道都不在界面进程里了**：每条转发起一个拨号代理
//! （`<本机后端> --dial`，`use: forward`），由它绑本机回环口、每接进一条连接开一条 direct-tcpip、
//! 双向对拷（竞速 / 跳板 / 鉴权与别的链路同一份，住后端 `dial/`）。本文件只剩三个命令面 ＋ 一张转发账：
//! 起 = 宿主起代理、等它回 ack（口绑好了才算起来）；停 = 丢掉那条链路（代理子进程随之收掉 ⇒
//! 本地口释放、在飞隧道全断）；列 = 账上每一条的状态与累计连接数（代理每接一条报一行）。
//!
//! 〔墓碑 —— 原头注要点逐字：「**每转发一条独立 SSH 会话**(存注册表保活)+ 本地 `TcpListener` + accept 循环」
//!  「**仅 drop session Arc 关不掉连接** —— russh `Handle::drop` 是 no-op……故必须主动 Disconnect」。〕
//! 那两件今天在代理进程里：它收到 stdin EOF 就丢 listener ＋ 主动断开 SSH；界面这侧丢掉链路就是那一下。
//!
//! v1 **即席**(不持久化 config)。

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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ForwardStatus {
    pub id: String,
    pub origin: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    /// "running"（代理那条链路还在）| "error"（代理收工了）。
    pub state: String,
    pub error: Option<String>,
    // **C03 大整数策略**：量纲是**累计连接数**，`Number.MAX_SAFE_INTEGER` = 2^53-1 条
    // ——单条转发即使每秒 1000 个连接也要 28.5 万年才到 ⇒ f64 精度足够。
    // 不是 Option ⇒ 不需要 `| null`（那条守卫只管 Option）。
    #[cfg_attr(test, ts(type = "number"))]
    pub conn_count: u64,
}

struct ForwardEntry {
    spec: ForwardSpec,
    /// 读代理计数行的那个任务：它手里握着那条链路（代理子进程）。**停 = abort 它** ⇒ 链路被丢 ⇒ 代理收工。
    /// 任务自己结束（代理退了 / 远端断了）⇒ 状态读成 `error`。
    pump: AbortHandle,
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

/// F58：启动一条本地端口转发。代理那侧绑好 `127.0.0.1:localPort`（口绑不上 / 连不上 / 鉴权失败都在 ack 里）
/// 才算起来 —— 任一失败 → Err（不进账）。返回转发 id。
#[tauri::command]
pub async fn start_forward(spec: ForwardSpec) -> Result<String, String> {
    validate_spec(&spec)?;
    let cfg = crate::load_remote_config_by_label(&spec.origin)
        .ok_or_else(|| format!("未找到远端配置: {}", spec.origin))?;
    let mut link =
        crate::dial_host::forward(&cfg, spec.local_port, &spec.remote_host, spec.remote_port)
            .await
            .map_err(|e| format!("连接 {} 失败: {e}", spec.origin))?;
    let conn_count = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&conn_count);
    let origin = spec.origin.clone();
    // 代理每接进一条连接报一行 —— 计数照抄它的数；管子关了 = 这条转发没了。
    let task = tokio::spawn(async move {
        loop {
            match link.next_accepted().await {
                Ok(Some(n)) => counter.store(n, Ordering::Relaxed),
                Ok(None) => {
                    tracing::warn!("端口转发 [{origin}] 的代理收工了（远端断开 / 进程退出）");
                    break;
                }
                Err(e) => {
                    tracing::warn!("端口转发 [{origin}] 的计数读不下去了：{e}");
                    break;
                }
            }
        }
    });
    let id = next_id();
    registry().lock().unwrap().insert(
        id.clone(),
        ForwardEntry {
            spec,
            pump: task.abort_handle(),
            conn_count,
        },
    );
    Ok(id)
}

/// F58：停止一条转发 —— 丢掉那条链路（代理子进程随之收掉：本地口释放、在飞隧道全断）。移除账上条目。
#[tauri::command]
pub async fn stop_forward(id: String) -> Result<(), String> {
    let entry = registry()
        .lock()
        .unwrap()
        .remove(&id)
        .ok_or_else(|| format!("未找到转发: {id}"))?;
    entry.pump.abort();
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
            state: if e.pump.is_finished() {
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
#[path = "../../../tests/bridge/port_forward_tests.rs"]
mod tests;
