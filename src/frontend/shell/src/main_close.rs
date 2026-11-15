//! 关主窗那一下（系统标题栏的 X · 窗口管理器的关）归壳一处：壳拦下关窗，自己问会打断什么、自己弹框、自己销毁，**不经网页 JS**
//! ——网页一卡，靠它接的那一份就没人处理、窗关不掉。
//!
//! 进程退不退照 [`closing_exits`]：主窗销毁时壳只顺手收掉设置窗（`lib.rs::windows_to_destroy_after`），还开着的查看窗 / agent 窗会把进程吊住
//! ⇒ 那时关主窗不是退出，不问。会退 ⇒ 问本机后端一条 `machine-interrupts`（带 `appExit` ＋ `fleet` ＋ 文件窗口那张表的两个数，[`request_of`]；
//! 与「现在重启」同一问，[`BUDGET`] 为期）。会断什么由后端排成「中断 · 保留」两行写好（`rows`），这里只画：空 ⇒ 直接关，
//! 不空 ⇒ 原生确认框（不靠网页画），取消 ⇒ 不关。本机后端问不到 · 到期 ⇒ 框里只说一句「问不到」（`quitNow.dialog.unknown`）。
//!
//! 买不到：主窗先关、最后一个查看窗后关（那一下才退）这条路不问；原生框的默认焦点由系统定（不保证落在［取消］）。

use crate::chan::host::busy::FilewinTotals;
use cmd_wire::machine::{
    FilewinBusy, InterruptAct, InterruptRow, MachineInterruptsReply, MachineInterruptsReq,
};
use copy_core::{copy_text, Discard, Why};
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// 那一问的期限：本机后端问到各台（各扫一遍活会话 ＋ 回程）再答（与界面 `settings/interrupts.ts::INTERRUPTS_BUDGET_MS` 同一个数）。
pub(crate) const BUDGET: Duration = Duration::from_secs(10);

/// 关了主窗进程会不会退：除主窗与设置窗之外没有别的窗口。
pub(crate) fn closing_exits(alive: &[&str]) -> bool {
    alive
        .iter()
        .all(|l| *l == crate::MAIN_WINDOW_LABEL || *l == crate::SETTINGS_WINDOW_LABEL)
}

/// 退出那一问：各台照各自的退出行为答（`appExit` ＋ `fleet`），停到再启动，带上各文件窗口手上的那两个数。
pub(crate) fn request_of(files: FilewinTotals) -> MachineInterruptsReq {
    MachineInterruptsReq {
        machine: None,
        app_exit: Some(true),
        fleet: Some(true),
        act: Some(InterruptAct::Stop),
        files: Some(FilewinBusy {
            unsaved: files.unsaved,
            running: files.running,
        }),
    }
}

/// 确认框正文：后端写好的那几行，一行抬头、底下逐项。
pub(crate) fn dialog_text(rows: &[InterruptRow]) -> String {
    rows.iter()
        .map(|r| {
            std::iter::once(r.label.clone())
                .chain(r.items.iter().map(|i| format!("· {i}")))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// 关主窗那一下放不放行（`false` ＝ 在框里取消了）。问、弹框都由调用方给（判据喂假的）；`ask` 到 `budget` 没回 ⇒ 当问不到。
pub(crate) async fn settle<A, AF, C, CF>(exits: bool, ask: A, budget: Duration, confirm: C) -> bool
where
    A: FnOnce() -> AF,
    AF: Future<Output = Option<MachineInterruptsReply>>,
    C: FnOnce(String) -> CF,
    CF: Future<Output = bool>,
{
    if !exits {
        return true;
    }
    let body = match tokio::time::timeout(budget, ask()).await.ok().flatten() {
        None => copy_text("quitNow.dialog.unknown", &[]),
        Some(r) if r.rows.is_empty() => return true,
        Some(r) => dialog_text(&r.rows),
    };
    confirm(body).await
}

/// 问本机后端那一条；本机后端不在 · 问不成 · 回包读不懂 ⇒ `None`。
async fn ask_hub(
    req: MachineInterruptsReq,
    deadline: tokio::time::Instant,
) -> Option<MachineInterruptsReply> {
    let client = crate::inbound_client::local_client()?;
    match client.ask(None, &req, None, deadline).await {
        Ok(Ok(r)) => Some(r),
        Ok(Err(e)) => {
            tracing::warn!("[quit] 本机后端答的「会打断什么」读不懂：{e}");
            None
        }
        Err(e) => {
            tracing::warn!("[quit] 本机后端问不到会打断什么：{e:?}");
            None
        }
    }
}

/// 原生确认框（不经网页）：［仍然退出］⇒ `true`。
async fn confirm_native(window: tauri::WebviewWindow, body: String) -> bool {
    use tauri_plugin_dialog::{DialogExt as _, MessageDialogButtons, MessageDialogKind};
    let (tx, rx) = tokio::sync::oneshot::channel();
    window
        .dialog()
        .message(body)
        .title(copy_text("quitNow.dialog.title", &[]))
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            copy_text("quitNow.dialog.action", &[]),
            copy_text("kit.dialog.cancel", &[]),
        ))
        .parent(&window)
        .show(move |go| tx.send(go).discard(Why::PeerGone));
    rx.await.unwrap_or(false)
}

/// 挂上主窗关窗那一钩：拦下、照 [`settle`] 定、放行就销毁（销毁不再触发关窗请求）。正在问 ⇒ 再点 X 不叠第二问。
pub fn install(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    static ASKING: AtomicBool = AtomicBool::new(false);
    builder.on_window_event(|window, event| {
        let tauri::WindowEvent::CloseRequested { api, .. } = event else {
            return;
        };
        if window.label() != crate::MAIN_WINDOW_LABEL {
            return;
        }
        api.prevent_close();
        if ASKING.swap(true, Ordering::AcqRel) {
            return;
        }
        use tauri::Manager as _;
        let app = window.app_handle().clone();
        tauri::async_runtime::spawn(async move {
            let Some(main) = app.get_webview_window(crate::MAIN_WINDOW_LABEL) else {
                ASKING.store(false, Ordering::Release);
                return;
            };
            let alive: Vec<String> = app.webview_windows().keys().cloned().collect();
            let alive: Vec<&str> = alive.iter().map(String::as_str).collect();
            let deadline = tokio::time::Instant::now() + BUDGET;
            let req = request_of(crate::chan::host::busy::book().total());
            let dialog_on = main.clone();
            let go = settle(
                closing_exits(&alive),
                || ask_hub(req, deadline),
                BUDGET,
                |body| confirm_native(dialog_on, body),
            )
            .await;
            ASKING.store(false, Ordering::Release);
            if go {
                if let Err(e) = main.destroy() {
                    tracing::warn!("[quit] 销毁主窗失败：{e}");
                }
            }
        });
    })
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/main_close_tests.rs"]
mod tests;
