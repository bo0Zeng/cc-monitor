//! 后端→monitor 的帧：按 `cmd-wire` 的类型解（[`decode`]）· 进不来的每种说一次（[`UnreadNotes`]）。

use super::*;
use crate::copy_table::copy_text;

/// 一行会话记录的成品 ＋ 它在那份文件里的行号（`seq`）—— 进 [`flush_lines`] 之前的形状。所有行都从后端的帧来（远端流 · 本机流 · 旁路快照）；
/// `seq` 是后端给的行号（流不重放历史，与快照同处一个行号空间），前端按 `(session_id, seq)` 去重、按 `seq` 排序（`INVARIANTS §5` / `§9`）。
#[derive(Debug, Clone)]
pub struct JsonlLine {
    pub session_id: String,
    pub path: std::path::PathBuf,
    pub seq: u64,
    /// 那台后端给的成品：这一行的通用记录；`None` ＝ 不进界面（照占号）。
    pub record: Option<crate::ui_contract::RecordBody>,
    /// 这条记录自己的 `cwd`（后端给的）。
    pub cwd: Option<String>,
    /// 这一行之后（含它的 `\n`）那一个字节的偏移 = 下一行的起点（后端 `line.byte_offset` ·
    /// 快照的行区间末端）；说不准 ⇒ `None`。续点据它记「从哪个字节接着读」。
    pub end: Option<u64>,
    /// 这一行的对账键（后端 `line.rid`，原样转交；快照那一路没有 ⇒ `None`）。
    pub rid: Option<String>,
    /// 这一行是补历史（旁路快照补过来的、宣告那一刻已经在盘上的那几行，[`super::snapshot::catch_up_at`]），
    /// 不是刚写进来的。实时那两条流（远端 · 本机）恒 `false`。
    pub catch_up: bool,
}

/// 那台后端发来的一帧，按 `cmd-wire` 的类型解（记录那一格原文收下、不建树）。
pub(crate) type Frame = cmd_wire::frame::Frame<crate::ui_contract::RecordBody>;

/// 一行进不来的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unread {
    /// 帧门拦下的：那台那一版这种帧的形状与这一侧不同，或这一侧不认这种帧（[`resident_client::frames::Admit`]）。
    Differs(String),
    /// 门放过了、按类型解不开 ⇒ 两端契约对不上（`kind` 空 ＝ 连种类都读不出）。
    BadShape { kind: String, why: String },
}

impl std::fmt::Display for Unread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unread::Differs(kind) => write!(f, "frame `{kind}` differs from this side"),
            Unread::BadShape { kind, why } => write!(f, "frame `{kind}` malformed: {why}"),
        }
    }
}

/// 一行 ⇒ 一帧（按类型解）。解不开 ⇒ [`Unread::BadShape`]。
pub(crate) fn decode(line: &str) -> Result<Frame, Unread> {
    cmd_wire::frame::decode(line).map_err(|e| Unread::BadShape {
        kind: cmd_wire::frame::kind_of(line).unwrap_or_default(),
        why: e.to_string(),
    })
}

/// 一条连接上进不来的帧：日志与健康信息都**每种说一次**（形状不同 · 解不开分开算），
/// 之后只记账（[`crate::frame_tally::FrameTally`] 按 2 的幂次出一行、流结束出总账）。
pub(crate) struct UnreadNotes {
    /// 健康信息归哪台（`RemoteHealthPayload::origin`）。
    origin: String,
    /// 文案里那台叫什么。
    label: String,
    said: std::collections::BTreeSet<(bool, String)>,
}

impl UnreadNotes {
    pub(crate) fn new(origin: impl Into<String>, label: impl Into<String>) -> Self {
        UnreadNotes {
            origin: origin.into(),
            label: label.into(),
            said: Default::default(),
        }
    }

    /// 一行：先过帧门（`gate` ＝ 这条连接的客户端；远端那条流的门在循环里、交来的都已过门 ⇒ `None`），再按类型解。
    /// 进不来 ⇒ 记账；这一种在这条连接上第一次 ⇒ 日志一条（带原因）＋ 经 `health` 说一句。回 `None` ＝ 这一行不用（不断连）。
    pub(crate) fn take(
        &mut self,
        line: &str,
        gate: Option<&crate::inbound_client::InboundClient>,
        tally: &mut crate::frame_tally::FrameTally,
        health: &dyn Fn(crate::ui_contract::RemoteHealthPayload),
    ) -> Option<Frame> {
        use resident_client::frames::Admit;
        let why = match gate.map(|c| c.admit(line)) {
            Some(Admit::Differs(kind) | Admit::Unknown(kind)) => Unread::Differs(kind),
            None | Some(Admit::Pass(_)) => match decode(line) {
                Ok(f) => return Some(f),
                Err(why) => why,
            },
        };
        if let Some(n) = tally.note_unparsed(line) {
            tracing::warn!("{n}");
        }
        let key = match &why {
            Unread::Differs(kind) => (true, kind.clone()),
            Unread::BadShape { kind, .. } => (false, kind.clone()),
        };
        if self.said.insert(key) {
            tracing::warn!("[{}] {why}", self.origin);
            health(unread_health(&self.origin, &self.label, &why));
        }
        None
    }
}

/// 进不来那一帧的健康信息（每条连接每种一次）。
pub(crate) fn unread_health(
    origin: &str,
    label: &str,
    why: &Unread,
) -> crate::ui_contract::RemoteHealthPayload {
    let (kind, message, detail) = match why {
        Unread::Differs(k) => (
            "frame-differs",
            copy_text(
                "peerVersion.said.framesDiffer",
                &[("machine", &label.to_string())],
            ),
            format!("frames: {k}"),
        ),
        Unread::BadShape { kind: k, .. } => (
            "frame-shape",
            copy_text(
                "rsSshSource.health.badFrame",
                &[
                    ("host", &label.to_string()),
                    (
                        "kind",
                        &if k.is_empty() {
                            "?".to_string()
                        } else {
                            k.clone()
                        },
                    ),
                ],
            ),
            String::new(),
        ),
    };
    crate::ui_contract::RemoteHealthPayload {
        origin: origin.to_string(),
        kind: kind.to_string(),
        message,
        detail,
    }
}

/// 本机那两条载体（stdio · 脱离）的健康出口：窗口把手只在宿主层，`lib.rs` setup 装一次。
static LOCAL_HEALTH: std::sync::OnceLock<HealthOut> = std::sync::OnceLock::new();

pub(crate) fn install_local_health(out: HealthOut) {
    if LOCAL_HEALTH.set(out).is_err() {
        tracing::warn!("本机健康信息的出口装了第二次 —— 忽略");
    }
}

/// 本机载体说一句健康信息（出口没装 ⇒ 只有日志那一条）。
pub(crate) fn local_health(payload: crate::ui_contract::RemoteHealthPayload) {
    if let Some(out) = LOCAL_HEALTH.get() {
        if let Err(e) = out(payload) {
            tracing::warn!("local remote-health emit failed: {e}");
        }
    }
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/golden_tests.rs"]
mod golden_tests;
