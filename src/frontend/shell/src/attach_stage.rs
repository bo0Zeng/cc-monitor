//! **聊天附件的分块传，壳替界面走**：主界面只交那一份的字节（webview 只给 `File`，没有本机路径）与进度通道，
//! 逐块送 · 整份摘要 · 现造暂存键都是 `chunk_core::stage`（文件窗口与手机嵌的核心同一份）。
//!
//! ```text
//! 界面 attach-draft.ts ──attach_stage（原样字节 ＋ 头）──▶ 本文件 ──chunk_core::stage──▶ call_via(InboundBackends) ──▶ 那台 files-stage-chunk
//!        ◀── Staged {key, chunks, bytes, sha256} · 进度经 Tauri Channel 逐块报 ──
//! ```
//!
//! - 请求体是那一份的原始字节（Tauri IPC 原样字节，不走 JSON 数组）；头上带那台（[`ORIGIN_HEADER`]，名字 UTF-8 的十六进制：
//!   头只收可见 ASCII）· 撤单号（[`TICKET_HEADER`]）· 进度通道（[`MOVED_HEADER`]，Tauri `Channel` 的 `__CHANNEL__:<id>`）。
//! - 逐块问那台与 `chan_call` 同一条路（[`crate::chan::webview::call_via`] ＋ [`InboundBackends`]），失败那一形就是
//!   [`Fail`]：界面照 `chan.ts::decodeFail` 解回三层，那台断开（`hop` 层）记离线、连上从头传。
//! - 提交（`session-attachment-commit`）由界面问：这里只送进暂存区（同手机：核心送块，提交由 Kotlin 问）。
//! - ✕ 那一下 [`attach_stage_stop`]：下一块不再送（已送的块交那台的孤儿扫）。撤单按（窗口 · 单号）记，先于开传到也撤得住。

// 收 `Request<'_>`（引用）的异步命令：tauri 的命令宏在它旁边生成一道「必须回 `Result`」的编译期检查（`let _: &dyn …`），
// 那一句落在本模块里、不在任何函数里 ⇒ 放行只能写在模块这一层；本文件自己的代码不写 `let _`。
#![expect(
    clippy::let_underscore_must_use,
    reason = "tauri 命令宏给收引用的异步命令生成的「必须回 Result」检查"
)]

use crate::chan::host::InboundBackends;
use crate::chan::router::Backends;
use crate::chan::webview::{call_via, fail, Fail};
use crate::chan::wire::OursFault;
use crate::origin::Origin;
use cmd_wire::transfer::Staged;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

/// 一块的往返上限（远端一块 512 KiB 十六进制，慢线上也够）。
const STEP_BUDGET: Duration = Duration::from_secs(30);

/// 那台的名字（UTF-8 的十六进制）。
pub const ORIGIN_HEADER: &str = "ccm-origin";
/// 撤单号（这一扇窗里由界面编）。
pub const TICKET_HEADER: &str = "ccm-ticket";
/// 进度通道（`__CHANNEL__:<id>`）。
pub const MOVED_HEADER: &str = "ccm-moved";

/// 撤单：（窗口 · 单号）⇒ 撤了没有。
#[derive(Default)]
pub(crate) struct Tickets(Mutex<HashMap<(String, u64), Arc<AtomicBool>>>);

impl Tickets {
    /// 开传那一下取这一单的旗（先到的撤单已经插好旗 ⇒ 拿到的就是撤了的）。
    pub(crate) fn open(&self, label: &str, ticket: u64) -> Arc<AtomicBool> {
        let mut m = self.0.lock().unwrap_or_else(|e| e.into_inner());
        m.entry((label.to_string(), ticket)).or_default().clone()
    }

    pub(crate) fn stop(&self, label: &str, ticket: u64) {
        let mut m = self.0.lock().unwrap_or_else(|e| e.into_inner());
        m.entry((label.to_string(), ticket))
            .or_default()
            .store(true, Ordering::SeqCst);
    }

    /// 这一单收场：旗拔掉。
    pub(crate) fn close(&self, label: &str, ticket: u64) {
        let mut m = self.0.lock().unwrap_or_else(|e| e.into_inner());
        m.remove(&(label.to_string(), ticket));
    }
}

static TICKETS: LazyLock<Tickets> = LazyLock::new(Tickets::default);

/// 分块那一圈问那台的那一口：与 `chan_call` 同一条路。
struct Over<'a> {
    backends: &'a dyn Backends,
    origin: &'a Origin,
}

impl chunk_core::Ask for Over<'_> {
    type Err = Fail;
    async fn ask<C: cmd_wire::Call + Sync>(&self, req: &C) -> Result<C::Reply, Fail> {
        let broken = || fail(self.origin, C::NAME, OursFault::Broken.into());
        let body = serde_json::to_vec(req).map_err(|_| broken())?;
        let got = call_via(
            self.backends,
            self.origin.clone(),
            C::NAME.to_string(),
            body,
            None,
            STEP_BUDGET,
        )
        .await
        .map_err(|e| fail(self.origin, C::NAME, e))?;
        serde_json::from_slice(&got.0).map_err(|_| broken())
    }
}

/// 撤与进度那两口。
struct Watch<'a, F: FnMut(u64)> {
    stop: &'a AtomicBool,
    moved: F,
}

impl<F: FnMut(u64)> chunk_core::Watch for Watch<'_, F> {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
    fn moved(&mut self, done: u64, _total: Option<u64>) {
        (self.moved)(done);
    }
}

/// `bytes` 逐块送进那台暂存区的 `key` 那一份。判据用它喂合成句柄；生产由 [`attach_stage`] 喂 [`InboundBackends`]。
pub(crate) async fn stage_via(
    backends: &dyn Backends,
    origin: &Origin,
    key: String,
    bytes: &[u8],
    stop: &AtomicBool,
    moved: impl FnMut(u64),
) -> Result<Staged, Fail> {
    const OP: &str = <cmd_wire::files::FilesStageChunkReq as cmd_wire::Call>::NAME;
    chunk_core::stage(
        &Over { backends, origin },
        key,
        bytes,
        Some(bytes.len() as u64),
        &mut Watch { stop, moved },
        || fail(origin, OP, OursFault::Cancelled.into()),
        |_| fail(origin, OP, OursFault::Broken.into()),
    )
    .await
}

/// 头里那一格（没有 · 不是可见 ASCII ⇒ `None`）。
fn header<'a>(request: &'a tauri::ipc::Request<'_>, name: &str) -> Option<&'a str> {
    request.headers().get(name)?.to_str().ok()
}

/// 主界面交一份附件的字节：逐块送进那台暂存区，回 `Staged`（提交由界面问）。进度逐块经头里那条通道报已送字节。
/// 头缺 · 认不出 · 请求体不是原样字节 ⇒ 用法错（`chan_call` 空白名那一形）。
#[tauri::command]
pub async fn attach_stage(
    webview: tauri::Webview,
    request: tauri::ipc::Request<'_>,
) -> Result<Staged, Fail> {
    use std::str::FromStr as _;
    let misuse = |o: &Origin| fail(o, "attach_stage", OursFault::Misuse.into());
    let origin = header(&request, ORIGIN_HEADER)
        .and_then(cmd_wire::files::from_hex)
        .and_then(|b| String::from_utf8(b).ok())
        .map(Origin::named)
        .filter(|o| o.route("attach_stage").is_ok())
        .ok_or_else(|| misuse(&Origin::named(String::new())))?;
    let ticket: u64 = header(&request, TICKET_HEADER)
        .and_then(|t| t.parse().ok())
        .ok_or_else(|| misuse(&origin))?;
    let moved: tauri::ipc::Channel<u64> = header(&request, MOVED_HEADER)
        .and_then(|c| tauri::ipc::JavaScriptChannelId::from_str(c).ok())
        .map(|id| id.channel_on(webview.clone()))
        .ok_or_else(|| misuse(&origin))?;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(misuse(&origin));
    };
    let label = webview.label().to_string();
    let stop = TICKETS.open(&label, ticket);
    let key = chunk_core::key_of(*uuid::Uuid::new_v4().as_bytes());
    let got = stage_via(&InboundBackends, &origin, key, bytes, &stop, |done| {
        // 进度交不回去（那一扇窗没了）：块照送，结局由这一问交回。
        if let Err(e) = moved.send(done) {
            tracing::debug!("附件进度交不回窗口：{e}");
        }
    })
    .await;
    TICKETS.close(&label, ticket);
    got
}

/// ✕ 那一份：那一单下一块不再送（只拨旗；照全仓默认走异步，不占主线程）。
#[tauri::command]
pub async fn attach_stage_stop(webview: tauri::Webview, ticket: u64) {
    TICKETS.stop(webview.label(), ticket);
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/attach_stage_tests.rs"]
mod tests;
