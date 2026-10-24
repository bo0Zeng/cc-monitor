//! **拨号应答的客户端**：在一条**交给它的**双工管子上，读完拨号代理（后端那条链路）的
//! 阶段行与 ack，之后要么把管子原样交回（字节流），要么读一行收全的 exec 结果，要么逐行读转发计数。
//!
//! # 它为什么是通信层成员
//!
//! 它原来埋在 `stream_source/` 里（那时连 `russh` 握手也在界面进程里跑）。把 SSH 的全部活
//! 搬进后端的拨号代理之后，界面这一侧与 SSH 有关的**传输**只剩「读代理的应答」这一件 —— 就是本文件。
//! 那四样它只碰两样：**流**（交给它的管子）与**载荷**（ack 之后的字节它不看）。
//!
//! - `C4`：不读盘、不读环境变量 —— 行长上限由调用方给；
//! - `C5`：不起进程、不绑端口 —— 起代理进程的是宿主（`dial_host.rs`，不是成员），
//!   本文件只收那两根管子；
//! - `X2`：零期限字面量 —— 等 ack 要多久由宿主定、由宿主执行；
//! - `C1`：公开面只有 `LinkError` / `handshake` / `captured` 这类传输词。
//!
//! # 线上形状（契约 `cmd_wire::dial`，拨号核心写的就是那一份）
//!
//! ack 之前零到多行 `{"stage":{…}}`（[`StageLine`]：拨号阶段交 `on_stage`；「过了一步」只在请求带 `progress` 时有，交 `on_step`）
//! → 恰好一行 ack（[`DialAck`]）→ 之后按用法。
//! 🔴 **老代理出声**：ack 的 `uses` 不含所请求的用法 ⇒ [`LinkError::TooOld`]，不去解后面那些字节。

use cmd_wire::dial::{Captured, DialAck, Stage, StageLine};
use copy_core::copy_text;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

/// 读应答时出的错。每一形的下一步不同，所以不压成一句话。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// 代理一行都没回就走了（它自己的 stderr 上有原因）。
    Silent,
    /// 代理说拨不通。`fingerprint` = 握手时看到的指纹（TOFU 展示要它）。
    Refused {
        why: String,
        fingerprint: Option<String>,
        /// 库里记下的那一枚（[`DialAck::expected_fingerprint`] 原样）。
        expected: Option<String>,
        /// 没拨成的原因码（[`DialAck::reason`] 原样）。
        reason: Option<cmd_wire::machine::Why>,
        /// 后端写好的复制详情（[`DialAck::detail`] 原样；老后端 ⇒ `None`）。
        detail: Option<String>,
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
            LinkError::Silent => f.write_str(&copy_text("rsSshLink.dial.silent", &[])),
            LinkError::Refused { why, .. } => write!(
                f,
                "{}",
                copy_text("rsSshLink.dial.failed", &[("why", &why.to_string())])
            ),
            LinkError::TooOld { .. } => {
                write!(f, "{}", copy_core::backend_old(&copy_core::local_machine()))
            }
            LinkError::Garbled(_) => write!(f, "{}", copy_text("reason.dial.unparsable", &[])),
            LinkError::LineTooLong(_) => {
                write!(f, "{}", copy_text("reason.dial.unparsable", &[]))
            }
            // 管子读错的原话不上这一句（调用方把它交给复制详情）。
            LinkError::Io(_) => write!(f, "{}", copy_text("reason.local.linkLost", &[])),
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
    on_stage: &mut (dyn FnMut(Stage) + Send),
    on_step: &mut (dyn FnMut() + Send),
) -> Result<DialAck, LinkError> {
    loop {
        let Some(line) = read_line_capped(r, cap).await? else {
            return Err(LinkError::Silent);
        };
        let v: serde_json::Value = serde_json::from_str(&line)
            .map_err(|e| LinkError::Garbled(format!("{e}: {line:?}")))?;
        if let Some(stage) = v.get("stage") {
            match serde_json::from_value(stage.clone())
                .map_err(|e| LinkError::Garbled(format!("stage line: {e}: {line:?}")))?
            {
                // 拨号又过了一步（请求带 `progress` 才有）：只交一个记号（哪一步不分支）。
                StageLine::Step { .. } => on_step(),
                StageLine::Stage(stage) => on_stage(stage),
            }
            continue;
        }
        let ack: DialAck = serde_json::from_value(v)
            .map_err(|e| LinkError::Garbled(format!("ack line: {e}: {line:?}")))?;
        if !ack.ok {
            return Err(LinkError::Refused {
                why: ack
                    .error
                    .unwrap_or_else(|| copy_text("reason.io.unknown", &[])),
                fingerprint: ack.fingerprint,
                expected: ack.expected_fingerprint,
                reason: ack.reason,
                detail: ack.detail,
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
    serde_json::from_str(&line).map_err(|e| LinkError::Garbled(format!("{e}: {line:?}")))
}

#[cfg(test)]
#[path = "../../../tests/comms/inward/ssh_link_tests.rs"]
mod tests;
