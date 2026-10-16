//! 因对方版本说不成的那几句（命令发不成 · 接上时的新旧）· 那台做不到的那一句，和一句话里怎么称呼那台机器。全产品各一份（界面那一侧是 `chan-caller.ts::peerVersionSaid`）。

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

/// 见 [`backend_old`]：那台后端认得这条命令，但它那一版的形状跟这一侧不一样（typed 命令的形状指纹对不上，事前就不发）。
pub fn backend_differs(machine: &str) -> String {
    copy_text("peerVersion.said.differs", &[("machine", machine)])
}

/// 见 [`backend_old`]：接上了，那台比这一版新（这一侧不会把它换回去）。
pub fn backend_newer(machine: &str) -> String {
    copy_text("peerVersion.said.newer", &[("machine", machine)])
}

/// 见 [`backend_old`]：接上了，两边的版本比不出先后（任一边的构建标识解不出序 · 手上没带后端字节）。
pub fn backend_incomparable(machine: &str) -> String {
    copy_text("peerVersion.said.incomparable", &[("machine", machine)])
}

/// 见 [`backend_old`]：接上了，那台有几种帧的形状跟这一侧不一样（那几种收不到，别的照常）。
pub fn backend_frames_differ(machine: &str) -> String {
    copy_text("peerVersion.said.framesDiffer", &[("machine", machine)])
}

/// 见 [`backend_old`]。
pub fn reply_unreadable(machine: &str) -> String {
    copy_text("peerVersion.said.unreadable", &[("machine", machine)])
}

/// 那台握手时说过「这条接得下、这台做不到」的一个码（`hello.unavailable`）⇒ 置灰时说的那一句；认不出的码只说做不到（码不上屏）。
pub fn unavailable(code: &str, machine: &str) -> String {
    match code {
        "no_tmux" => copy_text("control.unavailable.noTmux", &[("machine", machine)]),
        "no_unix_mode" => copy_text("control.unavailable.noUnixMode", &[("machine", machine)]),
        _ => copy_text("control.unavailable.other", &[("machine", machine)]),
    }
}
