//! 「要让用户知道」的出错：壳推给界面的只有这一种（事件 [`EVENT`]），日志行永远不上屏。
//!
//! 一条 = 码 ＋ 文案键 ＋ 参数 ＋ 那句话（照键与参数从文案表取出来的）＋ 复制详情（原话、码、证据）。
//! 界面只排版：那句话当 toast 标题，详情挂［复制详情］，`reconnect` 有值就给［重新连接］。
//! 只记日志的出错照旧 `tracing::error!` / `warn!`，不经这里、也不会被推到界面上。
//!
//! 哪几种要告诉用户，由 [`UiError`] 的变体列全；每个变体的键在 [`UiError::payload`] 里写成字面量。

use crate::backend_policy::Death;
use crate::copy_table::copy_text;
use copy_core::detail::{Detail, Label};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 事件名（界面 `backend-errors.ts` 订的那一个）。
pub const EVENT: &str = crate::ui_contract::events::UI_ERROR;

/// 线上那一格（ts-rs 导出给界面）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
pub struct UiErrorPayload {
    /// 哪一种（稳定的码：界面按它认动作、不上屏）。
    pub code: String,
    /// 那句话的文案键。
    pub key: String,
    /// 填进那句话的参数（与表里那一条的 `args` 相等）。
    pub args: BTreeMap<String, String>,
    /// 照 `key` 与 `args` 从文案表取出来的那一句（界面原样当标题）。
    pub said: String,
    /// 复制详情那几行（时刻 · 本机 · 码 · 原话）。
    pub detail: String,
    /// 能重新连接的那一台（`origin`）；没有 ⇒ 省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub reconnect: Option<String>,
    /// 出来那一刻（毫秒）。
    #[cfg_attr(test, ts(type = "number"))]
    pub at: i64,
}

/// 本机后端那一命结束之后怎么样了。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    /// 监护器已经把它重新起了。
    Restarted,
    /// 没人再起它（脱离那条路没有监护 · 监护器放弃了）⇒ 本机连不上，要用户点［重新连接］。
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
}

impl UiError {
    /// 只记日志、不告诉用户的那几格 ⇒ `None`。
    pub fn payload(&self) -> Option<UiErrorPayload> {
        match self {
            UiError::LocalBackendDied {
                death,
                then,
                ledger_line,
            } => {
                let (code, key, said) = match (death, then) {
                    (Death::Crashed { .. }, Then::Restarted) => (
                        "local-exited",
                        "rsUiError.localExited.restarted",
                        copy_text("rsUiError.localExited.restarted", &[]),
                    ),
                    (Death::Crashed { .. }, Then::Down) => (
                        "local-exited",
                        "rsUiError.localExited.down",
                        copy_text("rsUiError.localExited.down", &[]),
                    ),
                    (Death::Refused { .. }, Then::Restarted) => (
                        "local-quit",
                        "rsUiError.localQuit.restarted",
                        copy_text("rsUiError.localQuit.restarted", &[]),
                    ),
                    (Death::Refused { .. }, Then::Down) => (
                        "local-quit",
                        "rsUiError.localQuit.down",
                        copy_text("rsUiError.localQuit.down", &[]),
                    ),
                    (Death::Misread { .. }, Then::Down) => (
                        "local-unreadable",
                        "rsUiError.localUnreadable.down",
                        copy_text("rsUiError.localUnreadable.down", &[]),
                    ),
                    // 读坏了而监护器已经重起它：用户那边只是闪断一下，只记日志。
                    (Death::Misread { .. }, Then::Restarted) => return None,
                    // 起不来：开机那一下走系统通知（`lib.rs`），手动起回的是失败 toast ⇒ 这里不再说一遍。
                    (Death::NeverStarted { .. }, _) => return None,
                };
                let reconnect =
                    (*then == Then::Down).then(|| crate::inbound_client::LOCAL_ORIGIN.to_string());
                Some(UiErrorPayload {
                    code: code.into(),
                    key: key.into(),
                    args: BTreeMap::new(),
                    said,
                    detail: Detail::new()
                        .item(Label::At, crate::detail::now())
                        .item(Label::Local, crate::detail::local_line())
                        .maybe(
                            Label::Code,
                            crate::backend_policy::exit_code(death).as_deref(),
                        )
                        .item(Label::Raw, ledger_line)
                        .render(),
                    reconnect,
                    at: now_ms(),
                })
            }
            UiError::MachinesUnreadable { path, why } => Some(UiErrorPayload {
                code: "machines-unreadable".into(),
                key: "rsUiError.machines.unreadable".into(),
                args: BTreeMap::new(),
                said: copy_text("rsUiError.machines.unreadable", &[]),
                detail: Detail::new()
                    .item(Label::At, crate::detail::now())
                    .item(Label::Local, crate::detail::local_line())
                    .item(Label::Path, path)
                    .item(Label::Raw, why)
                    .render(),
                reconnect: None,
                at: now_ms(),
            }),
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

type Emit = Box<dyn Fn(&UiErrorPayload) + Send + Sync>;

/// 出口（`lib.rs` setup 那一拍由 `LoggingState::install_error_emitter` 装上；之前的只记日志）。
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
    tracing::info!("告诉界面：{}（{}）", p.said, p.code);
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
