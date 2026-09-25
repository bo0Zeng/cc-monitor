//! **拨号应答的客户端**：在一条**交给它的**双工管子上，读完拨号代理（后端 `--dial`）的
//! 阶段行与 ack，之后要么把管子原样交回（字节流），要么读一行收全的 exec 结果，要么逐行读转发计数。
//!
//! # 🔴 通信层成员 `COMM-LAYER-MEMBER`〔C2 · 2026-09-24，`设计/05 §13.7`〕
//!
//! 这一枚标记是**盘上那一侧**的凭据（登记那一侧在
//! `tests/bridge/comm_boundary_registry_tests.rs` 的 `REGISTERED`，两向集合相等）。
//! 盖上它 = **上锁**：本文件从此被 `C1`–`C5` ＋ `X1`–`X6` 十一条一起管着。
//! 用户 `Q6` 裁「甲」（先承认传输面脏、C2 洗干净再收）—— 这一份就是从 `ssh_source.rs` 里洗出来的那段传输。
//!
//! # 它为什么是通信层成员
//!
//! 它原来埋在 `ssh_source.rs` 里（那时连 `russh` 握手也在界面进程里跑）。`设计/05 §13` 把 SSH 的全部活
//! 搬进后端的拨号代理之后，界面这一侧与 SSH 有关的**传输**只剩「读代理的应答」这一件 —— 就是本文件。
//! `05 §2` 那四样它只碰两样：**流**（交给它的管子）与**载荷**（ack 之后的字节它不看）。
//!
//! - `C4`：不读盘、不读环境变量 —— 行长上限由调用方给；
//! - `C5`：不起进程、不绑端口 —— 起代理进程的是宿主（`dial_host.rs`，不是成员），
//!   本文件只收那两根管子；
//! - `X2`：零期限字面量 —— 等 ack 要多久由宿主定、由宿主执行；
//! - `C1`：公开面只有 `ConnectStage` / `Ack` / `Captured` / `LinkError` / `handshake` 这类传输词。
//!
//! # 线上形状（与后端 `dial/mod.rs` 头注同一份，住那边，这里不抄第二份）
//!
//! ack 之前零到多行 `{"stage":{…}}` → 恰好一行 ack → 之后按用法（〔SR1b〕`files` 是一问一答，读应答那一行用 [`reply_line`]）。
//! 🔴 **老代理出声**：ack 的 `uses` 不含所请求的用法 ⇒ [`LinkError::TooOld`]，不去解后面那些字节。

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

/// 连接分阶段事件（测试连接时经 Tauri Channel 流给前端做泳道日志）。
///
/// 后端拨号代理在 ack 之前逐行报出来（请求带 `stages=true` 时），本侧**原样反序列化**进这个类型 ——
/// 形状两侧逐字段相同（后端 `dial::Stage`），对拍判据住 `ssh_link_tests.rs`。
/// 阶段取 russh 能干净观测的粒度 —— 不含 KEX（HostKey 触发即隐含 TCP ＋ KEX 已过）。
/// 〔C2〕搬自 `ssh_source.rs`：类型名与线上形状一个字没动（前端生成物 `ConnectStage.ts` 只有这段注释变了）。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConnectStage {
    /// 某地址开始拨号（TCP ＋ 握手）。
    Dialing { endpoint: String },
    /// 某地址握手到 host key 校验（带指纹；隐含 TCP ＋ KEX 已过）。
    HostKey {
        endpoint: String,
        fingerprint: String,
    },
    /// 某地址连接失败（reason = 粗分类 ＋ 原始错误）。
    Failed { endpoint: String, reason: String },
    /// 竞速胜出地址（其余在飞的已丢弃）。
    Won { endpoint: String },
    /// 鉴权结果。
    Auth { ok: bool, detail: Option<String> },
    /// 连接就绪（握手 ＋ 鉴权全过）。
    Established,
}

/// 代理的 ack（后端 `dial::DialAck`）。v1 代理没有 `endpoint`/`v`/`uses` ⇒ 缺省值。
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Ack {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    /// 握手时看到的 host key 指纹（失败时也尽量带上）。
    #[serde(default)]
    pub fingerprint: Option<String>,
    /// 竞速胜出的地址（`host:port`）。
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub v: u32,
    #[serde(default)]
    pub uses: Vec<String>,
}

/// 收全用法（`capture`）的那一行（后端 `dial::Captured`）。
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    pub stdout: String,
    pub stderr: String,
    /// `None` = 远端没送 exit-status。**不许把 `None` 当成 0**。
    pub exit_status: Option<u32>,
}

/// 读应答时出的错。每一形的下一步不同，所以不压成一句话。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// 代理一行都没回就走了（它自己的 stderr 上有原因）。
    Silent,
    /// 代理说拨不通。`fingerprint` = 握手时看到的指纹（TOFU 展示要它）。
    Refused {
        why: String,
        fingerprint: Option<String>,
    },
    /// 代理不认所请求的用法 —— 它比界面老。
    TooOld { wanted: String, v: u32 },
    /// 读到的不是约定的形状。
    Garbled(String),
    /// 一行超过调用方给的上限（对面不是我们的代理，或坏了）。
    LineTooLong(u64),
    /// 管子读错。
    Io(String),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkError::Silent => {
                f.write_str("拨号代理一个字节都没回就走了（它自己的 stderr 上有原因）")
            }
            LinkError::Refused { why, .. } => write!(f, "拨号代理拨不通: {why}"),
            LinkError::TooOld { wanted, v } => write!(
                f,
                "本机后端太旧（拨号应答 v{v}，不认 `{wanted}`）—— 重装本机后端后再试"
            ),
            LinkError::Garbled(s) => write!(f, "拨号代理回的不是约定的形状: {s}"),
            LinkError::LineTooLong(n) => {
                write!(f, "拨号代理回了一行 {n} 字节，超过上限 —— 拒收")
            }
            LinkError::Io(e) => write!(f, "读拨号代理失败: {e}"),
        }
    }
}

/// 有上限地读一行（不含换行）。EOF 且没读到东西 ⇒ `None`。
///
/// ⚠ **必须写成 `.take(` 这个点调用**：`byte_cap_registry` 认的是这一形。
async fn read_line_capped<R: AsyncBufRead + Unpin>(
    r: &mut R,
    cap: u64,
) -> Result<Option<String>, LinkError> {
    let mut line = String::new();
    let n = (&mut *r)
        .take(cap + 1)
        .read_line(&mut line)
        .await
        .map_err(|e| LinkError::Io(e.to_string()))?;
    if n == 0 {
        return Ok(None);
    }
    if n as u64 > cap && !line.ends_with('\n') {
        return Err(LinkError::LineTooLong(n as u64));
    }
    Ok(Some(line.trim_end_matches(['\n', '\r']).to_string()))
}

/// **读完握手**：阶段行逐条交给 `on_stage`，ack 回来。`want` = 这次请求的用法（`stream` / `capture` / `forward`）。
///
/// ack 说拨不通 ⇒ [`LinkError::Refused`]；ack 的 `uses` 不含 `want` ⇒ [`LinkError::TooOld`]。
/// 返回之后 `r` 停在 ack 那一行之后 —— 调用方接着按用法读（或把整条管子交出去）。
pub async fn handshake<R: AsyncBufRead + Unpin>(
    r: &mut R,
    want: &str,
    cap: u64,
    on_stage: &mut (dyn FnMut(ConnectStage) + Send),
) -> Result<Ack, LinkError> {
    loop {
        let Some(line) = read_line_capped(r, cap).await? else {
            return Err(LinkError::Silent);
        };
        let v: serde_json::Value = serde_json::from_str(&line)
            .map_err(|e| LinkError::Garbled(format!("{e}（原文 {line:?}）")))?;
        if let Some(stage) = v.get("stage") {
            let stage: ConnectStage = serde_json::from_value(stage.clone())
                .map_err(|e| LinkError::Garbled(format!("阶段行 {e}（原文 {line:?}）")))?;
            on_stage(stage);
            continue;
        }
        let ack: Ack = serde_json::from_value(v)
            .map_err(|e| LinkError::Garbled(format!("ack {e}（原文 {line:?}）")))?;
        if !ack.ok {
            return Err(LinkError::Refused {
                why: ack.error.unwrap_or_else(|| "(代理没说原因)".to_string()),
                fingerprint: ack.fingerprint,
            });
        }
        if !ack.uses.iter().any(|u| u == want) {
            return Err(LinkError::TooOld {
                wanted: want.to_string(),
                v: ack.v,
            });
        }
        return Ok(ack);
    }
}

/// `capture` 用法：ack 之后那一行。
pub async fn captured<R: AsyncBufRead + Unpin>(r: &mut R, cap: u64) -> Result<Captured, LinkError> {
    let Some(line) = read_line_capped(r, cap).await? else {
        return Err(LinkError::Silent);
    };
    serde_json::from_str(&line).map_err(|e| LinkError::Garbled(format!("{e}（原文 {line:?}）")))
}

/// `forward` 用法：下一条「接进了第 n 条连接」。管子关了 ⇒ `None`（转发收工了）。
pub async fn accepted<R: AsyncBufRead + Unpin>(
    r: &mut R,
    cap: u64,
) -> Result<Option<u64>, LinkError> {
    let Some(line) = read_line_capped(r, cap).await? else {
        return Ok(None);
    };
    let v: serde_json::Value = serde_json::from_str(&line)
        .map_err(|e| LinkError::Garbled(format!("{e}（原文 {line:?}）")))?;
    v.get("accepted")
        .and_then(serde_json::Value::as_u64)
        .map(Some)
        .ok_or_else(|| LinkError::Garbled(format!("转发计数行没有 accepted（原文 {line:?}）")))
}

/// 〔SR1b〕`files` 用法：ack 之后一问一答，这里读**一行应答**（JSON 对象）。管子关了 ⇒ [`LinkError::Silent`]。
pub async fn reply_line<R: AsyncBufRead + Unpin>(
    r: &mut R,
    cap: u64,
) -> Result<serde_json::Value, LinkError> {
    let Some(line) = read_line_capped(r, cap).await? else {
        return Err(LinkError::Silent);
    };
    serde_json::from_str(&line).map_err(|e| LinkError::Garbled(format!("{e}（原文 {line:?}）")))
}

#[cfg(test)]
#[path = "../../../tests/bridge/ssh_link_tests.rs"]
mod tests;
