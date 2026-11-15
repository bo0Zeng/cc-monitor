//! 数据面漂移记账：把「Claude Code 变了」从不可观测变成看一眼就知道。
//!
//! CC 的记录类型会自己长（`src/doc/INVARIANTS.md §18.1` 记着语料里未知 `type` 的读数），而仓里没有别的东西知道这件事。
//! `parse.rs` 对未知 `type` 刻意不 warn（几万条 `mode` 会刷屏）、未登记的 pidfile `kind` 一声不吭（`kind` 是授权型判据）——
//! 两者都对，但都不该是无声的。本模块只做一件事：记账。不改变任何行为，不发 warn，不影响渲染。
//!
//! 这里只交面（[`DriftFace`] 与它的线上词）；计数 · 封顶 · 空键 · 快照排序住通用的 [`DriftLedger`]，这一家开一本。
//!
//! # 按机器分
//!
//! 记录在那台机器自己的后端里解析 ⇒ 账天然按机器分；界面按机器经通道问那台后端（帧命令 `drift-report`）。
//! 计数是每条记录一次（解析热路径）；要看的是键的集合，不是数字。

use crate::agents::drift::{DriftFaceReport, DriftFaceWord, DriftLedger};
use std::sync::Mutex;

/// 记录那几个「降级点」。每一个都对应一处**刻意的**宽容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DriftFace {
    /// jsonl 里我们不认识的记录 `type`（`parse.rs` 抢救成 `CcMonitorUnrecognized`，刻意不 warn）。
    UnknownRecordType,
    /// 已知 `type` 但字段解析失败（**这一类值得警惕**：多半是 CC 改了已知类型的形状）。
    KnownTypeParseFailed,
    /// 进程状态文件里认不出的 `kind`（当后台会话，不成 tab）。
    UnknownSessionKind,
    /// 进程状态文件里认不出的 `status`（活动灯说不清）。
    UnknownSessionStatus,
    /// 进程状态文件里认不出的 `waitingFor`（在等什么说不清 ⇒ 「需手动」判不出种类）。
    UnknownWaitingFor,
    /// 记录里「延后加载的工具变了」附件里认不出的 MCP 状态表（`…McpServers`）：那一种状态的服务器不进会话的 MCP 那一格。
    UnknownMcpList,
}

impl DriftFaceWord for DriftFace {
    /// 线上那个词（漂移账里 `face` 那一格；界面按它排、壳不认）。
    fn word(self) -> &'static str {
        match self {
            DriftFace::UnknownRecordType => "unknown_record_type",
            DriftFace::KnownTypeParseFailed => "known_type_parse_failed",
            DriftFace::UnknownSessionKind => "unknown_session_kind",
            DriftFace::UnknownSessionStatus => "unknown_session_status",
            DriftFace::UnknownWaitingFor => "unknown_waiting_for",
            DriftFace::UnknownMcpList => "unknown_mcp_list",
        }
    }
}

/// 这台机器的账。
static LEDGER: Mutex<DriftLedger<DriftFace>> = Mutex::new(DriftLedger::new());

/// 记一次。**这是本模块唯一的写入口。**
pub(crate) fn record(face: DriftFace, key: &str) {
    DriftLedger::lock(&LEDGER).record(face, key);
}

/// 只读快照（也是诊断面的读口：帧命令 `drift-report` 经注册表 `RecordFace.drift` 够到它）。按需调用，不轮询。
pub(crate) fn snapshot() -> Vec<DriftFaceReport> {
    DriftLedger::lock(&LEDGER).snapshot()
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/drift_tests.rs"]
mod tests;
