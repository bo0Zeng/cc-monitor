//! 因对方版本说不成的那两句，和一句话里怎么称呼那台机器。全产品各一份（界面那一侧是 `chan-caller.ts::peerVersionSaid`）。

use crate::copy_text;

/// 一句话里称呼本机的那个词（与界面 `chan-caller.ts::machineName` 同一条）。
pub fn local_machine() -> String {
    copy_text("control.machine.local", &[])
}

/// 说话的这一层不知道那台叫什么时（通信层的兜底句 · 后端自己答「不认这条」）称呼它的那个词。
/// 知道名字的调用方一律传名字（[`local_machine`] 或那台的名字），不用它。
pub fn peer_machine() -> String {
    copy_text("control.machine.peer", &[])
}

/// 因对方版本说不成的两个码，全产品各一句（与界面 `chan-caller.ts::peerVersionSaid` 同两条键）。
/// `machine` 是给人看的称呼（本机 ⇒ [`local_machine`]）。
///
/// - 那台后端不认这条命令 ⇒ [`backend_old`]；
/// - 那台回了、回的东西认不出 ⇒ [`reply_unreadable`]（只说认不出，不猜版本；细目进日志）。
pub fn backend_old(machine: &str) -> String {
    copy_text("peerVersion.said.old", &[("machine", machine)])
}

/// 见 [`backend_old`]。
pub fn reply_unreadable(machine: &str) -> String {
    copy_text("peerVersion.said.unreadable", &[("machine", machine)])
}
