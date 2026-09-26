//! F58 本地端口转发管理台(-L)。把远端机(或其内网)端口映到本机 `127.0.0.1:localPort`。
//!
//! # 🔴 通信层成员 `COMM-LAYER-MEMBER`〔C2 · 2026-09-24，`设计/05 §13.7`〕
//!
//! 端口转发是纯字节搬运（本机口 ↔ 远端口），C2 之后本文件只剩命令面与转发账：绑口、开隧道、
//! 读配置全在宿主 `dial_host.rs`、绑口与隧道全在本机常驻后端里 ⇒ 十一条现打全绿，照 `Q6`「甲」收回。
//! 登记那一侧在 `tests/bridge/comm_boundary_registry_tests.rs` 的 `REGISTERED`（两向集合相等）。
//!
//! 〔C2 · `设计/05 §13` → SR1a〕**绑口与隧道都不在界面进程里了**：每条转发是本机常驻后端里的一条链路
//! （`use: forward`），由后端绑本机回环口、每接进一条连接开一条 direct-tcpip、
//! 双向对拷（竞速 / 跳板 / 鉴权与别的链路同一份、同一台远端复用同一条 SSH 连接，住后端 `dial/`）。
//! 本文件只剩三个命令面 ＋ 一张转发账：起 = 宿主开链路、等它回 ack（口绑好了才算起来）；
//! 停 = 丢掉那条链路（后端随之放掉本地口、收掉在飞隧道）；列 = 账上每一条的状态与累计连接数（每接一条报一行）。
//!
//! 〔墓碑 —— 原头注要点逐字：「**每转发一条独立 SSH 会话**(存注册表保活)+ 本地 `TcpListener` + accept 循环」
//!  「**仅 drop session Arc 关不掉连接** —— russh `Handle::drop` 是 no-op……故必须主动 Disconnect」。〕
//! 那两件今天在后端里：链路被关就丢 listener ＋ 收掉在飞隧道（SSH 连接是共用的，不断 —— 最后一条链路走了它才断）；
//! 界面这侧丢掉链路就是那一下。
//!
//! v1 **即席**(不持久化 config)。

use crate::copy_table::copy_text;
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
    /// 读计数行的那个任务：它手里握着那条链路。**停 = abort 它** ⇒ 链路被丢 ⇒ 后端那一侧收工。
    /// 任务自己结束（链路收尾了 / 远端断了）⇒ 状态读成 `error`。
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
        return Err(copy_text("rsPortForward.spec.localPort", &[]));
    }
    if spec.remote_host.trim().is_empty() {
        return Err(copy_text("rsPortForward.spec.remoteHost", &[]));
    }
    if spec.remote_port == 0 {
        return Err(copy_text("rsPortForward.spec.remotePort", &[]));
    }
    Ok(())
}

/// F58：启动一条本地端口转发。代理那侧绑好 `127.0.0.1:localPort`（口绑不上 / 连不上 / 鉴权失败都在 ack 里）
/// 才算起来 —— 任一失败 → Err（不进账）。返回转发 id。
#[tauri::command]
pub async fn start_forward(spec: ForwardSpec) -> Result<String, String> {
    validate_spec(&spec)?;
    // 查那台远端的配置、起代理 —— 都是宿主的事（`C4` 读配置 · `C5` 起进程），本文件只交一个机器标签。
    let mut link = crate::dial_host::forward(
        &crate::origin::Origin(spec.origin.clone()),
        spec.local_port,
        &spec.remote_host,
        spec.remote_port,
    )
    .await?;
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

/// F58：停止一条转发 —— 丢掉那条链路（后端随之放掉本地口、收掉在飞隧道）。移除账上条目。
#[tauri::command]
pub async fn stop_forward(id: String) -> Result<(), String> {
    let entry = registry()
        .lock()
        .unwrap()
        .remove(&id)
        .ok_or_else(|| copy_text("rsPortForward.stop.notFound", &[("id", &id.to_string())]))?;
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
