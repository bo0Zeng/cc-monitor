//! 托管那一种宿主：cc-monitor 自己持伪终端的终端。这一版还没有托管进程，名单恒空。

use super::TermRow;

/// 这台此刻的托管终端（每行 `host` 是 [`crate::wire::TerminalHost::Managed`]）。
pub(crate) fn rows() -> Vec<TermRow> {
    Vec::new()
}
