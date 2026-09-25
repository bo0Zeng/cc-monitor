//! 通道 · **webview 那一侧的宿主**：主界面（webview 里的 TS）经 Tauri IPC 说 `call` 的那一跳。
//!
//! # 为什么要有它（`设计/05 §3.3` · `§10` · C4a）
//!
//! 通道那一拍（`§10`）给了**进程外**的前端（文件窗口）一条路：回环 TCP ＋ 钥匙 ＋ 路由器。
//! 主界面**不在进程外** —— 它是 monitor 进程里的 webview，与后端之间隔着的是 Tauri IPC，
//! 不是一条 TCP 连接。⇒ 它说 `call` 用不着绑口、钥匙、拆帧、编号配对（那些是**那条连接**的事）；
//! 它要的只是 `§3.3` 那个签名在 Tauri IPC 这一跳上的样子：
//!
//! ```text
//! webview (src/ipc/chan.ts)  ──Tauri IPC──▶  chan_call（本文件）──▶ router::settle ──▶ 注入的 Backends
//!   第 0 跳：webview ↔ monitor                                      第 1 跳：monitor ↔ 后端
//! ```
//!
//! 跳号与回环那条同一张读法表（`wire::HopId` 头注）：第 0 跳是「前端 ↔ monitor」这一跳，第 1 跳是
//! 「monitor ↔ 后端」（经注入的句柄）。第 0 跳的期限在 TS 那一侧执行（过期就一个字节都不发）；
//! 第 1 跳的上界由 [`super::router::settle`] 执行 —— **与回环那条同一份**，不写第二份。
//!
//! # 🔴 为什么这一份**不是**通信层成员（与 `host.rs` 同一条理由）
//!
//! 它碰 Tauri（`#[tauri::command]` · `tauri::ipc::Response`）、按生产注入句柄（[`super::host::InboundBackends`]）、
//! 给「空白名」那一档当场拒 —— 那是**宿主**做的几件事（`C5` 的「绑口 / 注入在外」同形）。
//! 成员那一半是 TS 的 `src/ipc/chan.ts`（webview 手里那一半，与 `client.rs` 对称）。
//!
//! # 载荷原样
//!
//! 请求体在 IPC 上是一个字节数组（JSON 的数字数组 —— 查询的请求体都小），**应答**走
//! `tauri::ipc::Response`（原样字节，TS 拿 `ArrayBuffer`）—— 两个方向都不把载荷当 JSON 读，
//! 读它的是宿主注入的那个句柄（后端说 JSON，`host.rs` 那一格）与 TS 那侧的调用方。
//!
//! # 错误三层
//!
//! 失败时回 `{ err, body }`：`err` 是 [`super::wire::err_to_wire`] 给的**线上形状**（回环那条同一份），
//! `body` 是 `Refused` 那份不透明体。TS 那侧按它解回 `§3.3.1` 的三层。
//!
//! # 买不到
//!
//! - **不买对端撤活**：TS 那侧本地撤单是立即的（`Ours{Cancelled}`），而这一跳照跑到「还剩多少」为止
//!   —— 与回环那条同一条边界（`host.rs` 头注）。补它要一条「撤单」命令 ＋ 在飞编号表，本拍不开。
//! - **不买 `subscribe`**：webview 这一侧本拍零条流（登记在 `调研/第四波记录/C4a.md` §5.4）。

use super::host::InboundBackends;
use super::router::{self, Backends};
use super::wire::{err_to_wire, Body, CallError, CancelToken, Op, OursFault, WireErr};
use serde::Serialize;
use std::time::Duration;

/// 失败时交回 webview 的那一格：线上形状 ＋ `Refused` 的不透明体。
#[derive(Debug, Serialize)]
pub struct Fail {
    err: WireErr,
    body: Vec<u8>,
}

fn fail(e: CallError) -> Fail {
    let (err, body) = err_to_wire(e);
    Fail { err, body }
}

/// 经给定句柄走一次 `call`（第 1 跳）。判据用它喂合成句柄；生产由 [`chan_call`] 喂 [`InboundBackends`]。
pub(crate) async fn call_via(
    backends: &dyn Backends,
    origin: super::wire::Origin,
    op: String,
    payload: Vec<u8>,
    left: Duration,
) -> Result<Body, CallError> {
    let cancel = CancelToken::new();
    let fut = backends.call(origin, Op(op), Body(payload), left, cancel.clone());
    router::settle(fut, left, cancel).await
}

/// 主界面说 `call` 的那一条命令。`left_ms` = 这一跳还剩多少（TS 那侧由绝对时刻换算，过期的根本不发）。
///
/// 空白名当场按「用法错」拒（`Origin::route` 那道闸，同全仓吃 `Origin` 的命令）：
/// 空名交给句柄只会得到一句「没有控制通道」，那与真实原因（调用方没说哪台）毫无关系。
#[tauri::command]
pub async fn chan_call(
    origin: crate::origin::Origin,
    op: String,
    payload: Vec<u8>,
    left_ms: u64,
) -> Result<tauri::ipc::Response, Fail> {
    if origin.route("chan_call").is_err() {
        return Err(fail(OursFault::Misuse.into()));
    }
    match call_via(
        &InboundBackends,
        origin,
        op,
        payload,
        Duration::from_millis(left_ms),
    )
    .await
    {
        Ok(body) => Ok(tauri::ipc::Response::new(body.0)),
        Err(e) => Err(fail(e)),
    }
}

#[cfg(test)]
#[path = "../../../../tests/bridge/chan/webview_tests.rs"]
mod tests;
