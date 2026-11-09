//! 阻塞档命令的总期限：后端只登记上限（那条命令登记行上的 `.capped(..)`），发起方在请求信封里带它这一发愿意等多久（`within_ms`），
//! 分派那一层按它减余量收紧、装在执行这条命令的线程上。两个入口（帧面 · CLI 面）都经这里装。

use crate::platform::child::{Budget, Deadline, Until};
use crate::wire::Request;

/// 发起方期限里留给回程与界面收尾的那一段：后端在发起方放手之前这么久答出那句准话。
pub(crate) const MARGIN_MS: u64 = 2_000;

/// 发起方期限 − 余量，从此刻起换成截止时刻（没带 ⇒ `None`；不够余量 ⇒ 此刻就到）。
pub(crate) fn until_of(within_ms: Option<u64>) -> Option<Until> {
    within_ms.and_then(|ms| Until::after(Deadline::millis(ms.saturating_sub(MARGIN_MS))))
}

/// 这条命令登记的总期限上限（登记行上没写 · 不认得的命令 ⇒ `None`：不装，发起方带的期限对它不起作用）。
/// 没带发起方期限（CLI 面 · 不带这一格的前端）⇒ 按上限。协议参考按它逐条写。
pub(crate) fn cap_of(cmd: &str) -> Option<Deadline> {
    super::lookup(cmd).and_then(|s| s.cap)
}

/// 给这条命令装总期限（没登记上限 ⇒ 不装）。守卫活着期间有效：在执行这条命令的那根线程上调、跑完才放。
pub(crate) fn install(req: &Request) -> Option<Budget> {
    cap_of(&req.cmd).map(|cap| Budget::capped(cap, req.until))
}
