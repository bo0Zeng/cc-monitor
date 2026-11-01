//! 判据用的两个口子（脚本化的探测器 · 陈值）：本包的判据与上层的测试档（经 `test-support`）共用；发布构建里没有。

use super::{SessionRow, SessionSnapshot};
use crate::common::said::Said;

impl SessionSnapshot {
    /// 最后一次存进来的那份（**陈值**）。
    ///
    /// 🔴 生产代码一处都不许调它 —— 它只是让「陈值确实与这一刻不同」这件事**测得出来**。
    /// 由 `session_snapshot_tests` 的判据钉住「生产段零调用」。
    pub fn peek(&self) -> Vec<SessionRow> {
        self.rows.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// 拿一个脚本化的探测器造一份快照。
    pub fn with_prober(
        probe: impl Fn() -> Result<Vec<SessionRow>, Said> + Send + Sync + 'static,
    ) -> Self {
        Self::new(Box::new(probe))
    }
}
