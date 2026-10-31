//! 「要让用户知道」的出错：壳推给界面的只有这一种（事件 [`EVENT`]），日志行永远不上屏。
//!
//! 一条 = 那句话（照文案键与参数从文案表取出来的）＋ 复制详情（原话、码、证据）＋ 能重新连接的那一台。
//! 界面只排版：那句话当 toast 标题，详情挂［复制详情］，`reconnect` 有值就给［重新连接］。
//! 只记日志的出错照旧 `tracing::error!` / `warn!`，不经这里、也不会被推到界面上。
//!
//! 哪几种要告诉用户，由 [`UiError`] 的变体列全；每个变体的键在 [`UiError::payload`] 里写成字面量、当场取那一句。

use crate::backend_policy::Death;
use copy_core::copy_text;
use copy_core::detail::{Detail, Label};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// 事件名（界面 `backend-errors.ts` 订的那一个）。
pub const EVENT: &str = crate::ui_contract::events::UI_ERROR;

/// 线上那一格（ts-rs 导出给界面）：只带界面读的那几格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
pub struct UiErrorPayload {
    /// 照文案键与参数从文案表取出来的那一句（界面原样当标题）。
    pub said: String,
    /// 复制详情那几行（时刻 · 本机 · 码 · 原话）。
    pub detail: String,
    /// 能重新连接的那一台（`origin`）；没有 ⇒ 省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub reconnect: Option<String>,
}

/// 本机后端那一命结束之后怎么样了。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    /// 监护器已经把它重新起了。
    Restarted,
    /// 没人再起它（脱离那条路没有监护 · 监护器放弃了）⇒ 本机断开，要用户点［重新连接］。
    Down,
}

/// 要告诉用户的那几种出错。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiError {
    /// 本机后端那一命意外结束（死亡账判的那一格 ＋ 之后怎么样了）。账行只进复制详情。
    LocalBackendDied {
        death: Death,
        then: Then,
        ledger_line: String,
    },
    /// 设置里的机器表认不出 ⇒ 远端一台都不连。
    MachinesUnreadable { path: String, why: String },
    /// 那台一条连接上来了解不开的一种帧（每条连接每种一次；帧门拦下的只进日志）。`why` 只进复制详情。
    FrameUnreadable {
        host: String,
        kind: String,
        why: String,
    },
    /// 那台拥塞丢过帧：`said` 是那台写好的那一句，前面排机器名。
    Overflow { host: String, said: String },
    /// 一个会话的历史快照两次都没拉下（那个标签页只有实时消息）。`raw` 是最后一次的原话。
    HistoryUnfetched { sid: String, raw: String },
    /// 文件窗口开出来之后又不体面地退了：退出状态 ＋ 错误输出末几行（没有 ⇒ `None`），都只进复制详情。
    FileWindowExited {
        code: Option<String>,
        raw: Option<String>,
    },
}

/// 复制详情的头两行（时刻 · 本机）。
fn detail_head() -> Detail {
    Detail::new()
        .item(Label::At, crate::detail::now())
        .item(Label::Local, crate::detail::local_line())
}

impl UiError {
    /// 只记日志、不告诉用户的那几格 ⇒ `None`。
    pub fn payload(&self) -> Option<UiErrorPayload> {
        let (said, detail) = match self {
            UiError::LocalBackendDied {
                death,
                then,
                ledger_line,
            } => {
                let said = match (death, then) {
                    (Death::Crashed { .. }, Then::Restarted) => {
                        copy_text("rsUiError.localExited.restarted", &[])
                    }
                    (Death::Crashed { .. }, Then::Down) => {
                        copy_text("rsUiError.localExited.down", &[])
                    }
                    (Death::Refused { .. }, Then::Restarted) => {
                        copy_text("rsUiError.localQuit.restarted", &[])
                    }
                    (Death::Refused { .. }, Then::Down) => {
                        copy_text("rsUiError.localQuit.down", &[])
                    }
                    (Death::Misread { .. }, Then::Down) => {
                        copy_text("rsUiError.localUnreadable.down", &[])
                    }
                    // 读坏了而监护器已经重起它：用户那边只是闪断一下，只记日志。
                    (Death::Misread { .. }, Then::Restarted) => return None,
                    // 起不来：被拒那一句下一步在本机后端那一格（`LocalBackend`）里，手动起回的是失败 toast ⇒ 这里不再说一遍。
                    (Death::NeverStarted { .. }, _) => return None,
                };
                let reconnect =
                    (*then == Then::Down).then(|| crate::inbound_client::LOCAL_ORIGIN.to_string());
                let detail = detail_head()
                    .maybe(
                        Label::Code,
                        crate::backend_policy::exit_code(death).as_deref(),
                    )
                    .item(Label::Raw, ledger_line);
                return Some(UiErrorPayload {
                    said,
                    detail: detail.render(),
                    reconnect,
                });
            }
            UiError::MachinesUnreadable { path, why } => (
                copy_text("rsUiError.machines.unreadable", &[]),
                detail_head().item(Label::Path, path).item(Label::Raw, why),
            ),
            UiError::FrameUnreadable { host, kind, why } => (
                copy_text(
                    "rsSshSource.health.badFrame",
                    &[("host", host.as_str()), ("kind", kind.as_str())],
                ),
                detail_head().item(Label::Raw, why),
            ),
            UiError::Overflow { host, said } => (
                copy_text(
                    "rsUiError.overflow.said",
                    &[("host", host.as_str()), ("said", said.as_str())],
                ),
                detail_head(),
            ),
            UiError::HistoryUnfetched { sid, raw } => (
                copy_text("rsSshSource.snapshot.failed", &[("sid", sid.as_str())]),
                detail_head().item(Label::Raw, raw),
            ),
            UiError::FileWindowExited { code, raw } => (
                copy_text("rsFilewinProc.late.exited", &[]),
                detail_head()
                    .maybe(Label::Code, code.as_deref())
                    .maybe(Label::Raw, raw.as_deref()),
            ),
        };
        Some(UiErrorPayload {
            said,
            detail: detail.render(),
            reconnect: None,
        })
    }
}

type Emit = Box<dyn Fn(&UiErrorPayload) + Send + Sync>;

/// 出口（`lib.rs` setup 时由 `LoggingState::install_error_emitter` 装上；之前的只记日志）。
static EMIT: RwLock<Option<Emit>> = RwLock::new(None);
/// 设置「日志」页那个开关（后台出错 · 弹提示）。
static ENABLED: AtomicBool = AtomicBool::new(true);
/// 限频：60 秒里至多 20 条（再多界面也看不过来）。
static RECENT: Mutex<VecDeque<Instant>> = Mutex::new(VecDeque::new());
const RATE_WINDOW: Duration = Duration::from_secs(60);
const RATE_MAX: usize = 20;

pub(crate) fn install(emit: Emit) {
    *EMIT.write() = Some(emit);
}

pub(crate) fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// 告诉用户。只记日志的那几格（[`UiError::payload`] 给 `None`）什么都不推。
pub fn tell(e: UiError) {
    let Some(p) = e.payload() else { return };
    tracing::info!("告诉界面：{}", p.said);
    #[cfg(test)]
    if tests::captured(&p) {
        return;
    }
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let now = Instant::now();
    {
        let mut q = RECENT.lock();
        while q
            .front()
            .is_some_and(|t| now.duration_since(*t) > RATE_WINDOW)
        {
            q.pop_front();
        }
        if q.len() >= RATE_MAX {
            return;
        }
        q.push_back(now);
    }
    if let Some(emit) = EMIT.read().as_ref() {
        emit(&p);
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ui_error_tests.rs"]
pub(crate) mod tests;
